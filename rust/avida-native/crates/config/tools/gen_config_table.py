#!/usr/bin/env python3
"""Generate src/config_table.rs from avida-core/source/main/cAvidaConfig.h.

Usage (from anywhere):
    python3 tools/gen_config_table.py [path/to/cAvidaConfig.h] [path/to/config_table.rs]

Every CONFIG_ADD_GROUP / CONFIG_ADD_VAR / CONFIG_ADD_ALIAS /
CONFIG_ADD_CUSTOM_FORMAT / CONFIG_ADD_FORMAT_VAR invocation in the header is
translated, in declaration order.  Nothing is hand-picked: the Rust test
`table_is_complete` re-counts the macro invocations in the header and compares
them with the generated table.
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "..", "..", "..", "..", ".."))
DEFAULT_HEADER = os.path.normpath(os.path.join(HERE, "..", "testdata", "reference", "cAvidaConfig.h"))
DEFAULT_OUT = os.path.normpath(os.path.join(HERE, "..", "src", "config_table.rs"))

MACROS = (
    "CONFIG_ADD_GROUP",
    "CONFIG_ADD_VAR",
    "CONFIG_ADD_ALIAS",
    "CONFIG_ADD_CUSTOM_FORMAT",
    "CONFIG_ADD_FORMAT_VAR",
)

ESCAPES = {"n": "\n", "t": "\t", "r": "\r", "\\": "\\", '"': '"', "'": "'", "0": "\0", "?": "?"}


def strip_comments(text):
    """Remove // and /* */ comments that are outside string literals."""
    out = []
    i = 0
    n = len(text)
    while i < n:
        c = text[i]
        if c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            out.append(text[i : j + 1])
            i = j + 1
        elif text.startswith("//", i):
            while i < n and text[i] != "\n":
                i += 1
        elif text.startswith("/*", i):
            i = text.index("*/", i) + 2
        else:
            out.append(c)
            i += 1
    return "".join(out)


def split_args(body):
    """Split a macro argument list on top-level commas."""
    args, cur, depth, i = [], [], 0, 0
    while i < len(body):
        c = body[i]
        if c == '"':
            j = i + 1
            while body[j] != '"':
                j += 2 if body[j] == "\\" else 1
            cur.append(body[i : j + 1])
            i = j + 1
            continue
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
        if c == "," and depth == 0:
            args.append("".join(cur).strip())
            cur = []
        else:
            cur.append(c)
        i += 1
    args.append("".join(cur).strip())
    return args


def find_invocations(text):
    """Yield (macro, [args]) in source order."""
    hits = []
    for m in MACROS:
        start = 0
        while True:
            pos = text.find(m + "(", start)
            if pos < 0:
                break
            hits.append((pos, m))
            start = pos + 1
    hits.sort()
    for pos, m in hits:
        i = pos + len(m) + 1
        depth = 1
        j = i
        while depth:
            c = text[j]
            if c == '"':
                j += 1
                while text[j] != '"':
                    j += 2 if text[j] == "\\" else 1
            elif c == "(":
                depth += 1
            elif c == ")":
                depth -= 1
            j += 1
        yield m, split_args(text[i : j - 1])


def c_string(tok):
    """Decode one or more adjacent C string literals."""
    out = []
    i = 0
    tok = tok.strip()
    while i < len(tok):
        if tok[i].isspace():
            i += 1
            continue
        if tok[i] != '"':
            raise ValueError("expected string literal: %r" % tok)
        i += 1
        while tok[i] != '"':
            if tok[i] == "\\":
                e = tok[i + 1]
                if e not in ESCAPES:
                    raise ValueError("unsupported escape \\%s in %r" % (e, tok))
                out.append(ESCAPES[e])
                i += 2
            else:
                out.append(tok[i])
                i += 1
        i += 1
    return "".join(out)


def stringify(tok):
    """Emulate the preprocessor's # operator, then cBaseConfigEntry's quote stripping."""
    s = " ".join(tok.split()) if '"' not in tok else tok.strip()
    # cBaseConfigEntry ctor: if default_value[0] == '"' strip first and last char
    if s.startswith('"'):
        s = s[1:-1] if len(s) > 2 else ""
    return s


def rust_str(s):
    out = ['"']
    for ch in s:
        if ch == "\\":
            out.append("\\\\")
        elif ch == '"':
            out.append('\\"')
        elif ch == "\n":
            out.append("\\n")
        elif ch == "\t":
            out.append("\\t")
        elif ch == "\r":
            out.append("\\r")
        elif ord(ch) < 0x20 or ord(ch) > 0x7E:
            out.append("\\u{%x}" % ord(ch))
        else:
            out.append(ch)
    out.append('"')
    return "".join(out)


TYPES = {
    "int": ("i32", "EntryType::Int"),
    "double": ("f64", "EntryType::Double"),
    "bool": ("bool", "EntryType::Bool"),
    "cString": ("String", "EntryType::Str"),
}


def main():
    header = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_HEADER
    out_path = sys.argv[2] if len(sys.argv) > 2 else DEFAULT_OUT
    text = open(header).read()
    # Only the live (non-OVERRIDE_CONFIG) block of the class body holds settings.
    begin = text.index("#ifdef OVERRIDE_CONFIG")
    begin = text.index("#else", begin)
    end = text.index("#endif", begin)
    body = strip_comments(text[begin:end])

    groups = []  # (name, desc, [entry])
    formats = []  # (name, desc, [(name, desc)])
    last = None  # "group" or "format": what the most recent container was
    for macro, args in find_invocations(body):
        if macro == "CONFIG_ADD_GROUP":
            groups.append((args[0], c_string(args[1]), []))
        elif macro == "CONFIG_ADD_VAR":
            name, ctype, default, desc = args
            if ctype not in TYPES:
                raise ValueError("unknown type %s for %s" % (ctype, name))
            groups[-1][2].append(
                {"names": [name], "type": ctype, "default": stringify(default), "desc": c_string(desc)}
            )
        elif macro == "CONFIG_ADD_ALIAS":
            # primary_entry = global_group_list.GetLast()->GetLastEntry()
            groups[-1][2][-1]["names"].append(args[0])
        elif macro == "CONFIG_ADD_CUSTOM_FORMAT":
            formats.append((args[0], c_string(args[1]), []))
        elif macro == "CONFIG_ADD_FORMAT_VAR":
            formats[-1][2].append((args[0], c_string(args[1])))

    entries = [e for g in groups for e in g[2]]
    seen = set()
    for e in entries:
        for nm in e["names"]:
            if nm in seen:
                raise ValueError("duplicate setting name " + nm)
            seen.add(nm)

    w = []
    w.append("// @generated by tools/gen_config_table.py from")
    w.append("// avida-core/source/main/cAvidaConfig.h -- DO NOT EDIT BY HAND.")
    w.append("//")
    w.append("// %d groups, %d settings, %d custom formats." % (len(groups), len(entries), len(formats)))
    w.append("")
    w.append("use crate::config::{EntryDef, EntryType, FormatDef, FormatEntryDef, GroupDef};")
    w.append("use crate::string_util::{as_double, as_int, convert_double, convert_int};")
    w.append("")
    w.append("/// Every `CONFIG_ADD_GROUP` of `cAvidaConfig.h`, in declaration order, with its")
    w.append("/// `CONFIG_ADD_VAR` entries (and their `CONFIG_ADD_ALIAS` names).")
    w.append("pub static GROUPS: &[GroupDef] = &[")
    idx = 0
    for gname, gdesc, gentries in groups:
        w.append("    GroupDef {")
        w.append("        name: %s," % rust_str(gname))
        w.append("        desc: %s," % rust_str(gdesc))
        w.append("        first_id: %d," % idx)
        w.append("        entries: &[")
        for e in gentries:
            w.append("            EntryDef {")
            w.append("                id: %d," % idx)
            w.append("                names: &[%s]," % ", ".join(rust_str(n) for n in e["names"]))
            w.append("                ty: %s," % TYPES[e["type"]][1])
            w.append("                default: %s," % rust_str(e["default"]))
            w.append("                desc: %s," % rust_str(e["desc"]))
            w.append("            },")
            idx += 1
        w.append("        ],")
        w.append("    },")
    w.append("];")
    w.append("")
    w.append("/// Total number of `CONFIG_ADD_VAR` settings.")
    w.append("pub const NUM_ENTRIES: usize = %d;" % len(entries))
    w.append("")
    w.append("/// Every `CONFIG_ADD_CUSTOM_FORMAT` block with its `CONFIG_ADD_FORMAT_VAR` keywords.")
    w.append("pub static FORMATS: &[FormatDef] = &[")
    for fname, fdesc, fentries in formats:
        w.append("    FormatDef {")
        w.append("        name: %s," % rust_str(fname))
        w.append("        desc: %s," % rust_str(fdesc))
        w.append("        entries: &[")
        for n, d in fentries:
            w.append("            FormatEntryDef {")
            w.append("                name: %s," % rust_str(n))
            w.append("                desc: %s," % rust_str(d))
            w.append("            },")
        w.append("        ],")
        w.append("    },")
    w.append("];")
    w.append("")
    w.append("/// Typed storage for every setting of `cAvidaConfig` (one field per")
    w.append("/// `CONFIG_ADD_VAR`, lower-cased) plus the custom-format value lists.")
    w.append("#[derive(Debug, Clone, PartialEq)]")
    w.append("pub struct Config {")
    for gname, gdesc, gentries in groups:
        w.append("    // ---- %s ----" % gname)
        for e in gentries:
            for line in e["desc"].split("\n"):
                w.append(("    /// " + line.replace("\t", " ")).rstrip())
            if len(e["names"]) > 1:
                w.append("    ///")
                w.append("    /// Aliases: %s" % ", ".join(e["names"][1:]))
            w.append("    pub %s: %s," % (e["names"][0].lower(), TYPES[e["type"]][0]))
    w.append("    /// Value lists of the custom formats, indexed like [`FORMATS`]")
    w.append("    /// (`cBaseConfigCustomFormat::m_value`).")
    w.append("    pub(crate) format_values: Vec<Vec<String>>,")
    w.append("    /// Settings of the native port that `cAvidaConfig.h` does not have")
    w.append("    /// (see [`crate::port_settings`]); kept out of [`GROUPS`] so that")
    w.append("    /// nothing the C++ prints about its settings changes.")
    w.append("    pub port: crate::port_settings::PortSettings,")
    w.append("}")
    w.append("")
    w.append("impl Config {")
    w.append("    /// `cAvidaConfig::cAvidaConfig`: every setting holds its default")
    w.append("    /// (`LoadStr(GetDefault())`), every custom format list is empty.")
    w.append("    pub fn new() -> Self {")
    w.append("        Config {")
    for e in entries:
        f = e["names"][0].lower()
        d = rust_str(e["default"])
        t = e["type"]
        if t == "int":
            w.append("            %s: as_int(%s)," % (f, d))
        elif t == "double":
            w.append("            %s: as_double(%s)," % (f, d))
        elif t == "bool":
            w.append("            %s: as_int(%s) != 0," % (f, d))
        else:
            w.append("            %s: String::from(%s)," % (f, d))
    w.append("            format_values: vec![Vec::new(); FORMATS.len()],")
    w.append("            port: crate::port_settings::PortSettings::new(),")
    w.append("        }")
    w.append("    }")
    w.append("")
    w.append("    /// `cEntry_NAME::LoadStr` for the entry with table id `id`.")
    w.append("    pub(crate) fn load_str_by_id(&mut self, id: usize, s: &str) {")
    w.append("        match id {")
    for i, e in enumerate(entries):
        f = e["names"][0].lower()
        t = e["type"]
        if t == "int":
            w.append("            %d => self.%s = as_int(s)," % (i, f))
        elif t == "double":
            w.append("            %d => self.%s = as_double(s)," % (i, f))
        elif t == "bool":
            w.append("            %d => self.%s = as_int(s) != 0," % (i, f))
        else:
            w.append("            %d => self.%s = s.to_string()," % (i, f))
    w.append("            _ => {}")
    w.append("        }")
    w.append("    }")
    w.append("")
    w.append("    /// `cEntry_NAME::AsString` for the entry with table id `id`.")
    w.append("    pub(crate) fn as_string_by_id(&self, id: usize) -> String {")
    w.append("        match id {")
    for i, e in enumerate(entries):
        f = e["names"][0].lower()
        t = e["type"]
        if t == "int":
            w.append("            %d => convert_int(self.%s)," % (i, f))
        elif t == "double":
            w.append("            %d => convert_double(self.%s)," % (i, f))
        elif t == "bool":
            w.append('            %d => String::from(if self.%s { "1" } else { "0" }),' % (i, f))
        else:
            w.append("            %d => self.%s.clone()," % (i, f))
    w.append("            _ => String::new(),")
    w.append("        }")
    w.append("    }")
    w.append("")
    w.append("    /// `cEntry_NAME::EqualsString` for the entry with table id `id`.")
    w.append("    #[allow(clippy::float_cmp)]")
    w.append("    pub(crate) fn equals_string_by_id(&self, id: usize, s: &str) -> bool {")
    w.append("        match id {")
    for i, e in enumerate(entries):
        f = e["names"][0].lower()
        t = e["type"]
        if t == "int":
            w.append("            %d => self.%s == as_int(s)," % (i, f))
        elif t == "double":
            w.append("            %d => self.%s == as_double(s)," % (i, f))
        elif t == "bool":
            w.append("            %d => self.%s == (as_int(s) != 0)," % (i, f))
        else:
            w.append("            %d => self.%s == s," % (i, f))
    w.append("            _ => false,")
    w.append("        }")
    w.append("    }")
    for fi, (fname, fdesc, fentries) in enumerate(formats):
        w.append("")
        w.append("    /// `cAvidaConfig::%s.Get()`: %s (ordered raw lines)." % (fname, fdesc))
        w.append("    pub fn %s(&self) -> &[String] {" % fname.lower())
        w.append("        &self.format_values[%d]" % fi)
        w.append("    }")
    w.append("}")
    w.append("")
    with open(out_path, "w") as fp:
        fp.write("\n".join(w))
    print("wrote %s: %d groups, %d settings, %d formats" % (out_path, len(groups), len(entries), len(formats)))


if __name__ == "__main__":
    main()
