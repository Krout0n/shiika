/// Parser
///
/// Implementaion rules
/// - Call `skip_ws`/`skip_wsn` before calling other `parse_xx`

macro_rules! parse_error {
    ( $self:ident, $( $arg:expr ),* ) => ({
        let msg = format!( $( $arg ),* );
        $self.parseerror(&msg)
    })
}

mod base;
pub mod token;
pub mod lexer;
mod definition_parser;
mod expression_parser;
use crate::ast;
use crate::error::Error;
use crate::parser::lexer::Lexer;

pub struct Parser<'a> {
    pub lexer: Lexer<'a>,
    /// For debug print
    pub lv: usize,
}

impl<'a> Parser<'a> {
    pub fn new(src: &str) -> Parser {
        Parser {
            lexer: Lexer::new(src),
            lv: 0,
        }
    }

    pub fn parse(src: &str) -> Result<ast::Program, Error> {
        let mut parser = Parser::new(src);
        parser.parse_program()
    }

    fn parse_program(&mut self) -> Result<ast::Program, Error> {
        self.skip_wsn();
        Ok(ast::Program {
            toplevel_defs: self.parse_definitions()?,
            exprs: self.parse_exprs()?,
        })
    }
}
