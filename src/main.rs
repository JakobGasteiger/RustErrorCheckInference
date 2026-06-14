#![feature(rustc_private)]

extern crate rustc_ast;
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_span;
extern crate rustc_abi;

use rustc_hir::intravisit::{self, Visitor};
use std::collections::HashSet;


enum ReturnValueCheck {
    None,
    LesserZero,
    GreaterZero,
    NotEqZero,
    LesEqZero,
    GrEqZero,
    EqualZero,
    All,
    Indeterminate
}

#[derive(PartialEq, Eq, Hash)]
struct WrapperFunction {
    wrapper_function_id: rustc_hir::def_id::DefId,
    wrapped_function_id: rustc_hir::def_id::DefId,
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

fn find_external_functions<'tcx>(tcx: rustc_middle::ty::TyCtxt<'tcx>) -> HashSet<rustc_hir::def_id::DefId> {
    let mut extern_function_ids = HashSet::new();

    // go through all free (that is, top-level) items
    for item in tcx.hir_free_items().map(|id| tcx.hir_item(id)) {

        // consider only extern blocks, ignore extern blocks that arent extern "C"
        if let rustc_hir::ItemKind::ForeignMod{abi, items} = item.kind && matches!(abi, rustc_abi::ExternAbi::C{ .. }) {

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

fn find_wrapper_functions(tcx: rustc_middle::ty::TyCtxt<'_>, extern_function_ids: &HashSet<rustc_hir::def_id::DefId>) -> HashSet<WrapperFunction> {

    let mut wrapper_functions: HashSet<WrapperFunction> = HashSet::new();

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

        rustc_driver::Compilation::Continue
    }
}

// finds the checks on return values (=RV)
// TODO check happes crossfunctionally, variable assignment (recursive); including borrowing?
// TODO integrate with finder => only one run of visit_body()? maybe not
struct RVCheckFinder<'tcx> {
    tcx: rustc_middle::ty::TyCtxt<'tcx>,
    wrapper_function: WrapperFunction,
    extern_function_value_holder: rustc_hir::def_id::DefId,
}

impl<'tcx> rustc_hir::intravisit::Visitor<'tcx> for RVCheckFinder<'tcx> {
    fn visit_expr(&mut self, expr: &'tcx rustc_hir::Expr<'tcx>) {
        // TODO

        // TODO integrate this code responsible for variable assignment finding
        // check parent for assignment / binding
        // let parent = self.tcx.hir_parent_iter(expr.hir_id).next();
        // if let Some((_, rustc_hir::Node::LetStmt(let_stmt))) = parent {
        //     if let rustc_hir::PatKind::Binding(_, _, ident, _) = let_stmt.pat.kind {
        //         println!("-> bound to variable: {}", ident.name);
        //     }
        // }
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
                            self.tcx.def_path_str(callee_def_id), self.tcx.def_path_str(self.owner_def_id)
                        );
                        self.wrapper_functions.insert(WrapperFunction {
                            wrapper_function_id: callee_def_id,
                            wrapped_function_id: self.owner_def_id
                        });
                    }
                }
            }
        }
        rustc_hir::intravisit::walk_expr(self, expr);
    }
}
