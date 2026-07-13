// * responsible for generating error check specifications for wrapper functions

use std::collections::HashSet;
use std::ops::{Add, AddAssign};

use crate::error_check_spec_generation::driver::OtherStatistics;
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

    fn to_number_set(self) -> Option<HashSet<i8>> {
        let mut set = HashSet::new();
        match self {
            Self::Empty => {}
            Self::LesserZero => {
                set.insert(-1);
            }
            Self::GreaterZero => {
                set.insert(1);
            }
            Self::NotEqZero => {
                set.insert(-1);
                set.insert(1);
            }
            Self::LesEqZero => {
                set.insert(-1);
                set.insert(0);
            }
            Self::GrEqZero => {
                set.insert(0);
                set.insert(1);
            }
            Self::EqualZero => {
                set.insert(0);
            }
            Self::All => {
                set.insert(-1);
                set.insert(0);
                set.insert(1);
            }
            Self::Indeterminate => return None,
        }

        Some(set)
    }

    fn from_number_set(set: HashSet<i8>) -> ReturnValueCheck {
        match set.len() {
            0 => Self::Empty,
            1 => {
                if set.iter().any(|&x| x < 0) {
                    Self::LesserZero
                } else if set.contains(&0) {
                    Self::EqualZero
                } else if set.iter().any(|&x| x > 0) {
                    Self::GreaterZero
                } else {
                    Self::Indeterminate
                }
            }
            2 => {
                if set.iter().any(|&x| x < 0) && set.contains(&0) {
                    Self::LesEqZero
                } else if set.contains(&0) && set.iter().any(|&x| x > 0) {
                    Self::GrEqZero
                } else if set.iter().any(|&x| x < 0) && set.iter().any(|&x| x > 0) {
                    Self::NotEqZero
                } else {
                    Self::Indeterminate
                }
            }
            3 => Self::All,
            _ => Self::Indeterminate,
        }
    }

    // implementation via number sets is a bit roundabout, but easier than matching on every single possibility
    fn union(self, other: ReturnValueCheck) -> ReturnValueCheck {
        if self == ReturnValueCheck::Indeterminate || other == ReturnValueCheck::Indeterminate {
            return ReturnValueCheck::Indeterminate;
        }

        let mut as_num_set: HashSet<i8> = HashSet::new();
        if let Some(set) = self.to_number_set() {
            as_num_set.extend(set);
        }
        if let Some(set) = other.to_number_set() {
            as_num_set.extend(set);
        }
        Self::from_number_set(as_num_set)
    }

    fn intersection(self, other: ReturnValueCheck) -> ReturnValueCheck {
        if self == ReturnValueCheck::Indeterminate || other == ReturnValueCheck::Indeterminate {
            return ReturnValueCheck::Indeterminate;
        }

        let mut as_num_set: HashSet<i8> = HashSet::new();
        if let Some(self_set) = self.to_number_set()
            && let Some(other_set) = other.to_number_set()
        {
            as_num_set = self_set.intersection(&other_set).copied().collect();
        }
        Self::from_number_set(as_num_set)
    }

    fn without(self, other: ReturnValueCheck) -> ReturnValueCheck {
        if self == ReturnValueCheck::Indeterminate || other == ReturnValueCheck::Indeterminate {
            return ReturnValueCheck::Indeterminate;
        }

        if let Some(self_set) = self.to_number_set()
            && let Some(other_set) = other.to_number_set()
        {
            let difference: HashSet<i8> = self_set.difference(&other_set).copied().collect();
            Self::from_number_set(difference)
        } else {
            ReturnValueCheck::Indeterminate
        }
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Copy, Hash)]
pub enum ResultOrOptionVariant {
    ResultOk,
    ResultErr,
    OptionSome,
    OptionNone,
}

#[derive(PartialEq, Eq, Debug)]
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
    pub mode: ReturnType, // TODO implement bool returning funcitons properly if needed
    pub other_statistics: OtherStatistics,
}

impl<'tcx> rustc_hir::intravisit::Visitor<'tcx> for RVCheckFinder<'tcx> {
    // as we go through the expressions of the body, for each expr
    // we check if it is the current holder of the return value, and if so, what happens to it ( via ehck_use_site() )
    fn visit_expr(&mut self, expr: &'tcx rustc_hir::Expr<'tcx>) {
        // if we have already found a check, we do not look further
        if self.wrapper_function.return_value_check.is_some() {
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
                    let matches = matches!(res, rustc_hir::def::Res::Def(_, def_id) if def_id == self.wrapper_function.wrapped_function_id);
                    if matches {
                        println!("Found call to wrapped C function: {:?}", res);
                    }
                    matches
                } else {
                    false
                }
            }
            // already tracking: is this expr a Path to the current holder?
            (rustc_hir::ExprKind::Path(qpath), Some(holder)) => {
                let res = typeck_results.qpath_res(qpath, expr.hir_id);
                let matches = matches!(res, rustc_hir::def::Res::Local(hir_id) if hir_id == holder);
                if matches {
                    println!("Found use of holding variable: {:?}", res);
                }
                matches
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
            self.wrapper_function.return_value_check = rv_check;
            //println!("{:?}", self.wrapper_function);
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
        let mut return_value_check: Option<ReturnValueCheck> = None; // will only be returned if we do not find any sort of check, as those return directly

        let parents = self
            .tcx
            .hir_parent_iter(expr_being_checked.hir_id)
            .map(|(_, node)| node)
            .into_iter();
        //println!("Parent: {:?}", parents);

        // TODO support for rv borrowing?
        // TODO see pattern from apppend() in Libgit/src/repo.rs::976 : support this?

        let mut previous_parent = expr_being_checked; // will be needed as a possible function/method argument, if the funciton gets a whole expression as an argument

        // we go through all the layers of parents until there is something we can analyze (if any)
        for parent in parents {
            if let rustc_hir::Node::Expr(parent_expr) = parent.clone() {
                // println!("Analyzing if parent expression {:?} is return without check", parent_expr.kind);

                let result_type = single_expr_result_type(parent_expr);
                println!("Parent expression has result type {:?}", result_type);

                // if we find a Non-error return, with no check being found in the parents later in this loop, then no return is an error, and thus the check is "Empty"
                // the opposite check would be "All", which should never be used, since no function should always return an error
                // therefore not implemented (would be a bit more complicated, as it would not use the tracked value, as we do in this fucntion
                if result_type == Some(ResultOrOptionVariant::ResultOk)
                    || result_type == Some(ResultOrOptionVariant::OptionSome)
                {
                    return_value_check = Some(ReturnValueCheck::Empty);
                    // println!("- only temporary, continuing up chain...");
                    continue; // we continue to see if there is a check later in the parent chain, which would override this
                }
            }

            match parent {

                // on sth like let result = <tracked expr>: move holder to result's binding HirId
                rustc_hir::Node::LetStmt(local) => {
                    if let rustc_hir::PatKind::Binding(_, hir_id, ident, _) = local.pat.kind {
                        println!("RV identity moves to '{}'", ident.name);
                        self.wrapped_function_value_holder = Some(hir_id);
                        break;
                    }
                }

                rustc_hir::Node::Expr(parent_expr)
                    if let rustc_hir::ExprKind::Match(_matchee, arms, _) = parent_expr.kind =>
                {
                    println!("RV checked via match at {:?}", expr_being_checked.span);

                    return self.analyze_match_stmt(arms);
                }

                // TODO support first comparand, comparands other than zero ?
                // TODO support check in else if
                rustc_hir::Node::Expr(parent_expr)
                    if let rustc_hir::ExprKind::Binary(bin_op, _ex1, ex2) = parent_expr.kind =>
                {
                    let mut rv_check = ReturnValueCheck::parse_from_bin_op(&bin_op, &ex2)?;

                    // is this part of an if stmt?
                    if let Some((_, rustc_hir::Node::Expr(expr))) =
                        self.tcx.hir_parent_iter(parent_expr.hir_id).next()
                        && let rustc_hir::ExprKind::If(cond, then_block, else_block) = expr.kind
                    {
                        // cond should always be our binary expression: confirm this, abort if not
                        if cond.hir_id != parent_expr.hir_id {
                            return None;
                        }

                        println!("RV checked via If comparison at {:?}", expr.span);
                        println!("Binary Operation: {:?}", bin_op.node);

                        return self.analyze_if_stmt(rv_check, then_block, else_block);

                    // or is it a negated condition inside an if stmt?
                    } else if let Some((_, rustc_hir::Node::Expr(expr))) =
                        self.tcx.hir_parent_iter(parent_expr.hir_id).next()
                        && let rustc_hir::ExprKind::Unary(rustc_hir::UnOp::Not, _) = expr.kind
                    {
                        println!("Negation found at {:?}", expr.span);
                        self.other_statistics.condition_negations += 1;

                        if let Some((_, rustc_hir::Node::Expr(expr2))) =
                            self.tcx.hir_parent_iter(expr.hir_id).next()
                        && let rustc_hir::ExprKind::If(_cond, then_block, else_block) = expr2.kind {

                            println!("RV checked via If comparison at {:?}", expr.span);
                            println!("Negated Binary Operation: {:?}, so {:?}", bin_op.node, rv_check.opposite());

                            // since the bin op is negated, we need the opoosite rv check
                            rv_check = rv_check.opposite();

                            return self.analyze_if_stmt(rv_check, then_block, else_block);
                        }
                    }

                    // is this a top-level comparison, not part of an if stmt? (eg in a return statement)
                    // if self.mode == ReturnType::Bool {
                    //     println!("RV checked via top-level comparison at {:?}", expr_being_checked.span);
                    //     println!("Binary Operation: {:?}", bin_op.node);
                    //     return Some(rv_check);
                    // }
                }

                rustc_hir::Node::Expr(parent_expr)
                    if let rustc_hir::ExprKind::MethodCall(method, ..) = parent_expr.kind =>
                {
                    println!(
                        "RV will be passed to method '{}' at {:?}",
                        method.ident, expr_being_checked.span
                    );

                    if let Some(method_def_id) = self.get_method_def_id(&parent_expr) {
                        //println!("Method def id: {:?}", method_def_id);
                        let return_type =
                            get_function_or_method_return_type(&self.tcx, &method_def_id);
                        println!("Return type, parsed: {:?}", return_type);

                        if return_type == ReturnType::ResultOrOption {
                            return self.analyze_res_opt_method(&parent_expr, previous_parent); // cannot simply use expr_being checked, as it might be in a block or sth
                        
                        } else if return_type == ReturnType::Bool {

                            let mut rv_check = self.analyze_bool_method(&parent_expr)?;

                            // is this part of an if stmt?
                            if let Some((_, rustc_hir::Node::Expr(expr))) =
                                self.tcx.hir_parent_iter(parent_expr.hir_id).next()
                                && let rustc_hir::ExprKind::If(cond, then_block, else_block) =
                                    expr.kind
                            {
                                // cond should always be our binary expression: confirm this, abort if not
                                if cond.hir_id != parent_expr.hir_id {
                                    return None;
                                }

                                println!("RV checked via If comparison at {:?}", expr.span);

                                return self.analyze_if_stmt(rv_check, then_block, else_block);

                            // or a negated condition of an if stmt?
                            } else if let Some((_, rustc_hir::Node::Expr(expr))) =
                                self.tcx.hir_parent_iter(parent_expr.hir_id).next()
                                && let rustc_hir::ExprKind::Unary(rustc_hir::UnOp::Not, _) = expr.kind
                            {
                                println!("Negation found at {:?}", expr.span);
                                self.other_statistics.condition_negations += 1;

                                if let Some((_, rustc_hir::Node::Expr(expr2))) =
                                    self.tcx.hir_parent_iter(expr.hir_id).next()
                                && let rustc_hir::ExprKind::If(_cond, then_block, else_block) = expr2.kind {

                                    println!("RV checked via If comparison at {:?}", expr.span);

                                    // since the bin op is negated, we need the opoosite rv check
                                    rv_check = rv_check.opposite();

                                    return self.analyze_if_stmt(rv_check, then_block, else_block);
                                }
                            }
                        } else {
                            println!("Not Result/Option or Boolean return type: not supported");
                            return None;
                        }
                    }
                }

                rustc_hir::Node::Expr(parent_expr)
                    if let rustc_hir::ExprKind::Call(func, args) = parent_expr.kind =>
                {
                    println!(
                        "RV will be passed to function {:?} at {:?}",
                        self.get_function_def_id(&func).expect("IndeterminateDefId"),
                        expr_being_checked.span
                    );

                    if let Some(function_def_id) = self.get_function_def_id(&func) {
                        let return_type =
                            get_function_or_method_return_type(&self.tcx, &function_def_id);
                        println!("Return type, parsed: {:?}", return_type);

                        if return_type == ReturnType::ResultOrOption {
                            return self.analyze_res_opt_function(&func, args, previous_parent); // cannot simply use expr_being checked, as it might be in a block or sth
                        } else if return_type == ReturnType::Bool {
                            let mut rv_check = self.analyze_bool_function(&func)?;

                            // is this part of an if stmt?
                            if let Some((_, rustc_hir::Node::Expr(expr))) =
                                self.tcx.hir_parent_iter(parent_expr.hir_id).next()
                                && let rustc_hir::ExprKind::If(cond, then_block, else_block) =
                                    expr.kind
                            {
                                // cond should always be our binary expression: confirm this, abort if not
                                // should never trigger
                                if cond.hir_id != parent_expr.hir_id {
                                    return None;
                                }

                                println!("RV checked via If comparison at {:?}", expr.span);

                                return self.analyze_if_stmt(rv_check, then_block, else_block);

                            // or a negated condition of an if stmt?
                            } else if let Some((_, rustc_hir::Node::Expr(expr))) =
                                self.tcx.hir_parent_iter(parent_expr.hir_id).next()
                                && let rustc_hir::ExprKind::Unary(rustc_hir::UnOp::Not, _) = expr.kind
                            {
                                println!("Negation found at {:?}", expr.span);
                                self.other_statistics.condition_negations += 1;

                                if let Some((_, rustc_hir::Node::Expr(expr2))) =
                                    self.tcx.hir_parent_iter(expr.hir_id).next()
                                && let rustc_hir::ExprKind::If(_cond, then_block, else_block) = expr2.kind {

                                    println!("RV checked via If comparison at {:?}", expr.span);

                                    // since the bin op is negated, we need the opoosite rv check
                                    rv_check = rv_check.opposite();

                                    return self.analyze_if_stmt(rv_check, then_block, else_block);
                                }
                            }
                        } else {
                            println!("Not Result/Option or Boolean return type: not supported");
                            return None;
                        }
                    }
                }

                _ => {
                    println!(
                        "Cannot analyze at {:?}, continuing up parent expr chain; ExprKind was {:?}",
                        expr_being_checked.span,
                        expr_being_checked.kind
                    );
                }
            }

            if let rustc_hir::Node::Expr(parent) = parent {
                previous_parent = parent;
            }
        }

        if let Some(return_value_check) = return_value_check {
            println!(
                "Parent Chain exhausted, Error Condition is: {:?}",
                return_value_check
            );
        }

        return_value_check
    }

    fn analyze_if_stmt(
        self: &mut Self,
        rv_check: ReturnValueCheck,
        then_block: &rustc_hir::Expr,
        else_block: Option<&rustc_hir::Expr>,
    ) -> Option<ReturnValueCheck> {
        let inner_return = self.analyze_if_stmt_inner(rv_check, then_block, else_block);
        if inner_return.0 != inner_return.1.opposite() {
            println!("Err checks not equal to opposite of OK checks")
        }
        println!("Total Err Condition is {:?}", inner_return.0);
        Some(inner_return.0)
    }

    // we wrap this to hide some of the uglier implementation details related to recursion
    fn analyze_if_stmt_inner(
        self: &mut Self,
        rv_check: ReturnValueCheck,
        then_block: &rustc_hir::Expr,
        else_block: Option<&rustc_hir::Expr>,
    ) -> (ReturnValueCheck, ReturnValueCheck) {
        let mut if_stmt_total_err_check = ReturnValueCheck::Empty;
        let mut if_stmt_total_ok_check = ReturnValueCheck::Empty;

        let then_result_type = block_result_type(then_block);

        if let Some(then_result_type) = then_result_type {
            // if we are checking for an error (and returning as such), we found our rv check
            if matches!(then_result_type, ResultOrOptionVariant::ResultErr)
                || matches!(then_result_type, ResultOrOptionVariant::OptionNone)
            {
                println!("Block Error Condition is {:?}", rv_check);
                if_stmt_total_err_check = if_stmt_total_err_check.union(rv_check);
            //if we are checking for non-error (and thus returning ok), the opposite of the check is our error
            } else if matches!(then_result_type, ResultOrOptionVariant::ResultOk)
                || matches!(then_result_type, ResultOrOptionVariant::OptionSome)
            {
                println!("Block Ok Condition is {:?}", rv_check.clone());
                if_stmt_total_ok_check = if_stmt_total_ok_check.union(rv_check);
            } else {
                println!("Neither Error nor Normal Block");
            }
        }

        // if there is an else block
        if let Some(else_block) = else_block {
            // if an else if exists, analyze it recursively (else if is just an else containing an if stmt as expr)
            if let rustc_hir::ExprKind::If(else_cond, else_then_block, else_else_block) =
                else_block.kind
            {
                let mut else_cond_parsed = ReturnValueCheck::All; // temporary value 

                if let rustc_hir::ExprKind::Binary(bin_op, _ex1, ex2) = else_cond.kind {
                    if let Some(check) = ReturnValueCheck::parse_from_bin_op(&bin_op, &ex2) {
                        println!(
                            "Else If condition is Binary Operation: {:?}; parsed as {:?}",
                            bin_op.node, check
                        );
                        else_cond_parsed = check;
                    }
                } else if let rustc_hir::ExprKind::MethodCall(..) = else_cond.kind {
                    if let Some(check) = self.analyze_bool_method(else_cond) {
                        println!("Else If condition is Method Call; parsed as {:?}", check);
                        else_cond_parsed = check;
                    }
                } else if let rustc_hir::ExprKind::Call(func, ..) = else_cond.kind {
                    if let Some(check) = self.analyze_bool_function(func) {
                        println!("Else If condition is Function Call; parsed as {:?}", check);
                        else_cond_parsed = check;
                    }
                }

                let else_if_rv_check =
                    self.analyze_if_stmt_inner(else_cond_parsed, else_then_block, else_else_block);
                if_stmt_total_err_check = if_stmt_total_err_check
                    .union(else_if_rv_check.0)
                    .without(if_stmt_total_ok_check);
                if_stmt_total_ok_check = if_stmt_total_ok_check
                    .union(else_if_rv_check.1)
                    .without(if_stmt_total_err_check);
            } else {
                let else_result_type = block_result_type(else_block);

                if let Some(else_result_type) = else_result_type {
                    // if we are checking for an error (and returning as such), we found our rv check
                    if matches!(else_result_type, ResultOrOptionVariant::ResultErr)
                        || matches!(else_result_type, ResultOrOptionVariant::OptionNone)
                    {
                        if_stmt_total_err_check =
                            if_stmt_total_err_check.union(rv_check.opposite());
                    //if we are checking for non-error (and thus returning ok), the opposite of the check is our error
                    } else if matches!(else_result_type, ResultOrOptionVariant::ResultOk)
                        || matches!(else_result_type, ResultOrOptionVariant::OptionSome)
                    {
                        if_stmt_total_ok_check = if_stmt_total_ok_check.union(rv_check.opposite());
                    } else {
                        println!("Else is Neither Error nor Normal Block");
                    }
                }
            }

            // if the else block does not contain its own if stmt
        }

        println!(
            "Err and Ok condition for this if stmt are {:?}, {:?}",
            if_stmt_total_err_check, if_stmt_total_ok_check
        );
        (if_stmt_total_err_check, if_stmt_total_ok_check)
    }

    // parse any condition expression into a ReturnValueCheck
    // TODO make use of this function more widely
    fn parse_condition(&mut self, cond: &rustc_hir::Expr) -> Option<ReturnValueCheck> {
        match &cond.kind {
            rustc_hir::ExprKind::Binary(bin_op, _lhs, rhs) => {
                ReturnValueCheck::parse_from_bin_op(bin_op, rhs)
            }
            rustc_hir::ExprKind::MethodCall(..) => self.analyze_bool_method(cond),
            rustc_hir::ExprKind::Call(func, ..) => self.analyze_bool_function(func),
            rustc_hir::ExprKind::Unary(rustc_hir::UnOp::Not, inner) => {
                // negated condition -> opposite of the inner check
                self.parse_condition(inner).map(|c| c.opposite())
            }
            _ => None,
        }
    }

    fn analyze_match_stmt(self: &mut Self, arms: &[rustc_hir::Arm]) -> Option<ReturnValueCheck> {
        let mut match_total_err_check: ReturnValueCheck = ReturnValueCheck::Empty;
        let mut match_total_ok_check: ReturnValueCheck = ReturnValueCheck::Empty;

        for arm in arms {
            println!("Analyzing arm at {:?}", arm.span);

            let mut arm_pattern_check = ReturnValueCheck::All;

            if let rustc_hir::PatKind::Expr(pat_expr) = arm.pat.kind {
                // teest if the pat is a literal int
                if let rustc_hir::PatExprKind::Lit { lit, .. } = &pat_expr.kind {
                    if let rustc_ast::LitKind::Int(value, _) = lit.node {
                        if value == 0 {
                            arm_pattern_check = ReturnValueCheck::EqualZero;
                            println!("Arm pattern is 0, patterns rv check is EqualZero");
                        } else {
                            arm_pattern_check = ReturnValueCheck::Indeterminate;
                            println!("Arm pattern is Int but not 0, patterns rv check is Indeterminate");
                        }
                    }
                // test if the pat is a constant
                } else if let rustc_hir::PatExprKind::Path(qpath) = &pat_expr.kind {
                    let owner = self.wrapper_function.wrapper_function_id.as_local()?;
                    let typeck_results = self.tcx.typeck(owner);
                    let res = typeck_results.qpath_res(qpath, pat_expr.hir_id);
                    if let rustc_hir::def::Res::Def(rustc_hir::def::DefKind::Const{..}, def_id) = res {
                        // evaluate the constant
                        if let rustc_middle::mir::interpret::EvalToConstValueResult::Ok(const_val) = self.tcx.const_eval_poly(def_id) {
                            // extract the scalar value
                            // for integer constants:
                            if let rustc_middle::mir::ConstValue::Scalar(scalar) = const_val {
                                if let rustc_middle::mir::interpret::Scalar::Int(scalar_int) = scalar {
                                    let value = scalar_int.to_int(scalar_int.size());
                                    if value == 0 {
                                        arm_pattern_check = ReturnValueCheck::EqualZero;
                                        println!("Arm pattern is 0, patterns rv check is EqualZero");
                                    } else {
                                        arm_pattern_check = ReturnValueCheck::Indeterminate;
                                        println!("Arm pattern is Const Int but not 0, patterns rv check is Indeterminate");
                                    }
                                }
                            }
                        }
                    }
                } else {
                    arm_pattern_check = ReturnValueCheck::Indeterminate;
                    println!("Arm pattern is not a literal or constant, patterns rv check is Indeterminate");
                }
            }

            let mut arm_guard_check = ReturnValueCheck::All;

            if let Some(arm_guard) = arm.guard {
                println!("Stepping into Guard...");
                if let rustc_hir::ExprKind::Binary(arm_bin_op, _arm1_bin_ex1, arm_bin_ex2) =
                    arm_guard.kind
                {
                    println!("Guard is Binary Expression...");

                    if let Some(check) =
                        ReturnValueCheck::parse_from_bin_op(&arm_bin_op, &arm_bin_ex2)
                    {
                        arm_guard_check = check;
                        println!("Guard is Binary Operation: {:?}", arm_bin_op.node);
                    } else {
                        println!(
                            "Guard is Binary Operation, but could not parse: {:?}",
                            arm_bin_op.node
                        );
                    }
                } else if let rustc_hir::ExprKind::MethodCall(..) = arm_guard.kind {
                    if let Some(check) = self.analyze_bool_method(arm_guard) {
                        println!("Else If condition is Method Call; parsed as {:?}", check);
                        arm_guard_check = check;
                    }
                } else if let rustc_hir::ExprKind::Call(func, ..) = arm_guard.kind {
                    if let Some(check) = self.analyze_bool_function(func) {
                        println!("Else If condition is Function Call; parsed as {:?}", check);
                        arm_guard_check = check;
                    }
                }
            } else {
                println!("No guard found!");
            }

            let arm_total_check = arm_pattern_check.intersection(arm_guard_check);

            let arm_result_type = arm_result_type(arm.body);

            if let Some(arm_result_type) = arm_result_type {
                // if we are checking for an error (and returning as such), we found our rv check
                if matches!(
                    arm_result_type,
                    ResultOrOptionVariant::ResultErr | ResultOrOptionVariant::OptionNone
                ) {
                    println!("Arm Error Condition is {:?}", arm_total_check);

                    match_total_err_check = match_total_err_check
                        .union(arm_total_check)
                        .without(match_total_ok_check);

                    println!(
                        "Temporary value of match total Err check is {:?} after unioning with {:?}",
                        match_total_err_check, arm_total_check
                    );

                //if we are checking for non-error (and thus returning ok), the opposite of the check is our error
                // TODO expand beyond simple either/or (allow for multiple different error checks) ?
                } else if matches!(arm_result_type, ResultOrOptionVariant::ResultOk)
                    || matches!(arm_result_type, ResultOrOptionVariant::OptionSome)
                {
                    println!("Arm Ok Condition is {:?}", arm_total_check.clone());

                    match_total_ok_check = match_total_ok_check
                        .union(arm_total_check)
                        .without(match_total_err_check);

                    println!(
                        "Temporary value of match total OK check is {:?} after intersecting with {:?}",
                        match_total_ok_check, arm_total_check
                    );
                } else {
                    println!("Neither Error nor Normal Block");
                }
            }
        }

        // this code was removed, as it was not really necessary, and would not work for match statements that genuinely believe everything is an error
        // (which would be weird, but a possible bug in the code being analyzed, and thus should be handled)
        // // if our total remains All (unchanged), we did not find any error checks in the match statement, and thus return None
        // // a match that genuinely believes everything is an error would not make sense
        // // kinda ugly but works well enough
        // // TODO improve
        // if match_total_rv_check == ReturnValueCheck::All {
        //     println!("Match statement did not yield any error check, returning None");
        //     return None;
        // }

        println!(
            "Match statement total Err, OK checks are {:?}, {:?}",
            match_total_err_check, match_total_ok_check
        );

        println!(
            "Match statement total Err check is {:?}",
            match_total_err_check
        );

        Some(match_total_err_check)
    }
}

#[allow(non_snake_case)]
pub fn find_RV_checks(
    tcx: rustc_middle::ty::TyCtxt<'_>,
    wrapper_function: &mut WrapperFunction,
    other_statistics: &mut OtherStatistics,
) {
    println!(
        "\nFor Wrapper Function {}",
        tcx.def_path_str(wrapper_function.wrapper_function_id)
    );

    // we only support wrapper functions that return Result or Option, maybe bool to come, but might not be necessary
    if get_function_or_method_return_type(&tcx, &wrapper_function.wrapper_function_id)
        != ReturnType::ResultOrOption
    {
        other_statistics.not_result_or_option_return_types += 1;
        wrapper_function.return_value_check = Some(ReturnValueCheck::Empty);
        return;
    }
    // only works for local functions (no HIR body for external crates)
    let Some(owner_local_def_id) = wrapper_function.wrapper_function_id.as_local() else {
        println!("Not local!");
        wrapper_function.return_value_check = Some(ReturnValueCheck::Indeterminate);
        other_statistics.not_local_functions += 1;
        return;
    };
    // abort if function has no body
    let Some(body) = tcx.hir_maybe_body_owned_by(owner_local_def_id) else {
        println!("No body!");
        wrapper_function.return_value_check = Some(ReturnValueCheck::Indeterminate);
        return;
    };

    let mode = get_function_or_method_return_type(&tcx, &wrapper_function.wrapper_function_id);

    let mut finder = RVCheckFinder {
        tcx,
        wrapper_function: wrapper_function.clone(), // TODO remove this clone
        wrapped_function_value_holder: None,
        already_visited_functions: Vec::new(),
        mode,
        other_statistics: OtherStatistics::new(),
    };
    finder.visit_body(body);

    if finder.wrapper_function.return_value_check.is_none() {
        // TODO proper findinf for empty error checks
        println!(
            "No check found for wrapper function {}, setting Indeterminate",
            tcx.def_path_str(wrapper_function.wrapper_function_id)
        );
        finder.wrapper_function.return_value_check = Some(ReturnValueCheck::Indeterminate);
    }
    *other_statistics += finder.other_statistics.clone();
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
                    return single_expr_result_type(&ret_expr);
                }
            }

            // return for semicolonless expr, if there is one
            // TODO this possibly doesnt do anything? test
            if let rustc_hir::StmtKind::Expr(expr) = stmt.kind {
                println!("Return Expression Found");
                return single_expr_result_type(&expr);
            }
        }

        if let Some(tail) = block.expr {
            println!("Tail Found");
            return single_expr_result_type(&tail);
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
        return single_expr_result_type(&ret_expr);
    }

    single_expr_result_type(arm_body)
}

// finds the return for a single expression (no blocks or larger stuff like that)
fn single_expr_result_type(expr: &rustc_hir::Expr<'_>) -> Option<ResultOrOptionVariant> {
    if let rustc_hir::ExprKind::Call(func, _) = &expr.kind {
        if let rustc_hir::ExprKind::Path(qpath) = &func.kind {
            if let rustc_hir::QPath::Resolved(_, path) = qpath {
                if let Some(seg) = path.segments.last() {
                    if seg.ident.name.as_str() == "Ok" {
                        println!("Found Ok at {:?}", expr.span);
                        return Some(ResultOrOptionVariant::ResultOk);
                    } else if seg.ident.name.as_str() == "Err" {
                        println!("Found Err at {:?}", expr.span);
                        return Some(ResultOrOptionVariant::ResultErr);
                    } else if seg.ident.name.as_str() == "Some" {
                        println!("Found Some at {:?}", expr.span);
                        return Some(ResultOrOptionVariant::OptionSome);
                    }
                }
            }
        }
    // None is a path, not a call, must be treated differently
    } else if let rustc_hir::ExprKind::Path(qpath) = &expr.kind {
        if let rustc_hir::QPath::Resolved(_, path) = qpath {
            if let Some(seg) = path.segments.last() {
                if seg.ident.name.as_str() == "None" {
                    println!("Found None at {:?}", expr.span);
                    return Some(ResultOrOptionVariant::OptionNone);
                }
            }
        }
    }
    None
}

pub fn get_function_or_method_return_type(
    tcx: &rustc_middle::ty::TyCtxt<'_>,
    def_id: &rustc_hir::def_id::DefId,
) -> ReturnType {
    println!(
        "Getting return type for function/method {:?}",
        tcx.def_path_str(*def_id)
    );

    let return_type = tcx.fn_sig(*def_id).skip_binder().output().skip_binder();

    println!(
        "Return type: {:?} for function {:?}",
        return_type,
        tcx.def_path_str(*def_id)
    );

    match return_type.kind() {
        rustc_middle::ty::TyKind::Adt(adt_def, _args) => {
            let type_name = tcx.def_path_str(adt_def.did());
            if type_name == "core::result::Result"
                || type_name == "std::result::Result"
                || type_name == "std::io::Result"
                || type_name == "core::thread::Result"
                || type_name == "core::fmt::Result"
                || type_name == "core::option::Option"
                || type_name == "std::option::Option"
            {
                return ReturnType::ResultOrOption;
            }
        }

        rustc_middle::ty::TyKind::Bool => return ReturnType::Bool,

        _ => return ReturnType::Other,
    }

    ReturnType::Other
}

// merge two Option<ReturnValueCheck> by unioning them
fn merge_optioned_checks(
    a: Option<ReturnValueCheck>,
    b: Option<ReturnValueCheck>,
) -> Option<ReturnValueCheck> {
    match (a, b) {
        (None, x) => x,
        (x, None) => x,
        (Some(a), Some(b)) => Some(a.union(b)),
    }
}

#[test]
fn test_return_value_check_union() {
    let check1 = ReturnValueCheck::LesserZero;
    let check2 = ReturnValueCheck::GreaterZero;
    let union_check = check1.union(check2);
    assert_eq!(union_check, ReturnValueCheck::NotEqZero);

    let check3 = ReturnValueCheck::EqualZero;
    let union_check2 = check1.union(check3);
    assert_eq!(union_check2, ReturnValueCheck::LesEqZero);

    let check4 = ReturnValueCheck::GrEqZero;
    let union_check3 = check2.union(check4);
    assert_eq!(union_check3, ReturnValueCheck::GrEqZero);

    let check5 = ReturnValueCheck::All;
    let union_check4 = check1.union(check5);
    assert_eq!(union_check4, ReturnValueCheck::All);
}

#[test]
fn test_return_value_check_intersection() {
    let check1 = ReturnValueCheck::LesserZero;
    let check2 = ReturnValueCheck::GreaterZero;
    let intersection_check = check1.intersection(check2);
    assert_eq!(intersection_check, ReturnValueCheck::Empty);

    let check3 = ReturnValueCheck::EqualZero;
    let intersection_check2 = check1.intersection(check3);
    assert_eq!(intersection_check2, ReturnValueCheck::Empty);

    let check4 = ReturnValueCheck::GrEqZero;
    let intersection_check3 = check2.intersection(check4);
    assert_eq!(intersection_check3, ReturnValueCheck::GreaterZero);

    let check5 = ReturnValueCheck::All;
    let intersection_check4 = check1.intersection(check5);
    assert_eq!(intersection_check4, ReturnValueCheck::LesserZero);
}

#[test]
fn test_without() {
    // LesEqZero - EqualZero = LesserZero
    assert_eq!(
        ReturnValueCheck::LesEqZero.without(ReturnValueCheck::EqualZero),
        ReturnValueCheck::LesserZero
    );

    // LesEqZero - LesserZero = EqualZero
    assert_eq!(
        ReturnValueCheck::LesEqZero.without(ReturnValueCheck::LesserZero),
        ReturnValueCheck::EqualZero
    );

    // GrEqZero - EqualZero = GreaterZero
    assert_eq!(
        ReturnValueCheck::GrEqZero.without(ReturnValueCheck::EqualZero),
        ReturnValueCheck::GreaterZero
    );

    // NotEqZero - LesserZero = GreaterZero
    assert_eq!(
        ReturnValueCheck::NotEqZero.without(ReturnValueCheck::LesserZero),
        ReturnValueCheck::GreaterZero
    );

    // NotEqZero - GreaterZero = LesserZero
    assert_eq!(
        ReturnValueCheck::NotEqZero.without(ReturnValueCheck::GreaterZero),
        ReturnValueCheck::LesserZero
    );

    // All - LesserZero = GrEqZero
    assert_eq!(
        ReturnValueCheck::All.without(ReturnValueCheck::LesserZero),
        ReturnValueCheck::GrEqZero
    );

    // All - EqualZero = NotEqZero
    assert_eq!(
        ReturnValueCheck::All.without(ReturnValueCheck::EqualZero),
        ReturnValueCheck::NotEqZero
    );

    // anything without itself = Empty
    assert_eq!(
        ReturnValueCheck::LesserZero.without(ReturnValueCheck::LesserZero),
        ReturnValueCheck::Empty
    );
    assert_eq!(
        ReturnValueCheck::GrEqZero.without(ReturnValueCheck::GrEqZero),
        ReturnValueCheck::Empty
    );

    // anything without Empty = itself
    assert_eq!(
        ReturnValueCheck::LesserZero.without(ReturnValueCheck::Empty),
        ReturnValueCheck::LesserZero
    );

    // Empty without anything = Empty
    assert_eq!(
        ReturnValueCheck::Empty.without(ReturnValueCheck::LesserZero),
        ReturnValueCheck::Empty
    );

    // no overlap — result is self unchanged
    assert_eq!(
        ReturnValueCheck::LesserZero.without(ReturnValueCheck::GreaterZero),
        ReturnValueCheck::LesserZero
    );
    assert_eq!(
        ReturnValueCheck::EqualZero.without(ReturnValueCheck::LesserZero),
        ReturnValueCheck::EqualZero
    );

    // Indeterminate propagates
    assert_eq!(
        ReturnValueCheck::LesserZero.without(ReturnValueCheck::Indeterminate),
        ReturnValueCheck::Indeterminate
    );
    assert_eq!(
        ReturnValueCheck::Indeterminate.without(ReturnValueCheck::LesserZero),
        ReturnValueCheck::Indeterminate
    );
}
