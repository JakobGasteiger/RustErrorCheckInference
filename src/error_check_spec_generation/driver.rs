
// * responsible for the complier hook and managing the flow of the analysis

use crate::error_check_spec_generation::spec_generation::ReturnValueCheck;
use crate::error_check_spec_generation::spec_generation::find_RV_checks;
use crate::error_check_spec_generation::wrapper_func_finder::WrapperFunction;
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

        let mut wrapper_functions = find_wrapper_functions(tcx, &extern_function_ids);

        for mut wrapper_function in &mut wrapper_functions {
            find_RV_checks(tcx, &mut wrapper_function);
            //println!("{:?}", wrapper_function);
        }

        aggregate_and_print_statistics(&wrapper_functions);

        rustc_driver::Compilation::Continue
    }
}


fn aggregate_and_print_statistics(wrapper_functions: &Vec<WrapperFunction>) {
    let mut total = 0;
    let mut empty = 0;
    let mut gr_eq_zero = 0;
    let mut les_eq_zero = 0;
    let mut equal_zero = 0;
    let mut greater_zero = 0;
    let mut lesser_zero = 0;
    let mut not_eq_zero = 0;
    let mut all = 0;
    let mut indeterminate = 0;
    let mut indeterminate_not_local = 0;

    for wrapper_function in wrapper_functions {

        //println!("{:?}", wrapper_function);
        total += 1;
        match wrapper_function.return_value_check {
            ReturnValueCheck::Empty => empty += 1,
            ReturnValueCheck::GrEqZero => gr_eq_zero += 1,
            ReturnValueCheck::LesEqZero => les_eq_zero += 1,
            ReturnValueCheck::EqualZero => equal_zero += 1,
            ReturnValueCheck::GreaterZero => greater_zero += 1,
            ReturnValueCheck::LesserZero => lesser_zero += 1,
            ReturnValueCheck::NotEqZero => not_eq_zero += 1,
            ReturnValueCheck::All => all += 1,
            ReturnValueCheck::Indeterminate => indeterminate += 1,
            ReturnValueCheck::IndeterminateNotLocal => indeterminate_not_local += 1,
        }
    }

    println!("\n\nStatistics:");
    println!("Total wrapper functions: {}", total);
    println!("Empty: {}", empty);
    println!("GrEqZero: {}", gr_eq_zero);
    println!("LesEqZero: {}", les_eq_zero);
    println!("EqualZero: {}", equal_zero);
    println!("GreaterZero: {}", greater_zero);
    println!("LesserZero: {}", lesser_zero);
    println!("NotEqZero: {}", not_eq_zero);
    println!("All: {}", all);
    println!("Indeterminate: {}", indeterminate);
    println!("IndeterminateNotLocal: {}", indeterminate_not_local);
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