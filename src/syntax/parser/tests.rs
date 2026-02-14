use super::*;
use crate::ast::*;
use crate::lexer::Lexer;

fn parse(source: &str) -> File {
    let (tokens, _comments, lex_diags) = Lexer::new(source, 0).tokenize();
    assert!(lex_diags.is_empty(), "lex errors: {:?}", lex_diags);
    Parser::new(tokens).parse_file().unwrap()
}

#[test]
fn test_minimal_program() {
    let file = parse("program test\n\nfn main() {\n}");
    assert_eq!(file.kind, FileKind::Program);
    assert_eq!(file.name.node, "test");
    assert_eq!(file.items.len(), 1);
}

#[test]
fn test_function_with_params() {
    let file = parse("program test\n\nfn add(a: Field, b: Field) -> Field {\n    a + b\n}");
    assert_eq!(file.items.len(), 1);
    if let Item::Fn(f) = &file.items[0].node {
        assert_eq!(f.name.node, "add");
        assert_eq!(f.params.len(), 2);
        assert!(f.return_ty.is_some());
    } else {
        panic!("expected function");
    }
}

#[test]
fn test_let_binding() {
    let file = parse("program test\n\nfn main() {\n    let a: Field = 42\n}");
    if let Item::Fn(f) = &file.items[0].node {
        let block = f.body.as_ref().unwrap();
        assert_eq!(block.node.stmts.len(), 1);
    }
}

#[test]
fn test_function_call() {
    let file = parse("program test\n\nfn main() {\n    let a: Field = pub_read()\n}");
    if let Item::Fn(f) = &file.items[0].node {
        let block = f.body.as_ref().unwrap();
        if let Stmt::Let { init, .. } = &block.node.stmts[0].node {
            assert!(matches!(init.node, Expr::Call { .. }));
        }
    }
}

#[test]
fn test_binary_expr() {
    let file = parse("program test\n\nfn main() {\n    let c: Field = a + b * c\n}");
    if let Item::Fn(f) = &file.items[0].node {
        let block = f.body.as_ref().unwrap();
        if let Stmt::Let { init, .. } = &block.node.stmts[0].node {
            // Should be Add(a, Mul(b, c)) due to precedence
            if let Expr::BinOp { op, .. } = &init.node {
                assert_eq!(*op, BinOp::Add);
            } else {
                panic!("expected binary op");
            }
        }
    }
}

#[test]
fn test_module() {
    let file = parse("module merkle\n\npub fn verify(root: Digest) {\n}");
    assert_eq!(file.kind, FileKind::Module);
    assert_eq!(file.name.node, "merkle");
    if let Item::Fn(f) = &file.items[0].node {
        assert!(f.is_pub);
    }
}

#[test]
fn test_program_declarations() {
    let file = parse("program test\n\npub input: [Field; 3]\npub output: Field\nsec input: [Field; 5]\n\nfn main() {\n}");
    assert_eq!(file.declarations.len(), 3);
    assert!(matches!(file.declarations[0], Declaration::PubInput(_)));
    assert!(matches!(file.declarations[1], Declaration::PubOutput(_)));
    assert!(matches!(file.declarations[2], Declaration::SecInput(_)));
}

#[test]
fn test_sec_ram_declaration() {
    let file =
        parse("program test\n\nsec ram: {\n    17: Field,\n    42: Field,\n}\n\nfn main() {\n}");
    assert_eq!(file.declarations.len(), 1);
    if let Declaration::SecRam(entries) = &file.declarations[0] {
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, 17);
        assert_eq!(entries[1].0, 42);
    } else {
        panic!("expected SecRam declaration");
    }
}

#[test]
fn test_tuple_destructure_let() {
    let file = parse(
        "program test\nfn main() {\n    let (a, b): (Field, Field) = (pub_read(), pub_read())\n}",
    );
    if let Item::Fn(f) = &file.items[0].node {
        let block = f.body.as_ref().unwrap();
        assert_eq!(block.node.stmts.len(), 1);
        if let Stmt::Let {
            pattern: Pattern::Tuple(names),
            ..
        } = &block.node.stmts[0].node
        {
            assert_eq!(names.len(), 2);
            assert_eq!(names[0].node, "a");
            assert_eq!(names[1].node, "b");
        } else {
            panic!("expected tuple destructuring let");
        }
    }
}

#[test]
fn test_event_declaration() {
    let file = parse("program test\nevent Transfer {\n    from: Field,\n    to: Field,\n    amount: Field,\n}\nfn main() {\n}");
    assert_eq!(file.items.len(), 2); // event + fn
    if let Item::Event(e) = &file.items[0].node {
        assert_eq!(e.name.node, "Transfer");
        assert_eq!(e.fields.len(), 3);
        assert_eq!(e.fields[0].name.node, "from");
        assert_eq!(e.fields[1].name.node, "to");
        assert_eq!(e.fields[2].name.node, "amount");
    } else {
        panic!("expected event declaration");
    }
}

#[test]
fn test_reveal_statement() {
    let file = parse("program test\nevent Ev { x: Field }\nfn main() {\n    let a: Field = pub_read()\n    reveal Ev { x: a }\n}");
    if let Item::Fn(f) = &file.items[1].node {
        let block = f.body.as_ref().unwrap();
        assert_eq!(block.node.stmts.len(), 2);
        if let Stmt::Reveal { event_name, fields } = &block.node.stmts[1].node {
            assert_eq!(event_name.node, "Ev");
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].0.node, "x");
        } else {
            panic!("expected reveal statement");
        }
    }
}

#[test]
fn test_seal_statement() {
    let file = parse("program test\nevent Ev { x: Field, y: Field }\nfn main() {\n    seal Ev { x: pub_read(), y: pub_read() }\n}");
    if let Item::Fn(f) = &file.items[1].node {
        let block = f.body.as_ref().unwrap();
        assert_eq!(block.node.stmts.len(), 1);
        if let Stmt::Seal { event_name, fields } = &block.node.stmts[0].node {
            assert_eq!(event_name.node, "Ev");
            assert_eq!(fields.len(), 2);
        } else {
            panic!("expected seal statement");
        }
    }
}

#[test]
fn test_asm_basic() {
    let file = parse("program test\nfn main() {\n    asm { dup 0\n    add }\n}");
    if let Item::Fn(f) = &file.items[0].node {
        let block = f.body.as_ref().unwrap();
        assert_eq!(block.node.stmts.len(), 1);
        if let Stmt::Asm {
            body,
            effect,
            target,
        } = &block.node.stmts[0].node
        {
            assert!(body.contains("dup 0"));
            assert!(body.contains("add"));
            assert_eq!(*effect, 0);
            assert_eq!(*target, None);
        } else {
            panic!("expected asm statement");
        }
    }
}

#[test]
fn test_asm_with_effect() {
    let file = parse("program test\nfn main() {\n    asm(+1) { push 42 }\n}");
    if let Item::Fn(f) = &file.items[0].node {
        let block = f.body.as_ref().unwrap();
        if let Stmt::Asm { effect, .. } = &block.node.stmts[0].node {
            assert_eq!(*effect, 1);
        } else {
            panic!("expected asm statement");
        }
    }
}

#[test]
fn test_asm_between_statements() {
    let file = parse("program test\nfn main() {\n    let x: Field = pub_read()\n    asm { dup 0\nadd }\n    pub_write(x)\n}");
    if let Item::Fn(f) = &file.items[0].node {
        let block = f.body.as_ref().unwrap();
        assert_eq!(block.node.stmts.len(), 2);
        assert!(matches!(&block.node.stmts[0].node, Stmt::Let { .. }));
        assert!(matches!(&block.node.stmts[1].node, Stmt::Asm { .. }));
        assert!(block.node.tail_expr.is_some(), "pub_write(x) is tail expr");
    }
}

// --- cfg attribute parsing ---

#[test]
fn test_cfg_on_fn() {
    let file = parse("program test\n#[cfg(debug)]\nfn check() {}");
    if let Item::Fn(f) = &file.items[0].node {
        assert_eq!(f.cfg.as_ref().unwrap().node, "debug");
        assert_eq!(f.name.node, "check");
    } else {
        panic!("expected fn");
    }
}

#[test]
fn test_cfg_on_const() {
    let file = parse("program test\n#[cfg(release)]\nconst X: Field = 0");
    if let Item::Const(c) = &file.items[0].node {
        assert_eq!(c.cfg.as_ref().unwrap().node, "release");
        assert_eq!(c.name.node, "X");
    } else {
        panic!("expected const");
    }
}

#[test]
fn test_cfg_on_struct() {
    let file = parse("program test\n#[cfg(debug)]\nstruct Dbg { val: Field }");
    if let Item::Struct(s) = &file.items[0].node {
        assert_eq!(s.cfg.as_ref().unwrap().node, "debug");
        assert_eq!(s.name.node, "Dbg");
    } else {
        panic!("expected struct");
    }
}

#[test]
fn test_cfg_on_pub_fn() {
    let file = parse("program test\n#[cfg(release)]\npub fn fast() {}");
    if let Item::Fn(f) = &file.items[0].node {
        assert_eq!(f.cfg.as_ref().unwrap().node, "release");
        assert!(f.is_pub);
    } else {
        panic!("expected fn");
    }
}

#[test]
fn test_cfg_with_intrinsic() {
    let file = parse("module std.test\n#[cfg(debug)]\n#[intrinsic(add)]\npub fn add(a: Field, b: Field) -> Field");
    if let Item::Fn(f) = &file.items[0].node {
        assert_eq!(f.cfg.as_ref().unwrap().node, "debug");
        assert!(f.intrinsic.is_some());
    } else {
        panic!("expected fn");
    }
}

#[test]
fn test_no_cfg() {
    let file = parse("program test\nfn main() {}");
    if let Item::Fn(f) = &file.items[0].node {
        assert!(f.cfg.is_none());
    } else {
        panic!("expected fn");
    }
}

// --- match statement parsing ---

#[test]
fn test_match_basic() {
    let file = parse("program test\nfn main() {\n    let x: Field = pub_read()\n    match x {\n        0 => { pub_write(0) }\n        1 => { pub_write(1) }\n        _ => { pub_write(2) }\n    }\n}");
    if let Item::Fn(f) = &file.items[0].node {
        let block = f.body.as_ref().unwrap();
        assert_eq!(block.node.stmts.len(), 2);
        if let Stmt::Match { arms, .. } = &block.node.stmts[1].node {
            assert_eq!(arms.len(), 3);
            assert!(matches!(
                arms[0].pattern.node,
                MatchPattern::Literal(Literal::Integer(0))
            ));
            assert!(matches!(
                arms[1].pattern.node,
                MatchPattern::Literal(Literal::Integer(1))
            ));
            assert!(matches!(arms[2].pattern.node, MatchPattern::Wildcard));
        } else {
            panic!("expected match statement");
        }
    }
}

#[test]
fn test_match_bool_patterns() {
    let file = parse("program test\nfn main() {\n    let b: Bool = true\n    match b {\n        true => { pub_write(1) }\n        false => { pub_write(0) }\n    }\n}");
    if let Item::Fn(f) = &file.items[0].node {
        let block = f.body.as_ref().unwrap();
        if let Stmt::Match { arms, .. } = &block.node.stmts[1].node {
            assert_eq!(arms.len(), 2);
            assert!(matches!(
                arms[0].pattern.node,
                MatchPattern::Literal(Literal::Bool(true))
            ));
            assert!(matches!(
                arms[1].pattern.node,
                MatchPattern::Literal(Literal::Bool(false))
            ));
        } else {
            panic!("expected match statement");
        }
    }
}

#[test]
fn test_match_wildcard_only() {
    let file = parse("program test\nfn main() {\n    match pub_read() {\n        _ => { pub_write(0) }\n    }\n}");
    if let Item::Fn(f) = &file.items[0].node {
        let block = f.body.as_ref().unwrap();
        if let Stmt::Match { arms, .. } = &block.node.stmts[0].node {
            assert_eq!(arms.len(), 1);
            assert!(matches!(arms[0].pattern.node, MatchPattern::Wildcard));
        } else {
            panic!("expected match statement");
        }
    }
}

#[test]
fn test_match_struct_pattern() {
    let file = parse(
        "program test\nstruct Point { x: Field, y: Field }\nfn main() {\n    let p = Point { x: 1, y: 2 }\n    match p {\n        Point { x, y } => { pub_write(x) }\n    }\n}",
    );
    if let Item::Fn(f) = &file.items[1].node {
        let block = f.body.as_ref().unwrap();
        if let Stmt::Match { arms, .. } = &block.node.stmts[1].node {
            assert_eq!(arms.len(), 1);
            if let MatchPattern::Struct { name, fields } = &arms[0].pattern.node {
                assert_eq!(name.node, "Point");
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].field_name.node, "x");
                assert_eq!(fields[1].field_name.node, "y");
                assert!(matches!(fields[0].pattern.node, FieldPattern::Binding(ref v) if v == "x"));
                assert!(matches!(fields[1].pattern.node, FieldPattern::Binding(ref v) if v == "y"));
            } else {
                panic!("expected struct pattern");
            }
        } else {
            panic!("expected match statement");
        }
    }
}

#[test]
fn test_match_struct_pattern_with_literals() {
    let file = parse(
        "program test\nstruct Pair { a: Field, b: Field }\nfn main() {\n    let p = Pair { a: 1, b: 2 }\n    match p {\n        Pair { a: 0, b } => { pub_write(b) }\n        _ => { pub_write(0) }\n    }\n}",
    );
    if let Item::Fn(f) = &file.items[1].node {
        let block = f.body.as_ref().unwrap();
        if let Stmt::Match { arms, .. } = &block.node.stmts[1].node {
            assert_eq!(arms.len(), 2);
            if let MatchPattern::Struct { fields, .. } = &arms[0].pattern.node {
                assert!(matches!(
                    fields[0].pattern.node,
                    FieldPattern::Literal(Literal::Integer(0))
                ));
                assert!(matches!(fields[1].pattern.node, FieldPattern::Binding(ref v) if v == "b"));
            } else {
                panic!("expected struct pattern");
            }
            assert!(matches!(arms[1].pattern.node, MatchPattern::Wildcard));
        } else {
            panic!("expected match statement");
        }
    }
}

#[test]
fn test_match_struct_pattern_with_wildcard_field() {
    let file = parse(
        "program test\nstruct Pair { a: Field, b: Field }\nfn main() {\n    let p = Pair { a: 1, b: 2 }\n    match p {\n        Pair { a: _, b } => { pub_write(b) }\n    }\n}",
    );
    if let Item::Fn(f) = &file.items[1].node {
        let block = f.body.as_ref().unwrap();
        if let Stmt::Match { arms, .. } = &block.node.stmts[1].node {
            if let MatchPattern::Struct { fields, .. } = &arms[0].pattern.node {
                assert!(matches!(fields[0].pattern.node, FieldPattern::Wildcard));
            } else {
                panic!("expected struct pattern");
            }
        } else {
            panic!("expected match statement");
        }
    }
}

// --- #[test] attribute parsing ---

#[test]
fn test_test_attribute_on_fn() {
    let file =
        parse("program test\n#[test]\nfn check_math() {\n    assert(1 == 1)\n}\nfn main() {}");
    assert_eq!(file.items.len(), 2);
    if let Item::Fn(f) = &file.items[0].node {
        assert!(f.is_test, "function should be marked as test");
        assert_eq!(f.name.node, "check_math");
    } else {
        panic!("expected test function");
    }
    if let Item::Fn(f) = &file.items[1].node {
        assert!(!f.is_test, "main should not be marked as test");
        assert_eq!(f.name.node, "main");
    } else {
        panic!("expected main function");
    }
}

#[test]
fn test_test_attribute_with_cfg() {
    let file = parse("program test\n#[cfg(debug)]\n#[test]\nfn debug_check() {}\nfn main() {}");
    if let Item::Fn(f) = &file.items[0].node {
        assert!(f.is_test, "function should be marked as test");
        assert_eq!(f.cfg.as_ref().unwrap().node, "debug");
        assert_eq!(f.name.node, "debug_check");
    } else {
        panic!("expected test function");
    }
}

#[test]
fn test_no_test_attribute() {
    let file = parse("program test\nfn main() {}");
    if let Item::Fn(f) = &file.items[0].node {
        assert!(!f.is_test, "main should not be marked as test");
    } else {
        panic!("expected function");
    }
}

#[test]
fn test_no_arg_attribute_format() {
    let file = parse("program test\n#[test]\nfn t() {}\nfn main() {}");
    if let Item::Fn(f) = &file.items[0].node {
        assert!(f.is_test);
        assert!(f.intrinsic.is_none());
    } else {
        panic!("expected function");
    }
}

// --- Error path tests ---

fn parse_err(source: &str) -> Vec<crate::diagnostic::Diagnostic> {
    let (tokens, _comments, lex_diags) = Lexer::new(source, 0).tokenize();
    if !lex_diags.is_empty() {
        return lex_diags;
    }
    match Parser::new(tokens).parse_file() {
        Ok(_) => vec![],
        Err(diags) => diags,
    }
}

#[test]
fn test_error_missing_program_or_module() {
    let diags = parse_err("fn main() {}");
    assert!(!diags.is_empty(), "should error on missing program/module");
    assert!(
        diags[0].message.contains("expected 'program' or 'module'"),
        "should say what was expected, got: {}",
        diags[0].message
    );
    assert!(
        diags[0].help.is_some(),
        "should have help text for program/module declaration"
    );
}

#[test]
fn test_error_missing_closing_brace() {
    let diags = parse_err("program test\nfn main() {");
    assert!(!diags.is_empty(), "should error on missing closing brace");
    assert!(
        diags[0].message.contains("expected '}'"),
        "should expect closing brace, got: {}",
        diags[0].message
    );
}

#[test]
fn test_error_unexpected_token_in_expr() {
    let diags = parse_err("program test\nfn main() {\n    let x: Field = }\n}");
    assert!(
        !diags.is_empty(),
        "should error on unexpected token in expression"
    );
    assert!(
        diags[0].message.contains("expected expression"),
        "should say 'expected expression', got: {}",
        diags[0].message
    );
    assert!(
        diags[0].help.is_some(),
        "expression error should have help text"
    );
}

#[test]
fn test_error_missing_fn_body() {
    let diags = parse_err("program test\nfn main() {\n    let x: Field = 1");
    assert!(!diags.is_empty(), "should error on unclosed function body");
    let has_relevant_error = diags.iter().any(|d| d.message.contains("expected"));
    assert!(
        has_relevant_error,
        "should have an 'expected' error, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn test_error_invalid_type() {
    let diags = parse_err("program test\nfn main() {\n    let x: 42 = 0\n}");
    assert!(!diags.is_empty(), "should error on invalid type");
    assert!(
        diags[0].message.contains("expected type"),
        "should say 'expected type', got: {}",
        diags[0].message
    );
    assert!(
        diags[0].help.as_deref().unwrap().contains("Field"),
        "help should list valid types"
    );
}

#[test]
fn test_error_missing_arrow_in_return_type() {
    let diags = parse_err("program test\nfn foo() Field {}");
    assert!(
        !diags.is_empty(),
        "should error when return type arrow is missing"
    );
}

#[test]
fn test_error_expected_token_shows_found() {
    let diags = parse_err("program test\nfn main {}");
    assert!(!diags.is_empty());
    let msg = &diags[0].message;
    assert!(
        msg.contains("expected") && msg.contains("found"),
        "error should show both expected and found tokens, got: {}",
        msg
    );
}

#[test]
fn test_error_expected_item() {
    let diags = parse_err("program test\n42");
    assert!(
        !diags.is_empty(),
        "should error on bare integer at top level"
    );
    assert!(
        diags[0].message.contains("expected item"),
        "should say 'expected item', got: {}",
        diags[0].message
    );
    assert!(
        diags[0].help.is_some(),
        "expected item error should have help text"
    );
}

// --- Const generic expression parsing ---

#[test]
fn test_parse_array_size_add() {
    let file = parse("program test\nfn f(a: [Field; M + N]) {}");
    let func = match &file.items[0].node {
        Item::Fn(f) => f,
        _ => panic!("expected fn"),
    };
    match &func.params[0].ty.node {
        Type::Array(_, size) => {
            assert_eq!(format!("{}", size), "M + N");
        }
        other => panic!("expected array type, got {:?}", other),
    }
}

#[test]
fn test_parse_array_size_mul() {
    let file = parse("program test\nfn f(a: [Field; N * 2]) {}");
    let func = match &file.items[0].node {
        Item::Fn(f) => f,
        _ => panic!("expected fn"),
    };
    match &func.params[0].ty.node {
        Type::Array(_, size) => {
            assert_eq!(format!("{}", size), "N * 2");
        }
        other => panic!("expected array type, got {:?}", other),
    }
}

#[test]
fn test_parse_array_size_precedence() {
    let file = parse("program test\nfn f(a: [Field; M + N * 2]) {}");
    let func = match &file.items[0].node {
        Item::Fn(f) => f,
        _ => panic!("expected fn"),
    };
    match &func.params[0].ty.node {
        Type::Array(_, size) => {
            assert_eq!(format!("{}", size), "M + N * 2");
            match size {
                ArraySize::Add(a, b) => {
                    assert!(matches!(a.as_ref(), ArraySize::Param(n) if n == "M"));
                    assert!(matches!(b.as_ref(), ArraySize::Mul(..)));
                }
                other => panic!("expected Add, got {:?}", other),
            }
        }
        other => panic!("expected array type, got {:?}", other),
    }
}

#[test]
fn test_parse_array_size_parenthesized() {
    let file = parse("program test\nfn f(a: [Field; (M + N) * 2]) {}");
    let func = match &file.items[0].node {
        Item::Fn(f) => f,
        _ => panic!("expected fn"),
    };
    match &func.params[0].ty.node {
        Type::Array(_, size) => {
            assert_eq!(format!("{}", size), "(M + N) * 2");
            match size {
                ArraySize::Mul(a, b) => {
                    assert!(matches!(a.as_ref(), ArraySize::Add(..)));
                    assert!(matches!(b.as_ref(), ArraySize::Literal(2)));
                }
                other => panic!("expected Mul, got {:?}", other),
            }
        }
        other => panic!("expected array type, got {:?}", other),
    }
}

#[test]
fn test_parse_generic_call_size_expr() {
    let file = parse("program test\nfn f() { g<M + N>() }");
    let func = match &file.items[0].node {
        Item::Fn(f) => f,
        _ => panic!("expected fn"),
    };
    let body = func.body.as_ref().unwrap();
    let tail = body
        .node
        .tail_expr
        .as_ref()
        .expect("expected tail expression");
    match &tail.node {
        Expr::Call { generic_args, .. } => {
            assert_eq!(generic_args.len(), 1);
            assert_eq!(format!("{}", generic_args[0].node), "M + N");
        }
        other => panic!("expected Call, got {:?}", other),
    }
}
