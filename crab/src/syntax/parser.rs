use std::{cell::RefCell, rc::Rc};

use crate::common::{
    diagnostic::{DiagnosticBag, Position},
    types::{Number, Object, Type},
};

use super::{
    syntax_tree::{
        BinaryExpression, BlockStatement, CollectionExpression, CollectionKind, EachStatement,
        Expression, ExpressionStatement, ForStatement, FunctionExpression, IfStatement,
        LiteralExpression, NameExpression, ParenthesizedExpression, Statement, UnaryExpression,
        VarStatement, WhileStatement,
    },
    token::{Token, TokenKind},
};

pub(crate) struct Parser {
    tokens: Vec<Token>,
    current: usize,
    source: String,
    pub(super) diagnostic_bag: Rc<RefCell<DiagnosticBag>>,
}

impl Parser {
    pub(crate) fn new(
        tokens: Vec<Token>,
        diagnostic_bag: Rc<RefCell<DiagnosticBag>>,
        source: &str,
    ) -> Self {
        Self {
            tokens,
            source: source.to_owned(),
            current: 0,
            diagnostic_bag,
        }
    }

    pub(crate) fn parse(&mut self) -> Vec<Statement> {
        let mut statements = vec![];

        while self.peek(0).kind != TokenKind::Eof {
            statements.push(self.parse_statement());
        }

        self.match_token(TokenKind::Eof);
        statements
    }

    fn parse_statement(&mut self) -> Statement {
        match self.peek(0).kind {
            TokenKind::For => self.parse_for_statement(),
            TokenKind::While => self.parse_while_statement(),
            TokenKind::If => self.parse_if_statement(),
            TokenKind::OpenBrace => self.parse_block_statement(),
            TokenKind::Var => self.parse_var_statement(),
            TokenKind::Fun if self.peek(1).kind == TokenKind::Identifier => {
                let start = self.next_token().position;
                let name = self.next_token().lexeme;
                Statement::Function(name, self.parse_function(start))
            }

            TokenKind::Return => {
                let start = self.next_token().position;
                let newline = self.source[start.end..self.peek(0).position.start].contains('\n');
                let value =
                    if newline || self.token_matches(&[TokenKind::CloseBrace, TokenKind::Eof]) {
                        None
                    } else {
                        Some(self.parse_expression())
                    };
                Statement::Return(value, start)
            }

            _ => Statement::Expression(ExpressionStatement::new(self.parse_expression())),
        }
    }

    fn parse_for_statement(&mut self) -> Statement {
        self.match_token(TokenKind::For);
        let identifier = self.match_token(TokenKind::Identifier);

        if self.peek(0).kind == TokenKind::In {
            self.advance();
            let iterable = self.parse_expression();
            let body = Box::new(self.parse_statement());

            return Statement::Each(EachStatement {
                identifier,
                iterable,
                body,
            });
        }

        self.match_token(TokenKind::Equal);
        let lower_bound = self.parse_expression();
        self.match_token(TokenKind::To);
        let upper_bound = self.parse_expression();
        let body = self.parse_statement();
        Statement::For(ForStatement::new(
            identifier,
            lower_bound,
            upper_bound,
            body,
        ))
    }

    fn parse_while_statement(&mut self) -> Statement {
        self.match_token(TokenKind::While);
        let condition = self.parse_expression();
        let body = self.parse_statement();
        Statement::While(WhileStatement::new(condition, body))
    }

    fn parse_if_statement(&mut self) -> Statement {
        self.match_token(TokenKind::If);
        let condition = self.parse_expression();
        let consequence = self.parse_statement();
        let else_clause = match (self.peek(0).kind, self.peek(1).kind) {
            (TokenKind::Else, TokenKind::If) => {
                self.advance();
                Some(self.parse_if_statement())
            }

            (TokenKind::Else, _) => {
                self.advance();
                Some(self.parse_statement())
            }

            (_, _) => None,
        };
        Statement::If(IfStatement::new(condition, consequence, else_clause))
    }

    fn parse_block_statement(&mut self) -> Statement {
        self.match_token(TokenKind::OpenBrace);
        let mut statements = vec![];

        while !self.token_matches(&[TokenKind::CloseBrace, TokenKind::Eof]) {
            statements.push(self.parse_statement());
        }

        self.match_token(TokenKind::CloseBrace);
        Statement::Block(BlockStatement::new(statements))
    }

    fn parse_var_statement(&mut self) -> Statement {
        self.match_token(TokenKind::Var);
        let identifier = self.match_token(TokenKind::Identifier);
        let annotation = if self.peek(0).kind == TokenKind::Colon {
            self.advance();
            Some(self.parse_type())
        } else {
            None
        };
        self.match_token(TokenKind::Equal);
        let expression = self.parse_expression();
        let mut statement = VarStatement::new(identifier, expression);
        statement.annotation = annotation;
        Statement::Var(statement)
    }

    fn parse_expression(&mut self) -> Expression {
        self.parse_assignment_expression()
    }

    fn parse_assignment_expression(&mut self) -> Expression {
        let target = self.parse_or_expression();

        if self.peek(0).kind == TokenKind::Equal {
            self.advance();
            let value = self.parse_assignment_expression();
            let position = Position::from(target.get_position(), value.get_position());
            Self::collection(
                CollectionKind::Assign(Box::new(target), Box::new(value)),
                position,
            )
        } else {
            target
        }
    }

    fn parse_or_expression(&mut self) -> Expression {
        let mut left = self.parse_and_expression();

        while self.token_matches(&[TokenKind::PipePipe]) {
            let operator = self.next_token();
            let right = self.parse_and_expression();
            left = Expression::Binary(BinaryExpression::new(left, operator, right));
        }

        left
    }

    fn parse_and_expression(&mut self) -> Expression {
        let mut left = self.parse_equality_expression();

        while self.token_matches(&[TokenKind::AmpersandAmpersand]) {
            let operator = self.next_token();
            let right = self.parse_equality_expression();
            left = Expression::Binary(BinaryExpression::new(left, operator, right));
        }

        left
    }

    fn parse_equality_expression(&mut self) -> Expression {
        let mut left = self.parse_comparison_expression();

        while self.token_matches(&[TokenKind::BangEqual, TokenKind::EqualEqual]) {
            let operator = self.next_token();
            let right = self.parse_comparison_expression();
            left = Expression::Binary(BinaryExpression::new(left, operator, right));
        }

        left
    }

    fn parse_comparison_expression(&mut self) -> Expression {
        let mut left = self.parse_range_expression();

        while self.token_matches(&[
            TokenKind::Greater,
            TokenKind::Lesser,
            TokenKind::GreaterEqual,
            TokenKind::LesserEqual,
        ]) {
            let operator = self.next_token();
            let right = self.parse_range_expression();
            left = Expression::Binary(BinaryExpression::new(left, operator, right));
        }

        left
    }

    fn parse_range_expression(&mut self) -> Expression {
        let left = self.parse_additive_expression();

        if self.peek(0).kind == TokenKind::DotDot {
            self.advance();
            let right = self.parse_additive_expression();
            let position = Position::from(left.get_position(), right.get_position());
            Self::collection(
                CollectionKind::Range(Box::new(left), Box::new(right)),
                position,
            )
        } else {
            left
        }
    }

    fn parse_additive_expression(&mut self) -> Expression {
        let mut left = self.parse_multiplicative_expression();

        while self.token_matches(&[TokenKind::Plus, TokenKind::Minus]) {
            let operator = self.next_token();
            let right = self.parse_multiplicative_expression();
            left = Expression::Binary(BinaryExpression::new(left, operator, right));
        }

        left
    }

    fn parse_multiplicative_expression(&mut self) -> Expression {
        let mut left = self.parse_unary_expression();

        while self.token_matches(&[TokenKind::Star, TokenKind::Slash]) {
            let operator = self.next_token();
            let right = self.parse_unary_expression();
            left = Expression::Binary(BinaryExpression::new(left, operator, right));
        }

        left
    }

    fn parse_unary_expression(&mut self) -> Expression {
        if self.token_matches(&[TokenKind::Plus, TokenKind::Minus, TokenKind::Bang]) {
            let operator = self.next_token();
            let right = self.parse_unary_expression();
            Expression::Unary(UnaryExpression::new(operator, right))
        } else {
            self.parse_postfix_expression()
        }
    }

    fn parse_primary_expression(&mut self) -> Expression {
        match self.peek(0).kind {
            TokenKind::Fun => {
                let start = self.next_token().position;
                Expression::Function(self.parse_function(start))
            }

            TokenKind::OpenBracket => self.parse_collection_literal(),
            TokenKind::OpenParen => {
                let open_paren = self.next_token();
                let expression = self.parse_expression();
                let close_paren = self.match_token(TokenKind::CloseParen);
                Expression::Parenthesized(ParenthesizedExpression::new(
                    open_paren,
                    expression,
                    close_paren,
                ))
            }

            TokenKind::True | TokenKind::False => {
                let token = self.next_token();
                let value = token.kind == TokenKind::True;
                Expression::Literal(LiteralExpression::new(
                    Object::Boolean(value),
                    token.position,
                ))
            }

            TokenKind::Number => {
                let token = self.next_token();
                let value = match token.lexeme.parse::<Number>() {
                    Ok(value) => value,
                    Err(_) => {
                        self.diagnostic_bag
                            .borrow_mut()
                            .report(token.position.clone(), "Invalid integer literal");
                        Number::default()
                    }
                };
                Expression::Literal(LiteralExpression::new(
                    Object::Number(value),
                    token.position,
                ))
            }

            TokenKind::String => {
                let token = self.next_token();
                Expression::Literal(LiteralExpression::new(
                    Object::String(token.lexeme),
                    token.position,
                ))
            }

            _ => {
                let identifier = self.match_token(TokenKind::Identifier);
                Expression::Name(NameExpression::new(identifier))
            }
        }
    }

    fn collection(kind: CollectionKind, position: Position) -> Expression {
        Expression::Collection(CollectionExpression { kind, position })
    }

    fn parse_function(&mut self, start: Position) -> FunctionExpression {
        self.match_token(TokenKind::OpenParen);

        let mut parameters = Vec::new();

        while !self.token_matches(&[TokenKind::CloseParen, TokenKind::Eof]) {
            let name = self.match_token(TokenKind::Identifier);

            self.match_token(TokenKind::Colon);

            parameters.push((name, self.parse_type()));

            if self.peek(0).kind != TokenKind::Comma {
                break;
            }

            self.advance();
        }

        self.match_token(TokenKind::CloseParen);

        let result = if self.peek(0).kind == TokenKind::Arrow {
            self.advance();
            self.parse_type()
        } else {
            Type::Unit
        };

        let body = self.parse_block_statement();

        let end = self.tokens[self.current.saturating_sub(1).min(self.tokens.len() - 1)]
            .position
            .clone();

        FunctionExpression {
            parameters,
            result,
            body: Box::new(body),
            position: Position::from(start, end),
        }
    }

    fn parse_type(&mut self) -> Type {
        if self.peek(0).kind == TokenKind::Fun {
            self.advance();
            self.match_token(TokenKind::OpenParen);

            let mut parameters = Vec::new();

            while !self.token_matches(&[TokenKind::CloseParen, TokenKind::Eof]) {
                parameters.push(self.parse_type());

                if self.peek(0).kind != TokenKind::Comma {
                    break;
                }

                self.advance();
            }

            self.match_token(TokenKind::CloseParen);
            self.match_token(TokenKind::Arrow);

            return Type::Function(parameters, Box::new(self.parse_type()));
        }

        if self.peek(0).kind == TokenKind::OpenBracket {
            self.advance();

            let mut fields = std::collections::BTreeMap::new();

            while !self.token_matches(&[TokenKind::CloseBracket, TokenKind::Eof]) {
                let name = self.match_token(TokenKind::Identifier);

                self.match_token(TokenKind::Colon);

                if fields.insert(name.lexeme, self.parse_type()).is_some() {
                    self.diagnostic_bag
                        .borrow_mut()
                        .report(name.position, "Duplicate record type field");
                }

                if self.peek(0).kind != TokenKind::Comma {
                    break;
                }

                self.advance();
            }

            self.match_token(TokenKind::CloseBracket);

            return Type::Record(fields);
        }

        let token = self.match_token(TokenKind::Identifier);

        match token.lexeme.as_str() {
            "Unit" => Type::Unit,
            "Number" => Type::Number,
            "String" => Type::String,
            "Boolean" => Type::Boolean,
            "Range" => Type::Range,
            "List" => {
                self.match_token(TokenKind::Lesser);
                let element = self.parse_type();
                self.match_token(TokenKind::Greater);
                Type::List(Box::new(element))
            }

            "Map" => {
                self.match_token(TokenKind::Lesser);
                let key = self.parse_type();
                self.match_token(TokenKind::Comma);
                let value = self.parse_type();
                self.match_token(TokenKind::Greater);
                Type::Map(Box::new(key), Box::new(value))
            }

            _ => {
                self.diagnostic_bag
                    .borrow_mut()
                    .report(token.position, "Unknown type annotation");
                Type::Unit
            }
        }
    }

    fn parse_postfix_expression(&mut self) -> Expression {
        let mut expression = self.parse_primary_expression();
        loop {
            let start = expression.get_position();

            match self.peek(0).kind {
                TokenKind::OpenBracket => {
                    self.advance();
                    let index = self.parse_expression();
                    let end = self.match_token(TokenKind::CloseBracket).position;
                    expression = Self::collection(
                        CollectionKind::Index(Box::new(expression), Box::new(index)),
                        Position::from(start, end),
                    );
                }

                TokenKind::Dot => {
                    self.advance();
                    let field = self.match_token(TokenKind::Identifier);
                    let end = field.position.clone();
                    let index = Expression::Literal(LiteralExpression::new(
                        Object::String(field.lexeme),
                        field.position,
                    ));
                    expression = Self::collection(
                        CollectionKind::Index(Box::new(expression), Box::new(index)),
                        Position::from(start, end),
                    );
                }

                TokenKind::OpenParen => {
                    self.advance();

                    let mut arguments = Vec::new();

                    while !self.token_matches(&[TokenKind::CloseParen, TokenKind::Eof]) {
                        arguments.push(self.parse_expression());

                        if self.peek(0).kind != TokenKind::Comma {
                            break;
                        }

                        self.advance();
                    }

                    let end = self.match_token(TokenKind::CloseParen).position;

                    expression = Self::collection(
                        CollectionKind::Call(Box::new(expression), arguments),
                        Position::from(start, end),
                    );
                }

                _ => break,
            }
        }

        expression
    }

    fn parse_collection_literal(&mut self) -> Expression {
        let start = self.match_token(TokenKind::OpenBracket).position;
        let kind = if self.peek(0).kind == TokenKind::CloseBracket {
            CollectionKind::List(Vec::new())
        } else if self.peek(0).kind == TokenKind::Colon {
            self.advance();
            CollectionKind::Map(Vec::new())
        } else if self.peek(0).kind == TokenKind::Identifier
            && self.peek(1).kind == TokenKind::Colon
        {
            let mut fields = Vec::new();
            loop {
                let name = self.match_token(TokenKind::Identifier).lexeme;

                self.match_token(TokenKind::Colon);

                fields.push((name, self.parse_expression()));

                if self.peek(0).kind != TokenKind::Comma {
                    break;
                }

                self.advance();

                if self.token_matches(&[TokenKind::CloseBracket, TokenKind::Eof]) {
                    break;
                }
            }

            CollectionKind::Record(fields)
        } else {
            let first = self.parse_expression();

            if self.peek(0).kind == TokenKind::Semicolon {
                self.advance();
                CollectionKind::Repeat(Box::new(first), Box::new(self.parse_expression()))
            } else if self.peek(0).kind == TokenKind::Colon {
                self.advance();

                let mut entries = vec![(first, self.parse_expression())];

                while self.peek(0).kind == TokenKind::Comma {
                    self.advance();

                    if self.token_matches(&[TokenKind::CloseBracket, TokenKind::Eof]) {
                        break;
                    }

                    let key = self.parse_expression();

                    self.match_token(TokenKind::Colon);

                    entries.push((key, self.parse_expression()));
                }

                CollectionKind::Map(entries)
            } else {
                let mut values = vec![first];

                while self.peek(0).kind == TokenKind::Comma {
                    self.advance();

                    if self.token_matches(&[TokenKind::CloseBracket, TokenKind::Eof]) {
                        break;
                    }

                    values.push(self.parse_expression());
                }

                CollectionKind::List(values)
            }
        };
        let end = self.match_token(TokenKind::CloseBracket).position;
        Self::collection(kind, Position::from(start, end))
    }

    fn peek(&self, offset: usize) -> Token {
        let index = offset + self.current;

        if index < self.tokens.len() {
            self.tokens[index].clone()
        } else {
            self.tokens.last().expect("lexer always emits EOF").clone()
        }
    }

    fn advance(&mut self) {
        self.current += 1;
    }

    fn next_token(&mut self) -> Token {
        let token = self.peek(0);
        self.advance();
        token
    }

    fn match_token(&mut self, kind: TokenKind) -> Token {
        let token = self.peek(0);

        if kind == token.kind {
            self.next_token()
        } else {
            self.diagnostic_bag.borrow_mut().unexpected_token(
                token.position,
                kind.clone(),
                token.kind,
            );
            let token = Token::new(kind, "%GENERATED%".to_string(), self.peek(0).position);

            if self.peek(0).kind != TokenKind::Eof {
                self.advance();
            }

            token
        }
    }

    fn token_matches(&self, kinds: &[TokenKind]) -> bool {
        kinds.contains(&self.peek(0).kind)
    }
}
