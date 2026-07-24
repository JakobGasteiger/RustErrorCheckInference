
use crate::utils::error_spec::{ErrorSpecPredicate, WrapperFunctionSpec};
use std::io::Write;

pub fn print_eesi_like_output(tcx: rustc_middle::ty::TyCtxt, rustside_analysis_results: &Vec<WrapperFunctionSpec>) {
    
    let path = std::env::current_dir().unwrap().into_string().unwrap() + "/results_eesi_format.txt"; // ! can panic
    let mut output_file = std::fs::File::create(path).unwrap(); // ! can panic 

    let mut prev_func_names: Vec<String> = Vec::new();

    for func in rustside_analysis_results {

        // ! item_name can panic, replace with opt_item name if this ever becomes an actual problem
        let wrapped_function_name_sym = tcx.item_name(func.clone().wrapped_function_id); 
        let wrapped_function_name = wrapped_function_name_sym.as_str();

        // a function may be wrapped multiple times; in this case skip, otherwise add name to previously outputted functions
        if prev_func_names.contains(&wrapped_function_name.to_string()) {
            continue;
        } else {
            prev_func_names.push(wrapped_function_name.to_string());
        }

        let predicate_string = match func.return_value_check {
            Some(ErrorSpecPredicate::All) => "top",
            Some(ErrorSpecPredicate::GrEqZero) => ">=0",
            Some(ErrorSpecPredicate::LesEqZero) => "<=0",
            Some(ErrorSpecPredicate::GreaterZero) => ">0",
            Some(ErrorSpecPredicate::LesserZero) => "<0",
            Some(ErrorSpecPredicate::EqualZero) => "==0",
            Some(ErrorSpecPredicate::NotEqZero) => "!=0",
            Some(ErrorSpecPredicate::Empty) => "bottom",
            _ => {continue;} // if no sensible output found, print nothing
        };
        let _ = write!(output_file, "{wrapped_function_name}: {wrapped_function_name} {predicate_string}\n");
    }
}