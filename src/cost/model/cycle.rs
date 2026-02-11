use super::{CostModel, TableCost};

// ─── Cycle-Based Cost Model (OpenVM, SP1) ──────────────────────────

/// Generic cycle-based cost model for RISC-V backends (OpenVM, SP1).
/// These VMs measure cost in CPU cycles rather than per-table rows.
pub(crate) struct CycleCostModel {
    name: &'static str,
}

impl CycleCostModel {
    #[allow(dead_code)]
    pub(crate) fn openvm() -> Self {
        Self { name: "openvm" }
    }
    #[allow(dead_code)]
    pub(crate) fn sp1() -> Self {
        Self { name: "sp1" }
    }
}

impl CostModel for CycleCostModel {
    fn table_names(&self) -> &[&str] {
        &["cycles"]
    }
    fn table_short_names(&self) -> &[&str] {
        &["cyc"]
    }
    fn target_name(&self) -> &str {
        self.name
    }

    fn builtin_cost(&self, name: &str) -> TableCost {
        match name {
            "hash" => TableCost {
                processor: 400,
                ..Default::default()
            },
            "sponge_init" | "sponge_absorb" | "sponge_squeeze" => TableCost {
                processor: 200,
                ..Default::default()
            },
            "merkle_step" => TableCost {
                processor: 500,
                ..Default::default()
            },
            "split" => TableCost {
                processor: 2,
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
            processor: 4,
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
            processor: 3,
            ..Default::default()
        }
    }

    fn loop_overhead(&self) -> TableCost {
        TableCost {
            processor: 5,
            ..Default::default()
        }
    }

    fn hash_rows_per_permutation(&self) -> u64 {
        1
    }
}
