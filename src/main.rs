#![feature(rustc_private)]

extern crate rustc_abi;
extern crate rustc_ast;
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_span;

use rustc_hir::intravisit::Visitor;
use std::collections::HashSet;

use crate::ReturnValueCheck::*;

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
enum ReturnValueCheck {
    Empty,
    LesserZero,
    GreaterZero,
    NotEqZero,
    LesEqZero,
    GrEqZero,
    EqualZero,
    All, // should never be used, since no function should always return an error
    Indeterminate,
}

impl ReturnValueCheck {
    fn opposite(self) -> ReturnValueCheck {
        match self {
            Empty => All,
            LesserZero => GrEqZero,
            GreaterZero => LesEqZero,
            NotEqZero => EqualZero,
            LesEqZero => GreaterZero,
            GrEqZero => LesserZero,
            EqualZero => NotEqZero,
            All => Empty,
            _ => Indeterminate,
        }
    }

    fn parse_from_bin_op(
        bin_op: &rustc_hir::BinOp,
        comparand: &rustc_hir::Expr,
    ) -> Option<ReturnValueCheck> {
        // is our comparand a literal?
        if let rustc_hir::ExprKind::Lit(lit) = comparand.kind {
            // an int literal?
            if let rustc_ast::LitKind::Int(val, _) = lit.node {
                // is it 0?
                if val.get() != 0 {
                    // if not, abort, we only support predicates relative to zero
                    // TODO change this limitation?
                    return Some(Self::Indeterminate);
                }
            }
        }

        // if coparand is 0, parse
        match bin_op.node {
            rustc_hir::BinOpKind::Eq => Some(Self::EqualZero),
            rustc_hir::BinOpKind::Lt => Some(Self::LesserZero),
            rustc_hir::BinOpKind::Le => Some(Self::LesEqZero),
            rustc_hir::BinOpKind::Ne => Some(Self::NotEqZero),
            rustc_hir::BinOpKind::Ge => Some(Self::GrEqZero),
            rustc_hir::BinOpKind::Gt => Some(Self::GreaterZero),
            _ => None,
        }
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
struct WrapperFunction {
    wrapper_function_id: rustc_hir::def_id::DefId,
    wrapped_function_id: rustc_hir::def_id::DefId,
    return_value_check: ReturnValueCheck,
}

fn main() {
    let mut args: Vec<String> = std::env::args().collect();

    // When used as RUSTC_WRAPPER, cargo passes the path to rustc as the
    // first argument. We need to skip it.
    // why exactly? claude says so and it works ¯\_(ツ)_/¯
    if args.len() > 1 && args[1].contains("rustc") {
        args.remove(1);
    }

    // cancel when we're actually bulding; we only want to run the analysis on cargo check
    let is_build = args.iter().any(|a| a.contains("link"));
    if is_build {
        rustc_driver::run_compiler(&args, &mut rustc_driver::TimePassesCallbacks::default());
        return;
    }

    // callback / after_analysis will hook in
    rustc_driver::run_compiler(&args, &mut ExternFuncCheckCallbacks);
}

fn find_external_functions<'tcx>(
    tcx: rustc_middle::ty::TyCtxt<'tcx>,
) -> HashSet<rustc_hir::def_id::DefId> {
    let mut extern_function_ids = HashSet::new();

    // go through all free (that is, top-level) items
    for item in tcx.hir_free_items().map(|id| tcx.hir_item(id)) {
        // consider only extern blocks, ignore extern blocks that arent extern "C"
        if let rustc_hir::ItemKind::ForeignMod { abi, items } = item.kind
            && matches!(abi, rustc_abi::ExternAbi::C { .. })
        {
            let filename = tcx.sess.source_map().span_to_filename(item.span);
            println!("Found extern C Block in {}", filename.short());

            // go through the foreign functions in this block
            for foreign_item in items.iter().map(|id| tcx.hir_foreign_item(*id)) {
                if let rustc_hir::ForeignItemKind::Fn(..) = foreign_item.kind {
                    println!("Found a foreign function: {}", foreign_item.ident.name);
                    extern_function_ids.insert(foreign_item.owner_id.to_def_id());
                }
            }
        }
    }

    extern_function_ids
}

fn find_wrapper_functions(
    tcx: rustc_middle::ty::TyCtxt<'_>,
    extern_function_ids: &HashSet<rustc_hir::def_id::DefId>,
) -> Vec<WrapperFunction> {
    let mut wrapper_functions: Vec<WrapperFunction> = Vec::new();

    // go through all funtions, use visit_expr() to go through all expression and see if they are calls to an extern function
    for item in tcx.hir_free_items().map(|id| tcx.hir_item(id)) {
        if let rustc_hir::ItemKind::Fn { body: body_id, .. } = &item.kind {
            let body = tcx.hir_body(*body_id);
            let owner_def_id = item.owner_id.to_def_id();

            let mut finder = WrapperFuncFinder {
                tcx,
                extern_function_ids,
                owner_def_id,
                wrapper_functions: HashSet::new(),
            };
            finder.visit_body(body);
            wrapper_functions.extend(finder.wrapper_functions);
        }
    }
    wrapper_functions
}

#[allow(non_snake_case)]
fn find_RV_checks(tcx: rustc_middle::ty::TyCtxt<'_>, wrapper_function: WrapperFunction) {
    println!(
        "\nFor Wrapper Function {}",
        tcx.def_path_str(wrapper_function.wrapper_function_id)
    );

    let owner_local_def_id = wrapper_function.wrapper_function_id.expect_local();
    let body = tcx.hir_body_owned_by(owner_local_def_id);

    let mut finder = RVCheckFinder {
        tcx,
        wrapper_function: wrapper_function.clone(), // TODO remove this clone
        wrapped_function_value_holder: None,
    };
    finder.visit_body(body);
}

enum ResultType {
    Ok,
    Err,
}

// checks whether a blocks tail expression is Ok()` or Err()
fn branch_result_type(block_expr: &rustc_hir::Expr<'_>) -> Option<ResultType> {
    if let rustc_hir::ExprKind::Block(block, _) = block_expr.kind {
        if let Some(tail) = block.expr {
            if let rustc_hir::ExprKind::Call(func, _) = &tail.kind {
                if let rustc_hir::ExprKind::Path(qpath) = &func.kind {
                    if let rustc_hir::QPath::Resolved(_, path) = qpath {
                        if let Some(seg) = path.segments.last() {
                            if seg.ident.name.as_str() == "Ok" {
                                return Some(ResultType::Ok);
                            }
                            if seg.ident.name.as_str() == "Err" {
                                return Some(ResultType::Err);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

struct ExternFuncCheckCallbacks;

impl rustc_driver::Callbacks for ExternFuncCheckCallbacks {
    fn after_analysis<'tcx>(
        &mut self,
        _compiler: &rustc_interface::interface::Compiler,
        tcx: rustc_middle::ty::TyCtxt<'tcx>,
    ) -> rustc_driver::Compilation {
        // only analyze the primary crate (so not dependencies etc)
        if std::env::var("CARGO_PRIMARY_PACKAGE").is_err() {
            return rustc_driver::Compilation::Continue;
        }

        println!("Checker starting...");

        let extern_function_ids: HashSet<_> = find_external_functions(tcx);

        let wrapper_functions = find_wrapper_functions(tcx, &extern_function_ids);

        for wrapper_function in wrapper_functions {
            find_RV_checks(tcx, wrapper_function);
        }

        rustc_driver::Compilation::Continue
    }
}

// finds the checks on return values (=RV)
struct RVCheckFinder<'tcx> {
    tcx: rustc_middle::ty::TyCtxt<'tcx>,
    wrapper_function: WrapperFunction,
    // the current holder of the return value of the external function;
    // initially the extern call expression's own HirId
    // after "let result = call()"": result's binding HirId
    // after "let result2 = result": result2's binding HirId
    // etc
    wrapped_function_value_holder: Option<rustc_hir::HirId>,
}

impl<'tcx> rustc_hir::intravisit::Visitor<'tcx> for RVCheckFinder<'tcx> {
    // as we go through the expressions of the body, for each expr
    // we check if it is the current holder of the return value, and if so, what happens to it ( via ehck_use_site() )
    fn visit_expr(&mut self, expr: &'tcx rustc_hir::Expr<'tcx>) {
        let owner = self.wrapper_function.wrapper_function_id.expect_local();
        let typeck_results = self.tcx.typeck(owner);

        // does the current expr match what were tracking?
        let matches_tracked: bool = match (&expr.kind, self.wrapped_function_value_holder) {
            // not tracking yet: is this expr a call to the wrapped external func?
            (rustc_hir::ExprKind::Call(func, _), None) => {
                if let rustc_hir::ExprKind::Path(qpath) = &func.kind {
                    let res = typeck_results.qpath_res(qpath, func.hir_id);
                    matches!(res, rustc_hir::def::Res::Def(_, def_id) if def_id == self.wrapper_function.wrapped_function_id)
                } else {
                    false
                }
            }
            // already tracking: is this expr a Path to the current holder?
            (rustc_hir::ExprKind::Path(qpath), Some(holder)) => {
                let res = typeck_results.qpath_res(qpath, expr.hir_id);
                matches!(res, rustc_hir::def::Res::Local(hir_id) if hir_id == holder)
            }
            //otherwise
            _ => false,
        };

        if matches_tracked {
            if self.wrapped_function_value_holder.is_none() {
                self.wrapped_function_value_holder = Some(expr.hir_id);
            }
            let rv_check = self.check_use_site(expr);
            if let Some(rv_check) = rv_check {
                self.wrapper_function.return_value_check = rv_check; // TODO abort walk here?
            }
        }

        rustc_hir::intravisit::walk_expr(self, expr);
    }
}

impl<'tcx> RVCheckFinder<'tcx> {
    // recursivel called when a rv ist passed into another funciton to be checked
    fn analyze_error_check_function(
        &mut self,
        tcx: rustc_middle::ty::TyCtxt<'_>,
        error_check_function_id: rustc_hir::def_id::DefId,
        arg_index: usize,
    ) -> ReturnValueCheck {
        println!(
            "\nFor Sub Error Check Function {}",
            tcx.def_path_str(error_check_function_id)
        );

        // only works for local functions (no HIR body for external crates)
        let Some(local_def_id) = error_check_function_id.as_local() else {
            return ReturnValueCheck::Empty;
        };

        let body = self.tcx.hir_body_owned_by(local_def_id);

        // get the parameter pattern at arg_index
        let Some(param) = body.params.get(arg_index) else {
            return ReturnValueCheck::Empty;
        };

        // that parameter's binding hir id becomes the new tracked identity
        if let rustc_hir::PatKind::Binding(_, param_hir_id, _, _) = param.pat.kind {
            let new_wrapper_function = WrapperFunction {
                wrapper_function_id: error_check_function_id,
                wrapped_function_id: self.wrapper_function.wrapped_function_id,
                return_value_check: ReturnValueCheck::Empty,
            };

            let mut sub_finder = RVCheckFinder {
                tcx: self.tcx,
                wrapper_function: new_wrapper_function,
                wrapped_function_value_holder: Some(param_hir_id),
            };
            sub_finder.visit_body(body);
            return sub_finder.wrapper_function.return_value_check;
        }

        return ReturnValueCheck::Empty;
    }

    fn check_use_site(&mut self, expr: &'tcx rustc_hir::Expr<'tcx>) -> Option<ReturnValueCheck> {
        let parent = self.tcx.hir_parent_iter(expr.hir_id).next();

        // TODO support for rv borrowing?
        // TODO full support for match, cross-function
        match parent.map(|(_, node)| node) {
            // let result = <tracked expr>: move holder to result's binding HirId
            Some(rustc_hir::Node::LetStmt(local)) => {
                if let rustc_hir::PatKind::Binding(_, hir_id, ident, _) = local.pat.kind {
                    println!("RV identity moves to '{}'", ident.name);
                    self.wrapped_function_value_holder = Some(hir_id);
                }
            }

            // TODO
            Some(rustc_hir::Node::Expr(e)) if matches!(e.kind, rustc_hir::ExprKind::Match(..)) => {
                println!("RV checked via match at {:?}", expr.span);
            }

            Some(rustc_hir::Node::Expr(e))
                if let rustc_hir::ExprKind::Binary(bin_op, _ex1, ex2) = e.kind =>
            {
                // TODO into own function
                let rv_check = ReturnValueCheck::parse_from_bin_op(&bin_op, &ex2);
                // is this a comparison and part of an if stmt?
                if let Some(rv_check) = rv_check
                    && let Some((_, rustc_hir::Node::Expr(expr))) =
                        self.tcx.hir_parent_iter(e.hir_id).next()
                    && let rustc_hir::ExprKind::If(cond, then_block, _else_block) = expr.kind
                {
                    // cond should always be our binary expression: confirm this
                    if cond.hir_id != e.hir_id {
                        return None;
                    }

                    let then_result_type = branch_result_type(then_block);
                    println!("RV checked via comparison at {:?}", expr.span);
                    println!("Binary Operation: {:?}", bin_op.node);

                    // if we are checking for an error (and returning as such), we found our rv check
                    if let Some(ResultType::Err) = then_result_type {
                        println!("Error Condition is {:?}", rv_check);
                        return Some(rv_check);
                    //if we are checking for non-error (and thus returning ok), the opposite of the check is our error
                    // TODO expand beyond simple either/or (allow for multiple different error checks) ?
                    } else if let Some(ResultType::Ok) = then_result_type {
                        println!("Error Condition is {:?}", rv_check.clone().opposite());
                        return Some(rv_check.opposite());
                    }

                    println!("Neither Error nor Ok Block");
                }
            }

            // TODO ?
            Some(rustc_hir::Node::Expr(e))
                if matches!(e.kind, rustc_hir::ExprKind::MethodCall(..)) =>
            {
                if let rustc_hir::ExprKind::MethodCall(method, ..) = e.kind {
                    println!(
                        "RV checked via method '{}' at {:?}",
                        method.ident, expr.span
                    );
                }
            }

            // TODO boolean returns
            Some(rustc_hir::Node::Expr(e))
                if let rustc_hir::ExprKind::Call(func, args) = e.kind =>
            {
                println!("RV passed to another function at {:?}", expr.span);
                // which argument number is our RV when being passed in?
                if let Some(arg_index) = args.iter().position(|a| a.hir_id == expr.hir_id) {
                    if let rustc_hir::ExprKind::Path(qpath) = &func.kind {
                        let owner = self.wrapper_function.wrapper_function_id.expect_local();
                        let typeck_results = self.tcx.typeck(owner);
                        let res = typeck_results.qpath_res(qpath, func.hir_id);

                        if let rustc_hir::def::Res::Def(_, callee_def_id) = res {
                            println!(
                                "RV passed as arg {} to {} : recursing",
                                arg_index,
                                self.tcx.def_path_str(callee_def_id)
                            );

                            return Some(self.analyze_error_check_function(
                                self.tcx,
                                callee_def_id,
                                arg_index,
                            ));
                        }
                    }
                }
            }

            _ => {
                println!("RV use unclassified at {:?}", expr.span); // TODO no print at all here?
            }
        }

        None
    }
}

struct WrapperFuncFinder<'a, 'tcx> {
    tcx: rustc_middle::ty::TyCtxt<'tcx>,
    extern_function_ids: &'a HashSet<rustc_hir::def_id::DefId>,
    owner_def_id: rustc_hir::def_id::DefId,
    wrapper_functions: HashSet<WrapperFunction>,
}

impl<'a, 'tcx> rustc_hir::intravisit::Visitor<'tcx> for WrapperFuncFinder<'a, 'tcx> {
    fn visit_expr(&mut self, expr: &'tcx rustc_hir::Expr<'tcx>) {
        // function calls
        if let rustc_hir::ExprKind::Call(func, _args) = &expr.kind {
            // gets path to definition of function
            if let rustc_hir::ExprKind::Path(qpath) = &func.kind {
                let typeck_results = self.tcx.typeck(self.owner_def_id.expect_local());
                let resolution = typeck_results.qpath_res(qpath, func.hir_id);

                // resolutes to a definition
                if let rustc_hir::def::Res::Def(_, callee_def_id) = resolution {
                    if self.extern_function_ids.contains(&callee_def_id) {
                        println!(
                            "Call to external function {:?} in {}",
                            self.tcx.def_path_str(callee_def_id),
                            self.tcx.def_path_str(self.owner_def_id)
                        );
                        self.wrapper_functions.insert(WrapperFunction {
                            wrapper_function_id: self.owner_def_id,
                            wrapped_function_id: callee_def_id,
                            // until we find a specific check in the RV check finder step, we assume nothing is an error
                            return_value_check: ReturnValueCheck::Empty,
                        });
                    }
                }
            }
        }
        rustc_hir::intravisit::walk_expr(self, expr);
    }
}
