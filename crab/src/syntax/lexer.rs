use std::{cell::RefCell, rc::Rc};

use super::token::{Token, TokenKind};
use crate::common::diagnostic::{DiagnosticBag, Position};

pub(crate) struct Lexer<'a> {
    source: &'a str,
    // All positions are UTF-8 byte offsets at character boundaries.
    current: usize,
    pub(crate) diagnostic_bag: Rc<RefCell<DiagnosticBag>>,
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(source: &'a str, diagnostic_bag: Rc<RefCell<DiagnosticBag>>) -> Self {
        Self {
            source,
            current: 0,
            diagnostic_bag,
        }
    }

    pub(crate) fn lex(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token();

            match token.kind {
                TokenKind::Eof => {
                    tokens.push(token);
                    break;
                }

                TokenKind::Whitespace | TokenKind::Invalid => {}
                _ => tokens.push(token),
            }
        }

        tokens
    }

    fn peek(&self) -> Option<char> {
        self.source[self.current..].chars().next()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.current += ch.len_utf8();
        Some(ch)
    }

    fn next_token(&mut self) -> Token {
        let start = self.current;
        let Some(ch) = self.advance() else {
            return Token::new(TokenKind::Eof, String::new(), Position::new(start, start));
        };
        let kind = match ch {
            '+' => TokenKind::Plus,
            '-' if self.peek() == Some('>') => {
                self.advance();
                TokenKind::Arrow
            }

            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' if self.peek() == Some('/') => {
                self.advance();

                while self.peek().is_some_and(|ch| ch != '\n' && ch != '\r') {
                    self.advance();
                }

                TokenKind::Whitespace
            }

            '/' => TokenKind::Slash,
            '[' => TokenKind::OpenBracket,
            ']' => TokenKind::CloseBracket,
            ',' => TokenKind::Comma,
            ':' => TokenKind::Colon,
            ';' => TokenKind::Semicolon,
            '.' if self.peek() == Some('.') => {
                self.advance();
                TokenKind::DotDot
            }

            '.' => TokenKind::Dot,
            '(' => TokenKind::OpenParen,
            ')' => TokenKind::CloseParen,
            '{' => TokenKind::OpenBrace,
            '}' => TokenKind::CloseBrace,
            '>' | '<' | '!' | '=' | '&' | '|' => {
                let second = match ch {
                    '&' => '&',
                    '|' => '|',
                    _ => '=',
                };
                let paired = self.peek() == Some(second);

                if paired {
                    self.advance();
                }

                match (ch, paired) {
                    ('>', true) => TokenKind::GreaterEqual,
                    ('>', false) => TokenKind::Greater,
                    ('<', true) => TokenKind::LesserEqual,
                    ('<', false) => TokenKind::Lesser,
                    ('!', true) => TokenKind::BangEqual,
                    ('!', false) => TokenKind::Bang,
                    ('=', true) => TokenKind::EqualEqual,
                    ('=', false) => TokenKind::Equal,
                    ('&', true) => TokenKind::AmpersandAmpersand,
                    ('&', false) => TokenKind::Ampersand,
                    ('|', true) => TokenKind::PipePipe,
                    ('|', false) => TokenKind::Pipe,
                    _ => unreachable!(),
                }
            }

            '\'' | '"' => {
                let content_start = self.current;

                while self.peek().is_some_and(|next| next != ch) {
                    self.advance();
                }

                let content_end = self.current;

                if self.advance().is_none() {
                    self.diagnostic_bag.borrow_mut().report(
                        Position::new(start, self.current),
                        "Unterminated string literal",
                    );
                }

                return Token::new(
                    TokenKind::String,
                    self.source[content_start..content_end].to_owned(),
                    Position::new(start, self.current),
                );
            }

            ch if ch.is_ascii_whitespace() => {
                while self.peek().is_some_and(|ch| ch.is_ascii_whitespace()) {
                    self.advance();
                }

                TokenKind::Whitespace
            }

            ch if ch.is_ascii_digit() => {
                while self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                    self.advance();
                }

                TokenKind::Number
            }

            ch if ch.is_ascii_alphabetic() => {
                while self.peek().is_some_and(|ch| ch.is_ascii_alphanumeric()) {
                    self.advance();
                }

                TokenKind::get_lexeme_type(&self.source[start..self.current])
            }

            ch => {
                self.diagnostic_bag
                    .borrow_mut()
                    .unexpected_character(Position::new(start, self.current), ch);
                TokenKind::Invalid
            }
        };
        Token::new(
            kind,
            self.source[start..self.current].to_owned(),
            Position::new(start, self.current),
        )
    }
}
