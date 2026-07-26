
use std::collections::HashSet;

use crate::utils::error_spec::{ErrorSpecPredicate, FunctionErrorSpec, WrapperFunctionSpec};

#[derive(Debug, Clone)]
pub enum SpecComparisonResult {
    EqualOK,
    NotEqualPossibleBug,
    CannotCompare
}

pub fn compare_specs(tcx: rustc_middle::ty::TyCtxt<'_>, esss_specs: Option<Vec<FunctionErrorSpec>>, eesi_specs: Option<Vec<FunctionErrorSpec>>, ai_specs: Option<Vec<FunctionErrorSpec>>, rust_side_specs: Vec<WrapperFunctionSpec>) -> Vec<SpecComparisonResult> {

    println!("\n\nComparing C and rust Side Specs...");

    let correlated_c_side_specs = correlate_all_three(esss_specs, eesi_specs, ai_specs);

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

fn correlate_all_three(esss_specs: Option<Vec<FunctionErrorSpec>>, eesi_specs: Option<Vec<FunctionErrorSpec>>, ai_specs: Option<Vec<FunctionErrorSpec>>) -> Vec<FunctionErrorSpec> {

    println!("\n\nCorrelating ESSS, EESI, and AI specs");
    
    // Count how many sources we have
    let has_esss = esss_specs.is_some();
    let has_eesi = eesi_specs.is_some();
    let has_ai = ai_specs.is_some();
    
    let num_sources = has_esss as usize + has_eesi as usize + has_ai as usize;
    
    match num_sources {
        0 => {
            println!("We have no C Side Specs, returning empty set");
            Vec::new()
        }
        1 => {
            // Only one source present, return all its specs
            if has_esss {
                println!("We only have ESSS, returning that");
                esss_specs.unwrap_or_default()
            } else if has_eesi {
                println!("We only have EESI, returning that");
                eesi_specs.unwrap_or_default()
            } else {
                println!("We only have AI, returning that");
                ai_specs.unwrap_or_default()
            }
        }
        2 => {
            // Two sources present, correlate them
            if !has_ai {
                println!("No AI specs, using ESSS/EESI correlation");
                correlate_two(esss_specs, eesi_specs)
            } else if !has_eesi {
                println!("No EESI specs, using ESSS/AI correlation");
                correlate_two(esss_specs, ai_specs)
            } else {
                println!("No ESSS specs, using EESI/AI correlation");
                correlate_two(eesi_specs, ai_specs)
            }
        }
        3 => {
            // All three sources present, correlate all three
            println!("All three sources present, correlating ESSS, EESI, and AI");
            correlate_all_three_present(esss_specs.unwrap(), eesi_specs.unwrap(), ai_specs.unwrap())
        }
        _ => {
            println!("Unexpected number of sources, returning empty");
            Vec::new()
        }
    }
}

fn correlate_two(specs_a: Option<Vec<FunctionErrorSpec>>, specs_b: Option<Vec<FunctionErrorSpec>>) -> Vec<FunctionErrorSpec> {
    println!("\nCorrelating two spec sources");
    
    let specs_a = specs_a.unwrap_or_default();
    let specs_b = specs_b.unwrap_or_default();
    
    let mut correlated_specs: HashSet<FunctionErrorSpec> = HashSet::new();
    let mut total_common_functions: usize = 0;
    let mut total_matching: usize = 0;
    let mut total_not_matching: usize = 0;
    
    for spec_a in &specs_a {
        println!("\nLooking for matching spec for function {}", spec_a.func_name);
        
        for spec_b in &specs_b {
            if spec_b.func_name == spec_a.func_name {
                total_common_functions += 1;
                
                if spec_b.error_spec == spec_a.error_spec {
                    println!("They match ({:?})", spec_a.error_spec);
                    total_matching += 1;
                    correlated_specs.insert(spec_a.clone());
                } else {
                    println!("They don't match ({:?} vs {:?})", spec_a.error_spec, spec_b.error_spec);
                    total_not_matching += 1;
                }
            }
        }
    }
    
    println!("\nTwo-source Correlation Statistics:");
    println!("Total Functions in common: {}", total_common_functions);
    println!("Total Functions with matching specs: {}", total_matching);
    println!("Total Functions with non-matching specs: {}", total_not_matching);
    
    correlated_specs.into_iter().collect()
}

fn correlate_all_three_present(esss_specs: Vec<FunctionErrorSpec>, eesi_specs: Vec<FunctionErrorSpec>, ai_specs: Vec<FunctionErrorSpec>) -> Vec<FunctionErrorSpec> {
    println!("\nCorrelating all three spec sources");
    
    let mut correlated_specs: HashSet<FunctionErrorSpec> = HashSet::new();
    let mut total_common_functions: usize = 0;
    let mut total_matching: usize = 0;
    let mut total_not_matching: usize = 0;
    
    for esss_spec in &esss_specs {
        println!("\nLooking for EESI and AI spec for ESSS spec of function {}", esss_spec.func_name);
        
        let mut eesi_match: Option<&FunctionErrorSpec> = None;
        let mut ai_match: Option<&FunctionErrorSpec> = None;
        
        // find matching EESI spec
        for eesi_spec in &eesi_specs {
            if eesi_spec.func_name == esss_spec.func_name {
                eesi_match = Some(eesi_spec);
                break;
            }
        }
        
        // find matching AI spec
        for ai_spec in &ai_specs {
            if ai_spec.func_name == esss_spec.func_name {
                ai_match = Some(ai_spec);
                break;
            }
        }
        
        match (eesi_match, ai_match) {
            (Some(eesi_spec), Some(ai_spec)) => {
                total_common_functions += 1;
                
                println!("Found ESSS/EESI/AI triplet for function {}, testing if all specs match", esss_spec.func_name);
                if eesi_spec.error_spec == esss_spec.error_spec && ai_spec.error_spec == esss_spec.error_spec {
                    println!("All three match ({:?})", esss_spec.error_spec);
                    total_matching += 1;
                    correlated_specs.insert(esss_spec.clone());
                } else {
                    println!("They don't all match (ESSS: {:?}, EESI: {:?}, AI: {:?})", esss_spec.error_spec, eesi_spec.error_spec, ai_spec.error_spec);
                    total_not_matching += 1;
                }
            }
            _ => {
                println!("Missing EESI or AI spec for function {}", esss_spec.func_name);
            }
        }
    }
    
    println!("\nThree-source Correlation Statistics:");
    println!("Total Functions in common across all three: {}", total_common_functions);
    println!("Total Functions with matching specs: {}", total_matching);
    println!("Total Functions with non-matching specs: {}", total_not_matching);
    
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