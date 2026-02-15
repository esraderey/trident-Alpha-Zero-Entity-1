//! LSP utility functions: position/offset conversion, word extraction,
//! type formatting, and call context analysis.

use tower_lsp::lsp_types::*;

use crate::ast;
use crate::span::Span;

pub fn to_lsp_diagnostic(diag: &crate::diagnostic::Diagnostic, source: &str) -> Diagnostic {
    let start = byte_offset_to_position(source, diag.span.start as usize);
    let end = byte_offset_to_position(source, diag.span.end as usize);

    let severity = match diag.severity {
        crate::diagnostic::Severity::Error => DiagnosticSeverity::ERROR,
        crate::diagnostic::Severity::Warning => DiagnosticSeverity::WARNING,
    };

    let mut message = diag.message.clone();
    for note in &diag.notes {
        message.push_str("\nnote: ");
        message.push_str(note);
    }
    if let Some(help) = &diag.help {
        message.push_str("\nhelp: ");
        message.push_str(help);
    }

    Diagnostic {
        range: Range::new(start, end),
        severity: Some(severity),
        source: Some("trident".to_string()),
        message,
        ..Default::default()
    }
}

pub fn byte_offset_to_position(source: &str, offset: usize) -> Position {
    let offset = offset.min(source.len());
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += ch.len_utf16() as u32;
        }
    }
    Position::new(line, col)
}

pub fn span_to_range(source: &str, span: Span) -> Range {
    Range::new(
        byte_offset_to_position(source, span.start as usize),
        byte_offset_to_position(source, span.end as usize),
    )
}

/// Extract the word (identifier) at a given cursor position.
pub fn word_at_position(source: &str, pos: Position) -> String {
    let Some(offset) = position_to_byte_offset(source, pos) else {
        return String::new();
    };

    let bytes = source.as_bytes();
    let mut start = offset;
    while start > 0 && is_ident_char(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = offset;
    while end < bytes.len() && is_ident_char(bytes[end]) {
        end += 1;
    }

    // Include dot for qualified names like "hash.tip5"
    if start > 0 && bytes[start - 1] == b'.' {
        let mut dot_start = start - 1;
        while dot_start > 0 && is_ident_char(bytes[dot_start - 1]) {
            dot_start -= 1;
        }
        source[dot_start..end].to_string()
    } else if end < bytes.len() && bytes[end] == b'.' {
        let mut dot_end = end + 1;
        while dot_end < bytes.len() && is_ident_char(bytes[dot_end]) {
            dot_end += 1;
        }
        source[start..dot_end].to_string()
    } else {
        source[start..end].to_string()
    }
}

/// Check if there's a dot before the cursor and return the module prefix.
pub fn text_before_dot(source: &str, pos: Position) -> Option<String> {
    let offset = position_to_byte_offset(source, pos)?;
    let bytes = source.as_bytes();

    let mut i = offset;
    while i > 0 && is_ident_char(bytes[i - 1]) {
        i -= 1;
    }
    if i > 0 && bytes[i - 1] == b'.' {
        let dot_pos = i - 1;
        let mut start = dot_pos;
        while start > 0 && is_ident_char(bytes[start - 1]) {
            start -= 1;
        }
        if start < dot_pos {
            return Some(source[start..dot_pos].to_string());
        }
    }
    None
}

pub fn position_to_byte_offset(source: &str, pos: Position) -> Option<usize> {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        if line == pos.line && col == pos.character {
            return Some(i);
        }
        if ch == '\n' {
            if line == pos.line {
                return Some(i);
            }
            line += 1;
            col = 0;
        } else {
            col += ch.len_utf16() as u32;
        }
    }
    if line == pos.line {
        Some(source.len())
    } else {
        None
    }
}

pub fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

pub fn format_fn_signature(f: &ast::FnDef) -> String {
    let params: Vec<String> = f
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name.node, format_ast_type(&p.ty.node)))
        .collect();
    let ret = match &f.return_ty {
        Some(ty) => format!(" -> {}", format_ast_type(&ty.node)),
        None => String::new(),
    };
    format!("fn {}({}){}", f.name.node, params.join(", "), ret)
}

pub fn format_ast_type(ty: &ast::Type) -> String {
    match ty {
        ast::Type::Field => "Field".to_string(),
        ast::Type::XField => "XField".to_string(),
        ast::Type::Bool => "Bool".to_string(),
        ast::Type::U32 => "U32".to_string(),
        ast::Type::Digest => "Digest".to_string(),
        ast::Type::Array(inner, n) => format!("[{}; {}]", format_ast_type(inner), n),
        ast::Type::Tuple(elems) => {
            let parts: Vec<_> = elems.iter().map(format_ast_type).collect();
            format!("({})", parts.join(", "))
        }
        ast::Type::Named(path) => path.as_dotted(),
    }
}

/// Format a `TableCost` as a compact inline string for hover display.
pub fn format_cost_inline(cost: &crate::cost::TableCost) -> String {
    let model = crate::cost::create_cost_model("triton");
    let short_names = model.table_short_names();
    let n = cost.count as usize;
    let mut parts = Vec::new();
    for i in 0..n.min(short_names.len()) {
        if i == 0 || cost.values[i] > 0 {
            parts.push(format!("{}={}", short_names[i], cost.values[i]));
        }
    }
    format!(
        "{} | dominant: {}",
        parts.join(", "),
        cost.dominant_table(&short_names[..n.min(short_names.len())])
    )
}

/// Find the function name and active parameter index at a given position.
pub fn find_call_context(source: &str, pos: Position) -> Option<(String, u32)> {
    let offset = position_to_byte_offset(source, pos)?;
    let bytes = source.as_bytes();

    let mut depth = 0i32;
    let mut comma_count = 0u32;
    let mut i = offset;
    while i > 0 {
        i -= 1;
        match bytes[i] {
            b')' => depth += 1,
            b'(' => {
                if depth == 0 {
                    let mut name_end = i;
                    while name_end > 0 && bytes[name_end - 1] == b' ' {
                        name_end -= 1;
                    }
                    let mut name_start = name_end;
                    while name_start > 0
                        && (is_ident_char(bytes[name_start - 1]) || bytes[name_start - 1] == b'.')
                    {
                        name_start -= 1;
                    }
                    if name_start < name_end {
                        let name = source[name_start..name_end].to_string();
                        return Some((name, comma_count));
                    }
                    return None;
                }
                depth -= 1;
            }
            b',' if depth == 0 => comma_count += 1,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Position;

    // --- byte_offset_to_position ---

    #[test]
    fn test_byte_offset_first_line() {
        let src = "let x = 1\n";
        assert_eq!(byte_offset_to_position(src, 0), Position::new(0, 0));
        assert_eq!(byte_offset_to_position(src, 4), Position::new(0, 4));
    }

    #[test]
    fn test_byte_offset_second_line() {
        let src = "let x = 1\nlet y = 2\n";
        assert_eq!(byte_offset_to_position(src, 10), Position::new(1, 0));
        assert_eq!(byte_offset_to_position(src, 14), Position::new(1, 4));
    }

    #[test]
    fn test_byte_offset_clamps() {
        let src = "abc";
        let pos = byte_offset_to_position(src, 999);
        assert_eq!(pos, Position::new(0, 3));
    }

    // --- position_to_byte_offset ---

    #[test]
    fn test_position_to_offset_start() {
        let src = "let x = 1\nlet y = 2\n";
        assert_eq!(position_to_byte_offset(src, Position::new(0, 0)), Some(0));
        assert_eq!(position_to_byte_offset(src, Position::new(0, 4)), Some(4));
        assert_eq!(position_to_byte_offset(src, Position::new(1, 0)), Some(10));
        assert_eq!(position_to_byte_offset(src, Position::new(1, 4)), Some(14));
    }

    #[test]
    fn test_position_to_offset_end_of_line() {
        let src = "abc\ndef\n";
        assert_eq!(position_to_byte_offset(src, Position::new(0, 3)), Some(3));
    }

    #[test]
    fn test_position_to_offset_past_end() {
        let src = "abc";
        assert_eq!(position_to_byte_offset(src, Position::new(5, 0)), None);
    }

    // --- word_at_position ---

    #[test]
    fn test_word_simple() {
        let src = "let foo = bar\n";
        assert_eq!(word_at_position(src, Position::new(0, 4)), "foo");
        assert_eq!(word_at_position(src, Position::new(0, 10)), "bar");
    }

    #[test]
    fn test_word_at_start() {
        let src = "hello world\n";
        assert_eq!(word_at_position(src, Position::new(0, 0)), "hello");
    }

    #[test]
    fn test_word_qualified_after_dot() {
        let src = "let x = hash.tip5()\n";
        assert_eq!(word_at_position(src, Position::new(0, 14)), "hash.tip5");
    }

    #[test]
    fn test_word_qualified_before_dot() {
        let src = "let x = hash.tip5()\n";
        assert_eq!(word_at_position(src, Position::new(0, 9)), "hash.tip5");
    }

    #[test]
    fn test_word_on_boundary_picks_left() {
        let src = "let x = 1\n";
        assert_eq!(word_at_position(src, Position::new(0, 3)), "let");
    }

    #[test]
    fn test_word_between_symbols_empty() {
        let src = "a = b\n";
        assert_eq!(word_at_position(src, Position::new(0, 2)), "");
    }

    // --- text_before_dot ---

    #[test]
    fn test_dot_completion_prefix() {
        let src = "hash.t";
        assert_eq!(
            text_before_dot(src, Position::new(0, 6)),
            Some("hash".to_string())
        );
    }

    #[test]
    fn test_dot_completion_right_after_dot() {
        let src = "hash.";
        assert_eq!(
            text_before_dot(src, Position::new(0, 5)),
            Some("hash".to_string())
        );
    }

    #[test]
    fn test_no_dot_prefix() {
        let src = "let x = 1";
        assert_eq!(text_before_dot(src, Position::new(0, 5)), None);
    }

    // --- span_to_range ---

    #[test]
    fn test_span_to_range_single_line() {
        let src = "let foo = 1\n";
        let span = crate::span::Span::new(0, 4, 7);
        let range = span_to_range(src, span);
        assert_eq!(range.start, Position::new(0, 4));
        assert_eq!(range.end, Position::new(0, 7));
    }

    #[test]
    fn test_span_to_range_multi_line() {
        let src = "line1\nline2\nline3\n";
        let span = crate::span::Span::new(0, 6, 17);
        let range = span_to_range(src, span);
        assert_eq!(range.start, Position::new(1, 0));
        assert_eq!(range.end, Position::new(2, 5));
    }

    // --- to_lsp_diagnostic ---

    #[test]
    fn test_lsp_diagnostic_error() {
        let source = "let x: U32 = pub_read()\n";
        let diag = crate::diagnostic::Diagnostic::error(
            "type mismatch".to_string(),
            crate::span::Span::new(0, 13, 23),
        )
        .with_note("expected U32, found Field".to_string());

        let lsp_diag = to_lsp_diagnostic(&diag, source);
        assert_eq!(lsp_diag.severity, Some(DiagnosticSeverity::ERROR));
        assert!(lsp_diag.message.contains("type mismatch"));
        assert!(lsp_diag.message.contains("note: expected U32, found Field"));
        assert_eq!(lsp_diag.source, Some("trident".to_string()));
    }

    #[test]
    fn test_lsp_diagnostic_warning_with_help() {
        let source = "as_u32(x)\n";
        let diag = crate::diagnostic::Diagnostic::warning(
            "redundant".to_string(),
            crate::span::Span::new(0, 0, 9),
        )
        .with_help("already proven".to_string());

        let lsp_diag = to_lsp_diagnostic(&diag, source);
        assert_eq!(lsp_diag.severity, Some(DiagnosticSeverity::WARNING));
        assert!(lsp_diag.message.contains("help: already proven"));
    }

    // --- format_fn_signature ---

    #[test]
    fn test_format_fn_signature_no_params() {
        let f = crate::ast::FnDef {
            is_pub: false,
            is_test: false,
            is_pure: false,
            cfg: None,
            intrinsic: None,
            requires: vec![],
            ensures: vec![],
            name: crate::span::Spanned::dummy("main".to_string()),
            type_params: vec![],
            params: vec![],
            return_ty: None,
            body: None,
        };
        assert_eq!(format_fn_signature(&f), "fn main()");
    }

    #[test]
    fn test_format_fn_signature_with_return() {
        let f = crate::ast::FnDef {
            is_pub: true,
            is_test: false,
            is_pure: false,
            cfg: None,
            intrinsic: None,
            requires: vec![],
            ensures: vec![],
            name: crate::span::Spanned::dummy("add".to_string()),
            type_params: vec![],
            params: vec![
                crate::ast::Param {
                    name: crate::span::Spanned::dummy("a".to_string()),
                    ty: crate::span::Spanned::dummy(crate::ast::Type::Field),
                },
                crate::ast::Param {
                    name: crate::span::Spanned::dummy("b".to_string()),
                    ty: crate::span::Spanned::dummy(crate::ast::Type::Field),
                },
            ],
            return_ty: Some(crate::span::Spanned::dummy(crate::ast::Type::Field)),
            body: None,
        };
        assert_eq!(
            format_fn_signature(&f),
            "fn add(a: Field, b: Field) -> Field"
        );
    }

    // --- format_ast_type ---

    #[test]
    fn test_format_ast_types() {
        assert_eq!(format_ast_type(&crate::ast::Type::Field), "Field");
        assert_eq!(format_ast_type(&crate::ast::Type::XField), "XField");
        assert_eq!(format_ast_type(&crate::ast::Type::Bool), "Bool");
        assert_eq!(format_ast_type(&crate::ast::Type::U32), "U32");
        assert_eq!(format_ast_type(&crate::ast::Type::Digest), "Digest");
        assert_eq!(
            format_ast_type(&crate::ast::Type::Array(
                Box::new(crate::ast::Type::Field),
                crate::ast::ArraySize::Literal(5)
            )),
            "[Field; 5]"
        );
        assert_eq!(
            format_ast_type(&crate::ast::Type::Tuple(vec![
                crate::ast::Type::Field,
                crate::ast::Type::U32
            ])),
            "(Field, U32)"
        );
    }

    // --- is_ident_char ---

    #[test]
    fn test_ident_chars() {
        assert!(is_ident_char(b'a'));
        assert!(is_ident_char(b'Z'));
        assert!(is_ident_char(b'0'));
        assert!(is_ident_char(b'_'));
        assert!(!is_ident_char(b'.'));
        assert!(!is_ident_char(b' '));
        assert!(!is_ident_char(b'('));
    }

    // --- find_call_context ---

    #[test]
    fn test_find_call_context_simple() {
        let src = "pub_write(x, y)";
        let ctx = find_call_context(src, Position::new(0, 12));
        assert_eq!(ctx, Some(("pub_write".to_string(), 1)));
    }

    #[test]
    fn test_find_call_context_first_param() {
        let src = "pub_write(x)";
        let ctx = find_call_context(src, Position::new(0, 10));
        assert_eq!(ctx, Some(("pub_write".to_string(), 0)));
    }

    #[test]
    fn test_find_call_context_no_paren() {
        let src = "let x = 1";
        let ctx = find_call_context(src, Position::new(0, 5));
        assert_eq!(ctx, None);
    }

    #[test]
    fn test_find_call_context_nested() {
        let src = "split(field_add(a, b))";
        let ctx = find_call_context(src, Position::new(0, 19));
        assert_eq!(ctx, Some(("field_add".to_string(), 1)));
    }

    #[test]
    fn test_find_call_context_qualified_name() {
        let src = "math.add(x, y, z)";
        let ctx = find_call_context(src, Position::new(0, 15));
        assert_eq!(ctx, Some(("math.add".to_string(), 2)));
    }

    #[test]
    fn test_find_call_context_right_after_open_paren() {
        let src = "assert(";
        let ctx = find_call_context(src, Position::new(0, 7));
        assert_eq!(ctx, Some(("assert".to_string(), 0)));
    }

    #[test]
    fn test_find_call_context_space_before_paren() {
        let src = "foo (a, b)";
        let ctx = find_call_context(src, Position::new(0, 8));
        assert_eq!(ctx, Some(("foo".to_string(), 1)));
    }

    // --- format_cost_inline ---

    #[test]
    fn test_format_cost_inline_zero() {
        let cost = crate::cost::TableCost::from_slice(&[0, 0, 0, 0, 0, 0]);
        let s = format_cost_inline(&cost);
        assert!(s.contains("cc=0"), "should contain cc=0, got: {}", s);
        assert!(
            s.contains("dominant:"),
            "should contain dominant label, got: {}",
            s
        );
    }

    #[test]
    fn test_format_cost_inline_hash_dominant() {
        let cost = crate::cost::TableCost::from_slice(&[1, 6, 0, 1, 0, 0]);
        let s = format_cost_inline(&cost);
        assert!(s.contains("cc=1"), "should contain cc=1, got: {}", s);
        assert!(s.contains("hash=6"), "should contain hash=6, got: {}", s);
        assert!(
            s.contains("dominant: hash"),
            "dominant should be hash, got: {}",
            s
        );
        assert!(
            !s.contains("u32="),
            "zero u32 should be omitted, got: {}",
            s
        );
    }
}
