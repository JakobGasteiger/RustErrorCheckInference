
use std::collections::HashSet;

use crate::{spec_comparison, utils::error_spec::{ErrorSpec, FunctionErrorSpec, WrapperFunction}};

#[derive(Debug, Clone)]
pub enum SpecComparisonResult {
    EqualOK,
    NotEqualPossibleBug,
    CannotCompare
}

pub fn compare_specs(tcx: rustc_middle::ty::TyCtxt<'_>, c_side_specs: Vec<FunctionErrorSpec>, rust_side_specs: Vec<WrapperFunction>) -> Vec<SpecComparisonResult> {

    println!("\n\nComparing C and rust Side Specs...");

    let mut spec_comparison_results = Vec::new();

    for (c_side_spec, rust_side_spec) in find_pairs(tcx, c_side_specs, rust_side_specs) {

        // ! item_name can panic, replace with opt_item name if tthis becomes an actual problem
        let wrapped_function_name_sym = tcx.item_name(rust_side_spec.clone().wrapped_function_id); 
        let wrapped_function_name = wrapped_function_name_sym.as_str();

        println!("\nComparisong for Wrapping of {} in {}...", wrapped_function_name, tcx.def_path_str(rust_side_spec.wrapper_function_id));

        // if the retvalcheck was still none, we consider it indeterminate
        let rust_side_check = rust_side_spec.return_value_check.unwrap_or(ErrorSpec::Indeterminate);
        println!("Rust Side: {:?}", rust_side_check);

        let c_side_check = c_side_spec.error_spec;
        println!("C Side: {:?}", c_side_check);

        let spec_comparison_result = match (rust_side_check, c_side_check) {
            (ErrorSpec::Indeterminate, _) | (_, ErrorSpec::Indeterminate) => SpecComparisonResult::CannotCompare,
            (rs, c) if rs == c => SpecComparisonResult::EqualOK,
            _ => SpecComparisonResult::NotEqualPossibleBug,
        };

        println!("Comparison Result: {:?}", spec_comparison_result);
        spec_comparison_results.push(spec_comparison_result);
    }

    spec_comparison_results
}

fn find_pairs(tcx: rustc_middle::ty::TyCtxt<'_>, c_side_specs: Vec<FunctionErrorSpec>, rust_side_specs: Vec<WrapperFunction>) -> HashSet<(FunctionErrorSpec, WrapperFunction)> {
    
    let mut pairs = HashSet::new();
    
    for rust_side_spec in &rust_side_specs {
        // ! item_name can panic, replace with opt_item name if tthis becomes an actual problem
        let wrapped_function_name_sym = tcx.item_name(rust_side_spec.clone().wrapped_function_id); 
        let wrapped_function_name = wrapped_function_name_sym.as_str();
        println!("\nLooking for C Side spec for wrapped function {}", wrapped_function_name);

        for c_side_spec in &c_side_specs {
            if c_side_spec.func_name == wrapped_function_name {

                println!("Found Wrapping of {} in {}, adding this pair to Hasset for comparison. (duplicate output of pair possible at this stage)", wrapped_function_name, tcx.def_path_str(rust_side_spec.wrapper_function_id));
                pairs.insert((c_side_spec.clone(), rust_side_spec.clone()));
            }
        }
    }

    pairs
}

pub fn aggregate_and_print_comparison_statistics(spec_comparison_results: Vec<SpecComparisonResult>) {

    let mut total = 0;
    let mut equal_ok = 0;
    let mut not_equal_possible_bug = 0;
    let mut cannot_compare = 0;

    for spec_comparison_result in spec_comparison_results {
        total += 1;

        match spec_comparison_result {
            SpecComparisonResult::EqualOK => {
                equal_ok += 1;
            },
            SpecComparisonResult::NotEqualPossibleBug => {
                not_equal_possible_bug += 1;
            }
            SpecComparisonResult::CannotCompare => {
                cannot_compare += 1;
            }
        }

        println!("\n\nComparison Statistics:");
        println!("Total Comparisons: {}", total);
        println!("EqualOK: {}", equal_ok);
        println!("NotEqualPossibleBug: {}", not_equal_possible_bug);
        println!("CannotCompare: {}", cannot_compare);
    }
}