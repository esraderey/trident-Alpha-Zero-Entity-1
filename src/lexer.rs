use crate::diagnostic::Diagnostic;
use crate::lexeme::Lexeme;
use crate::span::{Span, Spanned};

/// A source comment preserved for the formatter.
#[derive(Clone, Debug)]
pub struct Comment {
    pub text: String, // includes the "//" prefix
    pub span: Span,
    pub trailing: bool, // true if a token appeared earlier on the same line
}

pub struct Lexer<'src> {
    source: &'src [u8],
    file_id: u16,
    pos: usize,
    diagnostics: Vec<Diagnostic>,
    comments: Vec<Comment>,
    /// Whether we've seen a non-whitespace token on the current line.
    token_on_line: bool,
}

impl<'src> Lexer<'src> {
    pub fn new(source: &'src str, file_id: u16) -> Self {
        Self {
            source: source.as_bytes(),
            file_id,
            pos: 0,
            diagnostics: Vec::new(),
            comments: Vec::new(),
            token_on_line: false,
        }
    }

    pub fn tokenize(mut self) -> (Vec<Spanned<Lexeme>>, Vec<Comment>, Vec<Diagnostic>) {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token();
            let is_eof = tok.node == Lexeme::Eof;
            tokens.push(tok);
            if is_eof {
                break;
            }
        }
        (tokens, self.comments, self.diagnostics)
    }

    fn next_token(&mut self) -> Spanned<Lexeme> {
        self.skip_whitespace_and_comments();

        if self.pos >= self.source.len() {
            return self.make_token(Lexeme::Eof, self.pos, self.pos);
        }

        let start = self.pos;
        let ch = self.source[self.pos];

        self.token_on_line = true;

        // Identifiers and keywords
        if is_ident_start(ch) {
            return self.scan_ident_or_keyword();
        }

        // Integer literals
        if ch.is_ascii_digit() {
            return self.scan_number();
        }

        // Symbols
        self.scan_symbol(start)
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            // Skip whitespace, tracking newlines
            while self.pos < self.source.len() && self.source[self.pos].is_ascii_whitespace() {
                if self.source[self.pos] == b'\n' {
                    self.token_on_line = false;
                }
                self.pos += 1;
            }

            // Collect line comments
            if self.pos + 1 < self.source.len()
                && self.source[self.pos] == b'/'
                && self.source[self.pos + 1] == b'/'
            {
                let start = self.pos;
                while self.pos < self.source.len() && self.source[self.pos] != b'\n' {
                    self.pos += 1;
                }
                let text = std::str::from_utf8(&self.source[start..self.pos])
                    .unwrap()
                    .to_string();
                self.comments.push(Comment {
                    text,
                    span: Span::new(self.file_id, start as u32, self.pos as u32),
                    trailing: self.token_on_line,
                });
                continue;
            }

            break;
        }
    }

    fn scan_ident_or_keyword(&mut self) -> Spanned<Lexeme> {
        let start = self.pos;
        while self.pos < self.source.len() && is_ident_continue(self.source[self.pos]) {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.source[start..self.pos]).unwrap();
        let token = Lexeme::from_keyword(text).unwrap_or_else(|| Lexeme::Ident(text.to_string()));
        self.make_token(token, start, self.pos)
    }

    fn scan_number(&mut self) -> Spanned<Lexeme> {
        let start = self.pos;
        while self.pos < self.source.len() && self.source[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.source[start..self.pos]).unwrap();
        match text.parse::<u64>() {
            Ok(n) => self.make_token(Lexeme::Integer(n), start, self.pos),
            Err(_) => {
                self.diagnostics.push(Diagnostic::error(
                    format!("integer literal '{}' is too large", text),
                    Span::new(self.file_id, start as u32, self.pos as u32),
                ));
                self.make_token(Lexeme::Integer(0), start, self.pos)
            }
        }
    }

    fn scan_symbol(&mut self, start: usize) -> Spanned<Lexeme> {
        let ch = self.source[self.pos];
        self.pos += 1;

        let token = match ch {
            b'(' => Lexeme::LParen,
            b')' => Lexeme::RParen,
            b'{' => Lexeme::LBrace,
            b'}' => Lexeme::RBrace,
            b'[' => Lexeme::LBracket,
            b']' => Lexeme::RBracket,
            b',' => Lexeme::Comma,
            b':' => Lexeme::Colon,
            b';' => Lexeme::Semicolon,
            b'+' => Lexeme::Plus,
            b'<' => Lexeme::Lt,
            b'&' => Lexeme::Amp,
            b'^' => Lexeme::Caret,
            b'#' => Lexeme::Hash,
            b'.' => {
                if self.peek() == Some(b'.') {
                    self.pos += 1;
                    Lexeme::DotDot
                } else {
                    Lexeme::Dot
                }
            }
            b'-' => {
                if self.peek() == Some(b'>') {
                    self.pos += 1;
                    Lexeme::Arrow
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        "unexpected '-'; subtraction uses sub() function".to_string(),
                        Span::new(self.file_id, start as u32, self.pos as u32),
                    ));
                    return self.next_token();
                }
            }
            b'=' => {
                if self.peek() == Some(b'=') {
                    self.pos += 1;
                    Lexeme::EqEq
                } else {
                    Lexeme::Eq
                }
            }
            b'*' => {
                if self.peek() == Some(b'.') {
                    self.pos += 1;
                    Lexeme::StarDot
                } else {
                    Lexeme::Star
                }
            }
            b'/' => {
                if self.peek() == Some(b'%') {
                    self.pos += 1;
                    Lexeme::SlashPercent
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        "unexpected '/'; division uses /% (divmod) operator".to_string(),
                        Span::new(self.file_id, start as u32, self.pos as u32),
                    ));
                    return self.next_token();
                }
            }
            b'_' => {
                // Could be start of identifier like _foo, or standalone underscore
                if self.pos < self.source.len() && is_ident_continue(self.source[self.pos]) {
                    // Back up and scan as identifier
                    self.pos = start;
                    return self.scan_ident_or_keyword();
                }
                Lexeme::Underscore
            }
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    format!("unexpected character '{}'", ch as char),
                    Span::new(self.file_id, start as u32, self.pos as u32),
                ));
                return self.next_token();
            }
        };

        self.make_token(token, start, self.pos)
    }

    fn peek(&self) -> Option<u8> {
        if self.pos < self.source.len() {
            Some(self.source[self.pos])
        } else {
            None
        }
    }

    fn make_token(&self, token: Lexeme, start: usize, end: usize) -> Spanned<Lexeme> {
        Spanned::new(token, Span::new(self.file_id, start as u32, end as u32))
    }
}

fn is_ident_start(ch: u8) -> bool {
    ch.is_ascii_alphabetic() || ch == b'_'
}

fn is_ident_continue(ch: u8) -> bool {
    ch.is_ascii_alphanumeric() || ch == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(source: &str) -> Vec<Lexeme> {
        let (tokens, _comments, diags) = Lexer::new(source, 0).tokenize();
        assert!(diags.is_empty(), "unexpected errors: {:?}", diags);
        tokens.into_iter().map(|t| t.node).collect()
    }

    #[test]
    fn test_keywords() {
        let tokens = lex("program fn let mut pub if else for in bounded return");
        assert_eq!(
            tokens,
            vec![
                Lexeme::Program,
                Lexeme::Fn,
                Lexeme::Let,
                Lexeme::Mut,
                Lexeme::Pub,
                Lexeme::If,
                Lexeme::Else,
                Lexeme::For,
                Lexeme::In,
                Lexeme::Bounded,
                Lexeme::Return,
                Lexeme::Eof,
            ]
        );
    }

    #[test]
    fn test_types() {
        let tokens = lex("Field XField Bool U32 Digest");
        assert_eq!(
            tokens,
            vec![
                Lexeme::FieldTy,
                Lexeme::XFieldTy,
                Lexeme::BoolTy,
                Lexeme::U32Ty,
                Lexeme::DigestTy,
                Lexeme::Eof,
            ]
        );
    }

    #[test]
    fn test_symbols() {
        let tokens = lex("( ) { } [ ] , : ; . .. -> = == + * *. < & ^ /% #");
        assert_eq!(
            tokens,
            vec![
                Lexeme::LParen,
                Lexeme::RParen,
                Lexeme::LBrace,
                Lexeme::RBrace,
                Lexeme::LBracket,
                Lexeme::RBracket,
                Lexeme::Comma,
                Lexeme::Colon,
                Lexeme::Semicolon,
                Lexeme::Dot,
                Lexeme::DotDot,
                Lexeme::Arrow,
                Lexeme::Eq,
                Lexeme::EqEq,
                Lexeme::Plus,
                Lexeme::Star,
                Lexeme::StarDot,
                Lexeme::Lt,
                Lexeme::Amp,
                Lexeme::Caret,
                Lexeme::SlashPercent,
                Lexeme::Hash,
                Lexeme::Eof,
            ]
        );
    }

    #[test]
    fn test_integers() {
        let tokens = lex("0 1 42 18446744073709551615");
        assert_eq!(
            tokens,
            vec![
                Lexeme::Integer(0),
                Lexeme::Integer(1),
                Lexeme::Integer(42),
                Lexeme::Integer(u64::MAX),
                Lexeme::Eof,
            ]
        );
    }

    #[test]
    fn test_identifiers() {
        let tokens = lex("foo bar_baz x1 _underscore");
        assert_eq!(
            tokens,
            vec![
                Lexeme::Ident("foo".into()),
                Lexeme::Ident("bar_baz".into()),
                Lexeme::Ident("x1".into()),
                Lexeme::Ident("_underscore".into()),
                Lexeme::Eof,
            ]
        );
    }

    #[test]
    fn test_comments() {
        let tokens = lex("foo // this is a comment\nbar");
        assert_eq!(
            tokens,
            vec![
                Lexeme::Ident("foo".into()),
                Lexeme::Ident("bar".into()),
                Lexeme::Eof,
            ]
        );
    }

    #[test]
    fn test_simple_program() {
        let tokens = lex("program test\n\nfn main() {\n    let a: Field = pub_read()\n}");
        assert_eq!(tokens[0], Lexeme::Program);
        assert_eq!(tokens[1], Lexeme::Ident("test".into()));
        assert_eq!(tokens[2], Lexeme::Fn);
        assert_eq!(tokens[3], Lexeme::Ident("main".into()));
    }

    #[test]
    fn test_event_keywords() {
        let tokens = lex("event emit seal");
        assert_eq!(
            tokens,
            vec![Lexeme::Event, Lexeme::Emit, Lexeme::Seal, Lexeme::Eof,]
        );
    }
}
