//! Avida's data-file writer and output manager: port of `Avida::Output::File`
//! and `Avida::Output::Manager` (avida-core/source/output/File.cc,
//! Manager.cc, Socket.cc; headers in avida-core/include/public/avida/output/).
//!
//! A data file collects a header (comments, a time stamp, and one numbered
//! legend line per column) while the first row is written, then emits header
//! and first row together at the first [`DataFile::endl`]; later rows go
//! straight to the file. Every `endl` flushes (C++ `std::endl`).
//!
//! The time stamp is local time laid out like C `ctime()`, as in the C++.
//! Comment lines are ignored by the consistency-test comparison
//! (avida-core/tests/_testrunner/testrunner.py).
//!
//! Ownership follows the C++ smart pointers: files from
//! [`Manager::create_with_path`] / [`Manager::append_with_path`] belong to the
//! caller and unregister themselves when dropped; files from
//! [`Manager::static_with_path`] are owned by the manager until
//! [`Manager::close`].

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::fmt::fmt_double;

/// `Avida::Output::OutputID` (an absolute path string).
pub type OutputId = String;

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

/// A value accepted by [`DataFile::write`]: one of the C++ `File::Write`
/// overloads. Each cell is printed as `ostream << value << " "`.
pub trait DataValue {
    /// Appends the value's cell text, including its trailing space(s).
    fn write_cell(&self, out: &mut String);

    /// False for `unsigned int`: QUIRK, that C++ overload drops its format
    /// argument (`Write(unsigned int, const char*, const char*)` calls
    /// `WriteColumnDesc(descr)`).
    fn keeps_format(&self) -> bool {
        true
    }
}

impl DataValue for f64 {
    fn write_cell(&self, out: &mut String) {
        out.push_str(&fmt_double(*self));
        out.push(' ');
    }
}

impl DataValue for i32 {
    fn write_cell(&self, out: &mut String) {
        out.push_str(&self.to_string());
        out.push(' ');
    }
}

/// C++ `long` (64-bit on the reference platform).
impl DataValue for i64 {
    fn write_cell(&self, out: &mut String) {
        out.push_str(&self.to_string());
        out.push(' ');
    }
}

impl DataValue for u32 {
    fn write_cell(&self, out: &mut String) {
        out.push_str(&self.to_string());
        out.push(' ');
    }

    fn keeps_format(&self) -> bool {
        false
    }
}

impl DataValue for &str {
    fn write_cell(&self, out: &mut String) {
        out.push_str(self);
        out.push(' ');
    }
}

impl DataValue for String {
    fn write_cell(&self, out: &mut String) {
        self.as_str().write_cell(out);
    }
}

impl DataValue for &String {
    fn write_cell(&self, out: &mut String) {
        self.as_str().write_cell(out);
    }
}

/// `Write(Apto::Array<int> list, ...)`: every element followed by a space,
/// one column description for the whole list.
impl DataValue for &[i32] {
    fn write_cell(&self, out: &mut String) {
        for v in self.iter() {
            v.write_cell(out);
        }
    }
}

impl DataValue for &Vec<i32> {
    fn write_cell(&self, out: &mut String) {
        self.as_slice().write_cell(out);
    }
}

// ---------------------------------------------------------------------------
// Time stamp
// ---------------------------------------------------------------------------

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// C `asctime(gmtime(&t))`: `"Thu Jan  1 00:00:00 1970\n"`
/// (`"%.3s %.3s%3d %.2d:%.2d:%.2d %d\n"`).
pub fn ctime_utc(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86_400);
    let secs = unix_secs.rem_euclid(86_400);
    let (h, m, s) = (secs / 3600, (secs / 60) % 60, secs % 60);
    // 1970-01-01 was a Thursday.
    let wday = (days + 4).rem_euclid(7) as usize;

    // Civil date from day count (H. Hinnant, "chrono-compatible low-level
    // date algorithms").
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);

    format!(
        "{} {}{:3} {:02}:{:02}:{:02} {}\n",
        WEEKDAYS[wday],
        MONTHS[(month - 1) as usize],
        day,
        h,
        m,
        s,
        year
    )
}

/// Current local time in the `ctime()` layout, as the C++ `ctime(&time)`.
fn ctime_now() -> String {
    use chrono::Offset;
    let now = chrono::Local::now();
    let offset = i64::from(now.offset().fix().local_minus_utc());
    ctime_utc(now.timestamp() + offset)
}

// ---------------------------------------------------------------------------
// Registry shared between the manager and the files it opened
// ---------------------------------------------------------------------------

/// `Manager::m_sockets` (the registered, i.e. open, output ids).
#[derive(Debug, Default)]
struct Registry {
    open: BTreeSet<OutputId>,
}

type SharedRegistry = Arc<Mutex<Registry>>;

fn lock(reg: &SharedRegistry) -> MutexGuard<'_, Registry> {
    reg.lock().unwrap_or_else(|e| e.into_inner())
}

// ---------------------------------------------------------------------------
// DataFile
// ---------------------------------------------------------------------------

/// Port of `Avida::Output::File` (avida-core/source/output/File.cc).
pub struct DataFile {
    id: OutputId,
    descr: String,
    filetype: String,
    format: String,
    descr_written: bool,
    /// `m_data`: the first row, held back until the header is written.
    data: String,
    num_cols: i32,
    fp: Box<dyn Write + Send>,
    /// `m_fp.fail()`: set when the stream could not be written.
    failed: bool,
    registry: Option<SharedRegistry>,
    is_static: bool,
}

impl std::fmt::Debug for DataFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataFile")
            .field("id", &self.id)
            .field("descr_written", &self.descr_written)
            .field("num_cols", &self.num_cols)
            .field("is_static", &self.is_static)
            .finish()
    }
}

impl DataFile {
    /// A data file writing to an arbitrary sink, not registered with any
    /// manager (for in-memory output and tests).
    pub fn from_writer(name: &str, sink: Box<dyn Write + Send>) -> Self {
        DataFile {
            id: name.to_string(),
            descr: String::new(),
            filetype: String::new(),
            format: String::new(),
            descr_written: false,
            data: String::new(),
            num_cols: 0,
            fp: sink,
            failed: false,
            registry: None,
            is_static: false,
        }
    }

    /// `File::File(world, name, append)`: opens (truncating, or appending) the
    /// file at `id`.
    fn open(id: &str, append: bool) -> io::Result<Self> {
        let file = if append {
            fs::OpenOptions::new().create(true).append(true).open(id)?
        } else {
            fs::File::create(id)?
        };
        Ok(Self::from_writer(id, Box::new(BufWriter::new(file))))
    }

    /// `Name()`.
    pub fn name(&self) -> &str {
        &self.id
    }

    /// `GetFileType()`.
    pub fn file_type(&self) -> &str {
        &self.filetype
    }

    /// `Fail()`.
    pub fn fail(&self) -> bool {
        self.failed
    }

    /// `Good()`.
    pub fn good(&self) -> bool {
        !self.failed
    }

    /// `HeaderDone()`.
    pub fn header_done(&self) -> bool {
        self.descr_written
    }

    /// `SetFileType(ft)`: only before the header has been written.
    pub fn set_file_type(&mut self, ft: &str) -> bool {
        if self.descr_written {
            return false;
        }
        self.filetype = ft.to_string();
        true
    }

    /// `OFStream()`: the underlying stream, for callers that print directly.
    /// Use [`crate::fmt`] for `<< double` semantics.
    pub fn ofstream(&mut self) -> &mut (dyn Write + Send) {
        &mut *self.fp
    }

    fn emit(&mut self, s: &str) {
        if self.fp.write_all(s.as_bytes()).is_err() {
            self.failed = true;
        }
    }

    /// `std::endl`: newline, then flush.
    fn emit_endl(&mut self) {
        self.emit("\n");
        self.flush();
    }

    /// `Write(value, descr)` for every overload: before the header is
    /// written, the value joins the held-back first row and the column is
    /// described; afterwards it goes straight to the file.
    pub fn write<V: DataValue>(&mut self, value: V, descr: &str) {
        self.write_with_format(value, descr, "");
    }

    /// `Write(value, descr, format)`; `format` feeds the `#format` line.
    pub fn write_with_format<V: DataValue>(&mut self, value: V, descr: &str, format: &str) {
        let mut cell = String::new();
        value.write_cell(&mut cell);
        if !self.descr_written {
            self.data.push_str(&cell);
            let format = if value.keeps_format() { format } else { "" };
            self.write_column_desc(descr, format);
        } else {
            self.emit(&cell);
        }
    }

    /// `WriteAnonymous(value)`: `m_fp << value << " "`, immediately and
    /// without a column description (QUIRK: even before the header is out).
    pub fn write_anonymous<V: DataValue>(&mut self, value: V) {
        let mut cell = String::new();
        value.write_cell(&mut cell);
        self.emit(&cell);
    }

    /// `WriteBlockElement(value, element, x_size)`: newline after every
    /// `x_size` elements.
    pub fn write_block_element<V: DataValue>(&mut self, value: V, element: i32, x_size: i32) {
        self.write_anonymous(value);
        if (element.wrapping_add(1)) % x_size == 0 {
            self.emit("\n");
        }
    }

    /// `WriteColumnDesc(descr, format)`: `"# %2d: %s\n"`.
    pub fn write_column_desc(&mut self, descr: &str, format: &str) {
        if !self.descr_written {
            self.num_cols += 1;
            self.descr
                .push_str(&format!("# {:2}: {}\n", self.num_cols, descr));
            if !format.is_empty() {
                self.format.push_str(format);
                self.format.push(' ');
            }
        }
    }

    /// `WriteComment(comment)`: `"# %s\n"` into the header.
    pub fn write_comment(&mut self, comment: &str) {
        if !self.descr_written {
            self.descr.push_str(&format!("# {comment}\n"));
        }
    }

    /// `WriteRawComment(comment)`: `"%s\n"` into the header.
    pub fn write_raw_comment(&mut self, comment: &str) {
        if !self.descr_written {
            self.descr.push_str(comment);
            self.descr.push('\n');
        }
    }

    /// `WriteRaw(str)`: `str` and a newline, to the file immediately.
    pub fn write_raw(&mut self, s: &str) {
        self.emit(s);
        self.emit("\n");
    }

    /// `WriteTimeStamp()`: `"# " + ctime(now)` into the header.
    pub fn write_time_stamp(&mut self) {
        if !self.descr_written {
            let stamp = ctime_now();
            self.descr.push_str("# ");
            self.descr.push_str(&stamp);
        }
    }

    /// `FlushComments()`: writes the header as it stands (no blank line) and
    /// marks it written.
    pub fn flush_comments(&mut self) {
        if !self.descr_written {
            let descr = std::mem::take(&mut self.descr);
            self.emit(&descr);
            self.descr_written = true;
            debug_assert!(self.data.is_empty());
        }
    }

    /// `Endl()`: the first call writes `#filetype`/`#format` (if set), the
    /// header and a blank line, then the held-back first row; later calls end
    /// the current row. Always flushes.
    pub fn endl(&mut self) {
        if !self.descr_written {
            if !self.filetype.is_empty() {
                let line = format!("#filetype {}", self.filetype);
                self.emit(&line);
                self.emit_endl();
            }
            if !self.format.is_empty() {
                let line = format!("#format {}", self.format);
                self.emit(&line);
                self.emit_endl();
            }
            let descr = std::mem::take(&mut self.descr);
            self.emit(&descr);
            self.emit_endl();
            let data = std::mem::take(&mut self.data);
            self.emit(&data);
            self.emit_endl();
            self.descr_written = true;
        } else {
            self.emit_endl();
        }
    }

    /// `Flush()`.
    pub fn flush(&mut self) {
        if self.fp.flush().is_err() {
            self.failed = true;
        }
    }
}

impl Drop for DataFile {
    /// `Socket::~Socket`: a non-static file unregisters its id.
    fn drop(&mut self) {
        let _ = self.fp.flush();
        if !self.is_static {
            if let Some(reg) = &self.registry {
                lock(reg).open.remove(&self.id);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Apto::FileSystem helpers
// ---------------------------------------------------------------------------

/// `Apto::FileSystem::GetCWD()` (empty if unavailable).
fn get_cwd() -> String {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// `Apto::FileSystem::GetAbsolutePath(path, working_dir)`: `working_dir/path`
/// unless `path` starts with `/` or `\`.
pub fn get_absolute_path(path: &str, working_dir: &str) -> String {
    match path.as_bytes().first() {
        None => working_dir.to_string(),
        Some(b'/') | Some(b'\\') => path.to_string(),
        Some(_) => format!("{working_dir}/{path}"),
    }
}

/// `Apto::FileSystem::MkDir(dirname)`: creates one directory level if the
/// name does not exist. Returns false if creation failed.
pub fn mk_dir(dirname: &str) -> bool {
    if Path::new(dirname).exists() {
        return true;
    }
    fs::create_dir(dirname).is_ok()
}

/// `Apto::String::Trim()` whitespace set.
fn trim_apto(s: &str) -> &str {
    s.trim_matches(|c| matches!(c, ' ' | '\x0c' | '\n' | '\r' | '\t' | '\x0b'))
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

/// Port of `Avida::Output::Manager` (avida-core/source/output/Manager.cc):
/// owns the data directory, translates paths into output ids, and tracks
/// which ids are open.
#[derive(Debug)]
pub struct Manager {
    /// Stands in for `GetCWD()`.
    working_dir: String,
    output_path: String,
    registry: SharedRegistry,
    /// `m_static_sockets`.
    static_files: BTreeMap<OutputId, DataFile>,
}

impl Manager {
    /// `Manager(output_path)`: trims the path, makes it absolute against the
    /// working directory, appends `/`, and creates the directory (one level,
    /// as `MkDir`). cWorld passes `GetAbsolutePath(DATA_DIR, working_dir)`.
    pub fn new(output_path: &str) -> Self {
        Self::with_working_dir(output_path, &get_cwd())
    }

    /// As [`Manager::new`], with `working_dir` standing in for the process's
    /// working directory, so a run embedded in another program (a GUI
    /// started from anywhere) writes where the command-line run would.
    pub fn with_working_dir(output_path: &str, working_dir: &str) -> Self {
        let mut path = get_absolute_path(trim_apto(output_path), working_dir);
        if let Some(tail) = path.chars().last() {
            if tail != '\\' && tail != '/' {
                path.push('/');
            }
            mk_dir(&path);
        }
        Manager {
            working_dir: working_dir.to_string(),
            output_path: path,
            registry: Arc::new(Mutex::new(Registry::default())),
            static_files: BTreeMap::new(),
        }
    }

    /// `OutputPath()` (absolute, ending in `/`).
    pub fn output_path(&self) -> &str {
        &self.output_path
    }

    /// `OutputIDFromPath(path)`: paths starting with `.`, `..`, `./`, `.\`
    /// or `/` are relative to the working directory, everything else to the
    /// data directory. Intermediate directories are created on the way.
    ///
    /// QUIRK: directories are created at `dir_prefix + path[..d]` with no
    /// separator, so for working-directory paths the prefix (the CWD, with no
    /// trailing slash) is glued to the path: `./sub/x` tries `<cwd>./sub`
    /// (which fails harmlessly) and `/abs/x` creates `<cwd>/abs`. The
    /// returned id for data-directory paths contains `//`
    /// (`<data>/` + `/` + path).
    pub fn output_id_from_path(&self, path: &str) -> OutputId {
        let path = trim_apto(path);
        let bytes = path.as_bytes();
        let at = |i: usize| bytes.get(i).copied().unwrap_or(0);

        // Determine directory prefix, default being the current data directory
        let mut dir_prefix = self.output_path.clone();
        if (at(0) == b'.'
            && (bytes.len() == 1
                || (bytes.len() > 1 && (at(1) == b'.' || at(1) == b'/' || at(1) == b'\\'))))
            || at(0) == b'/'
        {
            dir_prefix = self.working_dir.clone();
        }

        // Create directory structure, as necessary
        let find = |c: u8, from: usize| -> Option<usize> {
            bytes[from..].iter().position(|&b| b == c).map(|p| p + from)
        };
        let mut i = 0usize;
        while i < bytes.len() {
            let mut d = find(b'/', i);
            let b = find(b'\\', i);
            // QUIRK: as in C++ (`b != -1 && b < d` with d == -1), a backslash
            // only counts when a forward slash follows it.
            if let (Some(bv), Some(dv)) = (b, d) {
                if bv < dv {
                    d = Some(bv);
                }
            }
            let Some(d) = d else { break };
            if d > i {
                let dir = &path[i..d];
                if dir.len() > 2 || (dir != "." && dir != "..") {
                    mk_dir(&format!("{dir_prefix}{}", &path[..d]));
                }
            }
            i = d + 1;
        }

        get_absolute_path(path, &dir_prefix)
    }

    /// `IsOpen(output_id)`.
    pub fn is_open(&self, output_id: &str) -> bool {
        lock(&self.registry).open.contains(output_id)
    }

    /// `Close(output_id)`: forgets the id; drops (closes) a static file.
    /// Returns whether a static file was removed.
    pub fn close(&mut self, output_id: &str) -> bool {
        lock(&self.registry).open.remove(output_id);
        self.static_files.remove(output_id).is_some()
    }

    /// `FlushAll()`. Only the static files are reachable from here; files
    /// owned by callers flush at every `endl` and on drop. (FlushAll has no
    /// callers in avida-core.)
    pub fn flush_all(&mut self) {
        for f in self.static_files.values_mut() {
            f.flush();
        }
    }

    /// `File::createWithPath(world, path, append, feedback)`; the error text
    /// is what the C++ passes to `Feedback::Error`.
    fn create(&self, path: &str, append: bool) -> Result<DataFile, String> {
        let oid = self.output_id_from_path(path);
        if oid.is_empty() {
            return Err(format!("unable to translate path '{path}' to output id"));
        }
        if self.is_open(&oid) {
            return Err(format!("file '{oid}' already open"));
        }
        let mut file = DataFile::open(&oid, append)
            .map_err(|_| format!("unable to open file '{oid}' for writing"))?;
        // Socket::Socket -> RegisterSocket (ignored if already present).
        lock(&self.registry).open.insert(oid.clone());
        file.registry = Some(Arc::clone(&self.registry));
        Ok(file)
    }

    /// `File::CreateWithPath`: a new (truncated) file owned by the caller.
    pub fn create_with_path(&self, path: &str) -> Result<DataFile, String> {
        self.create(path, false)
    }

    /// `File::AppendWithPath`: as `create_with_path` but appending.
    pub fn append_with_path(&self, path: &str) -> Result<DataFile, String> {
        self.create(path, true)
    }

    /// `File::StaticWithPath`: the manager-owned file for `path`, created on
    /// first use and returned again on later calls until closed.
    pub fn static_with_path(&mut self, path: &str) -> Result<&mut DataFile, String> {
        let oid = self.output_id_from_path(path);
        if oid.is_empty() {
            return Err(format!("unable to translate path '{path}' to output id"));
        }
        if !self.static_files.contains_key(&oid) {
            let mut file = self.create(path, false)?;
            // registerAsStatic -> RegisterStaticSocket.
            file.is_static = true;
            self.static_files.insert(oid.clone(), file);
        }
        self.static_files
            .get_mut(&oid)
            .ok_or_else(|| format!("unable to register '{oid}' as a static socket"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A fresh directory under the system temp dir.
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("avida-apto-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    /// Replaces each `# <ctime>` line with a marker, so files differing only
    /// in their time stamp compare equal.
    fn mask_time_stamps(text: &str) -> String {
        text.lines()
            .map(|l| {
                let is_stamp = l.len() == 26
                    && l.starts_with("# ")
                    && WEEKDAYS.contains(&&l[2..5])
                    && MONTHS.contains(&&l[6..9]);
                if is_stamp {
                    "# <time stamp>".to_string()
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Rebuilds an expected Avida data file through `DataFile`, issuing the
    /// calls cStats makes (comments, time stamp, typed writes, endl), and
    /// checks the bytes. Values are re-parsed from the file, which
    /// round-trips through `%g`.
    fn replay(expected_path: &Path, out_dir: &Path) {
        let expected = fs::read_to_string(expected_path).expect("expected file");
        let mgr = Manager::new(out_dir.to_str().expect("utf8"));
        let name = expected_path.file_name().unwrap().to_str().unwrap();
        let mut df = mgr.create_with_path(name).expect("create");

        let mut lines = expected.lines();
        let mut descrs: Vec<String> = Vec::new();
        let mut formats: Vec<String> = Vec::new();
        for line in lines.by_ref() {
            if line.is_empty() {
                break; // blank line between header and data
            }
            if let Some(ft) = line.strip_prefix("#filetype ") {
                df.set_file_type(ft);
            } else if let Some(fm) = line.strip_prefix("#format ") {
                formats = fm.split_whitespace().map(str::to_string).collect();
            } else if mask_time_stamps(line) == "# <time stamp>" {
                df.write_time_stamp();
            } else if line.len() > 5
                && line.as_bytes()[4] == b':'
                && line[2..4].trim().parse::<i32>().is_ok()
            {
                descrs.push(line[6..].to_string());
            } else if let Some(c) = line.strip_prefix("# ") {
                df.write_comment(c);
            } else {
                panic!("unexpected header line {line:?}");
            }
        }
        for (r, row) in lines.enumerate() {
            let cells: Vec<&str> = row.strip_suffix(' ').unwrap_or(row).split(' ').collect();
            // The first row defines the legend; later rows may be ragged
            // (dead genotypes in a .spop omit their trailing columns).
            if r == 0 {
                assert_eq!(cells.len(), descrs.len(), "row {row:?}");
            }
            for (c, cell) in cells.iter().enumerate() {
                let descr = descrs.get(c).map(String::as_str).unwrap_or("");
                let fmt = if r == 0 {
                    formats.get(c).map(String::as_str).unwrap_or("")
                } else {
                    ""
                };
                if let Ok(i) = cell.parse::<i32>() {
                    df.write_with_format(i, descr, fmt);
                } else if let Ok(v) = cell.parse::<f64>() {
                    df.write_with_format(v, descr, fmt);
                } else {
                    df.write_with_format(*cell, descr, fmt);
                }
            }
            df.endl();
        }
        let out_path = df.name().to_string();
        drop(df);
        let got = fs::read_to_string(&out_path).expect("written file");
        assert_eq!(
            mask_time_stamps(&got),
            mask_time_stamps(&expected),
            "{}",
            expected_path.display()
        );
        assert!(got.ends_with("\n") == expected.ends_with("\n"));
    }

    #[test]
    fn reproduces_reference_data_files() {
        let data_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../avida-core/tests/heads_default_100u/expected/data");
        let out = temp_dir("replay");
        let mut n = 0;
        let mut entries: Vec<_> = fs::read_dir(&data_dir)
            .expect("reference data dir")
            .map(|e| e.expect("entry").path())
            .filter(|p| matches!(p.extension().and_then(|e| e.to_str()), Some("dat" | "spop")))
            .collect();
        entries.sort();
        for path in entries {
            replay(&path, &out);
            n += 1;
        }
        assert!(n >= 9, "replayed {n} files");
        let _ = fs::remove_dir_all(out);
    }

    #[test]
    fn header_layout_and_overload_quirks() {
        let dir = temp_dir("layout");
        let mut mgr = Manager::new(dir.to_str().unwrap());
        let df = mgr.static_with_path("x.dat").expect("static");
        df.set_file_type("test_data");
        df.write_comment("Avida test");
        df.write_raw_comment("#raw comment");
        df.write_anonymous(7); // QUIRK: goes out before the header
        df.write_with_format(1.5, "a double", "a");
        df.write_with_format(2u32, "an unsigned", "u"); // format dropped
        df.write_with_format(&[1, 2, 3][..], "a list", "l");
        df.write("str", "a string");
        df.write(1_i64 << 40, "a long");
        df.endl();
        assert!(!df.set_file_type("late"));
        df.write_comment("ignored after header");
        df.write(f64::NAN, "ignored");
        df.write(1e-5, "ignored");
        df.endl();
        df.write_raw("raw line");
        for e in 0..4 {
            df.write_block_element(e, e, 2);
        }
        df.flush();
        let id = df.name().to_string();
        assert!(id.ends_with("//x.dat"), "{id}");
        assert!(mgr.close(&id));
        assert!(!mgr.is_open(&id));

        let got = fs::read_to_string(&id).unwrap();
        let expect = "7 #filetype test_data\n#format a l \n# Avida test\n#raw comment\n\
#  1: a double\n#  2: an unsigned\n#  3: a list\n#  4: a string\n#  5: a long\n\n\
1.5 2 1 2 3 str 1099511627776 \nnan 1e-05 \nraw line\n0 1 \n2 3 \n";
        assert_eq!(got, expect);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn flush_comments_then_rows() {
        let dir = temp_dir("flushc");
        let mgr = Manager::new(dir.to_str().unwrap());
        let mut df = mgr.create_with_path("y.dat").unwrap();
        df.write_comment("c");
        df.write_time_stamp();
        df.flush_comments();
        assert!(df.header_done());
        df.write(3, "three");
        df.endl();
        let id = df.name().to_string();
        drop(df);
        let got = fs::read_to_string(&id).unwrap();
        assert_eq!(mask_time_stamps(&got), "# c\n# <time stamp>\n3 ");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn manager_paths_and_registration() {
        let dir = temp_dir("mgr");
        let base = dir.to_str().unwrap();
        // Trailing slash added, whitespace trimmed, one directory level made.
        let mut mgr = Manager::new(&format!("  {base}/data \n"));
        assert_eq!(mgr.output_path(), format!("{base}/data/"));
        assert!(dir.join("data").is_dir());

        // Data-directory relative ids, with intermediate directories.
        let id = mgr.output_id_from_path("sub/deeper/z.dat");
        assert_eq!(id, format!("{base}/data//sub/deeper/z.dat"));
        assert!(dir.join("data/sub/deeper").is_dir());
        // `..` components are not created; `.` prefix means working dir.
        let cwd = get_cwd();
        assert_eq!(mgr.output_id_from_path("./q.dat"), format!("{cwd}/./q.dat"));
        assert_eq!(mgr.output_id_from_path("."), format!("{cwd}/."));
        assert_eq!(
            mgr.output_id_from_path(".hidden"),
            format!("{base}/data//.hidden")
        );
        // Absolute paths are kept as they are. (Not run through
        // output_id_from_path here: its QUIRK would create `<cwd>/<first
        // component>` inside the crate directory.)
        assert_eq!(get_absolute_path("/abs.dat", "/wd"), "/abs.dat");
        assert_eq!(get_absolute_path("\\abs.dat", "/wd"), "\\abs.dat");
        assert_eq!(get_absolute_path("rel.dat", "/wd"), "/wd/rel.dat");
        assert_eq!(mgr.output_id_from_path(""), format!("{base}/data/"));

        // Non-static files: open while alive, unregistered on drop.
        let f = mgr.create_with_path("one.dat").unwrap();
        let id = f.name().to_string();
        assert!(mgr.is_open(&id));
        assert_eq!(
            mgr.create_with_path("one.dat").err(),
            Some(format!("file '{id}' already open"))
        );
        // A static request for an id held by a caller-owned file fails too.
        assert!(mgr.static_with_path("one.dat").is_err());
        drop(f);
        assert!(!mgr.is_open(&id));
        assert!(!mgr.close(&id));

        // Static files persist and are returned again.
        mgr.static_with_path("s.dat").unwrap().write(1, "one");
        mgr.static_with_path("s.dat").unwrap().write(2, "two");
        let sid = mgr.output_id_from_path("s.dat");
        assert!(mgr.is_open(&sid));
        mgr.static_with_path("s.dat").unwrap().endl();
        assert!(mgr.close(&sid));
        assert_eq!(
            fs::read_to_string(&sid).unwrap(),
            "#  1: one\n#  2: two\n\n1 2 \n"
        );

        // Append mode.
        let mut a = mgr.append_with_path("s.dat").unwrap();
        a.flush_comments();
        a.write_raw("more");
        drop(a);
        assert!(fs::read_to_string(&sid).unwrap().ends_with("1 2 \nmore\n"));

        // Unwritable location: a directory component that is a plain file.
        fs::write(dir.join("data/blocker"), "").unwrap();
        let bad = format!("{base}/data//blocker/f.dat");
        let err = mgr.create_with_path("blocker/f.dat").err().unwrap();
        assert_eq!(err, format!("unable to open file '{bad}' for writing"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ctime_layout_matches_c() {
        // From asctime(gmtime(&t)) on the reference platform.
        let cases: &[(i64, &str)] = &[
            (0, "Thu Jan  1 00:00:00 1970"),
            (951_782_400, "Tue Feb 29 00:00:00 2000"),
            (1_774_000_000, "Fri Mar 20 09:46:40 2026"),
            (1_774_083_464, "Sat Mar 21 08:57:44 2026"),
            (4_102_444_799, "Thu Dec 31 23:59:59 2099"),
            (1_234_567_890, "Fri Feb 13 23:31:30 2009"),
            (-86_400, "Wed Dec 31 00:00:00 1969"),
            (1_709_251_199, "Thu Feb 29 23:59:59 2024"),
            (1_000_000_000, "Sun Sep  9 01:46:40 2001"),
            (2_147_483_647, "Tue Jan 19 03:14:07 2038"),
        ];
        for &(t, s) in cases {
            assert_eq!(ctime_utc(t), format!("{s}\n"), "t = {t}");
        }
        assert_eq!(ctime_now().len(), 25);
    }

    #[test]
    fn in_memory_sink() {
        #[derive(Clone, Default)]
        struct Shared(Arc<Mutex<Vec<u8>>>);
        impl Write for Shared {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let sink = Shared::default();
        let mut df = DataFile::from_writer("mem", Box::new(sink.clone()));
        df.write(0.25, "q");
        df.endl();
        writeln!(df.ofstream(), "direct").unwrap();
        assert!(df.good() && !df.fail());
        let text = String::from_utf8(sink.0.lock().unwrap().clone()).unwrap();
        assert_eq!(text, "#  1: q\n\n0.25 \ndirect\n");
    }
}
