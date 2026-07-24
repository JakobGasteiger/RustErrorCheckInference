use crate::utils::error_spec::{ErrorSpecPredicate, WrapperFunctionSpec};
use std::io::Write;

pub fn print_ext_func_list_for_ai(tcx: rustc_middle::ty::TyCtxt, external_functions: &Vec<rustc_hir::def_id::DefId>) {
    
    let path = std::env::current_dir().unwrap().into_string().unwrap() + "/ext_func_list_for_ai.txt"; // ! can panic
    let mut output_file = std::fs::File::create(path).unwrap(); // ! can panic 

    let mut prev_func_names: Vec<String> = Vec::new();

    for func_id in external_functions {

        // ! item_name can panic, replace with opt_item name if this ever becomes an actual problem
        let ext_func_name_sym = tcx.item_name(func_id.clone()); 
        let ext_func_name = ext_func_name_sym.as_str();

        let _ = write!(output_file, "{ext_func_name}\n");
    }
}