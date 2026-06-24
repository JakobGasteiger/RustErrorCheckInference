
// * responsible for the complier hook and managing the flow of the analysis

use crate::error_spec_generation::spec_generation::find_RV_checks;
use crate::error_spec_generation::wrapper_func_finder::find_external_functions;
use crate::error_spec_generation::wrapper_func_finder::find_wrapper_functions;

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

        let sys_crate = find_sys_crate(tcx);

        let extern_function_ids: Vec<_> = find_external_functions(tcx, sys_crate);

        let wrapper_functions = find_wrapper_functions(tcx, &extern_function_ids);

        for wrapper_function in wrapper_functions {
            find_RV_checks(tcx, wrapper_function);
        }

        rustc_driver::Compilation::Continue
    }
}

fn find_sys_crate<'tcx>(tcx: rustc_middle::ty::TyCtxt<'tcx>) -> rustc_span::def_id::CrateNum {

    for cnum in tcx.crates(()) {
        let name = tcx.crate_name(*cnum);
        println!("Checking crate: {}", name.as_str());
        if name.as_str().ends_with("sys") {
            println!("Found *-sys crate: {}", name.as_str());
            return cnum.clone();
        }
    }

    // if no *-sys crate found, return own crate (actually quite useful for simplified testing)
    println!("No *-sys crate found, returning local");
    rustc_hir::def_id::LOCAL_CRATE
}