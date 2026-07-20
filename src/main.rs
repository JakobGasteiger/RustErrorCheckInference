#![feature(rustc_private)]

extern crate rustc_abi;
extern crate rustc_ast;
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_span;

mod error_check_spec_generation;
mod utils;
mod esss_parser;

use crate::{error_check_spec_generation::{driver::*, spec_generation::find_RV_checks, wrapper_func_finder::{find_external_functions, find_sys_crates, find_wrapper_functions}}, esss_parser::parser::parse_specs};


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

        let mut wrapper_functions  = find_wrapper_functions(tcx, &extern_function_ids);

        let mut other_statistics = OtherStatistics::new();

        for mut wrapper_function in &mut wrapper_functions {
            find_RV_checks(tcx, &mut wrapper_function, &mut other_statistics);
            //println!("{:?}", wrapper_function);
        }

        aggregate_and_print_error_check_statistics(&wrapper_functions);
        other_statistics.output();

        let c_side_specs = parse_specs();

        rustc_driver::Compilation::Continue
    }
}


fn main() {
    let mut args: Vec<String> = std::env::args().collect();

    // When used as RUSTC_WRAPPER, cargo passes the path to rustc as the
    // first argument. We need to skip it.
    // why exactly? claude says so and it works ¯\_(ツ)_/¯
    if args.len() > 1 && args[1].contains("rustc") {
        args.remove(1);
    }

    // * for reasons i struggle to understand, this check caused problems with some libs (curl specifically); removed, maybe bring back? not really important anyway
    // cancel when we're actually bulding; we only want to run the analysis on cargo check
    //let is_build = args.iter().any(|a| a.contains("link"));
    // if is_build {
    //     rustc_driver::run_compiler(&args, &mut rustc_driver::TimePassesCallbacks::default());
    //     return;
    // }

    // callback / after_analysis will hook in
    eprintln!("Wrapper is active");
    rustc_driver::run_compiler(&args, &mut ExternFuncCheckCallbacks);

}
