// * responsible for finding external functions and their wrappers

use crate::{rustc_hir::intravisit::Visitor, utils::error_spec::{ErrorSpecPredicate, WrapperFunctionSpec}};


struct WrapperFuncFinder<'a, 'tcx> {
    tcx: rustc_middle::ty::TyCtxt<'tcx>,
    extern_function_ids: &'a Vec<rustc_hir::def_id::DefId>,
    owner_def_id: rustc_hir::def_id::DefId,
    wrapper_functions: Vec<WrapperFunctionSpec>,
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
                        self.wrapper_functions.push(WrapperFunctionSpec {
                            wrapper_function_id: self.owner_def_id,
                            wrapped_function_id: callee_def_id,
                            // until we find a specific check in the RV check finder step, we assume nothing is an error
                            return_value_check: None,
                        });
                    }
                }
            }
        }
        rustc_hir::intravisit::walk_expr(self, expr);
    }
}

pub fn find_external_functions<'tcx>(
    tcx: rustc_middle::ty::TyCtxt<'tcx>,
    sys_crates: &Vec<rustc_span::def_id::CrateNum>,
) -> Vec<rustc_hir::def_id::DefId> {

    println!("\nLooking for external functions...");

    let mut external_functions = Vec::new();

    for sys_crate in sys_crates {

        println!("Looking for external functions in {}", tcx.crate_name(*sys_crate).as_str());

        let crate_external_functions = tcx
            .foreign_modules(*sys_crate)
            .values()
            .flat_map(|foreign_mod| foreign_mod.foreign_items.iter().copied())
            .filter(|did| matches!(tcx.def_kind(*did), rustc_hir::def::DefKind::Fn));

        external_functions.extend(crate_external_functions);
    }

    for ext_func in &external_functions {
        println!("Found external function: {:?}", tcx.def_path_str(*ext_func));
    }

    external_functions
}

pub fn find_wrapper_functions(
    tcx: rustc_middle::ty::TyCtxt<'_>,
    extern_function_ids: &Vec<rustc_hir::def_id::DefId>,
) -> Vec<WrapperFunctionSpec> {
    let mut wrapper_functions: Vec<WrapperFunctionSpec> = Vec::new();

    // go through all functions incl those in impl blocks, use visit_expr() to go through all expression and see if they are calls to an extern function
    for item in tcx.hir_free_items().map(|id| tcx.hir_item(id)) {
        if let rustc_hir::ItemKind::Fn { body: body_id, .. } = &item.kind {
            let body = tcx.hir_body(*body_id);
            let owner_def_id = item.owner_id.to_def_id();

            let mut finder = WrapperFuncFinder {
                tcx,
                extern_function_ids,
                owner_def_id,
                wrapper_functions: Vec::new(),
            };
            finder.visit_body(body);
            wrapper_functions.extend(finder.wrapper_functions);
        } else if let rustc_hir::ItemKind::Impl(impl_block) = &item.kind {
            // same as above for all the funcitons inide the impl block (code essentially copied)
            // TODO reduce biolerplate here?
            for impl_item in impl_block
                .items
                .iter()
                .map(|impl_item_id| tcx.hir_impl_item(*impl_item_id))
            {
                if let rustc_hir::ImplItemKind::Fn(_, body_id) = &impl_item.kind {
                    let body = tcx.hir_body(*body_id);
                    let owner_def_id = impl_item.owner_id.to_def_id();

                    let mut finder = WrapperFuncFinder {
                        tcx,
                        extern_function_ids,
                        owner_def_id,
                        wrapper_functions: Vec::new(),
                    };
                    finder.visit_body(body);
                    wrapper_functions.extend(finder.wrapper_functions);
                }
            }
        }
    }
    wrapper_functions
}

// sometime multiple crates are named *-sys: we look for external funcs in them all
pub fn find_sys_crates<'tcx>(tcx: rustc_middle::ty::TyCtxt<'tcx>) -> Vec<rustc_span::def_id::CrateNum> {
    let mut sys_crates = Vec::new();

    for cnum in tcx.crates(()) {
        let name = tcx.crate_name(*cnum);
        println!("Checking crate: {}", name.as_str());
        if name.as_str().ends_with("sys") {
            println!("Found sys crate: {}", name.as_str());
            sys_crates.push(cnum.clone());
        }
    }

    // if no *-sys crate found, return own crate (actually quite useful for simplified testing)
    if sys_crates.is_empty() {
        println!("No sys crate found, returning local");
        sys_crates.push(rustc_hir::def_id::LOCAL_CRATE);
    }

    println!("Sys crates:");
    for krate in &sys_crates {
        let name = tcx.crate_name(krate.clone());
        println!("{}", name.as_str());
    }
    sys_crates
}
