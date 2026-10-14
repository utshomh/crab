use crate::common::{
    diagnostic::Position,
    types::{Object, Type},
};

use super::token::Token;

#[derive(Debug, Clone)]
pub enum Expression {
    Literal(LiteralExpression),
    Name(NameExpression),
    Parenthesized(ParenthesizedExpression),
    Unary(UnaryExpression),
    Binary(BinaryExpression),
    Collection(CollectionExpression),
    Function(FunctionExpression),
}

impl Expression {
    pub(crate) fn get_position(&self) -> Position {
        match self {
            Expression::Literal(expression) => expression.get_position(),
            Expression::Name(expression) => expression.get_position(),
            Expression::Parenthesized(expression) => expression.get_position(),
            Expression::Unary(expression) => expression.get_position(),
            Expression::Binary(expression) => expression.get_position(),
            Expression::Collection(expression) => expression.position.clone(),
            Expression::Function(expression) => expression.position.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LiteralExpression {
    pub(crate) value: Object,
    position: Position,
}

impl LiteralExpression {
    pub(super) fn new(value: Object, position: Position) -> Self {
        Self { value, position }
    }

    pub(crate) fn get_position(&self) -> Position {
        self.position.clone()
    }
}

#[derive(Debug, Clone)]
pub struct NameExpression {
    pub(crate) identifier: Token,
}

impl NameExpression {
    pub(super) fn new(identifier: Token) -> Self {
        Self { identifier }
    }

    pub(crate) fn get_position(&self) -> Position {
        self.identifier.position.clone()
    }
}

#[derive(Debug, Clone)]
pub struct ParenthesizedExpression {
    open_paren: Token,
    pub(crate) expression: Box<Expression>,
    close_paren: Token,
}

impl ParenthesizedExpression {
    pub(super) fn new(open_paren: Token, expression: Expression, close_paren: Token) -> Self {
        Self {
            open_paren,
            expression: Box::new(expression),
            close_paren,
        }
    }

    pub(crate) fn get_position(&self) -> Position {
        Position::from(
            self.open_paren.position.clone(),
            self.close_paren.position.clone(),
        )
    }
}

#[derive(Debug, Clone)]
pub struct UnaryExpression {
    pub(crate) operator: Token,
    pub(crate) right: Box<Expression>,
}

impl UnaryExpression {
    pub(super) fn new(operator: Token, right: Expression) -> Self {
        Self {
            operator,
            right: Box::new(right),
        }
    }

    pub(crate) fn get_position(&self) -> Position {
        Position::from(self.operator.position.clone(), self.right.get_position())
    }
}

#[derive(Debug, Clone)]
pub struct BinaryExpression {
    pub(crate) left: Box<Expression>,
    pub(crate) operator: Token,
    pub(crate) right: Box<Expression>,
}

impl BinaryExpression {
    pub(super) fn new(left: Expression, operator: Token, right: Expression) -> Self {
        Self {
            left: Box::new(left),
            operator,
            right: Box::new(right),
        }
    }

    pub(crate) fn get_position(&self) -> Position {
        Position::from(self.left.get_position(), self.right.get_position())
    }
}

#[derive(Debug, Clone)]
pub struct CollectionExpression {
    pub(crate) kind: CollectionKind,
    pub(crate) position: Position,
}

#[derive(Debug, Clone)]
pub struct FunctionExpression {
    pub(crate) parameters: Vec<(Token, Type)>,
    pub(crate) result: Type,
    pub(crate) body: Box<Statement>,
    pub(crate) position: Position,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Expression(ExpressionStatement),
    Function(String, FunctionExpression),
    Return(Option<Expression>, Position),
    Var(VarStatement),
    Block(BlockStatement),
    If(IfStatement),
    While(WhileStatement),
    For(ForStatement),
    Each(EachStatement),
}

#[derive(Debug, Clone)]
pub struct ExpressionStatement {
    pub(crate) expression: Expression,
}

impl ExpressionStatement {
    pub(super) fn new(expression: Expression) -> Self {
        Self { expression }
    }
}

#[derive(Debug, Clone)]
pub struct VarStatement {
    pub(crate) identifier: Token,
    pub(crate) expression: Expression,
    pub(crate) annotation: Option<Type>,
}

impl VarStatement {
    pub(super) fn new(identifier: Token, expression: Expression) -> Self {
        Self {
            identifier,
            expression,
            annotation: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BlockStatement {
    pub(crate) statements: Vec<Statement>,
}

impl BlockStatement {
    pub(super) fn new(statements: Vec<Statement>) -> Self {
        Self { statements }
    }
}

#[derive(Debug, Clone)]
pub struct IfStatement {
    pub(crate) condition: Expression,
    pub(crate) consequence: Box<Statement>,
    pub(crate) else_clause: Box<Option<Statement>>,
}

impl IfStatement {
    pub(super) fn new(
        condition: Expression,
        consequence: Statement,
        else_clause: Option<Statement>,
    ) -> Self {
        Self {
            condition,
            consequence: Box::new(consequence),
            else_clause: Box::new(else_clause),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WhileStatement {
    pub(crate) condition: Expression,
    pub(crate) body: Box<Statement>,
}

impl WhileStatement {
    pub(super) fn new(condition: Expression, body: Statement) -> Self {
        Self {
            condition,
            body: Box::new(body),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ForStatement {
    pub(crate) identifier: Token,
    pub(crate) lower_bound: Expression,
    pub(crate) upper_bound: Expression,
    pub(crate) body: Box<Statement>,
}

impl ForStatement {
    pub(super) fn new(
        identifier: Token,
        lower_bound: Expression,
        upper_bound: Expression,
        body: Statement,
    ) -> Self {
        Self {
            identifier,
            lower_bound,
            upper_bound,
            body: Box::new(body),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum CollectionKind {
    List(Vec<Expression>),
    Repeat(Box<Expression>, Box<Expression>),
    Record(Vec<(String, Expression)>),
    Map(Vec<(Expression, Expression)>),
    Range(Box<Expression>, Box<Expression>),
    Index(Box<Expression>, Box<Expression>),
    Call(Box<Expression>, Vec<Expression>),
    Assign(Box<Expression>, Box<Expression>),
}

#[derive(Debug, Clone)]
pub struct EachStatement {
    pub(crate) identifier: Token,
    pub(crate) iterable: Expression,
    pub(crate) body: Box<Statement>,
}
