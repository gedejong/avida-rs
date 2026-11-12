//! Recursive descent parser, transliterated from `cParser.cc`.
//!
//! Evaluation order matters for the reported line numbers (`FILEPOS` reads the
//! scanner's current line, which depends on how far the lexer has looked
//! ahead), so every `new cAST...(FILEPOS, ...)` below takes its position at
//! the same point as the C++ argument list does.

use std::rc::Rc;

use crate::ast::*;
use crate::lexer::Lexer;
use crate::output;
use crate::types::{AsType, Tok, TypeInfo};

/// `ASParseError_t` (the cases the parser reports).
#[derive(Clone, Copy, PartialEq, Eq)]
enum ParseErr {
    UnexpectedToken,
    UnterminatedExpr,
    NullExpr,
    Eof,
    Empty,
    Internal,
}

/// `cParser`.
pub struct Parser {
    filename: Rc<str>,
    lexer: Option<Lexer>,
    tree: Option<NodeRef>,
    eof: bool,
    success: bool,
    cur_tok: Tok,
    next_tok: Tok,
    cur_text: Bytes,
    next_text: Bytes,
    err_eof: bool,
}

/// `cString::ParseEscapeSequences` on bytes. QUIRK: an unknown escape drops
/// both characters; a trailing backslash is dropped.
pub fn parse_escape_sequences(s: &[u8]) -> Bytes {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'\\' {
            i += 1;
            if i == s.len() {
                break;
            }
            match s[i] {
                b'b' => out.push(8),
                b'f' => out.push(12),
                b'n' => out.push(b'\n'),
                b'r' => out.push(b'\r'),
                b't' => out.push(b'\t'),
                b'\\' | b'"' | b'\'' | b'?' => out.push(s[i]),
                _ => {}
            }
        } else {
            out.push(s[i]);
        }
        i += 1;
    }
    out
}

impl Parser {
    pub fn new() -> Self {
        Parser {
            filename: Rc::from("(unknown)"),
            lexer: None,
            tree: None,
            eof: false,
            success: true,
            cur_tok: Tok::Invalid,
            next_tok: Tok::Invalid,
            cur_text: Vec::new(),
            next_text: Vec::new(),
            err_eof: false,
        }
    }

    /// `Parse(cFile&)`: `filename` is `input.GetFilename()`.
    pub fn parse(&mut self, filename: &str, src: Vec<u8>) -> bool {
        self.filename = Rc::from(filename);
        self.lexer = Some(Lexer::new(src));
        self.tree = self.parse_statement_list();
        if !self.eof && self.success {
            self.report(ParseErr::UnexpectedToken);
        }
        if self.tree.is_none() {
            self.report(ParseErr::Empty);
        }
        self.lexer = None;
        self.success
    }

    /// `ExtractTree()`.
    pub fn extract_tree(&mut self) -> Option<NodeRef> {
        self.tree.take()
    }

    // ---- token plumbing -------------------------------------------------

    fn lineno(&self) -> i32 {
        self.lexer.as_ref().map_or(0, |l| l.lineno())
    }

    fn pos(&self) -> FilePos {
        FilePos {
            filename: self.filename.clone(),
            line: self.lineno(),
        }
    }

    fn cur(&self) -> Tok {
        self.cur_tok
    }

    fn next_token(&mut self) -> Tok {
        if self.next_tok != Tok::Invalid {
            self.cur_tok = self.next_tok;
            self.next_tok = Tok::Invalid;
            self.cur_text = std::mem::take(&mut self.next_text);
        } else {
            let lx = self.lexer.as_mut().expect("lexer");
            self.cur_tok = lx.lex();
            self.cur_text = lx.text().to_vec();
        }
        self.cur_tok
    }

    fn peek_token(&mut self) -> Tok {
        if self.next_tok == Tok::Invalid {
            let lx = self.lexer.as_mut().expect("lexer");
            self.next_tok = lx.lex();
            self.next_text = lx.text().to_vec();
        }
        self.next_tok
    }

    fn text(&self) -> Bytes {
        self.cur_text.clone()
    }

    // ---- error reporting --------------------------------------------------

    fn report(&mut self, err: ParseErr) {
        self.success = false;
        let mut m: Vec<u8> = Vec::new();
        m.extend_from_slice(self.filename.as_bytes());
        m.push(b':');
        let ln = self.lineno();
        if ln != 0 {
            m.extend_from_slice(ln.to_string().as_bytes());
        } else {
            m.push(b'?');
        }
        m.extend_from_slice(b": error: ");
        match err {
            ParseErr::UnexpectedToken => {
                m.extend_from_slice(b"unexpected token '");
                m.extend_from_slice(&self.cur_text);
                m.extend_from_slice(b"'\n");
            }
            ParseErr::UnterminatedExpr => m.extend_from_slice(b"unterminated expression\n"),
            ParseErr::NullExpr => {
                m.extend_from_slice(b"expected expression, found '");
                m.extend_from_slice(&self.cur_text);
                m.extend_from_slice(b"'\n");
            }
            ParseErr::Eof => {
                if !self.err_eof {
                    m.extend_from_slice(b"unexpected end of file\n");
                    self.err_eof = true;
                } else {
                    // The prefix "file:line: error: " is still printed.
                }
            }
            ParseErr::Empty => m.extend_from_slice(b"empty script, no valid statements found\n"),
            ParseErr::Internal => {
                // The C++ prints "cParser.cc:<__LINE__>"; the line is the
                // PARSE_ERROR(INTERNAL) site in parseStatementList.
                m.extend_from_slice(b"internal parser error at cParser.cc:");
                m.extend_from_slice(PARSER_INTERNAL_LINE.to_string().as_bytes());
                m.push(b'\n');
            }
        }
        output::err_write(&m);
    }

    /// `PARSE_UNEXPECT()` (returns the NULL of the macro).
    fn unexpect<T>(&mut self) -> Option<T> {
        if self.cur() != Tok::Eof {
            self.report(ParseErr::UnexpectedToken);
        } else {
            self.report(ParseErr::Eof);
        }
        None
    }

    fn nn(&self, n: Option<NodeRef>) -> NodeRef {
        n.unwrap_or_else(|| Node::null(self.pos()))
    }

    // ---- productions -------------------------------------------------------

    fn parse_array_unpack(&mut self) -> Option<NodeRef> {
        if self.next_token() != Tok::Id {
            return self.unexpect();
        }
        let pos = self.pos();
        let mut names: Vec<Bytes> = vec![self.text()];
        let mut last_wild = false;
        let mut last_named = false;
        while self.next_token() != Tok::Eof {
            if self.cur() == Tok::Comma {
                self.next_token();
                if self.cur() == Tok::Id {
                    names.push(self.text());
                    continue;
                } else if self.cur() == Tok::ArrWild {
                    last_wild = true;
                    last_named = false;
                    break;
                } else {
                    self.report(ParseErr::UnexpectedToken);
                    break;
                }
            } else if self.cur() == Tok::ArrWild {
                last_wild = true;
                last_named = true;
                break;
            } else {
                return self.unexpect();
            }
        }
        if self.next_token() != Tok::ArrClose {
            return self.unexpect();
        }
        if self.next_token() != Tok::Assign {
            return self.unexpect();
        }
        self.next_token(); // consume '='
        let e = self.parse_expression();
        let expr = self.nn(e);
        let nodes = names
            .into_iter()
            .map(|n| UnpackNode {
                name: n,
                var_id: std::cell::Cell::new(-1),
                global: std::cell::Cell::new(false),
                ty: std::cell::RefCell::new(TypeInfo::invalid()),
            })
            .collect();
        Some(Node::new(
            pos,
            Kind::UnpackTarget {
                nodes,
                last_wild,
                last_named,
                expr,
            },
        ))
    }

    fn parse_argument_list(&mut self) -> NodeRef {
        let pos = self.pos();
        let mut v = Vec::new();
        let e = self.parse_expression();
        v.push(self.nn(e));
        while self.cur() == Tok::Comma {
            self.next_token(); // consume ','
            let e = self.parse_expression();
            v.push(self.nn(e));
        }
        Node::new(pos, Kind::ArgumentList(v))
    }

    fn parse_assignment(&mut self) -> Option<NodeRef> {
        let pos = self.pos();
        let var = self.text();
        self.next_token(); // consume id
        self.next_token(); // consume '='
        let e = self.parse_expression();
        let expr = self.nn(e);
        Some(Node::new(
            pos,
            Kind::Assignment {
                var,
                expr,
                id: std::cell::Cell::new(-1),
                global: std::cell::Cell::new(false),
            },
        ))
    }

    fn new_builtin(
        &self,
        pos: FilePos,
        name: &[u8],
        target: Option<NodeRef>,
        args: Option<NodeRef>,
    ) -> NodeRef {
        Node::new(
            pos,
            Kind::BuiltInCall {
                args,
                ty: std::cell::RefCell::new(TypeInfo::invalid()),
                builtin: builtin_from_name(name),
                target,
            },
        )
    }

    fn new_binary(&self, pos: FilePos, op: Tok, l: NodeRef, r: NodeRef) -> NodeRef {
        Node::new(
            pos,
            Kind::Binary {
                op,
                left: l,
                right: r,
                ty: std::cell::RefCell::new(TypeInfo::new(AsType::Invalid)),
                compare_ty: std::cell::RefCell::new(TypeInfo::new(AsType::Invalid)),
            },
        )
    }

    fn parse_call_expression(
        &mut self,
        target: Option<NodeRef>,
        required: bool,
    ) -> Option<NodeRef> {
        let mut ce: NodeRef = self.nn(target);

        if self.cur() == Tok::Dot && self.peek_token() == Tok::BuiltinMethod {
            self.next_token(); // consume '.'
            let pos = self.pos();
            let name = self.text();
            if self.next_token() != Tok::PrecOpen {
                return self.unexpect();
            }
            let mut args = None;
            if self.next_token() != Tok::PrecClose {
                args = Some(self.parse_argument_list());
            }
            if self.cur() != Tok::PrecClose {
                return self.unexpect();
            }
            self.next_token(); // consume ')'
                               // QUIRK: `cur != DOT || cur != IDX_OPEN` is always true, so a
                               // builtin method call always ends the call expression.
            return Some(self.new_builtin(pos, &name, Some(ce), args));
        }

        let mut eoe = false;
        while !eoe {
            if self.cur() == Tok::Dot {
                if self.next_token() != Tok::Id {
                    return self.unexpect();
                }
                let name = self.text();
                self.next_token(); // consume id

                if self.cur() == Tok::PrecOpen {
                    let pos = self.pos();
                    let mut args = None;
                    if self.next_token() != Tok::PrecClose {
                        args = Some(self.parse_argument_list());
                    }
                    if self.cur() != Tok::PrecClose {
                        return self.unexpect();
                    }
                    ce = Node::new(
                        pos,
                        Kind::ObjectCall {
                            object: ce,
                            name,
                            args,
                        },
                    );
                    self.next_token(); // consume ')'
                    if self.cur() != Tok::IdxOpen && self.cur() != Tok::Dot {
                        eoe = true;
                    }
                } else {
                    let pos = self.pos();
                    ce = Node::new(pos, Kind::ObjectReference { object: ce, name });
                    if required && self.cur() == Tok::Assign {
                        let pos = self.pos();
                        self.next_token(); // consume '='
                        let e = self.parse_expression();
                        let expr = self.nn(e);
                        ce = Node::new(pos, Kind::ObjectAssignment { target: ce, expr });
                        eoe = true;
                    }
                }
            } else if self.cur() == Tok::IdxOpen {
                loop {
                    self.next_token(); // consume '['
                    let pos = self.pos();
                    let e = self.parse_expression();
                    let r = self.nn(e);
                    ce = self.new_binary(pos, Tok::IdxOpen, ce, r);
                    if self.cur() != Tok::IdxClose {
                        return self.unexpect();
                    }
                    if self.next_token() != Tok::IdxOpen {
                        break;
                    }
                }
                if required && self.cur() == Tok::Assign {
                    let pos = self.pos();
                    self.next_token(); // consume '='
                    let e = self.parse_expression();
                    let expr = self.nn(e);
                    ce = Node::new(pos, Kind::ObjectAssignment { target: ce, expr });
                    eoe = true;
                }
            } else if required {
                return self.unexpect();
            } else {
                eoe = true;
            }
        }
        Some(ce)
    }

    fn parse_code_block(&mut self) -> Option<NodeRef> {
        // Swallow all newlines and suppress tokens
        while self.cur() == Tok::Endl || self.cur() == Tok::Suppress {
            self.next_token();
        }
        if self.cur() == Tok::ArrOpen {
            return self.parse_loose_block();
        }
        self.unexpect()
    }

    fn parse_expression(&mut self) -> Option<NodeRef> {
        let expr = self.parse_expr_p0();
        if expr.is_none() {
            if self.cur() != Tok::Eof {
                self.report(ParseErr::NullExpr);
            } else {
                self.report(ParseErr::Eof);
            }
        }
        expr
    }

    fn binary_level(
        &mut self,
        ops: &[Tok],
        sub: fn(&mut Self) -> Option<NodeRef>,
    ) -> Option<NodeRef> {
        let mut l = sub(self);
        loop {
            let c = self.cur();
            if ops.contains(&c) {
                self.next_token();
                let r = sub(self);
                if r.is_none() {
                    self.report(ParseErr::NullExpr);
                }
                let pos = self.pos();
                let ln = self.nn(l.take());
                let rn = self.nn(r);
                l = Some(self.new_binary(pos, c, ln, rn));
            } else {
                return l;
            }
        }
    }

    fn parse_expr_p0(&mut self) -> Option<NodeRef> {
        self.binary_level(&[Tok::ArrRange, Tok::ArrExpan], Self::parse_expr_p1)
    }
    fn parse_expr_p1(&mut self) -> Option<NodeRef> {
        self.binary_level(&[Tok::OpLogicAnd, Tok::OpLogicOr], Self::parse_expr_p2)
    }
    fn parse_expr_p2(&mut self) -> Option<NodeRef> {
        self.binary_level(&[Tok::OpBitAnd, Tok::OpBitOr], Self::parse_expr_p3)
    }
    fn parse_expr_p3(&mut self) -> Option<NodeRef> {
        self.binary_level(
            &[
                Tok::OpEq,
                Tok::OpLe,
                Tok::OpGe,
                Tok::OpLt,
                Tok::OpGt,
                Tok::OpNeq,
            ],
            Self::parse_expr_p4,
        )
    }
    fn parse_expr_p4(&mut self) -> Option<NodeRef> {
        self.binary_level(&[Tok::OpAdd, Tok::OpSub], Self::parse_expr_p5)
    }
    fn parse_expr_p5(&mut self) -> Option<NodeRef> {
        self.binary_level(&[Tok::OpMul, Tok::OpDiv, Tok::OpMod], Self::parse_expr_p6)
    }

    fn literal(&self, ty: AsType, value: Bytes) -> NodeRef {
        Node::new(
            self.pos(),
            Kind::Literal {
                ty: TypeInfo::new(ty),
                value,
            },
        )
    }

    fn parse_expr_p6(&mut self) -> Option<NodeRef> {
        let expr: NodeRef;
        let mut is_matrix = false;
        let c = self.cur();
        match c {
            Tok::Float => expr = self.literal(AsType::Float, self.text()),
            Tok::Int => expr = self.literal(AsType::Int, self.text()),
            Tok::Char => {
                let t = self.text();
                expr = self.literal(AsType::Char, parse_escape_sequences(&t[1..t.len() - 1]));
            }
            Tok::Bool => expr = self.literal(AsType::Bool, self.text()),
            Tok::Str => {
                let t = self.text();
                expr = self.literal(AsType::String, parse_escape_sequences(&t[1..t.len() - 1]));
            }
            Tok::Id => {
                if self.peek_token() == Tok::PrecOpen {
                    let pos = self.pos();
                    let name = self.text();
                    self.next_token(); // consume id token
                    let mut args = None;
                    if self.next_token() != Tok::PrecClose {
                        args = Some(self.parse_argument_list());
                    }
                    if self.cur() != Tok::PrecClose {
                        return self.unexpect();
                    }
                    expr = new_function_call(pos, name, args);
                } else {
                    expr = Node::new(
                        self.pos(),
                        Kind::VariableReference {
                            name: self.text(),
                            ty: std::cell::RefCell::new(TypeInfo::invalid()),
                            id: std::cell::Cell::new(-1),
                            global: std::cell::Cell::new(false),
                        },
                    );
                }
            }
            Tok::BuiltinCall => {
                if self.peek_token() == Tok::PrecOpen {
                    let pos = self.pos();
                    let name = self.text();
                    self.next_token(); // consume builtin method name token
                    let mut args = None;
                    if self.next_token() != Tok::PrecClose {
                        args = Some(self.parse_argument_list());
                    }
                    if self.cur() != Tok::PrecClose {
                        return self.unexpect();
                    }
                    expr = self.new_builtin(pos, &name, None, args);
                } else {
                    return self.unexpect();
                }
            }
            Tok::PrecOpen => {
                self.next_token(); // consume '('
                match self.parse_expression() {
                    None => {
                        self.report(ParseErr::NullExpr);
                        if self.cur() != Tok::PrecClose {
                            return self.unexpect();
                        }
                        // QUIRK: the C++ continues with a NULL expression.
                        self.next_token();
                        return None;
                    }
                    Some(e) => expr = e,
                }
                if self.cur() != Tok::PrecClose {
                    return self.unexpect();
                }
            }
            Tok::LiteralDict => {
                let e = self.parse_literal_dict();
                match e {
                    Some(e) => expr = e,
                    None => {
                        if self.cur() != Tok::ArrClose {
                            return self.unexpect();
                        }
                        self.next_token();
                        return None;
                    }
                }
                if self.cur() != Tok::ArrClose {
                    return self.unexpect();
                }
            }
            Tok::LiteralMatrix | Tok::ArrOpen => {
                if c == Tok::LiteralMatrix {
                    is_matrix = true;
                }
                let mut al = None;
                if self.next_token() != Tok::ArrClose {
                    al = Some(self.parse_argument_list());
                }
                if self.cur() != Tok::ArrClose {
                    return self.unexpect();
                }
                expr = Node::new(
                    self.pos(),
                    Kind::LiteralArray {
                        values: al,
                        is_matrix,
                    },
                );
            }
            Tok::OpBitNot | Tok::OpLogicNot | Tok::OpSub => {
                let op = c;
                self.next_token(); // consume operation
                let r = self.parse_expr_p6();
                match r {
                    None => {
                        self.report(ParseErr::NullExpr);
                        return None;
                    }
                    Some(r) => {
                        return Some(Node::new(
                            self.pos(),
                            Kind::Unary {
                                op,
                                expr: r,
                                ty: std::cell::RefCell::new(TypeInfo::new(AsType::Invalid)),
                            },
                        ));
                    }
                }
            }
            _ => return None,
        }

        self.next_token();
        self.parse_call_expression(Some(expr), false)
    }

    fn type_of_token(t: Tok) -> Option<AsType> {
        Some(match t {
            Tok::TypeArray => AsType::Array,
            Tok::TypeBool => AsType::Bool,
            Tok::TypeChar => AsType::Char,
            Tok::TypeDict => AsType::Dict,
            Tok::TypeFloat => AsType::Float,
            Tok::TypeInt => AsType::Int,
            Tok::TypeMatrix => AsType::Matrix,
            Tok::TypeString => AsType::String,
            Tok::TypeVar => AsType::Var,
            Tok::TypeVoid => AsType::Void,
            _ => return None,
        })
    }

    /// The `case TOKEN(ID)` arm shared by the type switches: an object type
    /// `Name @`. Returns `None` after reporting when the `@` is missing.
    fn object_type(&mut self) -> Option<TypeInfo> {
        if self.peek_token() != Tok::Ref {
            self.next_token();
            return self.unexpect();
        }
        let info = String::from_utf8_lossy(&self.text()).into_owned();
        self.next_token(); // consume id
        Some(TypeInfo::with_info(AsType::ObjectRef, &info))
    }

    fn parse_foreach_statement(&mut self) -> Option<NodeRef> {
        let t = self.next_token();
        let ty = if let Some(a) = Self::type_of_token(t) {
            TypeInfo::new(a)
        } else if t == Tok::Id {
            self.object_type()?
        } else {
            return self.unexpect();
        };

        if self.next_token() != Tok::Id {
            return self.unexpect();
        }
        let vpos = self.pos();
        let name = self.text();
        let var = Node::new(
            vpos,
            Kind::VariableDefinition {
                ty,
                name,
                assign: None,
                dims: None,
                id: std::cell::Cell::new(-1),
            },
        );

        if self.next_token() != Tok::PrecOpen {
            return self.unexpect();
        }
        self.next_token(); // consume '('

        let expr = self.parse_expression();

        if self.cur() != Tok::PrecClose {
            return self.unexpect();
        }
        self.next_token(); // consume ')'

        let code = self.parse_code_block();
        let code = self.nn(code);
        let pos = self.pos();
        let values = self.nn(expr);
        Some(Node::new(pos, Kind::Foreach { var, values, code }))
    }

    fn parse_function_define(&mut self) -> Option<NodeRef> {
        let fd = self.parse_function_header()?;
        let code = self.parse_code_block();
        if let Kind::FunctionDefinition { code: c, .. } = &fd.kind {
            *c.borrow_mut() = code;
        }
        Some(fd)
    }

    fn parse_function_header(&mut self) -> Option<NodeRef> {
        let t = self.next_token();
        let ty = if let Some(a) = Self::type_of_token(t) {
            TypeInfo::new(a)
        } else if t == Tok::Id {
            self.object_type()?
        } else {
            return self.unexpect();
        };

        if self.next_token() != Tok::Id {
            return self.unexpect();
        }
        let name = self.text();

        if self.next_token() != Tok::PrecOpen {
            return self.unexpect();
        }

        let mut args = None;
        if self.next_token() != Tok::PrecClose {
            args = self.parse_variable_definition_list();
        }
        if self.cur() != Tok::PrecClose {
            return self.unexpect();
        }
        self.next_token(); // consume ')'

        let pos = self.pos();
        Some(Node::new(
            pos,
            Kind::FunctionDefinition {
                ty,
                name,
                args: std::cell::RefCell::new(args),
                code: std::cell::RefCell::new(None),
            },
        ))
    }

    fn parse_id_statement(&mut self) -> Option<NodeRef> {
        match self.peek_token() {
            Tok::Assign => self.parse_assignment(),
            Tok::PrecOpen => {
                let pos = self.pos();
                let name = self.text();
                self.next_token(); // consume id token
                let mut args = None;
                if self.next_token() != Tok::PrecClose {
                    args = Some(self.parse_argument_list());
                }
                if self.cur() != Tok::PrecClose {
                    return self.unexpect();
                }
                self.next_token(); // consume ')'
                let fc = new_function_call(pos, name, args);
                if self.cur() == Tok::Dot || self.cur() == Tok::IdxOpen {
                    return self.parse_call_expression(Some(fc), true);
                }
                Some(fc)
            }
            Tok::Dot | Tok::IdxOpen => {
                let target = Node::new(
                    self.pos(),
                    Kind::VariableReference {
                        name: self.text(),
                        ty: std::cell::RefCell::new(TypeInfo::invalid()),
                        id: std::cell::Cell::new(-1),
                        global: std::cell::Cell::new(false),
                    },
                );
                self.next_token(); // consume id
                self.parse_call_expression(Some(target), true)
            }
            Tok::Ref => self.parse_variable_definition(),
            _ => self.unexpect(),
        }
    }

    fn parse_if_statement(&mut self) -> Option<NodeRef> {
        if self.next_token() != Tok::PrecOpen {
            return self.unexpect();
        }
        self.next_token();
        let cond = self.parse_expression();
        if self.cur() != Tok::PrecClose {
            return self.unexpect();
        }
        self.next_token();

        let pos = self.pos();
        let cond = self.nn(cond);
        let code = self.parse_code_block();
        let code = self.nn(code);

        let mut elifs = Vec::new();
        while self.cur() == Tok::CmdElseIf {
            if self.next_token() != Tok::PrecOpen {
                return self.unexpect();
            }
            self.next_token(); // consume '('
            let ec = self.parse_expression();
            if self.cur() != Tok::PrecClose {
                return self.unexpect();
            }
            self.next_token(); // consume ')'
            let ecode = self.parse_code_block();
            let cond = self.nn(ec);
            let code = self.nn(ecode);
            elifs.push(ElseIf { cond, code });
        }

        let mut else_code = None;
        if self.cur() == Tok::CmdElse {
            self.next_token(); // consume 'else'
            let c = self.parse_code_block();
            else_code = Some(self.nn(c));
        }
        Some(Node::new(
            pos,
            Kind::If {
                cond,
                code,
                elifs,
                else_code,
            },
        ))
    }

    fn parse_literal_dict(&mut self) -> Option<NodeRef> {
        let pos = self.pos();
        let mut maps = Vec::new();
        if self.peek_token() != Tok::ArrClose {
            loop {
                self.next_token(); // consume ',' (or '@{' on first pass)
                let idx = self.parse_expression();
                if self.cur() != Tok::DictMapping {
                    return self.unexpect();
                }
                self.next_token(); // consume '=>'
                let val = self.parse_expression();
                let idx = self.nn(idx);
                let val = self.nn(val);
                maps.push(Mapping { idx, val });
                if self.cur() != Tok::Comma {
                    break;
                }
            }
        } else {
            self.next_token();
        }
        Some(Node::new(pos, Kind::LiteralDict(maps)))
    }

    fn parse_loose_block(&mut self) -> Option<NodeRef> {
        let sl = self.parse_statement_list();
        if self.cur() != Tok::ArrClose {
            return self.unexpect();
        }
        self.next_token(); // consume '}'
        sl
    }

    fn parse_ref_statement(&mut self) -> Option<NodeRef> {
        match self.next_token() {
            Tok::ArrOpen => self.parse_array_unpack(),
            Tok::CmdFunction => self.parse_function_header(),
            _ => self.unexpect(),
        }
    }

    fn parse_return_statement(&mut self) -> Option<NodeRef> {
        self.next_token(); // consume 'return'
        let pos = self.pos();
        let e = self.parse_expression();
        let e = self.nn(e);
        Some(Node::new(pos, Kind::Return(e)))
    }

    fn parse_statement_list(&mut self) -> Option<NodeRef> {
        let pos = self.pos();
        let mut stmts: Vec<NodeRef> = Vec::new();
        let mut node: Option<NodeRef>;

        while self.next_token() != Tok::Eof {
            match self.cur() {
                Tok::ArrOpen => node = self.parse_loose_block(),
                Tok::CmdIf => node = self.parse_if_statement(),
                Tok::CmdForeach => node = self.parse_foreach_statement(),
                Tok::CmdFunction => node = self.parse_function_define(),
                Tok::CmdReturn => node = self.parse_return_statement(),
                Tok::CmdWhile => node = self.parse_while_statement(),
                Tok::Endl => continue,
                Tok::Id => node = self.parse_id_statement(),
                Tok::Ref => node = self.parse_ref_statement(),
                Tok::Suppress => continue,
                Tok::TypeArray
                | Tok::TypeBool
                | Tok::TypeChar
                | Tok::TypeDict
                | Tok::TypeFloat
                | Tok::TypeInt
                | Tok::TypeMatrix
                | Tok::TypeString
                | Tok::TypeVar => node = self.parse_variable_definition(),
                _ => return Some(Node::new(pos, Kind::StatementList(stmts))),
            }

            if node.is_none() && self.success {
                self.report(ParseErr::Internal);
            }

            if self.cur() == Tok::Suppress {
                if let Some(n) = &node {
                    n.suppressed.set(true);
                }
            } else if self.cur() != Tok::Endl {
                self.report(ParseErr::UnterminatedExpr);
            }

            // `AddNode(node.Release())` adds NULL too; only reachable after
            // an error, when the tree is discarded.
            let n = self.nn(node);
            stmts.push(n);
        }

        if self.cur() == Tok::Eof {
            self.eof = true;
        }
        Some(Node::new(pos, Kind::StatementList(stmts)))
    }

    fn parse_variable_definition(&mut self) -> Option<NodeRef> {
        let c = self.cur();
        let ty = if let Some(a) = Self::type_of_token(c).filter(|a| *a != AsType::Void) {
            TypeInfo::new(a)
        } else if c == Tok::Id {
            self.object_type()?
        } else {
            return self.unexpect();
        };

        if self.next_token() != Tok::Id {
            return self.unexpect();
        }
        let pos = self.pos();
        let name = self.text();
        let mut assign = None;
        let mut dims = None;

        match self.next_token() {
            Tok::Assign => {
                self.next_token();
                let e = self.parse_expression();
                assign = Some(self.nn(e));
            }
            Tok::PrecOpen => {
                if self.next_token() != Tok::PrecClose {
                    dims = Some(self.parse_argument_list());
                }
                if self.cur() != Tok::PrecClose {
                    return self.unexpect();
                }
                self.next_token(); // consume ')'
            }
            _ => {}
        }

        Some(Node::new(
            pos,
            Kind::VariableDefinition {
                ty,
                name,
                assign,
                dims,
                id: std::cell::Cell::new(-1),
            },
        ))
    }

    fn parse_variable_definition_list(&mut self) -> Option<NodeRef> {
        let pos = self.pos();
        let mut v = Vec::new();
        v.push(self.parse_variable_definition()?);
        while self.cur() == Tok::Comma {
            self.next_token(); // consume ','
            v.push(self.parse_variable_definition()?);
        }
        Some(Node::new(pos, Kind::VariableDefinitionList(v)))
    }

    fn parse_while_statement(&mut self) -> Option<NodeRef> {
        if self.next_token() != Tok::PrecOpen {
            return self.unexpect();
        }
        self.next_token();
        let cond = self.parse_expression();
        if self.cur() != Tok::PrecClose {
            return self.unexpect();
        }
        self.next_token();

        let code = self.parse_code_block();
        let code = self.nn(code);
        let pos = self.pos();
        let cond = self.nn(cond);
        Some(Node::new(pos, Kind::While { cond, code }))
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

/// `__LINE__` of `PARSE_ERROR(INTERNAL)` in `parseStatementList` (cParser.cc).
const PARSER_INTERNAL_LINE: i32 = 990;

fn new_function_call(pos: FilePos, name: Bytes, args: Option<NodeRef>) -> NodeRef {
    Node::new(
        pos,
        Kind::FunctionCall {
            name,
            args,
            ty: std::cell::RefCell::new(TypeInfo::new(AsType::Invalid)),
            id: std::cell::Cell::new(-1),
            global: std::cell::Cell::new(false),
            func: std::cell::RefCell::new(None),
        },
    )
}
