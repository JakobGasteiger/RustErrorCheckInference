// * responsible for the complier hook and managing the flow of the analysis

use std::ops::Add;
use std::ops::AddAssign;

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
        let crate_name = tcx.crate_name(rustc_hir::def_id::LOCAL_CRATE);

        println!("\n\nChecker starting; crate = {}", crate_name);

        let sys_crates = find_sys_crates(tcx);

        let extern_function_ids = find_external_functions(tcx, &sys_crates);

        let mut wrapper_functions = find_wrapper_functions(tcx, &extern_function_ids);

        let mut other_statistics = OtherStatistics::new();

        for mut wrapper_function in &mut wrapper_functions {
            find_RV_checks(tcx, &mut wrapper_function, &mut other_statistics);
            //println!("{:?}", wrapper_function);
        }

        aggregate_and_print_error_check_statistics(&wrapper_functions);
        other_statistics.output();

        rustc_driver::Compilation::Continue
    }
}

fn aggregate_and_print_error_check_statistics(wrapper_functions: &Vec<WrapperFunction>) {
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

    for wrapper_function in wrapper_functions {
        //println!("{:?}", wrapper_function);
        total += 1;
        match wrapper_function.return_value_check {
            Some(ReturnValueCheck::Empty) => empty += 1,
            Some(ReturnValueCheck::GrEqZero) => gr_eq_zero += 1,
            Some(ReturnValueCheck::LesEqZero) => les_eq_zero += 1,
            Some(ReturnValueCheck::EqualZero) => equal_zero += 1,
            Some(ReturnValueCheck::GreaterZero) => greater_zero += 1,
            Some(ReturnValueCheck::LesserZero) => lesser_zero += 1,
            Some(ReturnValueCheck::NotEqZero) => not_eq_zero += 1,
            Some(ReturnValueCheck::All) => all += 1,
            Some(ReturnValueCheck::Indeterminate) => indeterminate += 1,
            None => indeterminate += 1, // treat None as indeterminate
        }
    }

    println!("\n\nError Check Statistics:");
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
}

// sometime multiple crates are named *-sys: we look for external funcs in them all
fn find_sys_crates<'tcx>(tcx: rustc_middle::ty::TyCtxt<'tcx>) -> Vec<rustc_span::def_id::CrateNum> {
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OtherStatistics {
    // TODO add more statistics here
    pub bool_functions_not_yet_supported: usize,
    pub bool_methods_not_yet_supported: usize,
    pub not_result_or_option_return_types: usize,
    pub not_local_functions: usize,
    pub hardcoded_bool_methods_analyzed: usize,
    pub condition_negations: usize,
}

impl OtherStatistics {
    pub fn new() -> Self {
        OtherStatistics {
            bool_functions_not_yet_supported: 0,
            bool_methods_not_yet_supported: 0,
            not_result_or_option_return_types: 0,
            not_local_functions: 0,
            hardcoded_bool_methods_analyzed: 0,
            condition_negations:0,
        }
    }
}

impl Add for OtherStatistics {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        OtherStatistics {
            bool_functions_not_yet_supported: self.bool_functions_not_yet_supported
                + other.bool_functions_not_yet_supported,
            bool_methods_not_yet_supported: self.bool_methods_not_yet_supported
                + other.bool_methods_not_yet_supported,
            not_result_or_option_return_types: self.not_result_or_option_return_types
                + other.not_result_or_option_return_types,
            not_local_functions: self.not_local_functions + other.not_local_functions,
            hardcoded_bool_methods_analyzed: self.hardcoded_bool_methods_analyzed
                + other.hardcoded_bool_methods_analyzed,
            condition_negations: self.condition_negations + other.condition_negations
        }
    }
}

impl AddAssign for OtherStatistics {
    fn add_assign(&mut self, other: Self) {
        *self = self.clone() + other;
    }
}

impl OtherStatistics {
    pub fn output(&self) {
        println!("\nOther Statistics:");
        println!(
            "Boolean functions not yet supported: {}",
            self.bool_functions_not_yet_supported
        );
        println!(
            "Boolean methods not yet supported: {}",
            self.bool_methods_not_yet_supported
        );
        println!(
            "Not Result/Option return types: {}",
            self.not_result_or_option_return_types
        );
        println!("Not local functions: {}", self.not_local_functions);
        println!(
            "Hardcoded Bool functions analyzed: {}",
            self.hardcoded_bool_methods_analyzed
        );
        println!("Condition negations: {}", self.condition_negations);
    }
}
