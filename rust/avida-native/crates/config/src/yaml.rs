//! The small YAML layer of the experiment file (`avida.yaml`).
//!
//! [`parse`] turns a document into a [`Node`] tree with the line of every
//! key and value, built from the event stream of the `saphyr-parser` crate.
//! Scalars keep their source text exactly: nothing here interprets numbers,
//! booleans or dates, because every value of an experiment is handed to the
//! same text parsers the C++ used (`cString::AsInt`, `AsDouble`, the
//! environment and instruction-set parsers), and the consistency suite
//! compares output byte for byte. A plain `~`, `null` or empty value is
//! [`NodeKind::Null`]. Anchors, aliases and documents after the first are
//! refused; tags are ignored.
//!
//! [`quote`] and [`Emitter`] write the subset of YAML the converter and the
//! front ends produce: block mappings and sequences, flow mappings and
//! sequences on one line, comments, and scalars that are written plain
//! whenever [`parse`] reads them back as the same text.

use saphyr_parser::{Event, Parser, ScalarStyle};

/// What a [`Node`] holds.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    /// A plain `~`, `null` or an empty value.
    Null,
    /// A scalar with its text as written (after YAML unquoting and folding).
    Scalar {
        /// The scalar's text.
        text: String,
        /// Whether it was quoted or a block scalar (`|`, `>`).
        quoted: bool,
    },
    /// A sequence.
    Seq(Vec<Node>),
    /// A mapping, in document order (keys are unique scalars).
    Map(Vec<(Key, Node)>),
}

/// A mapping key.
#[derive(Debug, Clone, PartialEq)]
pub struct Key {
    /// The key text.
    pub text: String,
    /// 1-based line of the key.
    pub line: usize,
}

/// A value of the document with the line it starts on (1-based).
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// The value.
    pub kind: NodeKind,
    /// 1-based line.
    pub line: usize,
}

impl Node {
    /// The scalar text, or `None` for anything else (`Null` included).
    pub fn as_str(&self) -> Option<&str> {
        match &self.kind {
            NodeKind::Scalar { text, .. } => Some(text),
            _ => None,
        }
    }

    /// The entries of a mapping.
    pub fn as_map(&self) -> Option<&[(Key, Node)]> {
        match &self.kind {
            NodeKind::Map(m) => Some(m),
            _ => None,
        }
    }

    /// The items of a sequence.
    pub fn as_seq(&self) -> Option<&[Node]> {
        match &self.kind {
            NodeKind::Seq(s) => Some(s),
            _ => None,
        }
    }

    /// Whether this is [`NodeKind::Null`].
    pub fn is_null(&self) -> bool {
        matches!(self.kind, NodeKind::Null)
    }

    /// The value under `key` of a mapping.
    pub fn get(&self, key: &str) -> Option<&Node> {
        self.as_map()?
            .iter()
            .find(|(k, _)| k.text == key)
            .map(|(_, v)| v)
    }

    /// A short description of the node's kind for error messages.
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            NodeKind::Null => "an empty value",
            NodeKind::Scalar { .. } => "a single value",
            NodeKind::Seq(_) => "a list",
            NodeKind::Map(_) => "a mapping",
        }
    }
}

enum Frame {
    Seq(Vec<Node>, usize),
    Map(Vec<(Key, Node)>, usize, Option<Key>),
}

/// Parses a YAML document. `Ok(None)` for a document without content
/// (empty, or comments only). Errors carry `line N: ` in front.
pub fn parse(text: &str) -> Result<Option<Node>, String> {
    let mut stack: Vec<Frame> = Vec::new();
    let mut root: Option<Node> = None;
    let mut documents = 0;
    for item in Parser::new_from_str(text) {
        let (event, span) = item.map_err(|e| {
            format!(
                "line {}: {}",
                e.marker().line(),
                e.info().trim_end_matches('.')
            )
        })?;
        let line = span.start.line();
        let node = match event {
            Event::StreamStart | Event::StreamEnd | Event::Nothing | Event::DocumentEnd => continue,
            Event::DocumentStart(_) => {
                documents += 1;
                if documents > 1 {
                    return Err(format!(
                        "line {line}: only one YAML document is allowed in a file"
                    ));
                }
                continue;
            }
            Event::Alias(_) => {
                return Err(format!(
                    "line {line}: anchors and aliases (& and *) are not supported"
                ))
            }
            Event::SequenceStart(anchor, _) | Event::MappingStart(anchor, _) if anchor != 0 => {
                return Err(format!(
                    "line {line}: anchors and aliases (& and *) are not supported"
                ))
            }
            Event::SequenceStart(_, _) => {
                stack.push(Frame::Seq(Vec::new(), line));
                continue;
            }
            Event::MappingStart(_, _) => {
                stack.push(Frame::Map(Vec::new(), line, None));
                continue;
            }
            Event::SequenceEnd => match stack.pop() {
                Some(Frame::Seq(items, l)) => Node {
                    kind: NodeKind::Seq(items),
                    line: l,
                },
                _ => return Err(format!("line {line}: unbalanced sequence")),
            },
            Event::MappingEnd => match stack.pop() {
                Some(Frame::Map(entries, l, None)) => Node {
                    kind: NodeKind::Map(entries),
                    line: l,
                },
                _ => return Err(format!("line {line}: unbalanced mapping")),
            },
            Event::Scalar(value, style, anchor, _) => {
                if anchor != 0 {
                    return Err(format!(
                        "line {line}: anchors and aliases (& and *) are not supported"
                    ));
                }
                let quoted = style != ScalarStyle::Plain;
                let null = !quoted && matches!(value.as_ref(), "~" | "null" | "Null" | "NULL" | "");
                Node {
                    kind: if null {
                        NodeKind::Null
                    } else {
                        NodeKind::Scalar {
                            text: value.into_owned(),
                            quoted,
                        }
                    },
                    line,
                }
            }
        };
        match stack.last_mut() {
            None => root = Some(node),
            Some(Frame::Seq(items, _)) => items.push(node),
            Some(Frame::Map(entries, _, pending)) => match pending.take() {
                None => {
                    let text = match node.kind {
                        NodeKind::Scalar { text, .. } => text,
                        NodeKind::Null => String::new(),
                        _ => {
                            return Err(format!(
                                "line {}: a key must be a single value, not {}",
                                node.line,
                                node.kind_name()
                            ))
                        }
                    };
                    if entries.iter().any(|(k, _)| k.text == text) {
                        return Err(format!("line {}: key '{}' appears twice", node.line, text));
                    }
                    *pending = Some(Key {
                        text,
                        line: node.line,
                    });
                }
                Some(key) => entries.push((key, node)),
            },
        }
    }
    Ok(root)
}

/// Whether `s` can be written as a plain scalar that [`parse`] reads back
/// as the same text, in block context (`flow` false) or inside `[...]` or
/// `{...}` (`flow` true).
fn plain_ok(s: &str, flow: bool) -> bool {
    if s.is_empty() || matches!(s, "~" | "null" | "Null" | "NULL") {
        return false;
    }
    if s.starts_with("---") || s.starts_with("...") {
        return false;
    }
    let bytes = s.as_bytes();
    let first = bytes[0];
    let last = bytes[bytes.len() - 1];
    if first == b' ' || last == b' ' || last == b':' {
        return false;
    }
    if b"?:,[]{}#&*!|>'\"%@`".contains(&first) {
        return false;
    }
    if first == b'-' {
        // `-1` and `-x` are plain; `- x` and a lone `-` are not.
        match bytes.get(1) {
            None | Some(b' ') => return false,
            Some(c) if flow && b",[]{}".contains(c) => return false,
            _ => {}
        }
    }
    if s.contains(": ") || s.contains(" #") || s.contains("  ") {
        return false;
    }
    if s.chars().any(|c| c.is_control()) {
        return false;
    }
    if flow && s.bytes().any(|c| b",[]{}".contains(&c)) {
        return false;
    }
    true
}

/// `s` as a YAML scalar: plain when that reads back unchanged, otherwise
/// single-quoted, or double-quoted with escapes when it holds control
/// characters.
pub fn quote(s: &str, flow: bool) -> String {
    if plain_ok(s, flow) {
        return s.to_string();
    }
    if !s.chars().any(|c| c.is_control()) {
        return format!("'{}'", s.replace('\'', "''"));
    }
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            '\r' => o.push_str("\\r"),
            c if c.is_control() => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// A key as written in a mapping.
pub fn quote_key(s: &str) -> String {
    quote(s, true)
}

/// A flow mapping `{a: 1, b: x}` on one line.
pub fn flow_map(entries: &[(String, String)]) -> String {
    let inner: Vec<String> = entries
        .iter()
        .map(|(k, v)| format!("{}: {}", quote_key(k), quote(v, true)))
        .collect();
    format!("{{{}}}", inner.join(", "))
}

/// A flow sequence `[a, b]` on one line.
pub fn flow_seq(items: &[String]) -> String {
    let inner: Vec<String> = items.iter().map(|v| quote(v, true)).collect();
    format!("[{}]", inner.join(", "))
}

/// Line-oriented writer for block YAML.
#[derive(Debug, Default)]
pub struct Emitter {
    out: String,
}

impl Emitter {
    /// An empty document.
    pub fn new() -> Self {
        Emitter::default()
    }

    /// `# text` lines at `indent` (one per line of `text`; an empty line of
    /// `text` gives a bare `#`).
    pub fn comment(&mut self, indent: usize, text: &str) {
        for l in text.split('\n') {
            let l = l.trim_end();
            self.out.push_str(&" ".repeat(indent));
            if l.is_empty() {
                self.out.push_str("#\n");
            } else {
                self.out.push_str("# ");
                self.out.push_str(l);
                self.out.push('\n');
            }
        }
    }

    /// An empty line.
    pub fn blank(&mut self) {
        self.out.push('\n');
    }

    /// `key:` opening a nested block.
    pub fn open(&mut self, indent: usize, key: &str) {
        self.out.push_str(&" ".repeat(indent));
        self.out.push_str(&quote_key(key));
        self.out.push_str(":\n");
    }

    /// `key: value` with `value` quoted as needed.
    pub fn scalar(&mut self, indent: usize, key: &str, value: &str) {
        self.raw(indent, key, &quote(value, false));
    }

    /// `key: text` with `text` already in YAML form (a flow collection).
    pub fn raw(&mut self, indent: usize, key: &str, text: &str) {
        self.out.push_str(&" ".repeat(indent));
        self.out.push_str(&quote_key(key));
        self.out.push_str(": ");
        self.out.push_str(text);
        self.out.push('\n');
    }

    /// `- text` with `text` already in YAML form.
    pub fn item(&mut self, indent: usize, text: &str) {
        self.out.push_str(&" ".repeat(indent));
        self.out.push_str("- ");
        self.out.push_str(text);
        self.out.push('\n');
    }

    /// A block mapping as a list item: `- k1: v1` then `  k2: v2` ...
    /// (values already in YAML form).
    pub fn item_map(&mut self, indent: usize, entries: &[(String, String)]) {
        for (i, (k, v)) in entries.iter().enumerate() {
            self.out.push_str(&" ".repeat(indent));
            self.out.push_str(if i == 0 { "- " } else { "  " });
            self.out.push_str(&quote_key(k));
            self.out.push_str(": ");
            self.out.push_str(v);
            self.out.push('\n');
        }
    }

    /// The text written so far.
    pub fn finish(self) -> String {
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scalar_of(doc: &str) -> String {
        let n = parse(doc).unwrap().unwrap();
        n.get("k").and_then(Node::as_str).unwrap().to_string()
    }

    #[test]
    fn scalars_keep_their_text() {
        let n = parse("a: 0.0075\nb: 1.0\nc: 0x10\nd: true\ne: 30abc\nf: '07'\n")
            .unwrap()
            .unwrap();
        let m = n.as_map().unwrap();
        let texts: Vec<&str> = m.iter().map(|(_, v)| v.as_str().unwrap()).collect();
        assert_eq!(texts, ["0.0075", "1.0", "0x10", "true", "30abc", "07"]);
        assert_eq!(m[2].0.line, 3);
        let n = parse("a:\nb: ~\nc: ''\n").unwrap().unwrap();
        assert!(n.get("a").unwrap().is_null());
        assert!(n.get("b").unwrap().is_null());
        assert_eq!(n.get("c").unwrap().as_str(), Some(""));
        assert_eq!(parse("# nothing\n").unwrap(), None);
    }

    #[test]
    fn refusals() {
        assert!(parse("a: 1\na: 2\n").unwrap_err().contains("appears twice"));
        assert!(parse("a: &x 1\nb: *x\n").unwrap_err().contains("aliases"));
        assert!(parse("a: 1\n---\nb: 2\n")
            .unwrap_err()
            .contains("one YAML document"));
        assert!(parse("a: [1, 2\n").unwrap_err().starts_with("line "));
        assert!(parse("[1]: 2\n").unwrap_err().contains("key must be"));
    }

    #[test]
    fn quoting_round_trips() {
        let samples = [
            "plain",
            "",
            "with space",
            "a: b",
            "#x",
            "x #y",
            "-1",
            "-",
            "- x",
            "1,2,3",
            "a,b=5,c=-2",
            "{x}",
            "it's",
            "tab\there",
            "line\nbreak",
            "~",
            "null",
            "---",
            "trailing:",
            "x:y",
            "0..9",
            "@x",
            "  two",
            "a  b",
            "100",
            "1.5e-3",
            "C:\\path",
        ];
        for s in samples {
            for flow in [false, true] {
                let doc = if flow {
                    format!("k: [{}]\n", quote(s, true))
                } else {
                    format!("k: {}\n", quote(s, false))
                };
                let n = parse(&doc)
                    .unwrap_or_else(|e| panic!("{s:?}: {e}\n{doc}"))
                    .unwrap();
                let got = if flow {
                    n.get("k").unwrap().as_seq().unwrap()[0]
                        .as_str()
                        .unwrap()
                        .to_string()
                } else {
                    scalar_of(&doc)
                };
                assert_eq!(got, s, "{doc}");
            }
        }
        assert_eq!(quote("1,2,3", false), "1,2,3");
        assert_eq!(quote("-1", true), "-1");
        assert_eq!(
            flow_map(&[("a".into(), "1".into()), ("b".into(), "x y".into())]),
            "{a: 1, b: x y}"
        );
    }
}
