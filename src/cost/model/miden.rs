use super::{CostModel, TableCost};

// ─── Miden Cost Model ──────────────────────────────────────────────

/// Miden VM cost model — simplified 4-table model.
pub(crate) struct MidenCostModel;

impl CostModel for MidenCostModel {
    fn table_names(&self) -> &[&str] {
        &["processor", "hash", "chiplets", "stack"]
    }
    fn table_short_names(&self) -> &[&str] {
        &["cc", "hash", "chip", "stk"]
    }
    fn target_name(&self) -> &str {
        "miden"
    }

    fn builtin_cost(&self, name: &str) -> TableCost {
        match name {
            "hash" | "hperm" => TableCost {
                processor: 1,
                hash: 8,
                ..Default::default()
            },
            "split" | "u32split" => TableCost {
                processor: 1,
                u32_table: 16,
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
            op_stack: 2,
            ..Default::default()
        }
    }

    fn call_overhead(&self) -> TableCost {
        TableCost {
            processor: 2,
            jump_stack: 2,
            ..Default::default()
        }
    }

    fn stack_op(&self) -> TableCost {
        TableCost {
            processor: 1,
            op_stack: 1,
            ..Default::default()
        }
    }

    fn if_overhead(&self) -> TableCost {
        TableCost {
            processor: 2,
            op_stack: 1,
            ..Default::default()
        }
    }

    fn loop_overhead(&self) -> TableCost {
        TableCost {
            processor: 3,
            op_stack: 1,
            ..Default::default()
        }
    }

    fn hash_rows_per_permutation(&self) -> u64 {
        8
    }
}
