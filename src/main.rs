#![feature(rustc_private)]

extern crate rustc_abi;
extern crate rustc_ast;
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_span;

mod error_check_spec_generation;

use crate::error_check_spec_generation::driver::*;

fn main() {
    let mut args: Vec<String> = std::env::args().collect();

    // When used as RUSTC_WRAPPER, cargo passes the path to rustc as the
    // first argument. We need to skip it.
    // why exactly? claude says so and it works ¯\_(ツ)_/¯
    if args.len() > 1 && args[1].contains("rustc") {
        args.remove(1);
    }

    // cancel when we're actually bulding; we only want to run the analysis on cargo check
    let is_build = args.iter().any(|a| a.contains("link"));
    // if is_build {
    //     rustc_driver::run_compiler(&args, &mut rustc_driver::TimePassesCallbacks::default());
    //     return;
    // }

    // callback / after_analysis will hook in
    eprintln!("Wrapper is active");
    rustc_driver::run_compiler(&args, &mut ExternFuncCheckCallbacks);
}
