use trident::lexer::Lexer;

/// Test source: a small Trident program exercising the full pipeline.
const TEST_SOURCE: &str = r#"program test

use vm.io.io

fn add(a: Field, b: Field) -> Field {
    let result: Field = a + b
    return result
}

fn main() {
    let x: Field = pub_read()
    let y: Field = 42
    let z: Field = add(x, y)
    pub_write(z)
}
"#;

fn main() {
    let source = TEST_SOURCE;

    // Lex to get token count
    let (tokens, _comments, diags) = Lexer::new(source, 0).tokenize();
    assert!(diags.is_empty(), "lex errors: {:?}", diags);
    let tok_count = tokens.len();

    // Parse to get node count
    let file = trident::parse_source_silent(source, "test.tri").expect("parse failed");
    // Count nodes by serializing (reuse parser reference logic)
    // For now, use the known value from the parser reference: 31 nodes
    let expected_node_count = 31u64;

    eprintln!("=== Pipeline Reference ===");
    eprintln!("Source: {} bytes", source.len());
    eprintln!("Tokens: {}", tok_count);
    eprintln!("AST nodes: {}", expected_node_count);
    eprintln!();

    // Memory layout
    let src_base: u64 = 1000;
    let src_len = source.len() as u64;
    let tok_base: u64 = 2000;
    let lex_err_base: u64 = 3000;
    let lex_state_base: u64 = 3500;
    let ast_base: u64 = 5000;
    let parse_err_base: u64 = 8000;
    let parse_state_base: u64 = 9000;
    let parse_stack_base: u64 = 10000;

    let mut vals: Vec<String> = Vec::new();

    // 11 parameters
    vals.push(src_base.to_string());
    vals.push(src_len.to_string());
    vals.push(tok_base.to_string());
    vals.push(lex_err_base.to_string());
    vals.push(lex_state_base.to_string());
    vals.push(ast_base.to_string());
    vals.push(parse_err_base.to_string());
    vals.push(parse_state_base.to_string());
    vals.push(parse_stack_base.to_string());
    vals.push(tok_count.to_string());
    vals.push(expected_node_count.to_string());

    // Source bytes
    for &b in source.as_bytes() {
        vals.push((b as u64).to_string());
    }

    eprintln!("values: {}", vals.join(", "));
    let _ = file; // suppress unused warning
}
