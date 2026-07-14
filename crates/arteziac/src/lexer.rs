use logos::Logos;

#[derive(Logos, Copy, Clone, Debug, PartialEq)]
#[logos(skip r"[ \t\n\f]+")]
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

    #[regex("[a-zA-Z][a-zA-Z0-9_-]+")]
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
            Token::Plus => "+",
            Token::Minus => "-",
            Token::Mul => "*",
            Token::Div => "/",
            Token::Mod => "%",
            Token::Pow => "**",
            Token::Eqeq => "==",
            Token::Neq => "!=",
            Token::LT => "<",
            Token::GT => ">",
            Token::GTEQ => "<=",
            Token::LTEQ => ">=",
            Token::BitAnd => "&",
            Token::BitOr => "|",
            Token::BitXOr => "^",
            Token::BitLShift => "<<",
            Token::BitRShift => ">>",
            Token::BitNot => "~",
            Token::Eq => "=",
            Token::Dot => ".",
            Token::Arrow => "->",
            Token::Colon => ":",
            Token::PathSep => "::",
            Token::Comma => ",",
            Token::LParen => "(",
            Token::RParen => ")",
            Token::LBrace => "{",
            Token::RBrace => "}",
            Token::LBracket => "[",
            Token::RBracket => "]",
            
            Token::EOF => "EOF"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Token;
    use logos::Logos;

    #[test]
    fn test_keywords() {
        let lexer = Token::lexer("
            func
            let
            class
            pub
            return
            if
            else
            match
            for
            while
            in
            break
            continue
            import
            export
            scope
            nursery
            spawn
            within
            retry
            deadline
            defer
            select
            and
            or
            not
            as
            is
        ");

        let tokens: Vec<Token> = lexer.map(|tok| tok.unwrap()).collect();
        let expected = vec![
            Token::Func,
            Token::Let,
            Token::Class,
            Token::Pub,
            Token::Return,
            Token::If,
            Token::Else,
            Token::Match,
            Token::For,
            Token::While,
            Token::In,
            Token::Break,
            Token::Continue,
            Token::Import,
            Token::Export,
            Token::Scope,
            Token::Nursery,
            Token::Spawn,
            Token::Within,
            Token::Retry,
            Token::Deadline,
            Token::Defer,
            Token::Select,
            Token::And,
            Token::Or,
            Token::Not,
            Token::As,
            Token::Is
        ];

        assert_eq!(tokens, expected);
    }

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
            Token::Int, Token::Int, Token::Int, Token::Int,
            Token::Float, Token::Float, Token::Float,
            Token::Char, Token::Char, Token::Char,
            Token::String,
            Token::Ident, Token::Ident,
            Token::Bool, Token::Bool,
            Token::Duration, Token::Duration, Token::Duration, Token::Duration, Token::Duration
        ];

        assert_eq!(tokens, expected);
    }
}