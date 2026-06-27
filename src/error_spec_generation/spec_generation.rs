
// * responsible for generating error check specifications for wrapper functions

use crate::error_spec_generation::{spec_generation::ReturnValueCheck::*, wrapper_func_finder::WrapperFunction};
use crate::rustc_hir::intravisit::Visitor;

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum ReturnValueCheck {
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

#[derive(PartialEq, Eq)]
enum ResultType {
    Ok,
    Err,
    Some,
    None
}

#[derive(PartialEq, Eq)]
enum ReturnType {
    ResultOrOption,
    Bool,
    Other,
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

    // functions already visited while going through sub-error-check function: for fix point analysis; if there is a loop, abort to guarantee termination
    already_visited_functions: Vec<rustc_hir::def_id::DefId>
}

impl<'tcx> rustc_hir::intravisit::Visitor<'tcx> for RVCheckFinder<'tcx> {
    // as we go through the expressions of the body, for each expr
    // we check if it is the current holder of the return value, and if so, what happens to it ( via ehck_use_site() )
    fn visit_expr(&mut self, expr: &'tcx rustc_hir::Expr<'tcx>) {

        // if we have already found a check, we do not look further
        if !matches!(self.wrapper_function.return_value_check, ReturnValueCheck::Empty) {
            return;
        }

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
            println!("Checking use site...");
            let rv_check = self.check_use_site(expr);
            if let Some(rv_check) = rv_check {
                self.wrapper_function.return_value_check = rv_check; 
                // abort walk here
                //println!("Aborting walk");
                //return;
            }
        }

        //println!("Continuing walk...");
        rustc_hir::intravisit::walk_expr(self, expr);
    }
}

impl<'tcx> RVCheckFinder<'tcx> {

    // recursively called when a rv ist passed into another function to be checked
    // TODO unite with fin_RV_checks() ?
    fn analyze_error_check_function(
        &mut self,
        tcx: rustc_middle::ty::TyCtxt<'tcx>,
        error_check_function_id: rustc_hir::def_id::DefId,
        arg_index: usize,
    ) -> ReturnValueCheck {
        println!(
            "\nFor Sub Error Check Function {}",
            tcx.def_path_str(error_check_function_id)
        );

        // only works for local functions (no HIR body for external crates)
        let Some(local_def_id) = error_check_function_id.as_local() else {
            println!("Not local!");
            return ReturnValueCheck::Indeterminate;
        };
        // abort if function has no body
        let Some(body) = tcx.hir_maybe_body_owned_by(local_def_id) else {
            println!("No body!");
            return ReturnValueCheck::Indeterminate;
        };

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

            let mut new_visited_function_list = self.already_visited_functions.clone();
            new_visited_function_list.push(error_check_function_id);

            let mut sub_finder = RVCheckFinder{
                tcx: self.tcx,
                wrapper_function: new_wrapper_function,
                wrapped_function_value_holder: Some(param_hir_id),
                already_visited_functions: new_visited_function_list,
            };
            sub_finder.visit_body(body);
            return sub_finder.wrapper_function.return_value_check;
        }

        return ReturnValueCheck::Empty;
    }

    fn check_use_site(&mut self, expr_being_checked: &'tcx rustc_hir::Expr<'tcx>) -> Option<ReturnValueCheck> {

        let parent = self.tcx.hir_parent_iter(expr_being_checked.hir_id).next().map(|(_, node)| node);

        // TODO support for rv borrowing?
        // TODO see pattern from apppend() in Libgit/src/repo.rs::976 : support this?

        match parent {
            // on sth like let result = <tracked expr>: move holder to result's binding HirId
            Some(rustc_hir::Node::LetStmt(local)) => {
                if let rustc_hir::PatKind::Binding(_, hir_id, ident, _) = local.pat.kind {
                    println!("RV identity moves to '{}'", ident.name);
                    self.wrapped_function_value_holder = Some(hir_id);
                }
            }

            Some(rustc_hir::Node::Expr(parent_expr)) if let rustc_hir::ExprKind::Match(_matchee, arms, _) = parent_expr.kind => {

                println!("RV checked via match at {:?}", expr_being_checked.span);

                // only support 2-armed match stmts: one Err, one Ok arm
                // TODO change this?
                if arms.len() != 2 {
                    println!("Arm count !=2");
                    return None;
                }

                println!("Arm count ==2");

                return self.analyze_match_stmt(arms);
            }

            // TODO support first comparand, comparands other than zero ?
            Some(rustc_hir::Node::Expr(parent_expr))
                if let rustc_hir::ExprKind::Binary(bin_op, _ex1, ex2) = parent_expr.kind =>
            {
                let rv_check = ReturnValueCheck::parse_from_bin_op(&bin_op, &ex2);

                // is this a comparison and part of an if stmt?
                if let Some(rv_check) = rv_check.clone() // TODO remove clone?
                    && let Some((_, rustc_hir::Node::Expr(expr))) =
                        self.tcx.hir_parent_iter(parent_expr.hir_id).next()
                    && let rustc_hir::ExprKind::If(cond, then_block, _else_block) = expr.kind
                {
                    // cond should always be our binary expression: confirm this, abort if not
                    if cond.hir_id != parent_expr.hir_id {
                        return None;
                    }
                    
                    println!("RV checked via If comparison at {:?}", expr.span);
                    println!("Binary Operation: {:?}", bin_op.node);

                    return self.analyze_if_stmt(rv_check, then_block);
                    
                } 
            }

            // TODO
            Some(rustc_hir::Node::Expr(parent_expr))
                if matches!(parent_expr.kind, rustc_hir::ExprKind::MethodCall(..)) =>
            {
                if let rustc_hir::ExprKind::MethodCall(method, receiver, args, ..) = parent_expr.kind {

                    println!(
                        "RV checked via method '{}' at {:?}",
                        method.ident, expr_being_checked.span
                    );
                    return self.analyze_method_call(&method, expr_being_checked);
                }
            }

            // TODO boolean returns
            Some(rustc_hir::Node::Expr(parent_expr))
                if let rustc_hir::ExprKind::Call(func, args) = parent_expr.kind =>
            {
                println!("RV passed to another function at {:?}", expr_being_checked.span);
                return self.analyze_function_call(&func, args, expr_being_checked);
            }

            _ => {
                println!("RV use unclassified at {:?}", expr_being_checked.span);
            }
        }

        None
    }

    fn get_function_or_method_return_type(self: &Self, def_id: &rustc_hir::def_id::DefId) -> ReturnType {
        
        let return_type = self.tcx.fn_sig(*def_id).skip_binder().output().skip_binder();

        match return_type.kind() {
            rustc_middle::ty::TyKind::Adt(adt_def, _args) => {
                let type_name = self.tcx.def_path_str(adt_def.did());
                if type_name == "core::result::Result" || type_name == "std::result::Result"
                || type_name == "core::result::Option" || type_name == "std::result::Option" {
                    return ReturnType::ResultOrOption;
                }
            },

            rustc_middle::ty::TyKind::Bool => return ReturnType::Bool,

            _ => return ReturnType::Other
        }

        ReturnType::Other
    }

    // TODO: harmonize with get_method_def_id: pass call expr, not the function itself    
    fn get_function_def_id(self: &Self, func: &rustc_hir::Expr) -> Option<rustc_hir::def_id::DefId> {
        if let rustc_hir::ExprKind::Path(qpath) = &func.kind {
            let owner = self.wrapper_function.wrapper_function_id.expect_local();
            let typeck_results = self.tcx.typeck(owner);
            let res = typeck_results.qpath_res(qpath, func.hir_id);

            if let rustc_hir::def::Res::Def(_, callee_def_id) = res {
                return Some(callee_def_id);
            }
        }
        None
    }
    

    fn analyze_function_call(self: &mut Self, func: &rustc_hir::Expr, args: &[rustc_hir::Expr], expr_being_checked: &rustc_hir::Expr) -> Option<ReturnValueCheck> {

        if let Some(callee_def_id) = &self.get_function_def_id(func) {

            if self.get_function_or_method_return_type(callee_def_id) != ReturnType::ResultOrOption { return None; }

            // which argument number is our RV when being passed in?
            if let Some(arg_index) = args.iter().position(|a| a.hir_id == expr_being_checked.hir_id) {

                println!(
                    "RV passed as arg {} to {} : recursing",
                    arg_index,
                    self.tcx.def_path_str(*callee_def_id)
                );
                
                // if we find a recursion loop, we terminate analysis for this wrapper
                if self.already_visited_functions.contains(&callee_def_id) {
                    println!("Recursion loop found, aborting!");
                    return Some(ReturnValueCheck::Indeterminate);
                }

                return Some(self.analyze_error_check_function(
                    self.tcx,
                    callee_def_id.clone(),
                    arg_index,
                ));
            }
        }

        None
    }

    fn get_method_def_id(self: &Self, method_expr: &rustc_hir::Expr) -> Option<rustc_hir::def_id::DefId> {
        let owner = self.wrapper_function.wrapper_function_id.expect_local();
        let typeck_results = self.tcx.typeck(owner);

        if let Some(def_id) = typeck_results.type_dependent_def_id(method_expr.hir_id) {
            if def_id.is_local() {
                return Some(def_id);
            }
        }
        None
    }

    fn analyze_method_call(self: &Self, method: &rustc_hir::PathSegment, expr_being_checked: &rustc_hir::Expr) -> Option<ReturnValueCheck> {

        // TODO temp, actually implment function
        None
    }

    fn analyze_if_stmt(self: &Self, rv_check: ReturnValueCheck, then_block: &rustc_hir::Expr) -> Option<ReturnValueCheck> {

        let then_result_type = block_result_type(then_block);

        if let Some(arm1_result_type) = then_result_type {
            // if we are checking for an error (and returning as such), we found our rv check
            if matches!(arm1_result_type, ResultType::Err) || matches!(arm1_result_type, ResultType::None) {
                println!("Error Condition is {:?}", rv_check);
                return Some(rv_check);
            //if we are checking for non-error (and thus returning ok), the opposite of the check is our error
            // TODO expand beyond simple either/or (allow for multiple different error checks) ?
            } else if matches!(arm1_result_type, ResultType::Ok) || matches!(arm1_result_type, ResultType::Some) {
                println!("Error Condition is {:?}", rv_check.clone().opposite());
                return Some(rv_check.opposite());
            }

            println!("Neither Error nor Normal Block");
        }

        None
    }

    fn analyze_match_stmt(self: &Self, arms: &[rustc_hir::Arm]) -> Option<ReturnValueCheck> {

        if let Some(arm1_guard) = arms[0].guard {  

            println!("Stepping into Guard 1...");
            if let rustc_hir::ExprKind::Binary(arm1_bin_op, _arm1_bin_ex1, arm1_bin_ex2) = arm1_guard.kind {

                println!("Guard 1 is Binary Expression...");

                let rv_check = ReturnValueCheck::parse_from_bin_op(&arm1_bin_op, &arm1_bin_ex2);

                if let Some(rv_check) = rv_check {

                    let arm1_result_type = arm_result_type(arms[0].body);
                    println!("Binary Operation: {:?}", arm1_bin_op.node);

                    if let Some(arm1_result_type) = arm1_result_type {
                        // if we are checking for an error (and returning as such), we found our rv check
                        if matches!(arm1_result_type, ResultType::Err) || matches!(arm1_result_type, ResultType::None) {
                            println!("Error Condition is {:?}", rv_check);
                            return Some(rv_check);
                        //if we are checking for non-error (and thus returning ok), the opposite of the check is our error
                        // TODO expand beyond simple either/or (allow for multiple different error checks) ?
                        } else if matches!(arm1_result_type, ResultType::Ok) || matches!(arm1_result_type, ResultType::Some) {
                            println!("Error Condition is {:?}", rv_check.clone().opposite());
                            return Some(rv_check.opposite());
                        }

                        println!("Neither Error nor Normal Block");
                    }
                }
            }
        } else {
            println!("No guard found!");
        }
        
        None
    }
}


#[allow(non_snake_case)]
pub fn find_RV_checks(tcx: rustc_middle::ty::TyCtxt<'_>, wrapper_function: WrapperFunction) {
    println!(
        "\nFor Wrapper Function {}",
        tcx.def_path_str(wrapper_function.wrapper_function_id)
    );


    // only works for local functions (no HIR body for external crates)
    let Some(owner_local_def_id) = wrapper_function.wrapper_function_id.as_local() else {
        println!("Not local!");
        return;
    };
    // abort if function has no body
    let Some(body) = tcx.hir_maybe_body_owned_by(owner_local_def_id) else {
        println!("No body!");
        return;
    };

    let mut finder = RVCheckFinder {
        tcx,
        wrapper_function: wrapper_function.clone(), // TODO remove this clone
        wrapped_function_value_holder: None,
        already_visited_functions: Vec::new(),
    };
    finder.visit_body(body);
}


// checks whether a blocks tail expression is Ok()` or Err()
fn block_result_type(block_expr: &rustc_hir::Expr<'_>) -> Option<ResultType> {

    if let rustc_hir::ExprKind::Block(block, _) = block_expr.kind {
        
        // go through all the stmts in the block
        for stmt in block.stmts {

            // return for return stmt, if there is one
            if let rustc_hir::StmtKind::Semi(expr) = stmt.kind {
                if let rustc_hir::ExprKind::Ret(Some(ret_expr)) = expr.kind {
                    println!("Return Statement Found");
                    return expr_result_type(&ret_expr);
                }
            }

            // return for semicolonless expr, if there is one
            // TODO this possibly doesnt do anything? test
            if let rustc_hir::StmtKind::Expr(expr) = stmt.kind {
                println!("Return Expression Found");
                return expr_result_type(&expr);
            }
        }

        if let Some(tail) = block.expr {
            println!("Tail Found");
            return expr_result_type(&tail);
        }
    }
    None
}

// checks whether a match stmts arm expression is Ok() or Err()
fn arm_result_type(arm_body: &rustc_hir::Expr<'_>) -> Option<ResultType> {

    // reuse above function if we have a whole block as our match arm
    if let rustc_hir::ExprKind::Block(_, _) = arm_body.kind {
        return block_result_type(arm_body);
    }
    // return for return stmt, if the arm expr is one
    if let rustc_hir::ExprKind::Ret(Some(ret_expr)) = arm_body.kind {
        return expr_result_type(&ret_expr);
    }

    expr_result_type(arm_body)
}

fn expr_result_type(expr: &rustc_hir::Expr<'_>) -> Option<ResultType> {
    if let rustc_hir::ExprKind::Call(func, _) = &expr.kind {
        if let rustc_hir::ExprKind::Path(qpath) = &func.kind {
            if let rustc_hir::QPath::Resolved(_, path) = qpath {
                if let Some(seg) = path.segments.last() {
                    if seg.ident.name.as_str() == "Ok" {
                        println!("Found Ok");
                        return Some(ResultType::Ok);
                    } else if seg.ident.name.as_str() == "Err" {
                        println!("Found Err");
                        return Some(ResultType::Err);
                    } else if seg.ident.name.as_str() == "Some" {
                        println!("Found Some");
                        return Some(ResultType::Some);
                    }
                }
            }
        } 
    // None is a path, not a call, must be treated differently
    } else if let rustc_hir::ExprKind::Path(qpath) = &expr.kind {
        if let rustc_hir::QPath::Resolved(_, path) = qpath {
            if let Some(seg) = path.segments.last() {
                if seg.ident.name.as_str() == "None" {
                    println!("Found None");
                    return Some(ResultType::None);
                }
            }
        }
    }
    None
}
