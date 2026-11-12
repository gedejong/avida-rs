//! `cDumpASTVisitor`: indented text dump of the tree, written to stdout.

use crate::ast::*;
use crate::output;
use crate::types::{map_builtin, map_token, map_type, Tok};

pub struct DumpVisitor {
    depth: i32,
}

impl Default for DumpVisitor {
    fn default() -> Self {
        Self::new()
    }
}

fn w(s: &str) {
    output::write_str(s);
}
fn wb(b: &[u8]) {
    output::write(b);
}

impl DumpVisitor {
    /// The constructor prints `main:` and enters depth 1.
    pub fn new() -> Self {
        w("main:");
        output::endl();
        DumpVisitor { depth: 1 }
    }

    fn indent(&self) {
        for _ in 0..self.depth {
            w("  ");
        }
    }

    pub fn visit(&mut self, node: &Node) {
        match &node.kind {
            Kind::Null => output::crash(),
            Kind::Assignment { var, expr, .. } => {
                self.depth += 1;
                self.indent();
                wb(var);
                output::endl();
                self.depth -= 1;

                self.indent();
                w("=");
                output::endl();

                self.depth += 1;
                self.visit(expr);
                self.depth -= 1;
            }
            Kind::ArgumentList(v) => {
                self.depth += 1;
                for n in v {
                    self.visit(n);
                }
                self.depth -= 1;
            }
            Kind::ObjectAssignment { target, expr } => {
                self.depth += 1;
                self.visit(target);
                self.depth -= 1;

                self.indent();
                w("=");
                output::endl();

                self.depth += 1;
                self.visit(expr);
                self.depth -= 1;
            }
            Kind::Return(e) => {
                self.indent();
                w("return:");
                output::endl();
                self.depth += 1;
                self.visit(e);
                self.depth -= 1;
            }
            Kind::StatementList(v) => {
                for n in v {
                    self.visit(n);
                }
            }
            Kind::Foreach { var, values, code } => {
                self.indent();
                w("foreach:");
                output::endl();

                self.depth += 1;
                self.visit(var);

                self.indent();
                w("values:");
                output::endl();
                self.depth += 1;
                self.visit(values);

                self.depth -= 1;
                self.indent();
                w("code:");
                output::endl();

                self.depth += 1;
                self.visit(code);
                self.depth -= 1;

                self.depth -= 1;
            }
            Kind::If {
                cond,
                code,
                elifs,
                else_code,
            } => {
                self.indent();
                w("if:");
                output::endl();

                self.depth += 1;
                self.indent();
                w("condition:");
                output::endl();

                self.depth += 1;
                self.visit(cond);
                self.depth -= 1;

                self.indent();
                w("do:");
                output::endl();

                self.depth += 1;
                self.visit(code);
                self.depth -= 1;

                for ei in elifs {
                    self.indent();
                    w("elseif:");
                    output::endl();

                    self.depth += 1;
                    self.indent();
                    w("condition:");
                    output::endl();

                    self.depth += 1;
                    self.visit(&ei.cond);
                    self.depth -= 1;

                    self.indent();
                    w("do:");
                    output::endl();

                    self.depth += 1;
                    self.visit(&ei.code);
                    self.depth -= 1;

                    self.depth -= 1;
                }

                if let Some(ec) = else_code {
                    self.indent();
                    w("else:");
                    output::endl();

                    self.depth += 1;
                    self.visit(ec);
                    self.depth -= 1;
                }

                self.depth -= 1;
            }
            Kind::While { cond, code } => {
                self.indent();
                w("while:");
                output::endl();

                self.depth += 1;
                self.indent();
                w("condition:");
                output::endl();

                self.depth += 1;
                self.visit(cond);
                self.depth -= 1;

                self.indent();
                w("do:");
                output::endl();

                self.depth += 1;
                self.visit(code);
                self.depth -= 1;

                self.depth -= 1;
            }
            Kind::FunctionDefinition {
                ty,
                name,
                args,
                code,
            } => {
                self.indent();
                let has_code = code.borrow().is_some();
                w(if has_code { "" } else { "@" });
                w("function: ");
                w(&map_type(ty));
                w(" ");
                wb(name);
                w("(");
                // QUIRK: `GetArguments()->GetSize()` dereferences NULL for a
                // function without parameters (SIGSEGV in the C++).
                let a = args.borrow().clone();
                let a = match a {
                    Some(a) => a,
                    None => output::crash(),
                };
                let size = match &a.kind {
                    Kind::VariableDefinitionList(v) => v.len(),
                    _ => 0,
                };
                if size > 0 {
                    output::endl();
                    self.visit(&a);
                    self.indent();
                }
                w(")");
                output::endl();

                if let Some(c) = code.borrow().clone() {
                    self.indent();
                    w("{");
                    output::endl();

                    self.depth += 1;
                    self.visit(&c);
                    self.depth -= 1;

                    self.indent();
                    w("}");
                    output::endl();
                }
            }
            Kind::VariableDefinition {
                ty,
                name,
                assign,
                dims,
                ..
            } => {
                self.indent();
                w(&map_type(ty));
                w(" ");
                wb(name);
                output::endl();

                if let Some(d) = dims {
                    self.depth += 1;
                    self.indent();
                    w("dimensions:");
                    output::endl();

                    self.depth += 1;
                    self.visit(d);

                    self.depth -= 2;
                }

                if let Some(a) = assign {
                    self.depth += 1;
                    self.indent();
                    w("=");
                    output::endl();

                    self.depth += 1;
                    self.visit(a);

                    self.depth -= 2;
                }
            }
            Kind::VariableDefinitionList(v) => {
                self.depth += 1;
                for n in v {
                    self.visit(n);
                }
                self.depth -= 1;
            }
            Kind::Binary {
                op, left, right, ..
            } => {
                self.depth += 1;
                self.visit(left);
                self.depth -= 1;

                self.indent();
                w(map_token(*op));
                output::endl();

                self.depth += 1;
                self.visit(right);
                self.depth -= 1;
            }
            Kind::Unary { op, expr, .. } => {
                self.indent();
                w(map_token(*op));
                output::endl();

                self.depth += 1;
                self.visit(expr);
                self.depth -= 1;
            }
            Kind::BuiltInCall { args, builtin, .. } => {
                self.indent();
                w("builtin:");
                output::endl();
                self.depth += 1;

                self.indent();
                w("method:");
                output::endl();

                self.depth += 1;
                self.indent();
                w(map_builtin(*builtin));
                output::endl();
                self.depth -= 1;

                if let Some(a) = args {
                    self.indent();
                    w("with:");
                    output::endl();

                    self.depth += 1;
                    self.visit(a);
                    self.depth -= 1;
                }

                self.depth -= 1;
            }
            Kind::FunctionCall { name, args, .. } => {
                self.indent();
                w("call:");
                output::endl();
                self.depth += 1;

                self.indent();
                w("target:");
                output::endl();

                self.depth += 1;
                self.indent();
                wb(name);
                output::endl();
                self.depth -= 1;

                if let Some(a) = args {
                    self.indent();
                    w("with:");
                    output::endl();

                    self.depth += 1;
                    self.visit(a);
                    self.depth -= 1;
                }

                self.depth -= 1;
            }
            Kind::Literal { ty, value } => {
                self.indent();
                w("(");
                w(&map_type(ty));
                w(") ");
                wb(value);
                output::endl();
            }
            Kind::LiteralArray { values, is_matrix } => {
                self.indent();
                if *is_matrix {
                    w("$");
                }
                w("{");
                output::endl();
                self.depth += 1;

                // QUIRK: `GetValues()->Accept` on NULL for `{}` (SIGSEGV).
                match values {
                    Some(v) => self.visit(v),
                    None => output::crash(),
                }

                self.depth -= 1;
                self.indent();
                w("}");
                output::endl();
            }
            Kind::LiteralDict(maps) => {
                self.indent();
                w("@{");
                output::endl();
                self.depth += 1;

                self.depth += 1;
                for m in maps {
                    self.visit(&m.idx);
                    self.depth -= 1;
                    self.indent();
                    w("=>");
                    output::endl();
                    self.depth += 1;
                    self.visit(&m.val);
                }
                self.depth -= 1;

                self.depth -= 1;
                self.indent();
                w("}");
                output::endl();
            }
            Kind::ObjectCall { object, args, .. } => {
                self.indent();
                w("call:");
                output::endl();
                self.depth += 1;

                self.indent();
                w("target:");
                output::endl();

                self.depth += 1;
                self.visit(object);
                self.depth -= 1;

                if let Some(a) = args {
                    self.indent();
                    w("with:");
                    output::endl();

                    self.depth += 1;
                    self.visit(a);
                    self.depth -= 1;
                }

                self.depth -= 1;
            }
            Kind::ObjectReference { object, name } => {
                self.depth += 1;
                self.visit(object);
                self.depth -= 1;

                self.indent();
                w(map_token(Tok::Dot));
                output::endl();

                self.depth += 1;
                self.indent();
                wb(name);
                output::endl();
                self.depth -= 1;
            }
            Kind::VariableReference { name, .. } => {
                self.indent();
                wb(name);
                output::endl();
            }
            Kind::UnpackTarget {
                nodes,
                last_wild,
                last_named,
                expr,
            } => {
                self.depth += 1;

                self.indent();
                w("@{");
                self.depth += 1;

                for n in nodes {
                    output::endl();
                    self.indent();
                    wb(&n.name);
                }
                if *last_named {
                    w("..");
                } else if *last_wild {
                    output::endl();
                    self.indent();
                    w("..");
                }
                output::endl();
                self.depth -= 1;
                self.indent();
                w("}");
                output::endl();

                self.depth -= 1;
                self.indent();
                w("=");
                output::endl();
                self.depth += 1;

                self.visit(expr);

                self.depth -= 1;
            }
        }
    }
}
