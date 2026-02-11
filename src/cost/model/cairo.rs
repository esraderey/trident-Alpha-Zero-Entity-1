use super::{CostModel, TableCost};

// ─── Cairo Cost Model ──────────────────────────────────────────────

/// Cairo cost model — measures in steps and builtin invocations.
pub(crate) struct CairoCostModel;

impl CostModel for CairoCostModel {
    fn table_names(&self) -> &[&str] {
        &["steps", "builtins"]
    }
    fn table_short_names(&self) -> &[&str] {
        &["stp", "blt"]
    }
    fn target_name(&self) -> &str {
        "cairo"
    }

    fn builtin_cost(&self, name: &str) -> TableCost {
        match name {
            "hash" | "pedersen" => TableCost {
                processor: 3,
                hash: 1,
                ..Default::default()
            },
            "sponge_init" | "sponge_absorb" | "sponge_squeeze" => TableCost {
                processor: 5,
                hash: 1,
                ..Default::default()
            },
            _ => TableCost {
                processor: 1,
                ..Default::default()
            },
        }
    }

    fn binop_cost(&self, _op: &crate::ast::BinOp) -> TableCost {
        TableCost {
            processor: 1,
            ..Default::default()
        }
    }

    fn call_overhead(&self) -> TableCost {
        TableCost {
            processor: 2,
            ..Default::default()
        }
    }

    fn stack_op(&self) -> TableCost {
        TableCost {
            processor: 1,
            ..Default::default()
        }
    }

    fn if_overhead(&self) -> TableCost {
        TableCost {
            processor: 2,
            ..Default::default()
        }
    }

    fn loop_overhead(&self) -> TableCost {
        TableCost {
            processor: 4,
            ..Default::default()
        }
    }

    fn hash_rows_per_permutation(&self) -> u64 {
        1
    }
}
