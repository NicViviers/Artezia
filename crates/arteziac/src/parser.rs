use std::ops::Range;
use artezia_diag::{Diagnostic, Severity};
use super::lexer::Token;
use super::ast;

pub type Span = Range<usize>;

fn join(lhs: &Span, rhs: &Span) -> Span {
    lhs.start .. rhs.end
}

pub struct Parser {
    tokens: Vec<(Token, Span)>,
    pos: usize,
    next_id: u32,
    diags: Vec<Diagnostic>,
    eof_span: Span // src.len() .. src.len(), passed in at construction
}

impl Parser {
    pub fn new(tokens: Vec<(Token, Span)>) -> Self {
        Self {
            pos: 0,
            next_id: 0,
            diags: Vec::new(),
            eof_span: tokens.len() .. tokens.len(),
            tokens
        }
    }

    /// Returns current token
    fn cur(&self) -> (Token, Span) {
        self.tokens.get(self.pos).map(|(t, span)| (*t, span.clone())).unwrap_or_else(|| (Token::EOF, self.eof_span.clone()))
    }

    /// Returns next token
    fn peek(&self) -> (Token, Span) {
        self.tokens.get(self.pos + 1).map(|(t, span)| (*t, span.clone())).unwrap_or_else(|| (Token::EOF, self.eof_span.clone()))
    }

    /// Advances position to next token and returns span of current token
    fn advance(&mut self) -> Span {
        let range = self.cur().1;
        self.pos += 1;
        range
    }

    /// Consume the current token (and return it's span), advances position
    fn eat(&mut self, t: Token) -> Option<Span> {
        (self.cur().0 == t).then(|| self.advance())
    }

    /// Consume the current token (and return it's span), advancing position but NEVER fails
    fn expect(&mut self, t: Token, ctx: &str) -> Span {
        let (tok, span) = self.cur();

        if tok == t {
            self.advance();

            return span
        } else {
            let gap = span.start .. span.start;
            self.diags.push(Diagnostic::new(
                Severity::Error,
                gap.clone(),
                format!("expected {}, found {}", t.describe(), tok.describe())
            ).with_note(format!("while parsing {ctx}")));

            return gap
        }
    }

    fn mk_id(&mut self) -> ast::NodeId {
        self.next_id += 1;
        ast::NodeId(self.next_id - 1)
    }

    fn parse_prefix(&mut self) -> Option<ast::Expr> {
        let (tok, span) = self.cur();

        match tok {
            Token::Int => {
                self.advance();
                Some(ast::Expr::Int { id: self.mk_id(), span })
            }

            Token::Float => {
                self.advance();
                Some(ast::Expr::Float { id: self.mk_id(), span })
            }

            Token::Char => {
                self.advance();
                Some(ast::Expr::Char { id: self.mk_id(), span })
            }

            Token::String => {
                self.advance();
                Some(ast::Expr::Str { id: self.mk_id(), span })
            }

            Token::Ident => {
                self.advance();
                Some(ast::Expr::Var { id: self.mk_id(), span })
            }

            Token::Bool => {
                self.advance();
                Some(ast::Expr::Bool { id: self.mk_id(), span })
            }

            Token::Duration => {
                self.advance();
                Some(ast::Expr::Duration { id: self.mk_id(), span })
            }

            Token::Minus => {
                let start = self.advance();
                let rhs = self.parse_expr(23)?; // Unary minus / not must bind tighter than every infix op
                let span = join(&start, &rhs.span());
                Some(ast::Expr::Unary { id: self.mk_id(), op: ast::UnOp::Neg, rhs: Box::new(rhs), span })
            }

            _ => {
                self.diags.push(Diagnostic::new(
                    Severity::Error,
                    span.clone(),
                    format!("expected expression here, found {}", tok.describe())
                ));
                
                Some(ast::Expr::Error { id: self.mk_id(), span })
            }
        }
    }

    fn infix_binding_power(tok: Token) -> Option<(ast::BinOp, u8, u8)> {
        use ast::BinOp::*;

        Some(match tok {
            Token::Or => (Or, 3, 4),
            Token::And => (And, 5, 6),
            Token::Eqeq => (Eq, 7, 8),
            Token::LT => (Lt, 7, 8),
            Token::Plus => (Add, 11, 12),
            Token::Minus => (Sub, 11, 12),
            Token::Mul => (Mul, 13, 14),
            Token::Div => (Div, 13, 14),
            Token::Pow => (Pow, 16, 15),
            _ => return None
        })
    }

    fn parse_call(&mut self, callee: ast::Expr) -> Option<ast::Expr> {
        self.advance(); // `(`

        let mut args = Vec::new();

        while !matches!(self.cur().0, Token::RParen | Token::EOF) {
            // named arg? needs TWO tokens of lookahead: `ident :` but not `ident ::`
            let name_span = if self.cur().0 == Token::Ident && self.peek().0 == Token::Colon {
                let s = self.advance(); // ident
                self.advance(); // colon
                Some(s)
            } else {
                None
            };

            let value = self.parse_expr(0)?; // commas/parens reset min_bp
            let span = match &name_span {
                Some(n) => join(n, &value.span()),
                None => value.span()
            };

            args.push(ast::Arg { id: self.mk_id(), name_span, value, span });

            if self.eat(Token::Comma).is_none() { break; }
        }

        let end = self.expect(Token::RParen, "a call's argument list");
        let span = join(&callee.span(), &end);
        Some(ast::Expr::Call { id: self.mk_id(), callee: Box::new(callee), args, span })
    }

    fn parse_expr(&mut self, min_bp: u8) -> Option<ast::Expr> {
        let mut lhs = self.parse_prefix()?;


        loop {
            // postfix first: binds tighter than any infix
            match self.cur().0 {
                Token::LParen => { lhs = self.parse_call(lhs)?; continue; }
                Token::Dot => {
                    self.advance();

                    let name_span = self.expect(Token::Ident, "a field or method name");
                    let span = join(&lhs.span(), &name_span);
                    lhs = ast::Expr::Field {
                        id: self.mk_id(),
                        obj: Box::new(lhs),
                        name_span,
                        span
                    };
                    
                    continue;
                }

                Token::LBracket => {
                    self.advance();
                    
                    let index = self.parse_expr(0)?; // brackets reset min_bp
                    let end = self.expect(Token::RBracket, "an index expression");
                    let span = join(&lhs.span(), &end);
                    lhs = ast::Expr::Index {
                        id: self.mk_id(),
                        obj: Box::new(lhs),
                        index: Box::new(index),
                        span
                    };

                    continue;
                }

                _ => {}
            }

            // then infix
            let Some((op, l_bp, r_bp)) = Self::infix_binding_power(self.cur().0) else { break };

            if l_bp < min_bp { break; }

            self.advance();

            let rhs = self.parse_expr(r_bp)?;
            let span = join(&lhs.span(), &rhs.span());

            lhs = ast::Expr::Binary {
                id: self.mk_id(),
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span
            }
        }

        Some(lhs)
    }

    // TODO: parse_block, parse_stmt, parse_let, synchronize according to Step #2
}

#[cfg(test)]
mod tests {
    use logos::Logos;
    use super::{Span, Parser, ast};
    use super::super::lexer::Token;

    fn sample_tokens(src: &str) -> Vec<(Token, Span)> {
        Token::lexer(src).spanned().map(|(tok, span)| (tok.unwrap(), span)).collect()
    }

    #[test]
    fn test_prefix() {
        let tokens = sample_tokens("100 5.0 'a' \"foo\" bar true 100ms");
        let mut parser = Parser::new(tokens);
        let mut nodes = Vec::with_capacity(7);
        let expected = vec![
            ast::Expr::Int { id: ast::NodeId(0), span: 0 .. 3 },
            ast::Expr::Float { id: ast::NodeId(1), span: 4 .. 7 },
            ast::Expr::Char { id: ast::NodeId(2), span: 8 .. 11 },
            ast::Expr::Str { id: ast::NodeId(3), span: 12 .. 17 },
            ast::Expr::Var { id: ast::NodeId(4), span: 18 .. 21 },
            ast::Expr::Bool { id: ast::NodeId(5), span: 22 .. 26 },
            ast::Expr::Duration { id: ast::NodeId(6), span: 27 .. 32 }
        ];

        for _ in 0 .. 7 {
            nodes.push(parser.parse_prefix().unwrap());
        }

        assert_eq!(nodes, expected);
    }

    #[test]
    fn test_expr() {
        let tokens = sample_tokens("1 + 2 * 3");
        let mut parser = Parser::new(tokens);
        
        assert_eq!(parser.parse_expr(0).unwrap(), ast::Expr::Binary {
            id: ast::NodeId(4),
            op: ast::BinOp::Add,
            lhs: Box::new(ast::Expr::Int { id: ast::NodeId(0), span: 0 .. 1 }),
            rhs: Box::new(ast::Expr::Binary {
                id: ast::NodeId(3),
                op: ast::BinOp::Mul,
                lhs: Box::new(ast::Expr::Int { id: ast::NodeId(1), span: 4 .. 5 }),
                rhs: Box::new(ast::Expr::Int { id: ast::NodeId(2), span: 8 .. 9 }),
                span: 4 .. 9
            }),
            span: 0 .. 9
        });


        let tokens = sample_tokens("1 * 2 + 3");
        let mut parser = Parser::new(tokens);
        
        assert_eq!(parser.parse_expr(14).unwrap(), ast::Expr::Int { id: ast::NodeId(0), span: 0 .. 1 }); // Priority means breaking early here


        let tokens = sample_tokens("1 * 2 + 3");
        let mut parser = Parser::new(tokens);

        assert_eq!(parser.parse_expr(11).unwrap(), ast::Expr::Binary {
            id: ast::NodeId(4),
            op: ast::BinOp::Add,
            lhs: Box::new(ast::Expr::Binary {
                id: ast::NodeId(2),
                op: ast::BinOp::Mul,
                lhs: Box::new(ast::Expr::Int { id: ast::NodeId(0), span: 0..1 }),
                rhs: Box::new(ast::Expr::Int { id: ast::NodeId(1), span: 4..5 }),
                span: 0..5,
            }),
            rhs: Box::new(ast::Expr::Int { id: ast::NodeId(3), span: 8..9 }),
            span: 0..9,
        });


        let tokens = sample_tokens("2 ** 3 ** 2");
        let mut parser = Parser::new(tokens);

        assert_eq!(parser.parse_expr(16).unwrap(), ast::Expr::Binary {
            id: ast::NodeId(4),
            op: ast::BinOp::Pow,
            lhs: Box::new(ast::Expr::Int { id: ast::NodeId(0), span: 0 .. 1 }),
            rhs: Box::new(ast::Expr::Binary {
                id: ast::NodeId(3),
                op: ast::BinOp::Pow,
                lhs: Box::new(ast::Expr::Int { id: ast::NodeId(1), span: 5 .. 6 }),
                rhs: Box::new(ast::Expr::Int { id: ast::NodeId(2), span: 10 .. 11 }),
                span: 5 .. 11
            }),
            span: 0 .. 11
        });
    }
}