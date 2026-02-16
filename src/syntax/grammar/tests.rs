use super::trident_grammar;

#[test]
fn grammar_json_matches_existing() {
    let grammar = trident_grammar();
    let generated = grammar.to_json();
    let expected = include_str!("../../../tree-sitter/src/grammar.json");

    // Normalize both: strip trailing whitespace per line, ensure trailing newline
    let gen_lines = normalize(&generated);
    let exp_lines = normalize(expected);

    if gen_lines != exp_lines {
        // Find first difference for a useful error message
        for (i, (g, e)) in gen_lines.iter().zip(exp_lines.iter()).enumerate() {
            if g != e {
                panic!(
                    "grammar.json mismatch at line {}:\n  expected: {}\n  got:      {}",
                    i + 1,
                    e,
                    g
                );
            }
        }
        panic!(
            "grammar.json line count mismatch: expected {}, got {}",
            exp_lines.len(),
            gen_lines.len()
        );
    }
}

#[test]
fn rule_count() {
    let grammar = trident_grammar();
    // 59 rules in the existing grammar.json
    assert_eq!(
        grammar.rules.len(),
        59,
        "expected 59 grammar rules, got {}",
        grammar.rules.len()
    );
}

#[test]
fn first_and_last_rules() {
    let grammar = trident_grammar();
    assert_eq!(grammar.rules.first().map(|r| r.0), Some("source_file"));
    assert_eq!(grammar.rules.last().map(|r| r.0), Some("line_comment"));
}

#[test]
fn extras_are_whitespace_and_comments() {
    let grammar = trident_grammar();
    assert_eq!(grammar.extras.len(), 2);
}

fn normalize(s: &str) -> Vec<String> {
    s.lines().map(|l| l.trim_end().to_string()).collect()
}
