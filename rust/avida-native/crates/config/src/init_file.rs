//! Port of `cInitFile` (avida-core/source/tools/cInitFile.{h,cc}).
//!
//! Reads an Avida-style init file: handles `#include`/`#import`/`#define`/
//! `#filetype`/`#format` and caller-supplied custom directives, `$NAME`
//! mapping substitution, comment stripping, whitespace compression and `\`
//! line continuation, then offers keyword lookup over the remaining lines.

use crate::feedback::Feedback;
use crate::string_util::{
    clip, clip_end, compress_whitespace, find_char, get_word, pop, pop_word, replace, split_list,
    substring,
};
use std::collections::HashMap;

/// `cInitFile::sLine`.
#[derive(Debug, Clone)]
struct Line {
    line: String,
    file: String,
    line_num: i32,
    used: bool,
}

/// `cInitFile`.
#[derive(Debug, Clone)]
pub struct InitFile {
    filename: String,
    found: bool,
    opened: bool,
    feedback: Feedback,
    lines: Vec<Line>,
    ftype: String,
    format: Vec<String>,
    /// `m_imported_files` (QUIRK: never populated by the C++, so `#import`
    /// behaves like `#include`).
    imported_files: Vec<String>,
    /// `m_mappings`. QUIRK: `Apto::Map` iterates in hash order; this keeps
    /// insertion order, which only differs when several mappings overlap.
    mappings: Vec<(String, String)>,
    custom_directives: Vec<(String, String)>,
}

fn map_set(m: &mut Vec<(String, String)>, k: &str, v: &str) {
    if let Some(e) = m.iter_mut().find(|e| e.0 == k) {
        e.1 = v.to_string();
    } else {
        m.push((k.to_string(), v.to_string()));
    }
}

fn map_get<'a>(m: &'a [(String, String)], k: &str) -> Option<&'a str> {
    m.iter().find(|e| e.0 == k).map(|e| e.1.as_str())
}

/// `Apto::FileSystem::GetAbsolutePath` (POSIX): empty path gives the working
/// directory, a path not starting with '/' or '\\' is appended to it.
pub fn get_absolute_path(path: &str, working_dir: &str) -> String {
    if path.is_empty() {
        return working_dir.to_string();
    }
    let c = path.as_bytes()[0];
    if c != b'/' && c != b'\\' {
        return format!("{}/{}", working_dir, path);
    }
    path.to_string()
}

/// `std::getline` semantics for a whole file followed by `cFile::ReadLine`'s
/// loop: `split('\n')`, which also yields the final empty line after a
/// trailing newline (the C++ loop reads once more before seeing EOF).
fn c_line(raw: &str) -> String {
    // cString(const char*) stops at the first NUL.
    match raw.find('\0') {
        Some(z) => raw[..z].to_string(),
        None => raw.to_string(),
    }
}

impl InitFile {
    fn blank(filename: &str) -> Self {
        InitFile {
            filename: filename.to_string(),
            found: false,
            opened: false,
            feedback: Feedback::new(),
            lines: Vec::new(),
            ftype: "unknown".to_string(),
            format: Vec::new(),
            imported_files: Vec::new(),
            mappings: Vec::new(),
            custom_directives: Vec::new(),
        }
    }

    fn init(
        filename: &str,
        working_dir: &str,
        custom: Option<&[String]>,
        mappings: Option<&[(String, String)]>,
        fb: &mut Feedback,
    ) -> Self {
        let mut f = Self::blank(filename);
        if let Some(m) = mappings {
            for (k, v) in m {
                map_set(&mut f.mappings, k, v);
            }
        }
        let mut lines = Vec::new();
        f.opened = f.load_file(filename, &mut lines, working_dir, custom, fb);
        f.post_process(lines);
        f
    }

    /// `cInitFile(filename, working_dir, custom_directives, mappings)`:
    /// messages go to the object's own feedback.
    pub fn new(
        filename: &str,
        working_dir: &str,
        custom_directives: Option<&[String]>,
        mappings: Option<&[(String, String)]>,
    ) -> Self {
        let mut fb = Feedback::new();
        let mut f = Self::init(filename, working_dir, custom_directives, mappings, &mut fb);
        f.feedback = fb;
        f
    }

    /// `cInitFile(filename, working_dir, Feedback&, custom_directives, mappings)`:
    /// load messages go to the supplied feedback instead.
    pub fn with_feedback(
        filename: &str,
        working_dir: &str,
        feedback: &mut Feedback,
        custom_directives: Option<&[String]>,
        mappings: Option<&[(String, String)]>,
    ) -> Self {
        Self::init(filename, working_dir, custom_directives, mappings, feedback)
    }

    /// `cInitFile(filename, mappings, working_dir)`.
    pub fn with_mappings(filename: &str, mappings: &[(String, String)], working_dir: &str) -> Self {
        Self::new(filename, working_dir, None, Some(mappings))
    }

    /// `cInitFile(std::istream&, working_dir)` over in-memory text.
    ///
    /// QUIRK: stream-constructed objects never set `WasOpened()`, there is no
    /// `$NAME` substitution (no mappings exist yet) and directive failures are
    /// ignored.
    pub fn from_text(text: &str, working_dir: &str) -> Self {
        let mut f = Self::blank("(stream)");
        let mut fb = Feedback::new();
        let mut lines = Vec::new();
        let mut parts: Vec<&str> = text.split('\n').collect();
        if parts.last() == Some(&"") {
            parts.pop(); // std::getline yields no trailing empty line
        }
        let stream_name = f.filename.clone();
        for (i, raw) in parts.iter().enumerate() {
            let cur = c_line(raw);
            let linenum = i as i32 + 1;
            if cur.as_bytes().first() == Some(&b'#') {
                f.process_command(
                    cur,
                    &mut lines,
                    &stream_name,
                    linenum,
                    working_dir,
                    None,
                    &mut fb,
                );
            } else {
                lines.push(Line {
                    line: cur,
                    file: stream_name.clone(),
                    line_num: linenum,
                    used: false,
                });
            }
        }
        f.post_process(lines);
        f.feedback = fb;
        f
    }

    /// `cInitFile(std::istream&, ...)` from a list of lines.
    pub fn from_lines(lines: &[String], working_dir: &str) -> Self {
        let mut t = lines.join("\n");
        t.push('\n');
        Self::from_text(&t, working_dir)
    }

    /// `WasFound()`.
    pub fn was_found(&self) -> bool {
        self.found
    }
    /// `WasOpened()`.
    pub fn was_opened(&self) -> bool {
        self.opened
    }
    /// `GetFeedback()`.
    pub fn feedback(&self) -> &Feedback {
        &self.feedback
    }
    /// `GetCustomDirectives()`: directive name (without '#') to rest of line.
    pub fn custom_directives(&self) -> &[(String, String)] {
        &self.custom_directives
    }
    /// `GetNumLines()`.
    pub fn num_lines(&self) -> usize {
        self.lines.len()
    }
    /// `GetFiletype()`.
    pub fn filetype(&self) -> &str {
        &self.ftype
    }
    /// `GetFormat()`.
    pub fn format(&self) -> &[String] {
        &self.format
    }
    /// `MarkLineUsed(line_id)`.
    pub fn mark_line_used(&mut self, line_id: usize) {
        if let Some(l) = self.lines.get_mut(line_id) {
            l.used = true;
        }
    }
    /// `sLine::file` / `line_num` of line `line_id` (for diagnostics).
    pub fn line_origin(&self, line_id: usize) -> Option<(&str, i32)> {
        self.lines
            .get(line_id)
            .map(|l| (l.file.as_str(), l.line_num))
    }

    /// `cInitFile::loadFile`.
    fn load_file(
        &mut self,
        filename: &str,
        lines: &mut Vec<Line>,
        working_dir: &str,
        custom: Option<&[String]>,
        fb: &mut Feedback,
    ) -> bool {
        let path = get_absolute_path(filename, working_dir);
        let content = match std::fs::metadata(&path) {
            Ok(md) if md.is_file() => std::fs::read(&path).ok(),
            _ => None,
        };
        let content = match content {
            Some(c) => c,
            None => {
                fb.error(format!("unable to open file '{}'.", filename));
                return false;
            }
        };
        self.found = true;
        let text = String::from_utf8_lossy(&content).into_owned();
        let mut linenum = 0;
        for raw in text.split('\n') {
            linenum += 1;
            let mut buf = c_line(raw);
            // Variable substitution (first occurrence of each `$name` only).
            for i in 0..self.mappings.len() {
                let varname = format!("${}", self.mappings[i].0);
                let val = self.mappings[i].1.clone();
                replace(&mut buf, &varname, &val, 0);
            }
            if !buf.is_empty() && buf.as_bytes()[0] == b'#' {
                if !self.process_command(buf, lines, filename, linenum, working_dir, custom, fb) {
                    return false;
                }
            } else {
                lines.push(Line {
                    line: buf,
                    file: filename.to_string(),
                    line_num: linenum,
                    used: false,
                });
            }
        }
        true
    }

    /// `cInitFile::processCommand`.
    #[allow(clippy::too_many_arguments)]
    fn process_command(
        &mut self,
        mut cmdstr: String,
        lines: &mut Vec<Line>,
        filename: &str,
        linenum: i32,
        working_dir: &str,
        custom: Option<&[String]>,
        fb: &mut Feedback,
    ) -> bool {
        let cmd = pop_word(&mut cmdstr);
        if cmd == "#include" || cmd == "#import" {
            let mut path = pop_word(&mut cmdstr);
            let mut mapping = String::new();
            if find_char(&path, '=', 0) >= 0 {
                mapping = pop(&mut path, '=');
            }
            let first = path.as_bytes().first().copied().unwrap_or(0);
            if first == b'<' || first == b'"' {
                let last = path.as_bytes()[path.len() - 1];
                if (first == b'"' && last != b'"') || (first == b'<' && last != b'>') {
                    fb.error(format!(
                        "{}:{}: syntax error processing include directive",
                        filename, linenum
                    ));
                    return false;
                }
                // QUIRK: a lone quote gives Substring(1, -1) in C++ (assert
                // disabled); an empty path results here.
                path = if path.len() >= 2 {
                    substring(&path, 1, path.len() - 2)
                } else {
                    String::new()
                };
            }
            if !mapping.is_empty() {
                if let Some(v) = map_get(&self.mappings, &mapping) {
                    path = v.to_string();
                }
            }
            if cmd != "#import" || !self.imported_files.contains(&path) {
                if !self.load_file(&path, lines, working_dir, custom, fb) {
                    fb.error(format!(
                        "{}:{}: unable to process include directive",
                        filename, linenum
                    ));
                    return false;
                }
            }
        } else if cmd == "#filetype" {
            let ft = pop_word(&mut cmdstr);
            if self.ftype != "unknown" && self.ftype != ft {
                fb.error(format!(
                    "{}:{}: duplicate filetype directive",
                    filename, linenum
                ));
                return false;
            }
            self.ftype = ft;
        } else if cmd == "#format" {
            if !self.format.is_empty() {
                fb.error(format!(
                    "{}:{}: duplicate format directive",
                    filename, linenum
                ));
                return false;
            }
            self.format = split_list(&cmdstr, ' ');
        } else if cmd == "#define" {
            let mapping = pop_word(&mut cmdstr);
            if !mapping.is_empty() {
                let mut value = pop_word(&mut cmdstr);
                crate::string_util::trim(&mut value);
                if !value.is_empty() {
                    map_set(&mut self.mappings, &mapping, &value);
                } else {
                    self.mappings.retain(|e| e.0 != mapping);
                }
            } else {
                fb.error(format!(
                    "{}:{}: invalid define directive",
                    filename, linenum
                ));
                return false;
            }
        } else if let Some(cd) = custom {
            for name in cd {
                if cmd == format!("#{}", name) {
                    map_set(&mut self.custom_directives, name, &cmdstr);
                    break;
                }
            }
        }
        true
    }

    /// `cInitFile::postProcess`: comments, whitespace, continuation, empties.
    fn post_process(&mut self, mut lines: Vec<Line>) {
        self.mappings.clear();
        self.imported_files.clear();
        let n = lines.len();

        // PASS 1: strip comments, compress whitespace.
        for l in lines.iter_mut() {
            let cp = find_char(&l.line, '#', 0);
            if cp >= 0 {
                clip(&mut l.line, cp as usize, -1);
            }
            compress_whitespace(&mut l.line);
        }

        // PASS 2: merge continued lines.
        let mut prev: usize = 0;
        let mut continued = false;
        for i in 0..n {
            if continued {
                let add = std::mem::take(&mut lines[i].line);
                lines[prev].line.push_str(&add);
            } else {
                prev = i;
            }
            let pl = &mut lines[prev].line;
            if !pl.is_empty() && pl.as_bytes()[pl.len() - 1] == b'\\' {
                clip_end(pl, 1);
                continued = true;
            } else {
                continued = false;
            }
        }

        // PASS 3: drop empty lines.
        self.lines = lines.into_iter().filter(|l| !l.line.is_empty()).collect();
    }

    /// `cInitFile::Save(filename)`.
    ///
    /// QUIRK: the C++ reads `if (save_filename != "") save_filename = m_filename;`
    /// so a non-empty argument is replaced by the original file name and an
    /// empty argument stays empty (and fails to open).
    pub fn save(&self, in_filename: &str) -> std::io::Result<()> {
        let save = if !in_filename.is_empty() {
            self.filename.as_str()
        } else {
            in_filename
        };
        let mut out = String::new();
        for l in &self.lines {
            out.push_str(&l.line);
            out.push('\n');
        }
        std::fs::write(save, out)
    }

    /// `GetLine(line_num)`: empty string when out of range.
    pub fn get_line(&self, line_num: i32) -> String {
        if line_num < 0 || line_num as usize >= self.lines.len() {
            return String::new();
        }
        self.lines[line_num as usize].line.clone()
    }

    /// `GetLineAsDict(line_num)`: pairs each `#format` column name with the
    /// corresponding word of the line.
    pub fn get_line_as_dict(&self, line_num: i32) -> HashMap<String, String> {
        let mut dict = HashMap::new();
        let mut fmt = self.format.clone().into_iter();
        let mut line = if line_num >= 0 && (line_num as usize) < self.lines.len() {
            split_list(&self.lines[line_num as usize].line, ' ')
        } else {
            Vec::new()
        }
        .into_iter();
        while let (Some(k), Some(v)) = (fmt.next(), line.next()) {
            dict.insert(k, v);
        }
        dict
    }

    /// `Find(in_string, keyword, col)`: marks every line whose word `col` equals
    /// `keyword` as used.
    ///
    /// QUIRK: the loop does not stop at the first hit; `found_line` is the
    /// LAST matching line.
    pub fn find(&mut self, keyword: &str, col: usize) -> Option<String> {
        let mut found = None;
        for l in self.lines.iter_mut() {
            if get_word(&l.line, col) == keyword {
                l.used = true;
                found = Some(l.line.clone());
            }
        }
        found
    }

    /// `ReadString(name, def, warn_default)`: the rest of the line after the
    /// keyword. QUIRK: a missing keyword logs an *error* here.
    pub fn read_string(&mut self, name: &str, def: &str, warn_default: bool) -> String {
        if name.is_empty() {
            return def.to_string();
        }
        match self.find(name, 0) {
            None => {
                if warn_default {
                    self.feedback.error(format!(
                        "{} not in '{}', defaulting to: {}",
                        name, self.filename, def
                    ));
                }
                def.to_string()
            }
            Some(mut line) => {
                pop_word(&mut line);
                line
            }
        }
    }

    /// `ReadString(names, def, warn_default)`: first name (in order) found
    /// wins; a miss logs a *warning* naming `names[0]` (one name defers to
    /// `read_string`).
    pub fn read_string_names(&mut self, names: &[String], def: &str, warn_default: bool) -> String {
        if names.is_empty() {
            return def.to_string();
        }
        if names.len() == 1 {
            return self.read_string(&names[0], def, warn_default);
        }
        let mut found = None;
        for n in names {
            if let Some(l) = self.find(n, 0) {
                found = Some(l);
                break;
            }
        }
        match found {
            None => {
                if warn_default {
                    self.feedback.warning(format!(
                        "{} not in '{}', defaulting to: {}",
                        names[0], self.filename, def
                    ));
                }
                def.to_string()
            }
            Some(mut line) => {
                pop_word(&mut line);
                line
            }
        }
    }

    /// `WarnUnused()`: warn once, then notify each line never found.
    pub fn warn_unused(&mut self) -> bool {
        let mut found = false;
        for l in &self.lines {
            if !l.used {
                if !found {
                    found = true;
                    self.feedback
                        .warning(format!("unknown lines in input file '{}'.", self.filename));
                }
                self.feedback
                    .notify(format!("  {}:{}: {}", l.file, l.line_num, l.line));
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("avida_config_test_{}_{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn comments_whitespace_continuation() {
        let d = scratch("basic");
        std::fs::write(
            d.join("a.cfg"),
            "# header\nFOO   bar  baz # trailing\n\n  \tBAR 1 \\\n  2 \\\n 3\nLAST 5",
        )
        .unwrap();
        let mut f = InitFile::new("a.cfg", d.to_str().unwrap(), None, None);
        assert!(f.was_opened() && f.was_found());
        assert_eq!(f.num_lines(), 3);
        assert_eq!(f.get_line(0), "FOO bar baz");
        assert_eq!(f.get_line(1), "BAR 1 2 3");
        assert_eq!(f.get_line(2), "LAST 5");
        assert_eq!(f.get_line(3), "");
        assert_eq!(f.get_line(-1), "");
        assert_eq!(f.read_string("FOO", "x", true), "bar baz");
        assert_eq!(f.read_string("BAR", "x", true), "1 2 3");
        assert_eq!(f.read_string("NOPE", "dflt", true), "dflt");
        assert_eq!(f.feedback().num_errors(), 1);
        assert_eq!(
            f.feedback().message(0),
            "NOPE not in 'a.cfg', defaulting to: dflt"
        );
        assert_eq!(f.read_string("NOPE2", "dflt", false), "dflt");
        assert_eq!(f.feedback().num_messages(), 1);
        assert!(f.warn_unused());
        assert_eq!(
            f.feedback().message(1),
            "unknown lines in input file 'a.cfg'."
        );
        assert_eq!(f.feedback().message(2), "  a.cfg:7: LAST 5");
        assert_eq!(f.feedback().num_messages(), 3);
    }

    #[test]
    fn alias_search_and_last_wins() {
        let d = scratch("alias");
        std::fs::write(d.join("a.cfg"), "A 1\nB 2\nA 3\n").unwrap();
        let mut f = InitFile::new("a.cfg", d.to_str().unwrap(), None, None);
        // QUIRK: last matching line wins.
        assert_eq!(f.read_string("A", "", true), "3");
        let names = vec!["Z".to_string(), "B".to_string()];
        assert_eq!(f.read_string_names(&names, "d", true), "2");
        let names = vec!["Z".to_string(), "Y".to_string()];
        assert_eq!(f.read_string_names(&names, "d", true), "d");
        assert_eq!(
            f.feedback().message_type(0),
            crate::feedback::FeedbackKind::Warning
        );
        assert_eq!(
            f.feedback().message(0),
            "Z not in 'a.cfg', defaulting to: d"
        );
        assert!(!f.warn_unused());
    }

    #[test]
    fn include_define_mappings() {
        let d = scratch("inc");
        std::fs::write(d.join("inner.cfg"), "INNER $WHO\nINNER2 x\n").unwrap();
        std::fs::write(d.join("other.cfg"), "OTHER 1\n").unwrap();
        std::fs::write(
            d.join("main.cfg"),
            "#define WHO world\n#include inner.cfg\n#include \"inner.cfg\"\n#include <other.cfg>\n#include SEL=inner.cfg\nTAIL $WHO\n",
        )
        .unwrap();
        let f = InitFile::new("main.cfg", d.to_str().unwrap(), None, None);
        assert!(f.was_opened(), "{:?}", f.feedback());
        // first occurrence only; `$WHO` substitution applies after #define
        assert_eq!(f.num_lines(), 8);
        assert_eq!(f.get_line(0), "INNER world");
        let mut f = f;
        assert_eq!(f.read_string("TAIL", "", true), "world");
        assert_eq!(f.read_string("OTHER", "", true), "1");
        // mapping override from the command line
        std::fs::write(d.join("alt.cfg"), "ALT 9\n").unwrap();
        let maps = vec![("SEL".to_string(), "alt.cfg".to_string())];
        let mut f = InitFile::with_mappings("main.cfg", &maps, d.to_str().unwrap());
        assert_eq!(f.read_string("ALT", "", true), "9");
        assert_eq!(f.read_string("TAIL", "", true), "world");
    }

    #[test]
    fn include_errors() {
        let d = scratch("err");
        std::fs::write(d.join("bad.cfg"), "#include \"x.cfg\n").unwrap();
        std::fs::write(d.join("miss.cfg"), "A 1\n#include nothere.cfg\n").unwrap();
        std::fs::write(d.join("def.cfg"), "#define\n").unwrap();
        std::fs::write(d.join("ft.cfg"), "#filetype a\n#filetype b\n").unwrap();
        std::fs::write(d.join("fm.cfg"), "#format x y\n#format z\n").unwrap();
        let wd = d.to_str().unwrap();

        let f = InitFile::new("bad.cfg", wd, None, None);
        assert!(!f.was_opened() && f.was_found());
        assert_eq!(
            f.feedback().message(0),
            "bad.cfg:1: syntax error processing include directive"
        );

        let f = InitFile::new("miss.cfg", wd, None, None);
        assert!(!f.was_opened());
        assert_eq!(
            f.feedback().message(0),
            "unable to open file 'nothere.cfg'."
        );
        assert_eq!(
            f.feedback().message(1),
            "miss.cfg:2: unable to process include directive"
        );

        let f = InitFile::new("def.cfg", wd, None, None);
        assert_eq!(
            f.feedback().message(0),
            "def.cfg:1: invalid define directive"
        );
        let f = InitFile::new("ft.cfg", wd, None, None);
        assert_eq!(
            f.feedback().message(0),
            "ft.cfg:2: duplicate filetype directive"
        );
        let f = InitFile::new("fm.cfg", wd, None, None);
        assert_eq!(
            f.feedback().message(0),
            "fm.cfg:2: duplicate format directive"
        );

        let f = InitFile::new("absent.cfg", wd, None, None);
        assert!(!f.was_found() && !f.was_opened());
        assert_eq!(f.feedback().message(0), "unable to open file 'absent.cfg'.");

        // external feedback receives the messages instead
        let mut fb = Feedback::new();
        let f = InitFile::with_feedback("absent.cfg", wd, &mut fb, None, None);
        assert_eq!(fb.num_errors(), 1);
        assert_eq!(f.feedback().num_messages(), 0);
    }

    #[test]
    fn directives_and_dict() {
        let d = scratch("dir");
        std::fs::write(
            d.join("a.cfg"),
            "#filetype genotype_data\n#format id name\n#MYDIR hello  there\nX 1\n7 seven\n",
        )
        .unwrap();
        let custom = vec!["MYDIR".to_string()];
        let f = InitFile::new("a.cfg", d.to_str().unwrap(), Some(&custom), None);
        assert_eq!(f.filetype(), "genotype_data");
        assert_eq!(f.format(), &["id".to_string(), "name".to_string()]);
        assert_eq!(
            f.custom_directives(),
            &[("MYDIR".to_string(), "hello  there".to_string())]
        );
        let dct = f.get_line_as_dict(1);
        assert_eq!(dct.get("id").map(String::as_str), Some("7"));
        assert_eq!(dct.get("name").map(String::as_str), Some("seven"));
    }

    #[test]
    fn stream_construction() {
        let f = InitFile::from_text("A 1\n# c\n\nB  2\n", ".");
        assert!(!f.was_opened()); // QUIRK
        assert_eq!(f.num_lines(), 2);
        assert_eq!(f.line_origin(1), Some(("(stream)", 4)));
        let mut f = InitFile::from_lines(&["K v1 v2".to_string()], ".");
        assert_eq!(f.read_string("K", "", true), "v1 v2");
    }

    #[test]
    fn mark_used_and_warn() {
        let mut f = InitFile::from_text("A 1\nB 2\n", ".");
        f.mark_line_used(0);
        assert!(f.warn_unused());
        assert_eq!(f.feedback().message(1), "  (stream):2: B 2");
        assert_eq!(f.feedback().num_messages(), 2);
    }
}
