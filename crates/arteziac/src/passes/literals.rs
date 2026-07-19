use crate::analysis::{Analysis, LitValue};
use crate::ast;
use crate::parser::Span;
use artezia_diag::{Diagnostic, Severity};

pub fn decode_literals(
    file: &ast::File,
    src: &str,
    a: &mut Analysis,
    diags: &mut Vec<Diagnostic>,
) {
    let mut d = Decoder { src, a, diags };
    for item in &file.items {
        if let ast::Item::Func(f) = item {
            // Params and return types hold no literals; only bodies do.
            d.walk_block(&f.body);
        }
    }
}

struct Decoder<'a> {
    src: &'a str,
    a: &'a mut Analysis,
    diags: &'a mut Vec<Diagnostic>,
}

impl Decoder<'_> {
    fn walk_block(&mut self, b: &ast::Block) {
        for s in &b.stmts {
            self.walk_stmt(s);
        }
    }

    fn walk_stmt(&mut self, s: &ast::Stmt) {
        match s {
            ast::Stmt::Let { init, .. } => self.walk_expr(init),
            ast::Stmt::Expr(e) => self.walk_expr(e),
            ast::Stmt::While { cond, body, .. } => {
                self.walk_expr(cond);
                self.walk_block(body);
            }
            ast::Stmt::For { iter, body, .. } => {
                self.walk_expr(iter);
                self.walk_block(body);
            }
            ast::Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.walk_expr(v);
                }
            }
            ast::Stmt::Break { .. } | ast::Stmt::Continue { .. } => {}
        }
    }

    fn walk_expr(&mut self, e: &ast::Expr) {
        match e {
            ast::Expr::Int { id, span } => self.decode_int(*id, span),
            ast::Expr::Float { id, span } => self.decode_float(*id, span),
            ast::Expr::Bool { id, span } => self.decode_bool(*id, span),
            ast::Expr::Char { id, span } => self.decode_char_lit(*id, span),
            ast::Expr::Str { id, span } => self.decode_str(*id, span),
            ast::Expr::Duration { id, span } => self.decode_duration(*id, span),

            ast::Expr::Unary { rhs, .. } => self.walk_expr(rhs),
            ast::Expr::Binary { lhs, rhs, .. } => {
                self.walk_expr(lhs);
                self.walk_expr(rhs);
            }
            ast::Expr::Assign { target, value, .. } => {
                self.walk_expr(target);
                self.walk_expr(value);
            }
            ast::Expr::Call { callee, args, .. } => {
                self.walk_expr(callee);
                for arg in args {
                    self.walk_expr(&arg.value);
                }
            }
            ast::Expr::Field { obj, .. } => self.walk_expr(obj),
            ast::Expr::Index { obj, index, .. } => {
                self.walk_expr(obj);
                self.walk_expr(index);
            }
            ast::Expr::Range { lo, hi, .. } => {
                self.walk_expr(lo);
                self.walk_expr(hi);
            }
            ast::Expr::If { cond, then, els, .. } => {
                self.walk_expr(cond);
                self.walk_block(then);
                if let Some(e) = els {
                    self.walk_expr(e);
                }
            }
            ast::Expr::Block(b) => self.walk_block(b),
            ast::Expr::Scope { body, .. } => self.walk_block(body),
            ast::Expr::Spawn { call, .. } => self.walk_expr(call),
            ast::Expr::Within { dur, body, els, .. } => {
                self.walk_expr(dur);
                self.walk_block(body);
                if let Some(b) = els {
                    self.walk_block(b);
                }
            }

            ast::Expr::Var { .. } | ast::Expr::Error { .. } => {}
        }
    }

    fn text(&self, span: &Span) -> &str {
        &self.src[span.clone()]
    }

    fn error(&mut self, span: &Span, msg: impl Into<String>) {
        self.diags
            .push(Diagnostic::new(Severity::Error, span.clone(), msg.into()));
    }

    fn decode_int(&mut self, id: ast::NodeId, span: &Span) {
        // The lexer guaranteed the shape (digits and underscores). This pass checks the content (does it fit in i64?)
        let text = self.text(span).replace('_', "");
        let v = match text.parse::<i64>() {
            Ok(v) => v,
            Err(_) => {
                self.error(span, "integer literal is too large for `Int`");
                0
            }
        };
        self.a.values.insert(id, LitValue::Int(v));
    }

    fn decode_float(&mut self, id: ast::NodeId, span: &Span) {
        let v = self.text(span).parse::<f64>().unwrap_or_else(|_| {
            self.error(span, "invalid float literal");
            0.0
        });
        self.a.values.insert(id, LitValue::Float(v));
    }

    fn decode_bool(&mut self, id: ast::NodeId, span: &Span) {
        // Lexer regex was (true|false); text comparison is total
        let v = self.text(span) == "true";
        self.a.values.insert(id, LitValue::Bool(v));
    }

    fn decode_char_lit(&mut self, id: ast::NodeId, span: &Span) {
        // Span covers the full literal including quotes: 'a' or '\n'
        // Lexer regex '([^'\\\n]|\\.)' guarantees exactly one of those two shapes between the quotes
        let text = self.text(span);
        let inner = &text[1..text.len() - 1];
        let v = match decode_escape_or_char(inner) {
            Some(c) => c,
            None => {
                self.error(span, format!("unknown escape sequence `{inner}`"));
                '\0'
            }
        };
        self.a.values.insert(id, LitValue::Char(v));
    }

    fn decode_str(&mut self, id: ast::NodeId, span: &Span) {
        // Span includes the surrounding quotes; process escapes inside
        let text = self.text(span).to_owned(); // owned: we mutate self below
        let inner = &text[1..text.len() - 1];
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        let mut bad_escape = false;

        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next().and_then(escape_char) {
                Some(decoded) => out.push(decoded),
                None => {
                    bad_escape = true;
                    // keep going: report once, decode the rest best-effort
                }
            }
        }
        if bad_escape {
            self.error(span, "unknown escape sequence in string literal");
        }
        self.a.values.insert(id, LitValue::Str(out));
    }

    fn decode_duration(&mut self, id: ast::NodeId, span: &Span) {
        // Lexer regex [0-9]+(ns|us|ms|s|m|h): digits then suffix
        // Find the split point (first non-digit), normalize to nanoseconds
        let text = self.text(span);
        let split = text
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(text.len());
        let (digits, suffix) = text.split_at(split);
        let (digits, suffix) = (digits.to_owned(), suffix.to_owned());

        let multiplier: u64 = match suffix.as_str() {
            "ns" => 1,
            "us" => 1_000,
            "ms" => 1_000_000,
            "s" => 1_000_000_000,
            "m" => 60 * 1_000_000_000,
            "h" => 3_600 * 1_000_000_000,
            _ => {
                // Unreachable if the lexer regex is right; belt-and-braces
                self.error(span, format!("unknown duration suffix `{suffix}`"));
                1
            }
        };

        let v = digits
            .parse::<u64>()
            .ok()
            .and_then(|n| n.checked_mul(multiplier))
            .unwrap_or_else(|| {
                self.error(span, "duration literal is too large");
                0
            });
        self.a.values.insert(id, LitValue::Duration(v));
    }
}

/// `inner` is the content between a char literal's quotes: either a single
/// plain character or a backslash escape like `\n`
fn decode_escape_or_char(inner: &str) -> Option<char> {
    let mut chars = inner.chars();
    match chars.next()? {
        '\\' => escape_char(chars.next()?),
        c => Some(c),
    }
}

/// The escape table: `\n` → newline, etc. Extending escapes (e.g. \u{...})
/// happens HERE, once, for both char and string literals
fn escape_char(c: char) -> Option<char> {
    match c {
        'n' => Some('\n'),
        't' => Some('\t'),
        'r' => Some('\r'),
        '0' => Some('\0'),
        '\\' => Some('\\'),
        '\'' => Some('\''),
        '"' => Some('"'),
        _ => None,
    }
}
