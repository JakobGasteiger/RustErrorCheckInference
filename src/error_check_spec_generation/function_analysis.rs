
// * contains some functions related to analyzing sub-error-check-function/methods: soreted into own file for organization

use crate::error_check_spec_generation::{spec_generation::{RVCheckFinder, ReturnType, ReturnValueCheck}, wrapper_func_finder::WrapperFunction};
use crate::rustc_hir::intravisit::Visitor;

impl<'tcx> RVCheckFinder<'tcx> {
    // recursively called when a rv ist passed into another function to be checked
    // TODO unite with find_RV_checks() or analyze_function or analyze_method or sth ?
    pub fn analyze_sub_error_check_function(
        &mut self,
        tcx: rustc_middle::ty::TyCtxt<'tcx>,
        error_check_function_id: rustc_hir::def_id::DefId,
        arg_index: usize,
    ) -> ReturnValueCheck {
        println!(
            "\nFor Sub Error Check Function {}",
            tcx.def_path_str(error_check_function_id)
        );

        // only works for local functions (no HIR body for external crates)
        let Some(local_def_id) = error_check_function_id.as_local() else {
            println!("Not local!");
            return ReturnValueCheck::Indeterminate;
        };
        // abort if function has no body
        let Some(body) = tcx.hir_maybe_body_owned_by(local_def_id) else {
            println!("No body!");
            return ReturnValueCheck::Indeterminate;
        };

        // get the parameter at arg_index
        let Some(param) = body.params.get(arg_index) else {
            return ReturnValueCheck::Empty;
        };

        // that parameter's binding hir id becomes the new tracked identity
        if let rustc_hir::PatKind::Binding(_, param_hir_id, _, _) = param.pat.kind {
            let new_wrapper_function = WrapperFunction {
                wrapper_function_id: error_check_function_id,
                wrapped_function_id: self.wrapper_function.wrapped_function_id,
                return_value_check: ReturnValueCheck::Empty,
            };

            let mut new_visited_function_list = self.already_visited_functions.clone();
            new_visited_function_list.push(error_check_function_id);

            let mut sub_finder = RVCheckFinder{
                tcx: self.tcx,
                wrapper_function: new_wrapper_function,
                wrapped_function_value_holder: Some(param_hir_id),
                already_visited_functions: new_visited_function_list,
            };
            sub_finder.visit_body(body);
            return sub_finder.wrapper_function.return_value_check;
        }

        return ReturnValueCheck::Empty;
    }

    
    fn get_function_or_method_return_type(self: &Self, def_id: &rustc_hir::def_id::DefId) -> ReturnType {
        
        let return_type = self.tcx.fn_sig(*def_id).skip_binder().output().skip_binder();

        match return_type.kind() {
            rustc_middle::ty::TyKind::Adt(adt_def, _args) => {
                let type_name = self.tcx.def_path_str(adt_def.did());
                if type_name == "core::result::Result" || type_name == "std::result::Result"
                || type_name == "core::result::Option" || type_name == "std::result::Option" {
                    return ReturnType::ResultOrOption;
                }
            },

            rustc_middle::ty::TyKind::Bool => return ReturnType::Bool,

            _ => return ReturnType::Other
        }

        ReturnType::Other
    }

    // TODO: harmonize with get_method_def_id: pass call expr, not the function itself    
    pub fn get_function_def_id(self: &Self, func: &rustc_hir::Expr) -> Option<rustc_hir::def_id::DefId> {
        if let rustc_hir::ExprKind::Path(qpath) = &func.kind {
            let owner = self.wrapper_function.wrapper_function_id.expect_local();
            let typeck_results = self.tcx.typeck(owner);
            let res = typeck_results.qpath_res(qpath, func.hir_id);

            if let rustc_hir::def::Res::Def(_, callee_def_id) = res {
                return Some(callee_def_id);
            }
        }
        None
    }
    

    pub fn analyze_function_call(self: &mut Self, func: &rustc_hir::Expr, args: &[rustc_hir::Expr], expr_being_checked: &rustc_hir::Expr) -> Option<ReturnValueCheck> {

        if let Some(function_def_id) = &self.get_function_def_id(func) {

            // abort if we a re analyzing a method that doesn'T return result or option
            // TODO handle case bool
            if self.get_function_or_method_return_type(function_def_id) != ReturnType::ResultOrOption { 
                println!("Does not return Result or Option");
                return None; 
            }

            // which argument number is our RV when being passed in?
            if let Some(arg_index) = args.iter().position(|a| a.hir_id == expr_being_checked.hir_id) {

                println!(
                    "RV passed as arg {} to {} : recursing",
                    arg_index,
                    self.tcx.def_path_str(*function_def_id)
                );
                
                // if we find a recursion loop, we terminate analysis for this wrapper
                if self.already_visited_functions.contains(&function_def_id) {
                    println!("Recursion loop found, aborting!");
                    return Some(ReturnValueCheck::Indeterminate);
                }

                return Some(self.analyze_sub_error_check_function(
                    self.tcx,
                    function_def_id.clone(),
                    arg_index,
                ));
            }
        }

        None
    }

    pub fn get_method_def_id(self: &Self, method_expr: &rustc_hir::Expr) -> Option<rustc_hir::def_id::DefId> {
        let owner = self.wrapper_function.wrapper_function_id.expect_local();
        let typeck_results = self.tcx.typeck(owner);

        if let Some(def_id) = typeck_results.type_dependent_def_id(method_expr.hir_id) {
            if def_id.is_local() {
                return Some(def_id);
            }
        }
        None
    }

    pub fn analyze_method_call(self: &mut Self, method: &rustc_hir::Expr, expr_being_checked: &rustc_hir::Expr) -> Option<ReturnValueCheck> {

        if let Some(method_def_id) = self.get_method_def_id(method) && let rustc_hir::ExprKind::MethodCall(method, receiver, _args, ..) = method.kind {

            // abort if we a re analyzing a method that doesn'T return result or option
            // TODO handle case bool
            if self.get_function_or_method_return_type(&method_def_id) != ReturnType::ResultOrOption { 
                println!("Does not return Result or Option");
                return None; 
            }

            println!(
                "RV passed as to method {} : recursing",
                self.tcx.def_path_str(method_def_id)
            );
            
            // if we find a recursion loop, we terminate analysis for this wrapper
            if self.already_visited_functions.contains(&method_def_id) {
                println!("Recursion loop found, aborting!");
                return Some(ReturnValueCheck::Indeterminate);
            }

            return Some(self.analyze_sub_error_check_function(
                self.tcx,
                method_def_id.clone(),
                0, // self, which is the argument we care about, is always the first argument
            ));
        }

        // TODO temp, actually implment function
        None
    }
}