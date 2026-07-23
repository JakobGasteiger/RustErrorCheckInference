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
mod parser;
mod spec_comparison;
mod eesi_like_output;

use crate::{eesi_like_output::eesi_like_output::print_eesi_like_output, error_check_spec_generation::{driver::*, spec_generation::find_RV_checks, wrapper_func_finder::{find_external_functions, find_sys_crates, find_wrapper_functions}}, parser::{eesi_parser::{parse_eesi, print_eesi_statistics}, esss_parser::{parse_esss, print_esss_statistics}}, spec_comparison::comparer::{compare_specs, print_comparison_statistics}};


pub struct Callbacks;

impl rustc_driver::Callbacks for Callbacks {
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

        let mut wrapper_function_specs  = find_wrapper_functions(tcx, &extern_function_ids);
        let mut other_statistics = OtherRustAnalysisStatistics::new();
        for mut wrapper_function_spec in &mut wrapper_function_specs {
            find_RV_checks(tcx, &mut wrapper_function_spec, &mut other_statistics);
            //println!("{:?}", wrapper_function);
        }
        print_error_check_statistics(&wrapper_function_specs);
        other_statistics.output();

        let esss_specs = parse_esss();
        print_esss_statistics(&esss_specs);

        let eesi_specs = parse_eesi();
        print_eesi_statistics(&eesi_specs);

        let spec_comparison_results = compare_specs(tcx, esss_specs, eesi_specs, wrapper_function_specs.clone());
        print_comparison_statistics(spec_comparison_results);

        // do this last to minimize damge if function panics
        print_eesi_like_output(tcx, &wrapper_function_specs);

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
    rustc_driver::run_compiler(&args, &mut Callbacks);

}
