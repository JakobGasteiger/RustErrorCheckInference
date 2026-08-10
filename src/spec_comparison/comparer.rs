
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
            // Two sources present, correlate them with new lenient logic
            if !has_ai {
                println!("No AI specs, using ESSS/EESI correlation");
                correlate_two_lenient(esss_specs, eesi_specs)
            } else if !has_eesi {
                println!("No EESI specs, using ESSS/AI correlation");
                correlate_two_lenient(esss_specs, ai_specs)
            } else {
                println!("No ESSS specs, using EESI/AI correlation");
                correlate_two_lenient(eesi_specs, ai_specs)
            }
        }
        3 => {
            // All three sources present, correlate all three with lenient logic
            println!("All three sources present, correlating ESSS, EESI, and AI");
            correlate_all_three_lenient(esss_specs.unwrap(), eesi_specs.unwrap(), ai_specs.unwrap())
        }
        _ => {
            println!("Unexpected number of sources, returning empty");
            Vec::new()
        }
    }
}

/// Collects all specs from a source into a HashMap for easy lookup
fn specs_to_map(specs: &[FunctionErrorSpec]) -> std::collections::HashMap<String, ErrorSpecPredicate> {
    let mut map = std::collections::HashMap::new();
    for spec in specs {
        map.insert(spec.func_name.clone(), spec.error_spec);
    }
    map
}

/// Resolves a spec from multiple sources. Returns Some if no two sources disagree,
/// None if there is a disagreement between any two non-Indeterminate sources.
/// Priority: prefers non-Indeterminate specs, with AI as tiebreaker when all are Indeterminate.
fn resolve_spec(
    esss_spec: Option<ErrorSpecPredicate>,
    eesi_spec: Option<ErrorSpecPredicate>,
    ai_spec: Option<ErrorSpecPredicate>,
    func_name: &str,
) -> Option<ErrorSpecPredicate> {
    // Collect all non-Indeterminate specs
    let non_indeterminate: Vec<ErrorSpecPredicate> = vec![esss_spec, eesi_spec, ai_spec]
        .into_iter()
        .flatten()
        .filter(|s| !matches!(s, ErrorSpecPredicate::Indeterminate))
        .collect();
    
    // If we have multiple non-Indeterminate specs, check if they all agree
    if non_indeterminate.len() > 1 {
        let first = non_indeterminate[0];
        if non_indeterminate.iter().all(|s| *s == first) {
            // All non-Indeterminate specs agree
            println!("  Non-Indeterminate specs agree on {:?} for {}", first, func_name);
            return Some(first);
        } else {
            // Disagreement between non-Indeterminate specs
            println!("  DISAGREEMENT between non-Indeterminate specs for {}: {:?}", func_name, non_indeterminate);
            return None;
        }
    }
    
    // If we have exactly one non-Indeterminate spec, use it
    if non_indeterminate.len() == 1 {
        println!("  Using single non-Indeterminate spec {:?} for {}", non_indeterminate[0], func_name);
        return Some(non_indeterminate[0]);
    }
    
    // All are Indeterminate or missing - prefer AI, then EESI, then ESSS
    if let Some(ai) = ai_spec {
        println!("  All Indeterminate/missing, using AI spec {:?} for {}", ai, func_name);
        return Some(ai);
    }
    if let Some(eesi) = eesi_spec {
        println!("  All Indeterminate/missing, using EESI spec {:?} for {}", eesi, func_name);
        return Some(eesi);
    }
    if let Some(esss) = esss_spec {
        println!("  All Indeterminate/missing, using ESSS spec {:?} for {}", esss, func_name);
        return Some(esss);
    }
    
    // No specs at all
    println!("  No specs available for {}", func_name);
    None
}

fn correlate_two_lenient(
    specs_a: Option<Vec<FunctionErrorSpec>>,
    specs_b: Option<Vec<FunctionErrorSpec>>,
) -> Vec<FunctionErrorSpec> {
    println!("\nCorrelating two spec sources with lenient logic");
    
    let specs_a = specs_a.unwrap_or_default();
    let specs_b = specs_b.unwrap_or_default();
    
    let map_a = specs_to_map(&specs_a);
    let map_b = specs_to_map(&specs_b);
    
    let mut correlated_specs: HashSet<FunctionErrorSpec> = HashSet::new();
    let mut total_functions: usize = 0;
    let mut included: usize = 0;
    let mut excluded: usize = 0;
    
    // Collect all unique function names from both sources
    let mut all_funcs: HashSet<String> = HashSet::new();
    for spec in &specs_a {
        all_funcs.insert(spec.func_name.clone());
    }
    for spec in &specs_b {
        all_funcs.insert(spec.func_name.clone());
    }
    
    for func_name in &all_funcs {
        total_functions += 1;
        println!("\nProcessing function {}", func_name);
        
        let spec_a = map_a.get(func_name).cloned();
        let spec_b = map_b.get(func_name).cloned();
        
        if let Some(resolved) = resolve_spec(spec_a, spec_b, None, func_name) {
            correlated_specs.insert(FunctionErrorSpec {
                func_name: func_name.clone(),
                error_spec: resolved,
            });
            included += 1;
        } else {
            excluded += 1;
        }
    }
    
    println!("\nTwo-source Lenient Correlation Statistics:");
    println!("Total Functions: {}", total_functions);
    println!("Included (no disagreement): {}", included);
    println!("Excluded (disagreement): {}", excluded);
    
    correlated_specs.into_iter().collect()
}

fn correlate_all_three_lenient(
    esss_specs: Vec<FunctionErrorSpec>,
    eesi_specs: Vec<FunctionErrorSpec>,
    ai_specs: Vec<FunctionErrorSpec>,
) -> Vec<FunctionErrorSpec> {
    println!("\nCorrelating all three spec sources with lenient logic");
    
    let map_esss = specs_to_map(&esss_specs);
    let map_eesi = specs_to_map(&eesi_specs);
    let map_ai = specs_to_map(&ai_specs);
    
    let mut correlated_specs: HashSet<FunctionErrorSpec> = HashSet::new();
    let mut total_functions: usize = 0;
    let mut included: usize = 0;
    let mut excluded: usize = 0;
    
    // Collect all unique function names from all sources
    let mut all_funcs: HashSet<String> = HashSet::new();
    for spec in &esss_specs {
        all_funcs.insert(spec.func_name.clone());
    }
    for spec in &eesi_specs {
        all_funcs.insert(spec.func_name.clone());
    }
    for spec in &ai_specs {
        all_funcs.insert(spec.func_name.clone());
    }
    
    for func_name in &all_funcs {
        total_functions += 1;
        println!("\nProcessing function {}", func_name);
        
        let spec_esss = map_esss.get(func_name).cloned();
        let spec_eesi = map_eesi.get(func_name).cloned();
        let spec_ai = map_ai.get(func_name).cloned();
        
        if let Some(resolved) = resolve_spec(spec_esss, spec_eesi, spec_ai, func_name) {
            correlated_specs.insert(FunctionErrorSpec {
                func_name: func_name.clone(),
                error_spec: resolved,
            });
            included += 1;
        } else {
            excluded += 1;
        }
    }
    
    println!("\nThree-source Lenient Correlation Statistics:");
    println!("Total Functions: {}", total_functions);
    println!("Included (no disagreement): {}", included);
    println!("Excluded (disagreement): {}", excluded);
    
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