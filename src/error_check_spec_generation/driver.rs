
// * responsible for the complier hook and managing the flow of the analysis

use crate::error_check_spec_generation::spec_generation::find_RV_checks;
use crate::error_check_spec_generation::wrapper_func_finder::find_external_functions;
use crate::error_check_spec_generation::wrapper_func_finder::find_wrapper_functions;

pub struct ExternFuncCheckCallbacks;

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

        let sys_crates = find_sys_crates(tcx);

        let extern_function_ids = find_external_functions(tcx, &sys_crates);

        let wrapper_functions = find_wrapper_functions(tcx, &extern_function_ids);

        for wrapper_function in wrapper_functions {
            find_RV_checks(tcx, wrapper_function);
        }

        rustc_driver::Compilation::Continue
    }
}

// sometime multiple crates are named *-sys: we look for external funcs in them all
fn find_sys_crates<'tcx>(tcx: rustc_middle::ty::TyCtxt<'tcx>) -> Vec<rustc_span::def_id::CrateNum> {

    let mut sys_crates = Vec::new();

    for cnum in tcx.crates(()) {
        let name = tcx.crate_name(*cnum);
        println!("Checking crate: {}", name.as_str());
        if name.as_str().ends_with("sys") {
            println!("Found *-sys crate: {}", name.as_str());
            sys_crates.push(cnum.clone());
        }
    }

    // if no *-sys crate found, return own crate (actually quite useful for simplified testing)
    if sys_crates.is_empty() {
        println!("No *-sys crate found, returning local");
        sys_crates.push(rustc_hir::def_id::LOCAL_CRATE);
    }
    
    sys_crates
}