use std::collections::HashSet;

use crate::{error_check_spec_generation::spec_generation::RVCheckFinder, utils::ret_val_check::ReturnValueCheck::*};


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
    
    pub fn opposite(self) -> ReturnValueCheck {
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

    pub fn to_number_set(self) -> Option<HashSet<i8>> {
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

    pub fn from_number_set(set: HashSet<i8>) -> ReturnValueCheck {
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
    pub fn union(self, other: ReturnValueCheck) -> ReturnValueCheck {
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

    pub fn intersection(self, other: ReturnValueCheck) -> ReturnValueCheck {
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

    pub fn without(self, other: ReturnValueCheck) -> ReturnValueCheck {
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