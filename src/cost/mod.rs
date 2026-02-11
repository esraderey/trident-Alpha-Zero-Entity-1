pub mod analyzer;
/// Static cost analysis for Trident programs.
///
/// Computes the trace heights of all Algebraic Execution Tables for the
/// configured target VM by walking the AST and summing per-instruction costs.
/// This gives an upper bound on proving cost without executing the program.
pub mod model;
pub mod report;

// Public re-exports
pub use analyzer::ProgramCost;
pub use model::TableCost;

// Crate-internal re-exports
#[allow(unused_imports)]
pub(crate) use analyzer::{next_power_of_two, CostAnalyzer, FunctionCost};
pub(crate) use model::cost_builtin;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn analyze(source: &str) -> ProgramCost {
        let (tokens, _, _) = Lexer::new(source, 0).tokenize();
        let file = Parser::new(tokens).parse_file().unwrap();
        CostAnalyzer::new().analyze_file(&file)
    }

    #[test]
    fn test_next_power_of_two() {
        assert_eq!(next_power_of_two(0), 1);
        assert_eq!(next_power_of_two(1), 1);
        assert_eq!(next_power_of_two(2), 2);
        assert_eq!(next_power_of_two(3), 4);
        assert_eq!(next_power_of_two(1023), 1024);
        assert_eq!(next_power_of_two(1024), 1024);
        assert_eq!(next_power_of_two(1025), 2048);
    }

    #[test]
    fn test_simple_program_cost() {
        let cost = analyze(
            "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    let c: Field = a + b\n    pub_write(c)\n}",
        );
        // pub_read: 1cc + 1opstack each (x2)
        // a + b: dup a (1cc) + dup b (1cc) + add (1cc + 1opstack)
        // pub_write: dup c (1cc) + write_io (1cc + 1opstack)
        // let bindings: 1cc each (x3)
        assert!(cost.total.processor > 0);
        assert_eq!(cost.total.hash, 0);
        assert_eq!(cost.total.u32_table, 0);
        assert_eq!(cost.total.ram, 0);
        eprintln!(
            "Simple program cost: cc={}, opstack={}",
            cost.total.processor, cost.total.op_stack
        );
    }

    #[test]
    fn test_hash_dominates() {
        let cost = analyze(
            "program test\nfn main() {\n    let d: Digest = divine5()\n    let h: Digest = hash(d)\n    pub_write(h)\n}",
        );
        // hash: 6 hash table rows
        assert!(cost.total.hash >= 6);
        // If hash table is the tallest, dominant should be "hash"
        if cost.total.hash > cost.total.processor {
            assert_eq!(cost.total.dominant_table(), "hash");
        }
        eprintln!(
            "Hash program: cc={}, hash={}",
            cost.total.processor, cost.total.hash
        );
    }

    #[test]
    fn test_loop_cost_multiplied() {
        let cost = analyze(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    for i in 0..10 {\n        pub_write(x)\n    }\n}",
        );
        // Loop body: dup x (1cc) + write_io (1cc) = 2cc + overhead per iteration
        // 10 iterations, so total loop cost should be significantly > 10
        assert!(
            cost.total.processor >= 10,
            "loop cost should be at least 10 cc, got {}",
            cost.total.processor
        );
        eprintln!("Loop program: cc={}", cost.total.processor);
    }

    #[test]
    fn test_if_else_worst_case() {
        // Then branch is more expensive (has hash), so cost should include hash cost.
        let cost = analyze(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    if x == x {\n        let d: Digest = divine5()\n        let h: Digest = hash(d)\n    }\n}",
        );
        // If branch has hash (6 rows), else is empty.
        assert!(
            cost.total.hash >= 6,
            "if-branch hash cost should be included, got {}",
            cost.total.hash
        );
    }

    #[test]
    fn test_function_call_cost() {
        let cost = analyze(
            "program test\nfn double(x: Field) -> Field {\n    x + x\n}\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = double(a)\n    pub_write(b)\n}",
        );
        // Function call adds CALL_OVERHEAD (2cc, 2 jump_stack)
        assert!(
            cost.total.jump_stack >= 2,
            "function call should contribute to jump_stack"
        );
        eprintln!(
            "Call program: cc={}, jump={}",
            cost.total.processor, cost.total.jump_stack
        );
    }

    #[test]
    fn test_padded_height() {
        let cost = analyze(
            "program test\nfn main() {\n    let a: Field = pub_read()\n    pub_write(a)\n}",
        );
        // Padded height should be a power of 2.
        assert!(cost.padded_height.is_power_of_two());
        assert!(cost.padded_height >= cost.total.max_height());
    }

    #[test]
    fn test_cost_report_format() {
        let cost = analyze(
            "program test\nfn main() {\n    let a: Field = pub_read()\n    pub_write(a)\n}",
        );
        let report = cost.format_report();
        assert!(report.contains("Cost report:"));
        assert!(report.contains("TOTAL"));
        assert!(report.contains("Padded height:"));
        eprintln!("{}", report);
    }

    #[test]
    fn test_u32_cost() {
        let cost = analyze(
            "program test\nfn main() {\n    let a: Field = pub_read()\n    let b: Field = pub_read()\n    assert(a < b)\n}",
        );
        // lt uses u32 table
        assert!(
            cost.total.u32_table > 0,
            "lt should contribute to u32 table"
        );
    }

    #[test]
    fn test_emit_cost_no_hash() {
        let cost = analyze(
            "program test\nevent Ev { x: Field, y: Field }\nfn main() {\n    emit Ev { x: pub_read(), y: pub_read() }\n}",
        );
        // Open emit should have zero hash cost (no hashing)
        assert_eq!(cost.total.hash, 0, "open emit should have zero hash cost");
        assert!(cost.total.processor > 0);
    }

    #[test]
    fn test_seal_cost_has_hash() {
        let cost = analyze(
            "program test\nevent Ev { x: Field, y: Field }\nfn main() {\n    seal Ev { x: pub_read(), y: pub_read() }\n}",
        );
        // Seal should have hash cost (>= 6 rows for one hash)
        assert!(
            cost.total.hash >= 6,
            "seal should have hash cost >= 6, got {}",
            cost.total.hash
        );
    }

    #[test]
    fn test_boundary_warning_when_close() {
        // Construct a ProgramCost near the boundary
        let cost = ProgramCost {
            program_name: "test".to_string(),
            functions: Vec::new(),
            total: TableCost {
                processor: 1020,
                hash: 0,
                u32_table: 0,
                op_stack: 0,
                ram: 0,
                jump_stack: 0,
            },
            attestation_hash_rows: 0,
            padded_height: 1024,
            estimated_proving_secs: 0.0,
            loop_bound_waste: Vec::new(),
        };
        let warnings = cost.boundary_warnings();
        assert_eq!(warnings.len(), 1, "should warn when 4 rows from boundary");
        assert!(warnings[0].message.contains("4 rows below"));
    }

    #[test]
    fn test_h0001_hash_table_dominance() {
        let cost = ProgramCost {
            program_name: "test".to_string(),
            functions: Vec::new(),
            total: TableCost {
                processor: 10,
                hash: 60,
                u32_table: 0,
                op_stack: 0,
                ram: 0,
                jump_stack: 0,
            },
            attestation_hash_rows: 0,
            padded_height: 64,
            estimated_proving_secs: 0.0,
            loop_bound_waste: Vec::new(),
        };
        let hints = cost.optimization_hints();
        assert!(
            hints.iter().any(|h| h.message.contains("H0001")),
            "should emit H0001 when hash is 6x processor"
        );
    }

    #[test]
    fn test_h0002_headroom_hint() {
        let cost = ProgramCost {
            program_name: "test".to_string(),
            functions: Vec::new(),
            total: TableCost {
                processor: 500,
                hash: 0,
                u32_table: 0,
                op_stack: 0,
                ram: 0,
                jump_stack: 0,
            },
            attestation_hash_rows: 0,
            padded_height: 1024,
            estimated_proving_secs: 0.0,
            loop_bound_waste: Vec::new(),
        };
        let hints = cost.optimization_hints();
        assert!(
            hints.iter().any(|h| h.message.contains("H0002")),
            "should emit H0002 when >25% headroom"
        );
    }

    #[test]
    fn test_no_boundary_warning_when_far() {
        let cost = ProgramCost {
            program_name: "test".to_string(),
            functions: Vec::new(),
            total: TableCost {
                processor: 500,
                hash: 0,
                u32_table: 0,
                op_stack: 0,
                ram: 0,
                jump_stack: 0,
            },
            attestation_hash_rows: 0,
            padded_height: 1024,
            estimated_proving_secs: 0.0,
            loop_bound_waste: Vec::new(),
        };
        let warnings = cost.boundary_warnings();
        assert!(
            warnings.is_empty(),
            "should not warn when far from boundary"
        );
    }

    #[test]
    fn test_h0004_loop_bound_waste() {
        // Loop with bound 128 but only 10 iterations — should warn
        let cost = analyze(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    for i in 0..10 bounded 128 {\n        pub_write(x)\n    }\n}",
        );
        let hints = cost.optimization_hints();
        let h0004 = hints.iter().any(|h| h.message.contains("H0004"));
        assert!(
            h0004,
            "expected H0004 for bound 128 >> end 10, got: {:?}",
            hints
        );
    }

    #[test]
    fn test_h0004_no_waste_when_tight() {
        // Loop with bound close to end — should NOT warn
        let cost = analyze(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    for i in 0..10 bounded 16 {\n        pub_write(x)\n    }\n}",
        );
        let hints = cost.optimization_hints();
        let h0004 = hints.iter().any(|h| h.message.contains("H0004"));
        assert!(!h0004, "should not warn when bound is close to end");
    }

    #[test]
    fn test_asm_block_cost() {
        let cost = analyze(
            "program test\nfn main() {\n    asm {\n        push 1\n        push 2\n        add\n    }\n}",
        );
        // 3 instruction lines → at least 3 processor cycles
        assert!(
            cost.total.processor >= 3,
            "asm block with 3 instructions should cost at least 3 cc, got {}",
            cost.total.processor
        );
    }

    #[test]
    fn test_asm_block_comments_not_counted() {
        let cost = analyze(
            "program test\nfn main() {\n    asm {\n        // this is a comment\n        push 1\n    }\n}",
        );
        // Only 1 real instruction, comment should not count
        assert!(
            cost.total.processor >= 1,
            "asm block cost should count only instructions"
        );
    }

    #[test]
    fn test_stmt_costs_lines() {
        let source =
            "program test\n\nfn main() {\n    let x: Field = pub_read()\n    pub_write(x)\n}\n";
        let (tokens, _, _) = Lexer::new(source, 0).tokenize();
        let file = Parser::new(tokens).parse_file().unwrap();
        let mut analyzer = CostAnalyzer::new();
        // Populate fn_bodies for cost_fn
        analyzer.analyze_file(&file);
        let costs = analyzer.stmt_costs(&file, source);

        // Should have entries for the fn header (line 3) and each statement
        assert!(
            !costs.is_empty(),
            "stmt_costs should return non-empty results"
        );

        // fn main() is on line 3
        assert!(
            costs.iter().any(|(line, _)| *line == 3),
            "should have a cost entry for fn main() on line 3, got lines: {:?}",
            costs.iter().map(|(l, _)| l).collect::<Vec<_>>()
        );

        // let x = pub_read() is on line 4
        assert!(
            costs.iter().any(|(line, _)| *line == 4),
            "should have a cost entry for let statement on line 4"
        );

        // pub_write(x) is on line 5
        assert!(
            costs.iter().any(|(line, _)| *line == 5),
            "should have a cost entry for pub_write on line 5"
        );

        // Verify all costs have non-zero processor count
        for (line, cost) in &costs {
            if *line >= 3 && *line <= 5 {
                assert!(
                    cost.processor > 0 || cost.jump_stack > 0,
                    "line {} should have non-zero cost",
                    line
                );
            }
        }
    }

    #[test]
    fn test_cost_json_roundtrip() {
        let original = TableCost {
            processor: 10,
            hash: 6,
            u32_table: 33,
            op_stack: 8,
            ram: 5,
            jump_stack: 2,
        };
        let json = original.to_json_value();
        let parsed = TableCost::from_json_value(&json).expect("should parse JSON");
        assert_eq!(parsed.processor, original.processor);
        assert_eq!(parsed.hash, original.hash);
        assert_eq!(parsed.u32_table, original.u32_table);
        assert_eq!(parsed.op_stack, original.op_stack);
        assert_eq!(parsed.ram, original.ram);
        assert_eq!(parsed.jump_stack, original.jump_stack);
    }

    #[test]
    fn test_program_cost_json_roundtrip() {
        let cost = analyze(
            "program test\nfn helper(x: Field) -> Field {\n    x + x\n}\nfn main() {\n    let x: Field = pub_read()\n    pub_write(helper(x))\n}",
        );
        let json = cost.to_json();
        let parsed = ProgramCost::from_json(&json).expect("should parse program cost JSON");
        assert_eq!(parsed.total.processor, cost.total.processor);
        assert_eq!(parsed.total.hash, cost.total.hash);
        assert_eq!(parsed.padded_height, cost.padded_height);
        assert_eq!(parsed.functions.len(), cost.functions.len());
        for (orig, loaded) in cost.functions.iter().zip(parsed.functions.iter()) {
            assert_eq!(orig.name, loaded.name);
            assert_eq!(orig.cost.processor, loaded.cost.processor);
        }
    }

    #[test]
    fn test_comparison_format() {
        let old_cost = ProgramCost {
            program_name: "test".to_string(),
            functions: vec![
                FunctionCost {
                    name: "main".to_string(),
                    cost: TableCost {
                        processor: 10,
                        hash: 6,
                        u32_table: 0,
                        op_stack: 8,
                        ram: 0,
                        jump_stack: 2,
                    },
                    per_iteration: None,
                },
                FunctionCost {
                    name: "helper".to_string(),
                    cost: TableCost {
                        processor: 5,
                        hash: 0,
                        u32_table: 0,
                        op_stack: 3,
                        ram: 0,
                        jump_stack: 0,
                    },
                    per_iteration: None,
                },
            ],
            total: TableCost {
                processor: 15,
                hash: 6,
                u32_table: 0,
                op_stack: 11,
                ram: 0,
                jump_stack: 2,
            },
            attestation_hash_rows: 0,
            padded_height: 32,
            estimated_proving_secs: 0.0,
            loop_bound_waste: Vec::new(),
        };

        let new_cost = ProgramCost {
            program_name: "test".to_string(),
            functions: vec![
                FunctionCost {
                    name: "main".to_string(),
                    cost: TableCost {
                        processor: 12,
                        hash: 6,
                        u32_table: 0,
                        op_stack: 10,
                        ram: 0,
                        jump_stack: 2,
                    },
                    per_iteration: None,
                },
                FunctionCost {
                    name: "helper".to_string(),
                    cost: TableCost {
                        processor: 5,
                        hash: 0,
                        u32_table: 0,
                        op_stack: 3,
                        ram: 0,
                        jump_stack: 0,
                    },
                    per_iteration: None,
                },
            ],
            total: TableCost {
                processor: 17,
                hash: 6,
                u32_table: 0,
                op_stack: 13,
                ram: 0,
                jump_stack: 2,
            },
            attestation_hash_rows: 0,
            padded_height: 32,
            estimated_proving_secs: 0.0,
            loop_bound_waste: Vec::new(),
        };

        let comparison = old_cost.format_comparison(&new_cost);
        assert!(
            comparison.contains("Cost comparison:"),
            "should contain header"
        );
        assert!(comparison.contains("main"), "should contain function name");
        assert!(
            comparison.contains("helper"),
            "should contain helper function"
        );
        assert!(comparison.contains("TOTAL"), "should contain TOTAL");
        assert!(
            comparison.contains("+2"),
            "should show +2 delta for main and total"
        );
        assert!(comparison.contains("0"), "should show 0 delta for helper");
        assert!(
            comparison.contains("Padded height:"),
            "should contain padded height"
        );
    }
}
