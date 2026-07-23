
use std::{collections::HashSet, vec};

use crate::{spec_comparison, utils::error_spec::{ErrorSpecPredicate, FunctionErrorSpec, WrapperFunctionSpec}};

#[derive(Debug, Clone)]
pub enum SpecComparisonResult {
    EqualOK,
    NotEqualPossibleBug,
    CannotCompare
}

pub fn compare_specs(tcx: rustc_middle::ty::TyCtxt<'_>, esss_specs: Option<Vec<FunctionErrorSpec>>, eesi_specs: Option<Vec<FunctionErrorSpec>>, rust_side_specs: Vec<WrapperFunctionSpec>) -> Vec<SpecComparisonResult> {

    println!("\n\nComparing C and rust Side Specs...");

    let correlated_c_side_specs = correlate_esss_eesi(esss_specs, eesi_specs);

    let mut spec_comparison_results = Vec::new();

    for (c_side_spec, rust_side_spec) in find_pairs(tcx, correlated_c_side_specs, rust_side_specs) {

        // ! item_name can panic, replace with opt_item name if this ever becomes an actual problem
        let wrapped_function_name_sym = tcx.item_name(rust_side_spec.clone().wrapped_function_id); 
        let wrapped_function_name = wrapped_function_name_sym.as_str();

        println!("\nComparison for Wrapping of {} in {}...", wrapped_function_name, tcx.def_path_str(rust_side_spec.wrapper_function_id));

        // if the retvalcheck was still none, we consider it indeterminate
        let rust_side_check = rust_side_spec.return_value_check.unwrap_or(ErrorSpecPredicate::Indeterminate);
        println!("Rust Side: {:?}", rust_side_check);

        let c_side_check = c_side_spec.error_spec;
        println!("C Side: {:?}", c_side_check);

        let spec_comparison_result = match (rust_side_check, c_side_check) {
            (ErrorSpecPredicate::Indeterminate, _) | (_, ErrorSpecPredicate::Indeterminate) => SpecComparisonResult::CannotCompare,
            (rs, c) if rs == c => SpecComparisonResult::EqualOK,
            _ => SpecComparisonResult::NotEqualPossibleBug,
        };

        println!("Comparison Result: {:?}", spec_comparison_result);
        spec_comparison_results.push(spec_comparison_result);
    }

    spec_comparison_results
}

fn correlate_esss_eesi(esss_specs: Option<Vec<FunctionErrorSpec>>, eesi_specs: Option<Vec<FunctionErrorSpec>>) -> Vec<FunctionErrorSpec> {

    println!("\n\nCorrelating ESSS and EESI specs");
    
    // if we have no specs at all, our correlated specs should be empty
    if esss_specs.is_none() && eesi_specs.is_none() {
        println!("We have no C Side Specs, returning empty set");
        return Vec::new();
    }

    // else if we only have, esss, we return just that
    if let Some(esss_specs) = &esss_specs
    && eesi_specs.clone().is_none() {
        println!("We only have ESSS, returning that");
        return esss_specs.clone();
    }

    // else, if we only have eesi, we return just that
    if let Some(eesi_specs) = &eesi_specs 
    && esss_specs.clone().is_none() {
        println!("We only have EESI, returning that");
        return eesi_specs.clone();
    }

    // we can now safely unwrap both
    let esss_specs = esss_specs.unwrap();
    let eesi_specs = eesi_specs.unwrap();

    let mut correlated_specs: HashSet<FunctionErrorSpec> = HashSet::new();

    let mut total_common_functions: usize = 0;
    let mut total_matching: usize = 0;
    let mut total_not_matching: usize = 0;

    for esss_spec in &esss_specs { 
        println!("\nLooking for EESI spec for ESSS spec of function {}", esss_spec.func_name);

        for eesi_spec in &eesi_specs {

            if eesi_spec.func_name == esss_spec.func_name {

                total_common_functions += 1;

                println!("Found ESSS/EESI pair for function {}, testing if specs match", esss_spec.func_name);
                if eesi_spec.error_spec == esss_spec.error_spec {
                    println!("They match ({:?})", esss_spec.error_spec);
                    total_matching += 1;
                    correlated_specs.insert(esss_spec.clone());
                } else {
                    println!("They don't match (ESSS: {:?}, EESI: {:?})", esss_spec.error_spec, eesi_spec.error_spec);
                    total_not_matching += 1;
                }
            }
        }
    }

    println!("\nSpec Correlation Statistics:");
    println!("Total Functions in common between ESSS and EESI: {total_common_functions}");
    println!("Total Functions with matching specs: {total_matching}");
    println!("Total Functions with non-matching specs: {total_not_matching}");

    correlated_specs.into_iter().collect()
}

fn find_pairs(tcx: rustc_middle::ty::TyCtxt<'_>, c_side_specs: Vec<FunctionErrorSpec>, rust_side_specs: Vec<WrapperFunctionSpec>) -> HashSet<(FunctionErrorSpec, WrapperFunctionSpec)> {

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

pub fn print_comparison_statistics(spec_comparison_results: Vec<SpecComparisonResult>) {

    let mut total: usize = 0;
    let mut equal_ok: usize = 0;
    let mut not_equal_possible_bug: usize = 0;
    let mut cannot_compare: usize = 0;


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
    }
    
    println!("\n\nComparison Statistics:");
    println!("Total Comparisons: {}", total);
    println!("EqualOK: {}", equal_ok);
    println!("NotEqualPossibleBug: {}", not_equal_possible_bug);
    println!("CannotCompare: {}", cannot_compare);
}