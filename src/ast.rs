use crate::names::*;

#[derive(Debug, PartialEq)]
pub struct Program {
    pub toplevel_defs: Vec<Definition>,
    pub exprs: Vec<Expression>,
}

#[derive(Debug, PartialEq)]
pub enum Definition {
    ClassDefinition {
        name: ClassName,
        defs: Vec<Definition>,
    },
    InitializerDefinition {
        sig: InitializerSig,
        body_exprs: Vec<Expression>,
    },
    InstanceMethodDefinition {
        sig: MethodSignature,
        body_exprs: Vec<Expression>,
    },
    ClassMethodDefinition {
        sig: MethodSignature,
        body_exprs: Vec<Expression>,
    }
}

#[derive(Debug, PartialEq)]
pub struct MethodSignature { // REFACTOR: rename (MethodSig or MethodSignatureAst)
    pub name: MethodName,
    pub params: Vec<Param>,
    pub ret_typ: Typ,
}

#[derive(Debug, PartialEq)]
pub struct InitializerSig {
    pub params: Vec<IParam>,
    pub ret_typ: Typ,
}

#[derive(Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub typ: Typ,
}

#[derive(Debug, PartialEq)]
pub struct IParam {
    pub name: String,
    pub typ: Typ,
}

#[derive(Debug, PartialEq)]
pub struct Typ {
    pub name: String,
}

#[derive(Debug, PartialEq)]
pub struct Expression {
    pub body: ExpressionBody,
    pub primary: bool,
}

#[derive(Debug, PartialEq)]
pub enum ExpressionBody {
    LogicalNot {
        expr: Box<Expression>,
    },
    LogicalAnd {
        left: Box<Expression>,
        right: Box<Expression>,
    },
    LogicalOr {
        left: Box<Expression>,
        right: Box<Expression>,
    },
    If {
        cond_expr: Box<Expression>,
        then_expr: Box<Expression>,
        else_expr: Option<Box<Expression>> // Box is needed to aboid E0072
    },
    MethodCall {
        receiver_expr: Option<Box<Expression>>, // Box is needed to aboid E0072
        method_name: MethodName,
        arg_exprs: Vec<Expression>,
        may_have_paren_wo_args: bool,
    },
    // Local variable reference or method call with implicit receiver(self)
    BareName(String),
    ConstRef(String),
    SelfExpr,
    FloatLiteral {
        value: f32,
    },
    DecimalLiteral {
        value: i32,
    }
}

impl Expression {
    pub fn may_have_paren_wo_args(&self) -> bool {
        match self.body {
            ExpressionBody::MethodCall { may_have_paren_wo_args, .. } => may_have_paren_wo_args,
            ExpressionBody::BareName(_) => true,
            _ => false,
        }
    }
}

pub fn logical_not(expr: Expression) -> Expression {
    non_primary_expression(
        ExpressionBody::LogicalNot {
            expr: Box::new(expr),
        }
    )
}

pub fn logical_and(left: Expression, right: Expression) -> Expression {
    non_primary_expression(
        ExpressionBody::LogicalAnd {
            left: Box::new(left),
            right: Box::new(right),
        }
    )
}

pub fn logical_or(left: Expression, right: Expression) -> Expression {
    non_primary_expression(
        ExpressionBody::LogicalOr {
            left: Box::new(left),
            right: Box::new(right),
        }
    )
}

pub fn if_expr(cond_expr: Expression, then_expr: Expression, else_expr: Option<Expression>) -> Expression {
    non_primary_expression(
        ExpressionBody::If {
            cond_expr: Box::new(cond_expr),
            then_expr: Box::new(then_expr),
            else_expr: else_expr.map(|e| Box::new(e)),
        }
    )
}

pub fn method_call(receiver_expr: Option<Expression>,
                   method_name: &str,
                   arg_exprs: Vec<Expression>,
                   primary: bool,
                   may_have_paren_wo_args: bool) -> Expression {
    Expression {
        primary: primary,
        body: ExpressionBody::MethodCall {
            receiver_expr: receiver_expr.map(|e| Box::new(e)),
            method_name: MethodName(method_name.to_string()),
            arg_exprs,
            may_have_paren_wo_args,
        }
    }
}

pub fn bare_name(name: &str) -> Expression {
    primary_expression(ExpressionBody::BareName(name.to_string()))
}

pub fn const_ref(name: &str) -> Expression {
    primary_expression(ExpressionBody::ConstRef(name.to_string()))
}

pub fn bin_op_expr(left: Expression, op: &str, right: Expression) -> Expression {
    Expression {
        primary: false,
        body: ExpressionBody::MethodCall {
            receiver_expr: Some(Box::new(left)),
            method_name: MethodName(op.to_string()),
            arg_exprs: vec![right],
            may_have_paren_wo_args: false,
        }
    }
}

pub fn self_expression() -> Expression {
    primary_expression(ExpressionBody::SelfExpr)
}

pub fn float_literal(value: f32) -> Expression {
    primary_expression(ExpressionBody::FloatLiteral{ value })
}

pub fn decimal_literal(value: i32) -> Expression {
    primary_expression(ExpressionBody::DecimalLiteral{ value })
}

pub fn primary_expression(body: ExpressionBody) -> Expression {
    Expression { primary: true, body: body }
}

pub fn non_primary_expression(body: ExpressionBody) -> Expression {
    Expression { primary: false, body: body }
}

/// Extend `foo.bar` to `foo.bar args`
/// (expr must be a MethodCall or a BareName)
pub fn set_method_call_args(expr: Expression, args: Vec<Expression>) -> Expression {
    match expr.body {
        ExpressionBody::MethodCall { receiver_expr, method_name, arg_exprs, .. } => {
            if !arg_exprs.is_empty() {
                panic!("[BUG] cannot extend because arg_exprs is not empty: {:?}", arg_exprs);
            }

            Expression {
                primary: false,
                body: ExpressionBody::MethodCall {
                    receiver_expr,
                    method_name,
                    arg_exprs: args,
                    may_have_paren_wo_args: false,
                }
            }
        },
        ExpressionBody::BareName(s) => {
            Expression {
                primary: false,
                body: ExpressionBody::MethodCall {
                    receiver_expr: None,
                    method_name: MethodName(s.to_string()),
                    arg_exprs: args,
                    may_have_paren_wo_args: false,
                }
            }
        },
        b => panic!("[BUG] `extend' takes a MethodCall but got {:?}", b)
    }
}
