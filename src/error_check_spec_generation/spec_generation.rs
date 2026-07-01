// * responsible for generating error check specifications for wrapper functions

use crate::error_check_spec_generation::{
    spec_generation::ReturnValueCheck::*, wrapper_func_finder::WrapperFunction,
};
use crate::rustc_hir::intravisit::Visitor;

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum ReturnValueCheck {
    Empty,
    LesserZero,
    GreaterZero,
    NotEqZero,
    LesEqZero,
    GrEqZero,
    EqualZero,
    All, // should never be used, since no function should always return an error
    Indeterminate,
    IndeterminateNotLocal, // we cannot analyze external functions, so we cannot determine the check
}

impl ReturnValueCheck {
    fn opposite(self) -> ReturnValueCheck {
        match self {
            Empty => All,
            LesserZero => GrEqZero,
            GreaterZero => LesEqZero,
            NotEqZero => EqualZero,
            LesEqZero => GreaterZero,
            GrEqZero => LesserZero,
            EqualZero => NotEqZero,
            All => Empty,
            Indeterminate => Indeterminate,
            IndeterminateNotLocal => IndeterminateNotLocal,
        }
    }

    fn parse_from_bin_op(
        bin_op: &rustc_hir::BinOp,
        comparand: &rustc_hir::Expr,
    ) -> Option<ReturnValueCheck> {
        // is our comparand a literal?
        if let rustc_hir::ExprKind::Lit(lit) = comparand.kind {
            // an int literal?
            if let rustc_ast::LitKind::Int(val, _) = lit.node {
                // is it 0?
                if val.get() != 0 {
                    // if not, abort, we only support predicates relative to zero
                    // TODO change this limitation?
                    return Some(Self::Indeterminate);
                }
            }
        }

        // if coparand is 0, parse
        match bin_op.node {
            rustc_hir::BinOpKind::Eq => Some(Self::EqualZero),
            rustc_hir::BinOpKind::Lt => Some(Self::LesserZero),
            rustc_hir::BinOpKind::Le => Some(Self::LesEqZero),
            rustc_hir::BinOpKind::Ne => Some(Self::NotEqZero),
            rustc_hir::BinOpKind::Ge => Some(Self::GrEqZero),
            rustc_hir::BinOpKind::Gt => Some(Self::GreaterZero),
            _ => None,
        }
    }
}

#[derive(PartialEq, Eq)]
pub enum ResultOrOptionVariant {
    Ok,
    Err,
    Some,
    None,
}

#[derive(PartialEq, Eq)]
pub enum ReturnType {
    ResultOrOption,
    Bool,
    Other,
}

// finds the checks on return values (=RV)
pub struct RVCheckFinder<'tcx> {
    pub tcx: rustc_middle::ty::TyCtxt<'tcx>,
    pub wrapper_function: WrapperFunction,
    // the current holder of the return value of the external function;
    // initially the extern call expression's own HirId
    // after "let result = call()"": result's binding HirId
    // after "let result2 = result": result2's binding HirId
    // etc
    pub wrapped_function_value_holder: Option<rustc_hir::HirId>,

    // functions already visited while going through sub-error-check function: for fix point analysis; if there is a loop, abort to guarantee termination
    pub already_visited_functions: Vec<rustc_hir::def_id::DefId>,
}

impl<'tcx> rustc_hir::intravisit::Visitor<'tcx> for RVCheckFinder<'tcx> {
    // as we go through the expressions of the body, for each expr
    // we check if it is the current holder of the return value, and if so, what happens to it ( via ehck_use_site() )
    fn visit_expr(&mut self, expr: &'tcx rustc_hir::Expr<'tcx>) {
        // if we have already found a check, we do not look further
        if !matches!(
            self.wrapper_function.return_value_check,
            ReturnValueCheck::Empty
        ) {
            return;
        }

        let owner = self.wrapper_function.wrapper_function_id.expect_local();
        let typeck_results = self.tcx.typeck(owner);

        // does the current expr match what were tracking?
        let matches_tracked: bool = match (&expr.kind, self.wrapped_function_value_holder) {
            // not tracking yet: is this expr a call to the wrapped external func?
            (rustc_hir::ExprKind::Call(func, _), None) => {
                if let rustc_hir::ExprKind::Path(qpath) = &func.kind {
                    let res = typeck_results.qpath_res(qpath, func.hir_id);
                    matches!(res, rustc_hir::def::Res::Def(_, def_id) if def_id == self.wrapper_function.wrapped_function_id)
                } else {
                    false
                }
            }
            // already tracking: is this expr a Path to the current holder?
            (rustc_hir::ExprKind::Path(qpath), Some(holder)) => {
                let res = typeck_results.qpath_res(qpath, expr.hir_id);
                matches!(res, rustc_hir::def::Res::Local(hir_id) if hir_id == holder)
            }
            //otherwise
            _ => false,
        };

        if matches_tracked {
            if self.wrapped_function_value_holder.is_none() {
                self.wrapped_function_value_holder = Some(expr.hir_id);
            }
            println!("Checking use site...");
            let rv_check = self.check_use_site(expr);
            if let Some(rv_check) = rv_check {
                self.wrapper_function.return_value_check = rv_check;
                //println!("{:?}", self.wrapper_function);
            }
        }

        //println!("Continuing walk...");
        rustc_hir::intravisit::walk_expr(self, expr);
    }
}

impl<'tcx> RVCheckFinder<'tcx> {
    fn check_use_site(
        &mut self,
        expr_being_checked: &'tcx rustc_hir::Expr<'tcx>,
    ) -> Option<ReturnValueCheck> {
        let parent = self
            .tcx
            .hir_parent_iter(expr_being_checked.hir_id)
            .next()
            .map(|(_, node)| node);

        // TODO support for rv borrowing?
        // TODO see pattern from apppend() in Libgit/src/repo.rs::976 : support this?

        match parent {
            // on sth like let result = <tracked expr>: move holder to result's binding HirId
            Some(rustc_hir::Node::LetStmt(local)) => {
                if let rustc_hir::PatKind::Binding(_, hir_id, ident, _) = local.pat.kind {
                    println!("RV identity moves to '{}'", ident.name);
                    self.wrapped_function_value_holder = Some(hir_id);
                }
            }

            Some(rustc_hir::Node::Expr(parent_expr))
                if let rustc_hir::ExprKind::Match(_matchee, arms, _) = parent_expr.kind =>
            {
                println!("RV checked via match at {:?}", expr_being_checked.span);

                // only support 2-armed match stmts: one Err, one Ok arm
                // TODO change this?
                if arms.len() != 2 {
                    println!("Arm count !=2");
                    return None;
                }

                println!("Arm count ==2");

                return self.analyze_match_stmt(arms);
            }

            // TODO support first comparand, comparands other than zero ?
            // TODO support check in else if
            Some(rustc_hir::Node::Expr(parent_expr))
                if let rustc_hir::ExprKind::Binary(bin_op, _ex1, ex2) = parent_expr.kind =>
            {
                let rv_check = ReturnValueCheck::parse_from_bin_op(&bin_op, &ex2)?;

                // is this part of an if stmt?
                if let Some((_, rustc_hir::Node::Expr(expr))) =
                    self.tcx.hir_parent_iter(parent_expr.hir_id).next()
                    && let rustc_hir::ExprKind::If(cond, then_block, _else_block) = expr.kind
                {
                    // cond should always be our binary expression: confirm this, abort if not
                    if cond.hir_id != parent_expr.hir_id {
                        return None;
                    }

                    println!("RV checked via If comparison at {:?}", expr.span);
                    println!("Binary Operation: {:?}", bin_op.node);

                    return self.analyze_if_stmt(rv_check, then_block);
                }
            }

            Some(rustc_hir::Node::Expr(parent_expr))
                if let rustc_hir::ExprKind::MethodCall(method, ..) = parent_expr.kind =>
            {
                println!(
                    "RV passed to method '{}' at {:?}",
                    method.ident, expr_being_checked.span
                );

                if let Some(method_def_id) = self.get_method_def_id(&parent_expr) {
                    //println!("Method def id: {:?}", method_def_id);
                    let return_type = self.get_function_or_method_return_type(&method_def_id);
                    if return_type == ReturnType::ResultOrOption {
                        return self.analyze_res_opt_method_call(&parent_expr);
                    } else if return_type == ReturnType::Bool {
                        // TODO handle boolean returns
                        println!("Boolean return type: not yet fully supported");
                        let rv_check = self.get_bool_method_check(&parent_expr)?;
                        // is this part of an if stmt?
                        if let Some((_, rustc_hir::Node::Expr(expr))) =
                            self.tcx.hir_parent_iter(parent_expr.hir_id).next()
                            && let rustc_hir::ExprKind::If(cond, then_block, _else_block) =
                                expr.kind
                        {
                            // cond should always be our binary expression: confirm this, abort if not
                            if cond.hir_id != parent_expr.hir_id {
                                return None;
                            }

                            println!("RV checked via If comparison at {:?}", expr.span);

                            return self.analyze_if_stmt(rv_check, then_block);
                        }
                    } else {
                        println!("Not Result/Option or Boolean return type: not supported");
                        return None;
                    }
                }
            }

            Some(rustc_hir::Node::Expr(parent_expr))
                if let rustc_hir::ExprKind::Call(func, args) = parent_expr.kind =>
            {
                println!(
                    "RV passed to function {:?} at {:?}",
                    self.get_function_def_id(&func).expect("IndeterminateDefId"),
                    expr_being_checked.span
                );

                if let Some(function_def_id) = self.get_function_def_id(&func) {
                    let return_type = self.get_function_or_method_return_type(&function_def_id);
                    if return_type == ReturnType::ResultOrOption {
                        return self.analyze_res_opt_function_call(
                            &func,
                            args,
                            expr_being_checked,
                        );
                    } else if return_type == ReturnType::Bool {
                        // TODO handle boolean returns
                        println!("Boolean return type: not yet supported");
                        let rv_check = self.get_bool_function_check(&func)?;

                        // is this part of an if stmt?
                        if let Some((_, rustc_hir::Node::Expr(expr))) =
                            self.tcx.hir_parent_iter(parent_expr.hir_id).next()
                            && let rustc_hir::ExprKind::If(cond, then_block, _else_block) =
                                expr.kind
                        {
                            // cond should always be our binary expression: confirm this, abort if not
                            if cond.hir_id != parent_expr.hir_id {
                                return None;
                            }

                            println!("RV checked via If comparison at {:?}", expr.span);

                            return self.analyze_if_stmt(rv_check, then_block);
                        }
                    } else {
                        println!("Not Result/Option or Boolean return type: not supported");
                        return None;
                    }
                }
            }

            _ => {
                println!("RV use unclassified at {:?}", expr_being_checked.span);
            }
        }

        None
    }

    fn analyze_if_stmt(
        self: &Self,
        rv_check: ReturnValueCheck,
        then_block: &rustc_hir::Expr,
    ) -> Option<ReturnValueCheck> {
        let then_result_type = block_result_type(then_block);

        if let Some(arm1_result_type) = then_result_type {
            // if we are checking for an error (and returning as such), we found our rv check
            if matches!(arm1_result_type, ResultOrOptionVariant::Err)
                || matches!(arm1_result_type, ResultOrOptionVariant::None)
            {
                println!("Error Condition is {:?}", rv_check);
                return Some(rv_check);
            //if we are checking for non-error (and thus returning ok), the opposite of the check is our error
            // TODO expand beyond simple either/or (allow for multiple different error checks) ?
            } else if matches!(arm1_result_type, ResultOrOptionVariant::Ok)
                || matches!(arm1_result_type, ResultOrOptionVariant::Some)
            {
                println!("Error Condition is {:?}", rv_check.clone().opposite());
                return Some(rv_check.opposite());
            }

            println!("Neither Error nor Normal Block");
        }

        None
    }

    fn analyze_match_stmt(self: &Self, arms: &[rustc_hir::Arm]) -> Option<ReturnValueCheck> {
        if let Some(arm1_guard) = arms[0].guard {
            println!("Stepping into Guard 1...");
            if let rustc_hir::ExprKind::Binary(arm1_bin_op, _arm1_bin_ex1, arm1_bin_ex2) =
                arm1_guard.kind
            {
                println!("Guard 1 is Binary Expression...");

                let rv_check = ReturnValueCheck::parse_from_bin_op(&arm1_bin_op, &arm1_bin_ex2);

                if let Some(rv_check) = rv_check {
                    let arm1_result_type = arm_result_type(arms[0].body);
                    println!("Binary Operation: {:?}", arm1_bin_op.node);

                    if let Some(arm1_result_type) = arm1_result_type {
                        // if we are checking for an error (and returning as such), we found our rv check
                        if matches!(arm1_result_type, ResultOrOptionVariant::Err)
                            || matches!(arm1_result_type, ResultOrOptionVariant::None)
                        {
                            println!("Error Condition is {:?}", rv_check);
                            return Some(rv_check);
                        //if we are checking for non-error (and thus returning ok), the opposite of the check is our error
                        // TODO expand beyond simple either/or (allow for multiple different error checks) ?
                        } else if matches!(arm1_result_type, ResultOrOptionVariant::Ok)
                            || matches!(arm1_result_type, ResultOrOptionVariant::Some)
                        {
                            println!("Error Condition is {:?}", rv_check.clone().opposite());
                            return Some(rv_check.opposite());
                        }

                        println!("Neither Error nor Normal Block");
                    }
                }
            }
        } else {
            println!("No guard found!");
        }

        None
    }
}

#[allow(non_snake_case)]
pub fn find_RV_checks(tcx: rustc_middle::ty::TyCtxt<'_>, wrapper_function: &mut WrapperFunction) {
    println!(
        "\nFor Wrapper Function {}",
        tcx.def_path_str(wrapper_function.wrapper_function_id)
    );

    // only works for local functions (no HIR body for external crates)
    let Some(owner_local_def_id) = wrapper_function.wrapper_function_id.as_local() else {
        println!("Not local!");
        wrapper_function.return_value_check = ReturnValueCheck::IndeterminateNotLocal;
        return;
    };
    // abort if function has no body
    let Some(body) = tcx.hir_maybe_body_owned_by(owner_local_def_id) else {
        println!("No body!");
        wrapper_function.return_value_check = ReturnValueCheck::Indeterminate;
        return;
    };

    let mut finder = RVCheckFinder {
        tcx,
        wrapper_function: wrapper_function.clone(), // TODO remove this clone
        wrapped_function_value_holder: None,
        already_visited_functions: Vec::new(),
    };
    finder.visit_body(body);

    *wrapper_function = finder.wrapper_function.clone();
}

// checks whether a blocks tail expression is Ok(), Err(), Some(), or None
fn block_result_type(block_expr: &rustc_hir::Expr<'_>) -> Option<ResultOrOptionVariant> {
    if let rustc_hir::ExprKind::Block(block, _) = block_expr.kind {
        // go through all the stmts in the block
        for stmt in block.stmts {
            // return for return stmt, if there is one
            if let rustc_hir::StmtKind::Semi(expr) = stmt.kind {
                if let rustc_hir::ExprKind::Ret(Some(ret_expr)) = expr.kind {
                    println!("Return Statement Found");
                    return expr_result_type(&ret_expr);
                }
            }

            // return for semicolonless expr, if there is one
            // TODO this possibly doesnt do anything? test
            if let rustc_hir::StmtKind::Expr(expr) = stmt.kind {
                println!("Return Expression Found");
                return expr_result_type(&expr);
            }
        }

        if let Some(tail) = block.expr {
            println!("Tail Found");
            return expr_result_type(&tail);
        }
    }
    None
}

// checks whether a match stmts arm expression is Ok() or Err()
fn arm_result_type(arm_body: &rustc_hir::Expr<'_>) -> Option<ResultOrOptionVariant> {
    // reuse above function if we have a whole block as our match arm
    if let rustc_hir::ExprKind::Block(_, _) = arm_body.kind {
        return block_result_type(arm_body);
    }
    // return for return stmt, if the arm expr is one
    if let rustc_hir::ExprKind::Ret(Some(ret_expr)) = arm_body.kind {
        return expr_result_type(&ret_expr);
    }

    expr_result_type(arm_body)
}

fn expr_result_type(expr: &rustc_hir::Expr<'_>) -> Option<ResultOrOptionVariant> {
    if let rustc_hir::ExprKind::Call(func, _) = &expr.kind {
        if let rustc_hir::ExprKind::Path(qpath) = &func.kind {
            if let rustc_hir::QPath::Resolved(_, path) = qpath {
                if let Some(seg) = path.segments.last() {
                    if seg.ident.name.as_str() == "Ok" {
                        println!("Found Ok");
                        return Some(ResultOrOptionVariant::Ok);
                    } else if seg.ident.name.as_str() == "Err" {
                        println!("Found Err");
                        return Some(ResultOrOptionVariant::Err);
                    } else if seg.ident.name.as_str() == "Some" {
                        println!("Found Some");
                        return Some(ResultOrOptionVariant::Some);
                    }
                }
            }
        }
    // None is a path, not a call, must be treated differently
    } else if let rustc_hir::ExprKind::Path(qpath) = &expr.kind {
        if let rustc_hir::QPath::Resolved(_, path) = qpath {
            if let Some(seg) = path.segments.last() {
                if seg.ident.name.as_str() == "None" {
                    println!("Found None");
                    return Some(ResultOrOptionVariant::None);
                }
            }
        }
    }
    None
}
