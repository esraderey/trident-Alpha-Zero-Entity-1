mod expr;
mod items;
mod stmts;
mod types;

#[cfg(test)]
mod tests;

use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::lexeme::Lexeme;
use crate::span::{Span, Spanned};

const MAX_NESTING_DEPTH: u32 = 256;

pub(crate) struct Parser {
    tokens: Vec<Spanned<Lexeme>>,
    pos: usize,
    diagnostics: Vec<Diagnostic>,
    depth: u32,
}

impl Parser {
    pub(crate) fn new(tokens: Vec<Spanned<Lexeme>>) -> Self {
        Self {
            tokens,
            pos: 0,
            diagnostics: Vec::new(),
            depth: 0,
        }
    }

    pub(crate) fn parse_file(mut self) -> Result<File, Vec<Diagnostic>> {
        let file = if self.at(&Lexeme::Program) {
            self.parse_program()
        } else if self.at(&Lexeme::Module) {
            self.parse_module()
        } else {
            self.error_with_help(
                "expected 'program' or 'module' declaration at the start of file",
                "every .tri file must begin with `program <name>` or `module <name>`",
            );
            return Err(self.diagnostics);
        };

        if !self.diagnostics.is_empty() {
            return Err(self.diagnostics);
        }
        Ok(file)
    }

    fn enter_nesting(&mut self) -> bool {
        self.depth += 1;
        if self.depth > MAX_NESTING_DEPTH {
            self.error_with_help(
                "nesting depth exceeded (maximum 256 levels)",
                "simplify your program by extracting deeply nested code into functions",
            );
            return false;
        }
        true
    }

    fn exit_nesting(&mut self) {
        self.depth -= 1;
    }

    // --- Utility methods ---

    fn peek(&self) -> &Lexeme {
        &self.tokens[self.pos].node
    }

    fn current_span(&self) -> Span {
        self.tokens[self.pos].span
    }

    fn prev_span(&self) -> Span {
        if self.pos > 0 {
            self.tokens[self.pos - 1].span
        } else {
            self.current_span()
        }
    }

    fn advance(&mut self) -> &Spanned<Lexeme> {
        let tok = &self.tokens[self.pos];
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        tok
    }

    fn at(&self, token: &Lexeme) -> bool {
        std::mem::discriminant(self.peek()) == std::mem::discriminant(token)
    }

    fn eat(&mut self, token: &Lexeme) -> bool {
        if self.at(token) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, token: &Lexeme) -> Span {
        if self.at(token) {
            let span = self.current_span();
            self.advance();
            span
        } else {
            self.error_at_current(&format!(
                "expected {}, found {}",
                token.description(),
                self.peek().description()
            ));
            self.current_span()
        }
    }

    fn expect_ident(&mut self) -> Spanned<String> {
        if let Lexeme::Ident(name) = self.peek().clone() {
            let span = self.current_span();
            self.advance();
            Spanned::new(name, span)
        } else {
            self.error_at_current(&format!(
                "expected identifier, found {}",
                self.peek().description()
            ));
            Spanned::new("_error_".to_string(), self.current_span())
        }
    }

    fn try_ident(&mut self) -> Option<Spanned<String>> {
        if let Lexeme::Ident(name) = self.peek().clone() {
            let span = self.current_span();
            self.advance();
            Some(Spanned::new(name, span))
        } else {
            None
        }
    }

    fn expect_integer(&mut self) -> u64 {
        if let Lexeme::Integer(n) = self.peek() {
            let n = *n;
            self.advance();
            n
        } else {
            self.error_at_current(&format!(
                "expected integer literal, found {}",
                self.peek().description()
            ));
            0
        }
    }

    fn error_at_current(&mut self, msg: &str) {
        self.diagnostics
            .push(Diagnostic::error(msg.to_string(), self.current_span()));
    }

    fn error_with_help(&mut self, msg: &str, help: &str) {
        self.diagnostics.push(
            Diagnostic::error(msg.to_string(), self.current_span()).with_help(help.to_string()),
        );
    }

    fn parse_module_path(&mut self) -> ModulePath {
        let first = self.expect_ident();
        let mut parts = vec![first.node];
        while self.eat(&Lexeme::Dot) {
            if let Some(ident) = self.try_ident() {
                parts.push(ident.node);
            } else {
                break;
            }
        }
        ModulePath(parts)
    }
}
