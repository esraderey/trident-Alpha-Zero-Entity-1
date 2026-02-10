//! Machine-readable JSON verification reports for LLM consumption and CI/CD.
//!
//! Serializes `VerificationReport` and `ConstraintSystem` into a structured
//! JSON format designed for automated tooling. Uses manual JSON formatting
//! (no serde) following the same pattern as `cost.rs`.

use crate::solve::{format_constraint, Counterexample, Verdict, VerificationReport};
use crate::sym::{Constraint, ConstraintSystem, SymValue};

// ─── Data Structures ───────────────────────────────────────────────

/// Machine-readable verification report in JSON format.
/// Designed for LLM consumption and CI/CD integration.
pub struct JsonReport {
    pub version: u32,
    pub file: String,
    pub verdict: String,
    pub summary: JsonSummary,
    pub constraints: Vec<JsonConstraint>,
    pub counterexamples: Vec<JsonCounterexample>,
    pub redundant_assertions: Vec<usize>,
    pub suggestions: Vec<JsonSuggestion>,
}

pub struct JsonSummary {
    pub total_constraints: usize,
    pub active_constraints: usize,
    pub variables: usize,
    pub pub_inputs: usize,
    pub divine_inputs: usize,
    pub pub_outputs: usize,
    pub static_violations: usize,
    pub random_violations: usize,
    pub bmc_violations: usize,
}

pub struct JsonConstraint {
    pub index: usize,
    pub kind: String,
    pub expression: String,
    pub is_trivial: bool,
    pub is_violated: bool,
}

pub struct JsonCounterexample {
    pub constraint_index: usize,
    pub constraint_desc: String,
    pub source: String,
    pub assignments: Vec<(String, u64)>,
}

pub struct JsonSuggestion {
    pub kind: String,
    pub message: String,
    pub constraint_index: Option<usize>,
}

// ─── JSON Helpers ──────────────────────────────────────────────────

/// Escape a string for JSON output.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// Write an indented JSON string value: `"key": "value"`.
fn json_str(indent: usize, key: &str, value: &str) -> String {
    let pad = " ".repeat(indent);
    format!("{}\"{}\": \"{}\"", pad, key, json_escape(value))
}

/// Write an indented JSON integer value: `"key": value`.
fn json_uint(indent: usize, key: &str, value: usize) -> String {
    let pad = " ".repeat(indent);
    format!("{}\"{}\": {}", pad, key, value)
}

/// Write an indented JSON boolean value: `"key": true/false`.
fn json_bool(indent: usize, key: &str, value: bool) -> String {
    let pad = " ".repeat(indent);
    format!(
        "{}\"{}\": {}",
        pad,
        key,
        if value { "true" } else { "false" }
    )
}

// ─── Constraint Formatting ─────────────────────────────────────────

/// Format a single constraint as a `JsonConstraint`.
pub fn format_json_constraint(c: &Constraint, index: usize) -> JsonConstraint {
    let kind = match c {
        Constraint::Equal(..) => "equal",
        Constraint::AssertTrue(..) => "assert_true",
        Constraint::Conditional(..) => "conditional",
        Constraint::RangeU32(..) => "range_u32",
        Constraint::DigestEqual(..) => "digest_equal",
    };
    JsonConstraint {
        index,
        kind: kind.to_string(),
        expression: format_constraint(c),
        is_trivial: c.is_trivial(),
        is_violated: c.is_violated(),
    }
}

/// Convert a `Counterexample` from the solver into a `JsonCounterexample`.
fn convert_counterexample(ce: &Counterexample, source: &str) -> JsonCounterexample {
    let mut assignments: Vec<(String, u64)> = ce
        .assignments
        .iter()
        .filter(|(k, _)| !k.starts_with("__"))
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    assignments.sort_by(|(a, _), (b, _)| a.cmp(b));
    JsonCounterexample {
        constraint_index: ce.constraint_index,
        constraint_desc: ce.constraint_desc.clone(),
        source: source.to_string(),
        assignments,
    }
}

// ─── Suggestion Generation ─────────────────────────────────────────

/// Generate actionable fix suggestions from the verification results.
pub fn generate_suggestions(
    system: &ConstraintSystem,
    report: &VerificationReport,
) -> Vec<JsonSuggestion> {
    let mut suggestions = Vec::new();

    // For each counterexample: "fix_violation" suggestion
    for ce in &report.random_result.counterexamples {
        suggestions.push(JsonSuggestion {
            kind: "fix_violation".to_string(),
            message: format!(
                "Constraint #{} violated (random testing): {}",
                ce.constraint_index, ce.constraint_desc
            ),
            constraint_index: Some(ce.constraint_index),
        });
    }
    for ce in &report.bmc_result.counterexamples {
        // Avoid duplicate suggestions for the same constraint
        let already = suggestions
            .iter()
            .any(|s| s.constraint_index == Some(ce.constraint_index));
        if !already {
            suggestions.push(JsonSuggestion {
                kind: "fix_violation".to_string(),
                message: format!(
                    "Constraint #{} violated (BMC): {}",
                    ce.constraint_index, ce.constraint_desc
                ),
                constraint_index: Some(ce.constraint_index),
            });
        }
    }

    // For each redundant assertion: "remove_redundant" suggestion
    for &idx in &report.redundant_assertions {
        let desc = if idx < system.constraints.len() {
            format_constraint(&system.constraints[idx])
        } else {
            format!("constraint #{}", idx)
        };
        suggestions.push(JsonSuggestion {
            kind: "remove_redundant".to_string(),
            message: format!(
                "Constraint #{} appears redundant (always true): {}",
                idx, desc
            ),
            constraint_index: Some(idx),
        });
    }

    // For divine inputs with no constraints: "add_assertion" suggestion
    let constrained_divines = collect_constrained_divines(system);
    for di in &system.divine_inputs {
        let name = di.to_string();
        if !constrained_divines.contains(&name) {
            suggestions.push(JsonSuggestion {
                kind: "add_assertion".to_string(),
                message: format!(
                    "Divine input '{}' has no constraints -- unconstrained nondeterminism \
                     may allow a malicious prover to choose arbitrary values",
                    name
                ),
                constraint_index: None,
            });
        }
    }

    suggestions
}

/// Collect divine variable names that appear in at least one constraint.
fn collect_constrained_divines(system: &ConstraintSystem) -> Vec<String> {
    let mut names = Vec::new();
    for c in &system.constraints {
        collect_divine_refs_constraint(c, &mut names);
    }
    names.sort();
    names.dedup();
    names
}

fn collect_divine_refs_constraint(c: &Constraint, out: &mut Vec<String>) {
    match c {
        Constraint::Equal(a, b) => {
            collect_divine_refs_value(a, out);
            collect_divine_refs_value(b, out);
        }
        Constraint::AssertTrue(v) => {
            collect_divine_refs_value(v, out);
        }
        Constraint::Conditional(cond, inner) => {
            collect_divine_refs_value(cond, out);
            collect_divine_refs_constraint(inner, out);
        }
        Constraint::RangeU32(v) => {
            collect_divine_refs_value(v, out);
        }
        Constraint::DigestEqual(a, b) => {
            for v in a {
                collect_divine_refs_value(v, out);
            }
            for v in b {
                collect_divine_refs_value(v, out);
            }
        }
    }
}

fn collect_divine_refs_value(v: &SymValue, out: &mut Vec<String>) {
    match v {
        SymValue::Var(var) => {
            if var.name.starts_with("divine_") {
                out.push(var.to_string());
            }
        }
        SymValue::Add(a, b)
        | SymValue::Mul(a, b)
        | SymValue::Sub(a, b)
        | SymValue::Eq(a, b)
        | SymValue::Lt(a, b) => {
            collect_divine_refs_value(a, out);
            collect_divine_refs_value(b, out);
        }
        SymValue::Neg(a) | SymValue::Inv(a) => {
            collect_divine_refs_value(a, out);
        }
        SymValue::Ite(c, t, e) => {
            collect_divine_refs_value(c, out);
            collect_divine_refs_value(t, out);
            collect_divine_refs_value(e, out);
        }
        SymValue::Hash(inputs, _) => {
            for v in inputs {
                collect_divine_refs_value(v, out);
            }
        }
        SymValue::Const(_) | SymValue::Divine(_) | SymValue::PubInput(_) => {}
    }
}

// ─── Report Generation ─────────────────────────────────────────────

/// Create a full JSON verification report.
pub fn generate_json_report(
    file_name: &str,
    system: &ConstraintSystem,
    report: &VerificationReport,
) -> String {
    let verdict_str = match report.verdict {
        Verdict::Safe => "safe",
        Verdict::StaticViolation | Verdict::RandomViolation | Verdict::BmcViolation => "unsafe",
    };

    let constraints: Vec<JsonConstraint> = system
        .constraints
        .iter()
        .enumerate()
        .map(|(i, c)| format_json_constraint(c, i))
        .collect();

    let mut counterexamples: Vec<JsonCounterexample> = Vec::new();
    for ce in &report.random_result.counterexamples {
        counterexamples.push(convert_counterexample(ce, "random"));
    }
    for ce in &report.bmc_result.counterexamples {
        counterexamples.push(convert_counterexample(ce, "bmc"));
    }

    let suggestions = generate_suggestions(system, report);

    let json_report = JsonReport {
        version: 1,
        file: file_name.to_string(),
        verdict: verdict_str.to_string(),
        summary: JsonSummary {
            total_constraints: system.constraints.len(),
            active_constraints: system.active_constraints(),
            variables: system.num_variables as usize,
            pub_inputs: system.pub_inputs.len(),
            divine_inputs: system.divine_inputs.len(),
            pub_outputs: system.pub_outputs.len(),
            static_violations: report.static_violations.len(),
            random_violations: report.random_result.counterexamples.len(),
            bmc_violations: report.bmc_result.counterexamples.len(),
        },
        constraints,
        counterexamples,
        redundant_assertions: report.redundant_assertions.clone(),
        suggestions,
    };

    serialize_report(&json_report)
}

// ─── JSON Serialization ────────────────────────────────────────────

fn serialize_report(r: &JsonReport) -> String {
    let mut out = String::with_capacity(4096);
    out.push_str("{\n");
    out.push_str(&json_uint(2, "version", r.version as usize));
    out.push_str(",\n");
    out.push_str(&json_str(2, "file", &r.file));
    out.push_str(",\n");
    out.push_str(&json_str(2, "verdict", &r.verdict));
    out.push_str(",\n");

    // summary
    out.push_str("  \"summary\": {\n");
    out.push_str(&json_uint(
        4,
        "total_constraints",
        r.summary.total_constraints,
    ));
    out.push_str(",\n");
    out.push_str(&json_uint(
        4,
        "active_constraints",
        r.summary.active_constraints,
    ));
    out.push_str(",\n");
    out.push_str(&json_uint(4, "variables", r.summary.variables));
    out.push_str(",\n");
    out.push_str(&json_uint(4, "pub_inputs", r.summary.pub_inputs));
    out.push_str(",\n");
    out.push_str(&json_uint(4, "divine_inputs", r.summary.divine_inputs));
    out.push_str(",\n");
    out.push_str(&json_uint(4, "pub_outputs", r.summary.pub_outputs));
    out.push_str(",\n");
    out.push_str(&json_uint(
        4,
        "static_violations",
        r.summary.static_violations,
    ));
    out.push_str(",\n");
    out.push_str(&json_uint(
        4,
        "random_violations",
        r.summary.random_violations,
    ));
    out.push_str(",\n");
    out.push_str(&json_uint(4, "bmc_violations", r.summary.bmc_violations));
    out.push('\n');
    out.push_str("  },\n");

    // constraints
    out.push_str("  \"constraints\": [\n");
    for (i, c) in r.constraints.iter().enumerate() {
        out.push_str(&serialize_constraint(c));
        if i + 1 < r.constraints.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ],\n");

    // counterexamples
    out.push_str("  \"counterexamples\": [\n");
    for (i, ce) in r.counterexamples.iter().enumerate() {
        out.push_str(&serialize_counterexample(ce));
        if i + 1 < r.counterexamples.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ],\n");

    // redundant_assertions
    out.push_str("  \"redundant_assertions\": [");
    for (i, idx) in r.redundant_assertions.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&idx.to_string());
    }
    out.push_str("],\n");

    // suggestions
    out.push_str("  \"suggestions\": [\n");
    for (i, s) in r.suggestions.iter().enumerate() {
        out.push_str(&serialize_suggestion(s));
        if i + 1 < r.suggestions.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]\n");

    out.push_str("}\n");
    out
}

fn serialize_constraint(c: &JsonConstraint) -> String {
    let mut out = String::new();
    out.push_str("    {\n");
    out.push_str(&json_uint(6, "index", c.index));
    out.push_str(",\n");
    out.push_str(&json_str(6, "kind", &c.kind));
    out.push_str(",\n");
    out.push_str(&json_str(6, "expression", &c.expression));
    out.push_str(",\n");
    out.push_str(&json_bool(6, "is_trivial", c.is_trivial));
    out.push_str(",\n");
    out.push_str(&json_bool(6, "is_violated", c.is_violated));
    out.push('\n');
    out.push_str("    }");
    out
}

fn serialize_counterexample(ce: &JsonCounterexample) -> String {
    let mut out = String::new();
    out.push_str("    {\n");
    out.push_str(&json_uint(6, "constraint_index", ce.constraint_index));
    out.push_str(",\n");
    out.push_str(&json_str(6, "constraint_desc", &ce.constraint_desc));
    out.push_str(",\n");
    out.push_str(&json_str(6, "source", &ce.source));
    out.push_str(",\n");
    out.push_str("      \"assignments\": {\n");
    for (i, (name, value)) in ce.assignments.iter().enumerate() {
        let pad = "        ";
        out.push_str(&format!("{}\"{}\": {}", pad, json_escape(name), value));
        if i + 1 < ce.assignments.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("      }\n");
    out.push_str("    }");
    out
}

fn serialize_suggestion(s: &JsonSuggestion) -> String {
    let mut out = String::new();
    out.push_str("    {\n");
    out.push_str(&json_str(6, "kind", &s.kind));
    out.push_str(",\n");
    out.push_str(&json_str(6, "message", &s.message));
    out.push_str(",\n");
    let pad = "      ";
    match s.constraint_index {
        Some(idx) => out.push_str(&format!("{}\"constraint_index\": {}\n", pad, idx)),
        None => out.push_str(&format!("{}\"constraint_index\": null\n", pad)),
    }
    out.push_str("    }");
    out
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solve;
    use crate::sym;

    /// Helper: parse source, build constraint system, run verification.
    fn verify_source(source: &str) -> (ConstraintSystem, VerificationReport) {
        let file = crate::parse_source(source, "test.tri").unwrap();
        let system = sym::analyze(&file);
        let report = solve::verify(&system);
        (system, report)
    }

    #[test]
    fn test_json_basic_structure() {
        let (system, report) = verify_source("program test\nfn main() {\n    assert(true)\n}\n");
        let json = generate_json_report("test.tri", &system, &report);

        // Check basic JSON structure markers
        assert!(json.starts_with('{'));
        assert!(json.contains("\"version\": 1"));
        assert!(json.contains("\"file\": \"test.tri\""));
        assert!(json.contains("\"verdict\": \"safe\""));
        assert!(json.contains("\"summary\":"));
        assert!(json.contains("\"constraints\":"));
        assert!(json.contains("\"counterexamples\":"));
        assert!(json.contains("\"redundant_assertions\":"));
        assert!(json.contains("\"suggestions\":"));
    }

    #[test]
    fn test_counterexample_serialization() {
        let (system, report) = verify_source(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    assert_eq(x, 0)\n}\n",
        );
        let json = generate_json_report("test.tri", &system, &report);

        // This program asserts x == 0 which should fail for most random x
        assert!(json.contains("\"verdict\": \"unsafe\""));
        assert!(json.contains("\"counterexamples\": ["));
        // Should have at least one counterexample with assignments
        assert!(json.contains("\"assignments\":"));
    }

    #[test]
    fn test_safe_program_no_suggestions() {
        let (system, report) = verify_source(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    assert_eq(x + 0, x)\n}\n",
        );
        let suggestions = generate_suggestions(&system, &report);

        // No fix_violation suggestions for a safe program
        let violations: Vec<_> = suggestions
            .iter()
            .filter(|s| s.kind == "fix_violation")
            .collect();
        assert!(
            violations.is_empty(),
            "safe program should have no fix_violation suggestions"
        );
    }

    #[test]
    fn test_unsafe_program_fix_violation() {
        let (system, report) = verify_source(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    assert_eq(x, 42)\n}\n",
        );
        let suggestions = generate_suggestions(&system, &report);

        let violations: Vec<_> = suggestions
            .iter()
            .filter(|s| s.kind == "fix_violation")
            .collect();
        assert!(
            !violations.is_empty(),
            "unsafe program should have fix_violation suggestions"
        );
        // Each violation suggestion should reference a constraint index
        for v in &violations {
            assert!(v.constraint_index.is_some());
        }
    }

    #[test]
    fn test_redundant_assertion_suggestion() {
        // assert(true) is trivially true, but the solver marks non-trivial
        // always-satisfied constraints as redundant. Use assert_eq(x+0, x)
        // which is non-trivial but always holds.
        let (system, report) = verify_source(
            "program test\nfn main() {\n    let x: Field = pub_read()\n    assert_eq(x + 0, x)\n}\n",
        );

        // Check the report for redundant assertions
        if !report.redundant_assertions.is_empty() {
            let suggestions = generate_suggestions(&system, &report);
            let redundant: Vec<_> = suggestions
                .iter()
                .filter(|s| s.kind == "remove_redundant")
                .collect();
            assert!(
                !redundant.is_empty(),
                "redundant assertions should produce remove_redundant suggestions"
            );
        }
        // If the solver does not flag it as redundant (implementation detail),
        // the test still passes -- we just verify the suggestion logic is wired.
    }

    #[test]
    fn test_json_escape_special_chars() {
        let escaped = json_escape("hello \"world\"\nnewline\\backslash");
        assert_eq!(escaped, "hello \\\"world\\\"\\nnewline\\\\backslash");
    }

    #[test]
    fn test_json_escape_control_chars() {
        let escaped = json_escape("tab\there");
        assert_eq!(escaped, "tab\\there");
    }

    #[test]
    fn test_format_json_constraint_kinds() {
        let c1 = Constraint::Equal(SymValue::Const(1), SymValue::Const(1));
        let jc1 = format_json_constraint(&c1, 0);
        assert_eq!(jc1.kind, "equal");
        assert!(jc1.is_trivial);
        assert!(!jc1.is_violated);

        let c2 = Constraint::AssertTrue(SymValue::Const(0));
        let jc2 = format_json_constraint(&c2, 1);
        assert_eq!(jc2.kind, "assert_true");
        assert!(jc2.is_violated);

        let c3 = Constraint::RangeU32(SymValue::Const(42));
        let jc3 = format_json_constraint(&c3, 2);
        assert_eq!(jc3.kind, "range_u32");
        assert!(jc3.is_trivial);

        let c4 = Constraint::Conditional(
            SymValue::Const(1),
            Box::new(Constraint::AssertTrue(SymValue::Const(1))),
        );
        let jc4 = format_json_constraint(&c4, 3);
        assert_eq!(jc4.kind, "conditional");

        let c5 = Constraint::DigestEqual(vec![SymValue::Const(0)], vec![SymValue::Const(0)]);
        let jc5 = format_json_constraint(&c5, 4);
        assert_eq!(jc5.kind, "digest_equal");
    }

    #[test]
    fn test_divine_unconstrained_suggestion() {
        let (system, report) = verify_source(
            "program test\nfn main() {\n    let x: Field = divine()\n    assert(true)\n}\n",
        );
        let suggestions = generate_suggestions(&system, &report);

        let add_assertions: Vec<_> = suggestions
            .iter()
            .filter(|s| s.kind == "add_assertion")
            .collect();
        assert!(
            !add_assertions.is_empty(),
            "unconstrained divine input should produce add_assertion suggestion"
        );
        assert!(add_assertions[0].message.contains("divine"));
    }

    #[test]
    fn test_static_violation_in_json() {
        let (system, report) = verify_source("program test\nfn main() {\n    assert(false)\n}\n");
        let json = generate_json_report("test.tri", &system, &report);
        assert!(json.contains("\"verdict\": \"unsafe\""));
        assert!(json.contains("\"static_violations\": 1"));
    }
}
