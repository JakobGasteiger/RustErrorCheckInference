use std::{collections::HashSet, ops::ControlFlow::Continue};

use crate::{error_check_spec_generation::spec_generation::RVCheckFinder, utils::error_spec::ErrorSpec::*};


// Rust side spec
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct WrapperFunction {
    pub wrapper_function_id: rustc_hir::def_id::DefId,
    pub wrapped_function_id: rustc_hir::def_id::DefId,
    pub return_value_check: Option<ErrorSpec>,
}

// C side spec
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct FunctionErrorSpec {
    pub func_name: String,
    pub error_spec: ErrorSpec, 
}

impl FunctionErrorSpec {
    pub fn new(func_name: String, error_spec: ErrorSpec) -> Self {
        FunctionErrorSpec {
            func_name,
            error_spec
        }
    }
}


#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub enum ErrorSpec {
    Empty,
    LesserZero,
    GreaterZero,
    NotEqZero,
    LesEqZero,
    GrEqZero,
    EqualZero,
    All,
    Indeterminate,
}

impl ErrorSpec {
    
    pub fn opposite(self) -> ErrorSpec {
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

    pub fn parse_from_bin_op(
        bin_op: &rustc_hir::BinOp,
        comparand: &rustc_hir::Expr,
        rv_check_finder: &RVCheckFinder
    ) -> Option<ErrorSpec> {

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
        // else, is it a constant?
        } else if let rustc_hir::ExprKind::Path(qpath) = &comparand.kind {
            let owner = rv_check_finder.wrapper_function.wrapper_function_id.as_local()?;
            let typeck_results = rv_check_finder.tcx.typeck(owner);
            let res = typeck_results.qpath_res(qpath, comparand.hir_id);
            if let rustc_hir::def::Res::Def(rustc_hir::def::DefKind::Const{..}, def_id) = res {
                // evaluate the constant
                if let rustc_middle::mir::interpret::EvalToConstValueResult::Ok(const_val) = rv_check_finder.tcx.const_eval_poly(def_id) {
                    // extract the scalar value
                    // for integer constants:
                    if let rustc_middle::mir::ConstValue::Scalar(scalar) = const_val {
                        if let rustc_middle::mir::interpret::Scalar::Int(scalar_int) = scalar {
                            let value = scalar_int.to_int(scalar_int.size());
                            if value != 0 {
                                println!("Comparand is Const Int but not 0, patterns rv check is Indeterminate");
                                return Some(Self::Indeterminate);
                            }
                        }
                    }
                }
            }
        } else {
            println!("Comparand is not a literal or constant, patterns rv check is Indeterminate");
            return Some(Self::Indeterminate);
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

    pub fn to_number_set(self) -> Option<HashSet<i128>> {
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

    pub fn from_number_set(set: HashSet<i128>) -> ErrorSpec {

        let contains_lesser_zero: bool = set.iter().any(|&x| x < 0);
        let contains_greater_zero: bool = set.iter().any(|&x| x > 0);
        let contains_zero: bool = set.contains(&0);

        match (contains_lesser_zero, contains_zero, contains_greater_zero) {
            (false, false, false) => ErrorSpec::Empty,
            (true, false, false) => ErrorSpec::LesserZero,
            (true, true, false) => ErrorSpec::LesEqZero,
            (false, false, true) => ErrorSpec::GreaterZero,
            (false, true, true) => ErrorSpec::GrEqZero,
            (true, false, true) => ErrorSpec::NotEqZero,
            (false, true, false) => ErrorSpec::EqualZero,
            (true, true, true) => ErrorSpec::All,
            _ => ErrorSpec::Indeterminate
        }
    }

    // implementation via number sets is a bit roundabout, but easier than matching on every single possibility
    pub fn union(self, other: ErrorSpec) -> ErrorSpec {
        if self == ErrorSpec::Indeterminate || other == ErrorSpec::Indeterminate {
            return ErrorSpec::Indeterminate;
        }

        let mut as_num_set: HashSet<i128> = HashSet::new();
        if let Some(set) = self.to_number_set() {
            as_num_set.extend(set);
        }
        if let Some(set) = other.to_number_set() {
            as_num_set.extend(set);
        }
        Self::from_number_set(as_num_set)
    }

    pub fn intersection(self, other: ErrorSpec) -> ErrorSpec {
        if self == ErrorSpec::Indeterminate || other == ErrorSpec::Indeterminate {
            return ErrorSpec::Indeterminate;
        }

        let mut as_num_set: HashSet<i128> = HashSet::new();
        if let Some(self_set) = self.to_number_set()
            && let Some(other_set) = other.to_number_set()
        {
            as_num_set = self_set.intersection(&other_set).copied().collect();
        }
        Self::from_number_set(as_num_set)
    }

    pub fn without(self, other: ErrorSpec) -> ErrorSpec {
        if self == ErrorSpec::Indeterminate || other == ErrorSpec::Indeterminate {
            return ErrorSpec::Indeterminate;
        }

        if let Some(self_set) = self.to_number_set()
            && let Some(other_set) = other.to_number_set()
        {
            let difference: HashSet<i128> = self_set.difference(&other_set).copied().collect();
            Self::from_number_set(difference)
        } else {
            ErrorSpec::Indeterminate
        }
    }
}