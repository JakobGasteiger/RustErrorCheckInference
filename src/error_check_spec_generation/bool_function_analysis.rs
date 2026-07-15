// * contains some functions related to analyzing sub-error-check-function/methods which return bool: sorted into own file for organization

use crate::{error_check_spec_generation::{
    driver::OtherStatistics,
    spec_generation::{
        RVCheckFinder, ReturnType
    },
    wrapper_func_finder::WrapperFunction,
}, utils::error_spec::ErrorSpec};
use crate::rustc_hir::intravisit::Visitor;

impl<'tcx> RVCheckFinder<'tcx> {
    // recursively called when a rv ist passed into another function to be checked
    // TODO unite with find_RV_checks() or analyze_function or analyze_method or sth ?
    pub fn analyze_bool_sub_error_check_function(
        &mut self,
        tcx: rustc_middle::ty::TyCtxt<'tcx>,
        error_check_function_id: rustc_hir::def_id::DefId,
        arg_index: usize,
    ) -> Option<ErrorSpec> {
        println!(
            "\nFor Sub Error Check Function {}",
            tcx.def_path_str(error_check_function_id)
        );

        // only works for local functions (no HIR body for external crates)
        let Some(local_def_id) = error_check_function_id.as_local() else {
            println!("Not local!");
            self.other_statistics.not_local_functions += 1;
            return Some(ErrorSpec::Indeterminate);
        };
        // abort if function has no body
        let Some(body) = tcx.hir_maybe_body_owned_by(local_def_id) else {
            println!("No body!");
            return Some(ErrorSpec::Indeterminate);
        };

        // get the parameter at arg_index
        let Some(param) = body.params.get(arg_index) else {
            return Some(ErrorSpec::Indeterminate);
        };

        // that parameter's binding hir id becomes the new tracked identity
        if let rustc_hir::PatKind::Binding(_, param_hir_id, _, _) = param.pat.kind {
            let new_wrapper_function = WrapperFunction {
                wrapper_function_id: error_check_function_id,
                wrapped_function_id: self.wrapper_function.wrapped_function_id,
                return_value_check: None,
            };

            let mut new_visited_function_list = self.already_visited_functions.clone();
            new_visited_function_list.push(error_check_function_id);

            let mut sub_finder = RVCheckFinder {
                tcx: self.tcx,
                wrapper_function: new_wrapper_function,
                wrapped_function_value_holder: Some(param_hir_id),
                already_visited_functions: new_visited_function_list,
                mode: ReturnType::Bool,
                other_statistics: OtherStatistics::new(),
            };
            sub_finder.visit_body(body);
            self.other_statistics += sub_finder.other_statistics.clone();
            return sub_finder.wrapper_function.return_value_check;
        }

        return None;
    }

    pub fn analyze_bool_function(
        self: &mut Self,
        func: &rustc_hir::Expr,
    ) -> Option<ErrorSpec> {
        println!("Boolean return type: not yet supported");
        self.other_statistics.bool_functions_not_yet_supported += 1;

        if let Some(function_def_id) = self.get_function_def_id(func) {
            println!(
                "RV passed as arg to bool-returning function {} : recursing",
                self.tcx.def_path_str(function_def_id)
            );

            // if we find a recursion loop, we terminate analysis for this wrapper
            if self.already_visited_functions.contains(&function_def_id) {
                println!("Recursion loop found, aborting!");
                return Some(ErrorSpec::Indeterminate);
            }
        }

        // TODO temp, actually implement function
        // TODO possibly not really necessary? se how often actually used
        Some(ErrorSpec::Indeterminate)
    }

    pub fn analyze_bool_method(
        self: &mut Self,
        method_expr: &rustc_hir::Expr,
    ) -> Option<ErrorSpec> {
        println!("Boolean return type: not yet fully supported");

        if let Some(method_def_id) = self.get_method_def_id(method_expr)
            && let rustc_hir::ExprKind::MethodCall(..) = method_expr.kind
        {
            println!(
                "RV passed to bool-returning method {} : recursing",
                self.tcx.def_path_str(method_def_id)
            );

            // if we find a recursion loop, we terminate analysis for this wrapper
            if self.already_visited_functions.contains(&method_def_id) {
                println!("Recursion loop found, aborting!");
                return Some(ErrorSpec::Indeterminate);
            }

            let method_name = self.tcx.def_path_str(method_def_id);
            println!(
                "Checking if boolean function {} is hardcoded...",
                method_name
            );

            // hardcoded support for some common boolean methods from std
            if method_name.ends_with("is_null") {
                self.other_statistics.hardcoded_bool_methods_analyzed += 1;
                println!("... yes, it is!");
                return Some(ErrorSpec::EqualZero);
            } else if method_name.ends_with("is_negative") {
                self.other_statistics.hardcoded_bool_methods_analyzed += 1;
                println!("... yes, it is!");
                return Some(ErrorSpec::LesserZero);
            } else if method_name.ends_with("is_positive") {
                self.other_statistics.hardcoded_bool_methods_analyzed += 1;
                println!("... yes, it is!");
                return Some(ErrorSpec::GreaterZero);
            }

            println!("... no, it isn't :(")

            // // hardcoded support for some common boolean methods from std
            // better to just use the more versatile ends_with option above
            // match method_name.as_str() {
            //     "i8::is_negative" | "i16::is_negative" |  "i32::is_negative"  | "i64::is_negative" | "i28::is_negative" | "isize::is_negative" => {
            //         self.other_statistics.hardcoded_bool_methods_analyzed += 1;
            //         return Some(ReturnValueCheck::LesserZero);
            //     },
            //     "i8::is_positive" | "i16::is_positive" |  "i32::is_positive"  | "i64::is_positive" | "i28::is_positive" | "isize::is_positive" => {
            //         self.other_statistics.hardcoded_bool_methods_analyzed += 1;
            //         return Some(ReturnValueCheck::GreaterZero);
            //     },
            //     "std::ptr::is_null" | "pointer::is_null"  => {
            //         self.other_statistics.hardcoded_bool_methods_analyzed += 1;
            //         return Some(ReturnValueCheck::EqualZero);
            //     }
            //     _ => {}
            // }
        }

        // TODO temp, actually implement function ?
        // TODO probably not really necessary? see how often actually used

        self.other_statistics.bool_methods_not_yet_supported += 1;
        Some(ErrorSpec::Indeterminate)
    }
}
