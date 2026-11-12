//! The flex scanner of `cLexer.l`, transliterated to a hand-written longest
//! match scanner (ties go to the earlier rule, as in flex).

use crate::types::Tok;

/// `cLexer`: scans a byte buffer; `lineno` mirrors `yylineno`.
pub struct Lexer {
    src: Vec<u8>,
    pos: usize,
    lineno: i32,
    text: Vec<u8>,
}

/// Fixed-text rules in the order they appear in `cLexer.l`.
const FIXED: &[(&str, Tok)] = &[
    (";", Tok::Suppress),
    ("\n", Tok::Endl),
    (",", Tok::Comma),
    ("~", Tok::OpBitNot),
    ("&", Tok::OpBitAnd),
    ("|", Tok::OpBitOr),
    ("!", Tok::OpLogicNot),
    ("&&", Tok::OpLogicAnd),
    ("||", Tok::OpLogicOr),
    ("+", Tok::OpAdd),
    ("-", Tok::OpSub),
    ("*", Tok::OpMul),
    ("/", Tok::OpDiv),
    ("%", Tok::OpMod),
    (".", Tok::Dot),
    ("=", Tok::Assign),
    ("@", Tok::Ref),
    ("==", Tok::OpEq),
    ("<=", Tok::OpLe),
    (">=", Tok::OpGe),
    ("<", Tok::OpLt),
    (">", Tok::OpGt),
    ("!=", Tok::OpNeq),
    ("(", Tok::PrecOpen),
    (")", Tok::PrecClose),
    ("[", Tok::IdxOpen),
    ("]", Tok::IdxClose),
    ("{", Tok::ArrOpen),
    ("}", Tok::ArrClose),
    (":", Tok::ArrRange),
    ("^", Tok::ArrExpan),
    ("..", Tok::ArrWild),
    ("@{", Tok::LiteralDict),
    ("=>", Tok::DictMapping),
    ("${", Tok::LiteralMatrix),
    ("array", Tok::TypeArray),
    ("bool", Tok::TypeBool),
    ("char", Tok::TypeChar),
    ("dict", Tok::TypeDict),
    ("float", Tok::TypeFloat),
    ("int", Tok::TypeInt),
    ("matrix", Tok::TypeMatrix),
    ("string", Tok::TypeString),
    ("var", Tok::TypeVar),
    ("void", Tok::TypeVoid),
    ("if", Tok::CmdIf),
    ("else", Tok::CmdElse),
    ("elseif", Tok::CmdElseIf),
    ("while", Tok::CmdWhile),
    ("foreach", Tok::CmdForeach),
    ("function", Tok::CmdFunction),
    ("asbool", Tok::BuiltinCall),
    ("aschar", Tok::BuiltinCall),
    ("asint", Tok::BuiltinCall),
    ("asfloat", Tok::BuiltinCall),
    ("asstring", Tok::BuiltinCall),
    ("isarray", Tok::BuiltinCall),
    ("isbool", Tok::BuiltinCall),
    ("ischar", Tok::BuiltinCall),
    ("isdict", Tok::BuiltinCall),
    ("isint", Tok::BuiltinCall),
    ("isfloat", Tok::BuiltinCall),
    ("ismatrix", Tok::BuiltinCall),
    ("isstring", Tok::BuiltinCall),
    ("clear", Tok::BuiltinMethod),
    ("copy", Tok::BuiltinMethod),
    ("haskey", Tok::BuiltinMethod),
    ("keys", Tok::BuiltinMethod),
    ("len", Tok::BuiltinMethod),
    ("remove", Tok::BuiltinMethod),
    ("resize", Tok::BuiltinMethod),
    ("values", Tok::BuiltinMethod),
    ("return", Tok::CmdReturn),
    ("true", Tok::Bool),
    ("false", Tok::Bool),
];

impl Lexer {
    pub fn new(src: Vec<u8>) -> Self {
        Lexer {
            src,
            pos: 0,
            lineno: 1,
            text: Vec::new(),
        }
    }

    /// `yylineno`.
    pub fn lineno(&self) -> i32 {
        self.lineno
    }

    /// `YYText()`: the text of the last match.
    pub fn text(&self) -> &[u8] {
        &self.text
    }

    fn rest(&self) -> &[u8] {
        &self.src[self.pos..]
    }

    /// `[ \t]+`
    fn m_ws(r: &[u8]) -> usize {
        r.iter().take_while(|&&c| c == b' ' || c == b'\t').count()
    }

    /// `PREFIX.*\n` where `.` excludes newline.
    fn m_comment(r: &[u8], prefix: &[u8]) -> usize {
        if !r.starts_with(prefix) {
            return 0;
        }
        let mut i = prefix.len();
        while i < r.len() && r[i] != b'\n' {
            i += 1;
        }
        if i < r.len() {
            i + 1
        } else {
            0
        }
    }

    /// `([a-zA-Z]|_+[a-zA-Z])[a-zA-Z0-9_]*`
    fn m_id(r: &[u8]) -> usize {
        let mut i = 0;
        while i < r.len() && r[i] == b'_' {
            i += 1;
        }
        if i < r.len() && r[i].is_ascii_alphabetic() {
            i += 1;
            while i < r.len() && (r[i].is_ascii_alphanumeric() || r[i] == b'_') {
                i += 1;
            }
            i
        } else {
            0
        }
    }

    /// `[0-9]*\.[0-9]+([eE][-+]?[0-9]+)?`
    fn m_float(r: &[u8]) -> usize {
        let mut i = 0;
        while i < r.len() && r[i].is_ascii_digit() {
            i += 1;
        }
        if i >= r.len() || r[i] != b'.' {
            return 0;
        }
        i += 1;
        let ds = i;
        while i < r.len() && r[i].is_ascii_digit() {
            i += 1;
        }
        if i == ds {
            return 0;
        }
        if i < r.len() && (r[i] == b'e' || r[i] == b'E') {
            let mut j = i + 1;
            if j < r.len() && (r[j] == b'-' || r[j] == b'+') {
                j += 1;
            }
            let es = j;
            while j < r.len() && r[j].is_ascii_digit() {
                j += 1;
            }
            if j > es {
                i = j;
            }
        }
        i
    }

    /// `[0-9]+`
    fn m_int(r: &[u8]) -> usize {
        r.iter().take_while(|c| c.is_ascii_digit()).count()
    }

    /// `\"([^"\\\n]|\\\"|\\\\|\\.)*\"` (and `\"\"`).
    fn m_string(r: &[u8]) -> usize {
        if r.first() != Some(&b'"') {
            return 0;
        }
        let mut i = 1;
        while i < r.len() {
            match r[i] {
                b'"' => return i + 1,
                b'\n' => return 0,
                b'\\' => {
                    if i + 1 < r.len() && r[i + 1] != b'\n' {
                        i += 2;
                    } else {
                        return 0;
                    }
                }
                _ => i += 1,
            }
        }
        0
    }

    /// `\'([^'\\\n]|\\\'|\\\\|\\.)\'`
    fn m_char(r: &[u8]) -> usize {
        if r.first() != Some(&b'\'') || r.len() < 3 {
            return 0;
        }
        let (body, i) = match r[1] {
            b'\'' | b'\n' => return 0,
            b'\\' => {
                if r[2] == b'\n' {
                    return 0;
                }
                (true, 3)
            }
            _ => (true, 2),
        };
        if body && i < r.len() && r[i] == b'\'' {
            i + 1
        } else {
            0
        }
    }

    /// `yylex()`: returns the next token, `Tok::Eof` at end of input.
    pub fn lex(&mut self) -> Tok {
        loop {
            if self.pos >= self.src.len() {
                self.text.clear();
                return Tok::Eof;
            }
            let r = self.rest();
            // (length, token or None for ignored rules); the first rule
            // reaching the longest length wins.
            let mut best_len = 0usize;
            let mut best: Option<Tok> = None;
            let mut ignored = false;

            let mut consider = |len: usize, tok: Option<Tok>, ign: bool| {
                if len > best_len {
                    best_len = len;
                    best = tok;
                    ignored = ign;
                }
            };
            consider(Self::m_comment(r, b"#"), None, true);
            consider(Self::m_comment(r, b"//"), None, true);
            consider(Self::m_ws(r), None, true);
            for (s, t) in FIXED {
                if r.starts_with(s.as_bytes()) {
                    consider(s.len(), Some(*t), false);
                }
            }
            consider(Self::m_id(r), Some(Tok::Id), false);
            consider(Self::m_float(r), Some(Tok::Float), false);
            consider(Self::m_int(r), Some(Tok::Int), false);
            consider(Self::m_string(r), Some(Tok::Str), false);
            consider(Self::m_char(r), Some(Tok::Char), false);
            if r[0] != b'\n' {
                consider(1, Some(Tok::Unknown), false);
            }
            let len = best_len;
            debug_assert!(len > 0);
            let text = r[..len].to_vec();
            self.lineno += text.iter().filter(|&&c| c == b'\n').count() as i32;
            self.pos += len;
            self.text = text;
            if ignored {
                continue;
            }
            return best.unwrap_or(Tok::Unknown);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<(Tok, String)> {
        let mut l = Lexer::new(s.as_bytes().to_vec());
        let mut v = Vec::new();
        loop {
            let t = l.lex();
            if t == Tok::Eof {
                break;
            }
            v.push((t, String::from_utf8_lossy(l.text()).into_owned()));
        }
        v
    }

    #[test]
    fn keywords_and_ids() {
        let t = toks("int intx elseif else _a __ x1");
        let k: Vec<Tok> = t.iter().map(|x| x.0).collect();
        assert_eq!(
            k,
            vec![
                Tok::TypeInt,
                Tok::Id,
                Tok::CmdElseIf,
                Tok::CmdElse,
                Tok::Id,
                Tok::Unknown,
                Tok::Unknown,
                Tok::Id
            ]
        );
    }

    #[test]
    fn numbers_and_ops() {
        let k: Vec<Tok> = toks("1..2 1.5e3 .5 5. 1e5 a@{ =>")
            .iter()
            .map(|x| x.0)
            .collect();
        assert_eq!(
            k,
            vec![
                Tok::Int,
                Tok::ArrWild,
                Tok::Int,
                Tok::Float,
                Tok::Float,
                Tok::Int,
                Tok::Dot,
                Tok::Int,
                Tok::Id,
                Tok::Id,
                Tok::LiteralDict,
                Tok::DictMapping
            ]
        );
    }

    #[test]
    fn comments_and_strings() {
        let t = toks("# c\n\"a\\\"b\" 'x' // z\n#end");
        let k: Vec<Tok> = t.iter().map(|x| x.0).collect();
        assert_eq!(k, vec![Tok::Str, Tok::Char, Tok::Unknown, Tok::Id]);
    }
}
