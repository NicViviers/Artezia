use logos::Logos;
use super::parser::Span;

#[derive(Logos, Copy, Clone, Debug, PartialEq)]
#[logos(skip r"[ \t\r]+")]
pub enum Token {
    // Keywords
    #[token("func")]
    Func,

    #[token("let")]
    Let,

    #[token("class")]
    Class,

    #[token("pub")]
    Pub,

    #[token("return")]
    Return,

    #[token("if", priority = 6)]
    If,

    #[token("else")]
    Else,

    #[token("match")]
    Match,

    #[token("for")]
    For,

    #[token("while")]
    While,

    #[token("in", priority = 6)]
    In,

    #[token("break")]
    Break,

    #[token("continue")]
    Continue,

    #[token("import")]
    Import,

    #[token("export")]
    Export,

    #[token("scope")]
    Scope,

    #[token("nursery")]
    Nursery,

    #[token("spawn")]
    Spawn,

    #[token("within")]
    Within,

    #[token("retry")]
    Retry,

    #[token("deadline")]
    Deadline,

    #[token("defer")]
    Defer,

    #[token("select")]
    Select,

    #[token("and")]
    And,

    #[token("or", priority = 6)]
    Or,

    #[token("not")]
    Not,

    #[token("as", priority = 6)]
    As,

    #[token("is", priority = 6)]
    Is,
    // TODO: Implement async, await, yield, const, static, unsafe, where, comptime


    // Literals
    #[regex("-?[0-9]+")]
    Int,

    #[regex("-?[0-9]+\\.[0-9]+")]
    Float,

    #[regex(r"'([^'\\\n]|\\.)'")]
    Char,

    // TODO: Is '.' for match-all really the only option here?
    #[regex("\".*\"", allow_greedy = true)]
    String,

    #[regex("[a-zA-Z_][a-zA-Z0-9_-]*")]
    Ident,

    #[regex("(true|false)")]
    Bool,

    #[regex("[0-9]+(us|ns|ms|s|h)")]
    Duration,


    // Operators & punctuation
    #[token("+")]
    Plus,

    #[token("-")]
    Minus,

    #[token("*")]
    Mul,

    #[token("/")]
    Div,

    #[token("%")]
    Mod,

    #[token("**")]
    Pow,

    #[token("==")]
    Eqeq,

    #[token("!=")]
    Neq,

    #[token("<")]
    LT,

    #[token(">")]
    GT,

    #[token("<=")]
    GTEQ,

    #[token(">=")]
    LTEQ,

    #[token("&")]
    BitAnd,

    #[token("|")]
    BitOr,

    #[token("^")]
    BitXOr,

    #[token("<<")]
    BitLShift,

    #[token(">>")]
    BitRShift,

    #[token("~")]
    BitNot,

    #[token("=")]
    Eq,

    // TODO: Implement +=, -=, *=, /=, and bitwise versions
    #[token(".")]
    Dot,

    #[token("..")]
    DotDot,

    #[token("..=")]
    DotDotEq,

    #[token("->")]
    Arrow,

    #[token(":")]
    Colon,

    #[token("::")]
    PathSep,

    #[token(",")]
    Comma,

   #[token("(")]
   LParen,

   #[token(")")]
   RParen,

   #[token("[")]
   LBracket,

   #[token("]")]
   RBracket,

   #[token("{")]
   LBrace,

   #[token("}")]
   RBrace,


   #[token(";")]
   Semi,

   #[token("\n")]
   Newline,
   StmtEnd,
   Error,
   EOF
}

impl Token {
    pub fn describe(self) -> &'static str {
        match self {
            Token::Func => "`func`",
            Token::Let => "`let`",
            Token::Class => "`class`",
            Token::Pub => "`pub`",
            Token::Return => "`return`",
            Token::If => "`if`",
            Token::Else => "`else`",
            Token::Match => "`match`",
            Token::For => "`for`",
            Token::While => "`while`",
            Token::In => "`in`",
            Token::Break => "`break`",
            Token::Continue => "`continue`",
            Token::Import => "`import`",
            Token::Export => "`export`",
            Token::Scope => "`scope`",
            Token::Nursery => "`nursery`",
            Token::Spawn => "`spawn`",
            Token::Within => "`within`",
            Token::Retry => "`retry`",
            Token::Deadline => "`deadline`",
            Token::Defer => "`defer`",
            Token::Select => "`select`",
            Token::And => "`and`",
            Token::Or => "`or`",
            Token::Not => "`not`",
            Token::As => "`as`",
            Token::Is => "`is`",
            Token::Int => "an integer",
            Token::Float => "a float",
            Token::Char => "a character",
            Token::String => "a string",
            Token::Ident => "an identifier",
            Token::Bool => "a boolean",
            Token::Duration => "a duration",
            Token::Plus => "`+`",
            Token::Minus => "`-`",
            Token::Mul => "`*`",
            Token::Div => "`/`",
            Token::Mod => "`%`",
            Token::Pow => "`**`",
            Token::Eqeq => "`==`",
            Token::Neq => "`!=`",
            Token::LT => "`<`",
            Token::GT => "`>`",
            Token::GTEQ => "`<=`",
            Token::LTEQ => "`>=`",
            Token::BitAnd => "`&`",
            Token::BitOr => "`|`",
            Token::BitXOr => "`^`",
            Token::BitLShift => "`<<`",
            Token::BitRShift => "`>>`",
            Token::BitNot => "`~`",
            Token::Eq => "`=`",
            Token::Dot => "`.`",
            Token::DotDot => "`..`",
            Token::DotDotEq => "`..=`",
            Token::Arrow => "`->`",
            Token::Colon => "`:`",
            Token::PathSep => "`::`",
            Token::Comma => "`,`",
            Token::LParen => "`(`",
            Token::RParen => "`)`",
            Token::LBrace => "`{`",
            Token::RBrace => "`}`",
            Token::LBracket => "`[`",
            Token::RBracket => "`]`",
            
            Token::Semi => "a semi-colon",
            Token::Newline => "a new line",
            Token::StmtEnd => "StmtEnd",
            Token::Error => "Error",
            Token::EOF => "EOF"
        }
    }
}

fn insert_stmt_ends(raw: Vec<(Token, Span)>) -> Vec<(Token, Span)> {
    let mut out = Vec::with_capacity(raw.len());
    for (tok, span) in raw {
        match tok {
            Token::Newline => {
                let ends_stmt = matches!(
                    out.last().map(|(t, _): &(Token, Span)| *t),
                    Some(Token::Ident | Token::Int | Token::Float | Token::String
                        | Token::Char | Token::Bool | Token::Duration
                        | Token::RParen | Token::RBracket | Token::RBrace
                        | Token::Return | Token::Break | Token::Continue)
                );
                if ends_stmt { out.push((Token::StmtEnd, span)); }
                // else: continuation — the newline vanishes entirely
            }
            Token::Semi => out.push((Token::StmtEnd, span)),
            t => out.push((t, span)),
        }
    }
    out
}

pub fn lex(src: &str) -> Vec<(Token, Span)> {
    let raw: Vec<(Token, Span)> = Token::lexer(src)
        .spanned()
        .map(|(tok, span)| (tok.unwrap_or(Token::Error), span))
        .collect();
    insert_stmt_ends(raw)
}

#[cfg(test)]
mod tests {
    use super::Token;
    use logos::Logos;

    #[test]
    fn test_literals() {
        let lexer = Token::lexer("
            0 38 105 -7
            0.0 -1.797 97.0051
            't' '0' '\\n'
            \"foo, bar!\"
            main foo
            true false
            10us 95ns 1000ms 5s 2h
        ");

        let tokens: Vec<Token> = lexer.map(|tok| tok.unwrap()).collect();
        let expected = vec![
            Token::Newline,
            Token::Int, Token::Int, Token::Int, Token::Int,
            Token::Newline,
            Token::Float, Token::Float, Token::Float,
            Token::Newline,
            Token::Char, Token::Char, Token::Char,
            Token::Newline,
            Token::String,
            Token::Newline,
            Token::Ident, Token::Ident,
            Token::Newline,
            Token::Bool, Token::Bool,
            Token::Newline,
            Token::Duration, Token::Duration, Token::Duration, Token::Duration, Token::Duration,
            Token::Newline
        ];

        assert_eq!(tokens, expected);
    }
}