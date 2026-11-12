//! Analyze scripts run through a mock host (no test CPU) and compared with
//! the output of the C++ reference binary (`cbuild/work/avida -a`). The
//! commands that run genomes are covered with the real world by
//! `tests/consistency.rs`.
//!
//! Expected files come from two places:
//! - the repository's consistency tests (`avida-core/tests/<test>/expected`)
//!   where those are current;
//! - `tests/fixtures/`, captured from the reference binary for scripts
//!   whose repository expectations need a test CPU (`RECALCULATE`) or do
//!   not exist. `fixtures/detail_lineage` is `_analyze_detail_all` without
//!   its `RECALCULATE` steps; `fixtures/lang` exercises the control
//!   commands. Each `stdout.txt` is the reference output after the
//!   `In analyze mode!!` banner line.

use std::cell::RefCell;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use avida_analyze::{Analyze, AnalyzeError, AnalyzeHost, MAX_BATCHES};
use avida_apto::OutputManager;
use avida_config::{Config, Feedback, InitFile};
use avida_genome::{InstCodeConfig, InstLib, InstLibEntry, InstSetRegistry};

// -- Mock host ---------------------------------------------------------------

/// A host with configuration, output files and an instruction set, but no
/// test CPU and no world RNG.
struct MockHost {
    config: Config,
    working_dir: String,
    output: OutputManager,
    inst_sets: InstSetRegistry,
    /// The analyze-mode flag of the world context.
    analyze_mode: bool,
}

impl MockHost {
    fn new(working_dir: &Path, data_dir: &Path, inst_lines: &[String]) -> Self {
        let mut config = Config::new();
        config.random_seed = 100;
        MockHost {
            config,
            working_dir: working_dir.to_string_lossy().into_owned(),
            output: OutputManager::new(&data_dir.to_string_lossy()),
            inst_sets: registry(inst_lines),
            analyze_mode: false,
        }
    }
}

impl AnalyzeHost for MockHost {
    fn config(&self) -> &Config {
        &self.config
    }
    fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }
    fn working_dir(&self) -> &str {
        &self.working_dir
    }
    fn output(&mut self) -> &mut OutputManager {
        &mut self.output
    }
    fn inst_sets(&self) -> &InstSetRegistry {
        &self.inst_sets
    }
    fn num_tasks(&self) -> i32 {
        9
    }
    fn task_desc(&self, task_id: i32) -> Result<String, AnalyzeError> {
        const LOGIC9: [&str; 9] = [
            "not", "nand", "and", "orn", "or", "andn", "nor", "xor", "equ",
        ];
        Ok(LOGIC9[task_id as usize].to_string())
    }
    fn input_size(&self) -> i32 {
        3
    }
    fn analyze_mode(&self) -> bool {
        self.analyze_mode
    }
    fn set_analyze_mode(&mut self, on: bool) {
        self.analyze_mode = on;
    }
}

/// A one-set registry whose library holds exactly the instructions named
/// by the `INST` lines (the first ones starting `nop-` are the NOPs) plus
/// the library's `NULL` instruction, which no set may use.
fn registry(inst_lines: &[String]) -> InstSetRegistry {
    let mut names: Vec<String> = Vec::new();
    for l in inst_lines {
        if let Some(rest) = l.strip_prefix("INST ") {
            let n = rest.split([' ', ':']).next().unwrap().to_string();
            if !names.contains(&n) {
                names.push(n);
            }
        }
    }
    let nops = names
        .iter()
        .take_while(|n| n.starts_with("nop-"))
        .enumerate()
        .map(|(i, n)| (n.clone(), i as i32))
        .collect();
    let mut entries: Vec<InstLibEntry> = names.iter().map(|n| InstLibEntry::named(n)).collect();
    entries.push(InstLibEntry::named("NULL"));
    let null = entries.len() as i32 - 1;
    let lib = Arc::new(InstLib::new(entries, nops, 0, null));
    let mut reg = InstSetRegistry::new();
    let mut fb = Feedback::new();
    let ok = reg.load_inst_sets(
        inst_lines,
        InstCodeConfig::default(),
        &mut |_hw: i32| Some(lib.clone()),
        &mut fb,
    );
    assert!(ok, "{fb:?}");
    reg
}

fn init_lines(dir: &Path, file: &str) -> Vec<String> {
    let f = InitFile::new(file, &dir.to_string_lossy(), None, None);
    assert!(f.was_opened(), "{}/{file}", dir.display());
    (0..f.num_lines()).map(|i| f.get_line(i as i32)).collect()
}

/// `INSTSET`/`INST` lines of a new-style instruction-set file.
fn inst_set_file(dir: &Path, file: &str) -> Vec<String> {
    init_lines(dir, file)
        .into_iter()
        .filter(|l| l.starts_with("INSTSET ") || l.starts_with("INST "))
        .collect()
}

/// A legacy `name redundancy ...` instruction-set file as one `INSTSET`.
fn legacy_inst_set_file(dir: &Path, file: &str) -> Vec<String> {
    let mut lines = vec!["INSTSET legacy:hw_type=0".to_string()];
    for l in init_lines(dir, file) {
        lines.push(format!("INST {}", l.split(' ').next().unwrap()));
    }
    lines
}

// -- Helpers -----------------------------------------------------------------

#[derive(Clone, Default)]
struct Captured(Rc<RefCell<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Captured {
    fn text(&self) -> String {
        String::from_utf8(self.0.borrow().clone()).unwrap()
    }
}

struct Run {
    analyze: Analyze,
    out: Captured,
    err: Captured,
}

impl Run {
    /// Unwrap `r`, showing the console output when it failed.
    fn ok(&self, r: Result<(), AnalyzeError>) {
        if let Err(e) = r {
            panic!(
                "{e:?}\n--- stdout:\n{}\n--- stderr:\n{}",
                self.out.text(),
                self.err.text()
            );
        }
    }
}

fn analyze_for(host: &mut MockHost) -> Run {
    let out = Captured::default();
    let err = Captured::default();
    let analyze = Analyze::with_streams(host, Box::new(out.clone()), Box::new(err.clone()));
    Run { analyze, out, err }
}

fn repo_tests() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core/tests")
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// A fresh, empty directory under the cargo test temp dir.
fn scratch(name: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("avida-analyze")
        .join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// QUIRK (as in C++): `DETAIL_AVERAGE` hands the absolute output id back to
/// `StaticWithPath`, whose `OutputIDFromPath` treats it as relative to the
/// process directory and creates `<cwd>/<absolute path>` directories. Remove
/// those (empty) directories again so the crate directory stays clean.
fn remove_quirk_dirs(data: &Path) {
    let Ok(cwd) = std::env::current_dir() else {
        return;
    };
    let Ok(rel) = data.strip_prefix("/") else {
        return;
    };
    let mut p = cwd.join(rel);
    while p.starts_with(&cwd) && p != cwd {
        if std::fs::remove_dir(&p).is_err() {
            break;
        }
        match p.parent() {
            Some(parent) => p = parent.to_path_buf(),
            None => break,
        }
    }
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// Line-by-line equality with the first difference in the message.
fn assert_same_text(actual: &str, expected: &str, what: &str) {
    let a: Vec<&str> = actual.lines().collect();
    let e: Vec<&str> = expected.lines().collect();
    for (i, (x, y)) in a.iter().zip(e.iter()).enumerate() {
        assert_eq!(x, y, "{what}: first difference at line {}", i + 1);
    }
    assert_eq!(a.len(), e.len(), "{what}: line count");
    assert_eq!(actual, expected, "{what}: trailing newline");
}

// -- analyze_resize_max_batches -----------------------------------------------

#[test]
fn resize_max_batches() {
    let cfg = repo_tests().join("analyze_resize_max_batches/config");
    let data = scratch("resize");
    let mut host = MockHost::new(&cfg, &data, &inst_set_file(&cfg, "instset-heads.cfg"));
    let mut run = analyze_for(&mut host);
    let r = run.analyze.run_file(&mut host, "analyze_max_batches.cfg");
    assert!(matches!(r, Err(AnalyzeError::Exit(1))), "{r:?}");
    assert_eq!(
        run.out.text(),
        "Increasing max batches to 4000\n\
         Increasing max batches to 8000\n\
         Increasing max batches to 48000\n\
         Increasing max batches to 50000\n"
    );
    assert_eq!(run.err.text(), "  Error: max batches is 50000\n");
    assert_eq!(run.analyze.num_batches(), MAX_BATCHES);
    assert_eq!(run.analyze.current_batch(), 49999);
}

// -- _analyze_detail_all (the part before RECALCULATE) -----------------------

/// Strip the per-cell `bgcolor` the current binary writes and the
/// repository's (older) expected HTML lacks; nothing else differs.
fn strip_cell_bgcolor(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(p) = rest.find("<td") {
        let end = p + rest[p..].find('>').unwrap();
        out.push_str(&rest[..p]);
        let cell = &rest[p..end];
        match cell.find(" bgcolor=\"#") {
            Some(b) if b + 18 == cell.len() => {
                out.push_str(&cell[..b]);
                out.push(' ');
            }
            _ => out.push_str(cell),
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

#[test]
fn detail_all_initial() {
    let cfg = repo_tests().join("_analyze_detail_all/config");
    let expected = repo_tests().join("_analyze_detail_all/expected/data");
    let data = scratch("detail_initial");
    let mut host = MockHost::new(
        &cfg,
        &data,
        &legacy_inst_set_file(&cfg, "instset-classic.cfg"),
    );
    let mut run = analyze_for(&mut host);
    let script = [
        "VERBOSE",
        "LOAD data/detail-5000.pop",
        "LOAD data/historic-50000.pop",
        "DETAIL detail-initial.dat id parent_id num_cpus total_cpus length update_born update_dead depth sequence task_list",
        "DETAIL detail-initial.html id parent_id num_cpus total_cpus length update_born update_dead depth sequence task_list",
    ];
    for line in script {
        let (cmd, args) = line.split_once(' ').unwrap_or((line, ""));
        let r = run.analyze.run_command(&mut host, cmd, args);
        run.ok(r);
    }
    assert_same_text(
        &read(&data.join("detail-initial.dat")),
        &read(&expected.join("detail-initial.dat")),
        "detail-initial.dat",
    );
    let html = read(&data.join("detail-initial.html"));
    assert_same_text(
        &strip_cell_bgcolor(&html),
        &read(&expected.join("detail-initial.html")),
        "detail-initial.html (cell colours stripped)",
    );
    // The colours themselves, as the reference binary writes them.
    assert_eq!(html.matches("bgcolor=\"#CCCCFF\"").count(), 1162);
    assert_eq!(html.matches("bgcolor=\"#FFFFFF\"").count(), 16779);
    assert_eq!(
        html.lines().nth(29).unwrap(),
        "<tr><td align=center bgcolor=\"#FFFFFF\">506633 <td align=center bgcolor=\"#FFFFFF\">472492 <td align=center bgcolor=\"#FFFFFF\">22 <td align=center bgcolor=\"#FFFFFF\">199 <td align=center bgcolor=\"#CCCCFF\">54 <td align=center bgcolor=\"#FFFFFF\">4887 <td align=center bgcolor=\"#FFFFFF\">-1 <td align=center bgcolor=\"#FFFFFF\">53 <td  bgcolor=\"#FFFFFF\">rucavcgkszrcqxczqcppxzcbceiqcraqpchnltqcqtuttttycasvab <td  bgcolor=\"#FFFFFF\"> </tr>"
    );
}

#[test]
fn detail_lineage_and_alignment() {
    let cfg = repo_tests().join("_analyze_detail_all/config");
    let fx = fixtures().join("detail_lineage");
    let data = scratch("detail_lineage");
    let mut host = MockHost::new(
        &cfg,
        &data,
        &legacy_inst_set_file(&cfg, "instset-classic.cfg"),
    );
    let mut run = analyze_for(&mut host);
    let lines = init_lines(&fx, "analyze.cfg");
    let mut cmds = run.analyze.parse_commands(&host, &lines);
    let r = run.analyze.process_commands(&mut host, &mut cmds);
    run.ok(r);
    assert_same_text(&run.out.text(), &read(&fx.join("stdout.txt")), "stdout");
    assert_eq!(run.err.text(), "");
    for f in [
        "detail-initial.dat",
        "detail-lin.html",
        "detail-aligned.dat",
    ] {
        let want = if f == "detail-initial.dat" {
            repo_tests()
                .join("_analyze_detail_all/expected/data")
                .join(f)
        } else {
            fx.join(f)
        };
        assert_same_text(&read(&data.join(f)), &read(&want), f);
    }
}

// -- Control commands ---------------------------------------------------------

#[test]
fn control_commands_script() {
    let cfg = repo_tests().join("_analyze_detail_all/config");
    let fx = fixtures().join("lang");
    let data = scratch("lang");
    let mut host = MockHost::new(
        &fx,
        &data,
        &legacy_inst_set_file(&cfg, "instset-classic.cfg"),
    );
    let mut run = analyze_for(&mut host);
    let r = run.analyze.run_file(&mut host, "analyze.cfg");
    run.ok(r);
    assert_same_text(
        &run.out.text(),
        &read(&fx.join("expected/stdout.txt")),
        "stdout",
    );
    assert_eq!(run.err.text(), "");
    for f in ["lang0.dat", "lang0.html", "lang3.dat"] {
        assert_same_text(&read(&data.join(f)), &read(&fx.join("expected").join(f)), f);
    }
    assert_eq!(host.config.world_x, 77);
    assert_eq!(run.analyze.variable((b'w' - b'a') as usize), "77");
    assert_eq!(run.analyze.batch(3).name, "three");
}

#[test]
fn unknown_keyword_exits() {
    let cfg = repo_tests().join("_analyze_detail_all/config");
    let data = scratch("unknown");
    let mut host = MockHost::new(
        &cfg,
        &data,
        &legacy_inst_set_file(&cfg, "instset-classic.cfg"),
    );
    let mut run = analyze_for(&mut host);
    let r = run.analyze.run_command(&mut host, "NOT_A_COMMAND", "x");
    assert!(matches!(r, Err(AnalyzeError::Exit(1))));
    assert_eq!(
        run.err.text(),
        "error: Unknown analysis keyword 'NOT_A_COMMAND'.\n"
    );
}

#[test]
fn test_cpu_commands_fail_loudly() {
    let cfg = repo_tests().join("_analyze_detail_all/config");
    let data = scratch("unsupported");
    let mut host = MockHost::new(
        &cfg,
        &data,
        &legacy_inst_set_file(&cfg, "instset-classic.cfg"),
    );
    let mut run = analyze_for(&mut host);
    run.analyze
        .run_command(&mut host, "LOAD_SEQUENCE", "rucavcc")
        .unwrap();
    // A host without a test CPU refuses every command that needs one.
    for cmd in ["RECALCULATE", "PRINT", "TRACE"] {
        let r = run.analyze.run_command(&mut host, cmd, "");
        match r {
            Err(AnalyzeError::Unsupported { what, cpp }) => {
                assert_eq!(what, "test CPU", "{cmd}");
                assert!(cpp.contains("CreateTestCPU"), "{cpp}");
            }
            other => panic!("{cmd}: {other:?}"),
        }
    }
    // Outside analyze mode the landscape actions do nothing.
    run.analyze
        .run_command(&mut host, "FullLandscape", "")
        .unwrap();
    // In analyze mode they need the job queue, seeded from the world RNG.
    host.analyze_mode = true;
    let r = run.analyze.run_command(&mut host, "FullLandscape", "");
    match r {
        Err(AnalyzeError::Unsupported { what, .. }) => assert_eq!(what, "analyze job"),
        other => panic!("{other:?}"),
    }
    // HillClimb reads past the end of the genome in C++: it stops.
    let r = run.analyze.run_command(&mut host, "HillClimb", "");
    match r {
        Err(AnalyzeError::Fatal(m)) => assert!(m.contains("HillClimb"), "{m}"),
        other => panic!("{other:?}"),
    }
    // AnalyzePopulation only warns in analyze mode, and needs the
    // population outside it.
    run.analyze
        .run_command(&mut host, "AnalyzePopulation", "")
        .unwrap();
    host.analyze_mode = false;
    let r = run.analyze.run_command(&mut host, "AnalyzePopulation", "");
    match r {
        Err(AnalyzeError::Unsupported { cpp, .. }) => {
            assert!(cpp.contains("cActionAnalyzePopulation"), "{cpp}")
        }
        other => panic!("{other:?}"),
    }
    host.analyze_mode = true;
    let r = run.analyze.run_command(&mut host, "MAP_TASKS", "");
    assert!(matches!(r, Err(AnalyzeError::Unsupported { .. })), "{r:?}");
}

// -- Reduction and summary commands -------------------------------------------

/// `fixtures/reduce`: FILTER, SAMPLE_ORGANISMS / SAMPLE_GENOTYPES (the
/// analyze RNG, seed 100), KEEP_TOP, FIND_CLADE, FIND_GENOTYPE,
/// FIND_LINEAGE, FIND_LAST_COMMON_ANCESTOR, HISTOGRAM (.dat and .html),
/// DETAIL_TIMELINE, DETAIL_BATCHES, DETAIL_AVERAGE, DETAIL_INDEX,
/// PRINT_TASKS and STATUS over the `_analyze_detail_all` populations.
#[test]
fn reduction_and_summary_script() {
    let cfg = repo_tests().join("_analyze_detail_all/config");
    let fx = fixtures().join("reduce");
    let data = scratch("reduce");
    let mut host = MockHost::new(
        &cfg,
        &data,
        &legacy_inst_set_file(&cfg, "instset-classic.cfg"),
    );
    let mut run = analyze_for(&mut host);
    let lines = init_lines(&fx, "analyze.cfg");
    let mut cmds = run.analyze.parse_commands(&host, &lines);
    let r = run.analyze.process_commands(&mut host, &mut cmds);
    run.ok(r);
    assert_same_text(
        &run.out.text(),
        &read(&fx.join("expected/stdout.txt")),
        "stdout",
    );
    assert_eq!(run.err.text(), "");
    remove_quirk_dirs(&data);
    let mut names: Vec<String> = std::fs::read_dir(fx.join("expected"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("red-"))
        .collect();
    names.sort();
    assert_eq!(names.len(), 14);
    for f in &names {
        assert_same_text(&read(&data.join(f)), &read(&fx.join("expected").join(f)), f);
    }
}

/// `FIND_ORGANISM random` over genotypes that are all non-viable (as
/// loaded from a .pop file) retries forever in C++; the port refuses.
#[test]
fn find_organism_endless_retry_is_fatal() {
    let cfg = repo_tests().join("_analyze_detail_all/config");
    let data = scratch("find_organism");
    let mut host = MockHost::new(
        &cfg,
        &data,
        &legacy_inst_set_file(&cfg, "instset-classic.cfg"),
    );
    let mut run = analyze_for(&mut host);
    let r = run
        .analyze
        .run_command(&mut host, "LOAD", "data/detail-5000.pop");
    run.ok(r);
    let r = run
        .analyze
        .run_command(&mut host, "FIND_ORGANISM", "random");
    assert!(matches!(r, Err(AnalyzeError::Fatal(_))), "{r:?}");
}
