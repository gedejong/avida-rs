//! Registration tests: every name the C++ accepts resolves, descriptions, ids,
//! dedup, env requirements, and the real `environment.cfg` task strings.

use avida_tasks::{EnvReqs, Feedback, TaskKind, TaskLib};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // crates/tasks -> rust/avida-native -> rust -> repo
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .canonicalize()
        .expect("repo root")
}

fn cpp_task_lib() -> String {
    // The copy kept from the retired C++ (git tag `cpp-final`).
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/reference/cTaskLib.cc"))
        .expect("read cTaskLib.cc")
}

/// Task names the C++ `AddTask` compares against: every `name == "..."` in the
/// body of `cTaskLib::AddTask`.
fn scraped_names() -> BTreeSet<String> {
    let src = cpp_task_lib();
    let start = src.find("cTaskEntry* cTaskLib::AddTask").expect("AddTask");
    let end = src[start..]
        .find("void cTaskLib::NewTask")
        .expect("NewTask")
        + start;
    let body = &src[start..end];
    let mut names = BTreeSet::new();
    let pat = "name == \"";
    let mut rest = body;
    while let Some(i) = rest.find(pat) {
        rest = &rest[i + pat.len()..];
        let j = rest.find('"').unwrap();
        names.insert(rest[..j].to_string());
        rest = &rest[j..];
    }
    names
}

/// A valid argument string for the tasks that have required arguments.
fn args_for(name: &str) -> &'static str {
    match name {
        "matchstr" => "string=0101",
        "matchprodstr" => "string=0101,tag=1",
        "match_number" => "target=5,halflife=2",
        "sort_inputs" => "size=4,halflife=2",
        "fibonacci_seq" => "target=3",
        "mult" | "div" | "log" | "log2" | "log10" | "sqrt" | "sine" | "cosine" => "halflife=2",
        "optimize" => "function=1",
        "sg_path_traversal" => "pathlen=3,sgname=grid,poison=P",
        "opinion_is" => "opinion=1",
        _ => "",
    }
}

#[test]
fn every_name_in_cpp_add_task_resolves() {
    let names = scraped_names();
    // 180 argument-free registrations + 34 Load_* families
    assert_eq!(names.len(), 214, "scrape found {:?}", names);
    for n in [
        "not",
        "echo_dup",
        "xor-max",
        "logic_3CP",
        "math_2AAA",
        "royal-road-wd",
    ] {
        assert!(names.contains(n), "{}", n);
    }

    let mut lib = TaskLib::new();
    for name in &names {
        let mut er = EnvReqs::new();
        let mut fb = Feedback::new();
        let id = lib.add_task(name, args_for(name), &mut er, &mut fb);
        assert!(id.is_some(), "task '{}' rejected: {:?}", name, fb.messages);
        assert!(fb.messages.is_empty(), "{}: {:?}", name, fb.messages);
        let entry = lib.get_task(id.unwrap()).unwrap();
        assert_eq!(entry.name(), name);
    }
    assert_eq!(lib.get_size(), 214);
}

#[test]
fn unknown_name_reports_cpp_message() {
    let mut lib = TaskLib::new();
    let mut er = EnvReqs::new();
    let mut fb = Feedback::new();
    assert_eq!(lib.add_task("bogus", "", &mut er, &mut fb), None);
    assert_eq!(fb.errors(), vec!["unknown/unprocessed task entry 'bogus'"]);
    // names are case sensitive (`cString::operator==`)
    let mut fb = Feedback::new();
    assert_eq!(lib.add_task("NOT", "", &mut er, &mut fb), None);
    assert_eq!(lib.get_size(), 0);
}

#[test]
fn ids_follow_registration_order_and_argless_tasks_dedupe() {
    let mut lib = TaskLib::new();
    let mut er = EnvReqs::new();
    let mut fb = Feedback::new();
    let names = [
        "not", "nand", "and", "orn", "or", "andn", "nor", "xor", "equ",
    ];
    for (i, n) in names.iter().enumerate() {
        assert_eq!(lib.add_task(n, "", &mut er, &mut fb), Some(i));
    }
    // already registered, no arguments: same entry
    assert_eq!(lib.add_task("nor", "", &mut er, &mut fb), Some(6));
    assert_eq!(lib.get_size(), 9);
    // an alias name is a separate entry that shares the evaluator
    assert_eq!(lib.add_task("not_dup", "", &mut er, &mut fb), Some(9));
    assert_eq!(
        lib.get_task(9).unwrap().kind(),
        lib.get_task(0).unwrap().kind()
    );
    // tasks with arguments are never deduplicated
    assert_eq!(
        lib.add_task("mult", "halflife=1", &mut er, &mut fb),
        Some(10)
    );
    assert_eq!(
        lib.add_task("mult", "halflife=1", &mut er, &mut fb),
        Some(11)
    );
    assert_eq!(lib.get_size(), 12);
    // an unknown name after others does not disturb ids
    assert_eq!(lib.add_task("bogus", "", &mut er, &mut fb), None);
    assert_eq!(lib.add_task("echo", "", &mut er, &mut fb), Some(12));
}

#[test]
fn descriptions_and_property_ids() {
    let mut lib = TaskLib::new();
    let mut er = EnvReqs::new();
    let mut fb = Feedback::new();
    let want = [
        ("not", "Not"),
        ("orn", "OrNot"),
        ("equ", "Equals"),
        ("andn_dup", "AndNot_dup"),
        ("xor-max", "Xor-max"),
        ("logic_3AB", "Logic 3AB (A+B+C == 1)"),
        ("math_1AB", "Math 1AB (2X/3)"),
        ("math_2AJ", "Math 2AJ ((sqrt(X)+Y)/(X-7))"),
        ("fib_2", "Second and Third Fib number (1)"),
        ("comm_echo", "Echo of Neighbor's Input"),
        ("move", "Successfully Moved"),
        ("movetoevent", "Move to a target area"),
        ("exploded2", "Organism exploded"),
        ("ai-display-cost", "Autoinducer cost paid"),
    ];
    for (name, desc) in want {
        let id = lib.add_task(name, "", &mut er, &mut fb).unwrap();
        assert_eq!(lib.get_task(id).unwrap().desc(), desc, "{}", name);
    }
    let id = lib.add_task("nand", "", &mut er, &mut fb).unwrap();
    let e = lib.get_task(id).unwrap();
    assert_eq!(e.average_property_id(), "environment.triggers.nand.average");
    assert_eq!(e.count_property_id(), "environment.triggers.nand.count");
    assert!(!e.has_arguments());
    assert!(e.arguments().is_none());

    for (name, args, desc) in [
        ("matchstr", "string=01", "MatchStr"),
        ("match_number", "target=1,halflife=1", "Match Number"),
        ("sort_inputs", "size=2,halflife=1", "Sort Inputs"),
        ("optimize", "function=2", "Optimize"),
        ("log10", "halflife=1", "Logarithm (base-10)"),
        ("eat-target-orn", "", "ConsumeTargetOrn"),
        ("royal-road", "", "royal-road"),
        (
            "opinion_is",
            "opinion=3",
            "Whether organism's opinion is set to value.",
        ),
    ] {
        let id = lib.add_task(name, args, &mut er, &mut fb).unwrap();
        let e = lib.get_task(id).unwrap();
        assert_eq!(e.desc(), desc);
        assert!(e.has_arguments());
    }
    assert!(fb.messages.is_empty());
}

#[test]
fn matchprodstr_registers_matchstr_evaluator() {
    // QUIRK: Load_MatchProdStr passes Task_MatchStr to NewTask.
    let mut lib = TaskLib::new();
    let mut er = EnvReqs::new();
    let mut fb = Feedback::new();
    let id = lib
        .add_task("matchprodstr", "string=1100,tag=2", &mut er, &mut fb)
        .unwrap();
    assert_eq!(lib.get_task(id).unwrap().kind(), TaskKind::MatchStr);
    assert_eq!(lib.get_task(id).unwrap().desc(), "MatchProdStr");
    assert_eq!(lib.get_match_strings(), vec!["1100".to_string()]);
    assert_eq!(lib.get_match_string(0), "1100");
    assert_eq!(lib.get_match_string(1), "");
    assert_eq!(lib.get_match_string(-1), "");
    assert_eq!(lib.get_number_of_match_strings(), 1);
    assert_eq!(er.get_min_outputs(), 4);
}

#[test]
fn neighbor_flags_and_environment_side_effects() {
    let mut lib = TaskLib::new();
    let mut er = EnvReqs::new();
    let mut fb = Feedback::new();
    assert!(!lib.use_neighbor_input());
    lib.add_task("not", "", &mut er, &mut fb).unwrap();
    assert!(!lib.use_neighbor_input());
    lib.add_task("comm_not", "", &mut er, &mut fb).unwrap();
    assert!(lib.use_neighbor_input());
    assert!(!lib.use_neighbor_output());

    lib.add_task("eat-target", "target_id=3", &mut er, &mut fb)
        .unwrap();
    lib.add_task("move-ft", "", &mut er, &mut fb).unwrap();
    lib.add_task("form-group-id", "group_size=5,group_id=7", &mut er, &mut fb)
        .unwrap();
    lib.add_task("live-on-patch-id", "patch_id=0", &mut er, &mut fb)
        .unwrap();
    assert_eq!(lib.target_ids(), &[3, 1]);
    assert_eq!(lib.group_ids(), &[7, 0]);
    assert_eq!(lib.take_target_ids(), vec![3, 1]);
    assert!(lib.target_ids().is_empty());
    assert_eq!(lib.take_group_ids(), vec![7, 0]);
}

#[test]
fn env_reqs_set_by_loaders() {
    let mut lib = TaskLib::new();
    let mut fb = Feedback::new();

    let mut er = EnvReqs::new();
    lib.add_task(
        "sort_inputs",
        "size=10,direction=-1,halflife=2.0",
        &mut er,
        &mut fb,
    )
    .unwrap();
    assert_eq!((er.get_min_inputs(), er.get_min_outputs()), (10, 20));
    assert!(er.get_true_rand_inputs());

    // binary optimize: varlength * numvars (from the repo's environment.cfg)
    let mut er = EnvReqs::new();
    lib.add_task(
        "optimize",
        "function=9,binary=1,varlength=3,numvars=31,basepow=1.3,maxFx=10.0,minFx=0.0",
        &mut er,
        &mut fb,
    )
    .unwrap();
    assert_eq!(er.get_min_outputs(), 93);

    // continuous optimize: 1 output for F1, 2 otherwise
    let mut er = EnvReqs::new();
    lib.add_task("optimize", "function=1", &mut er, &mut fb)
        .unwrap();
    assert_eq!(er.get_min_outputs(), 1);
    let mut er = EnvReqs::new();
    lib.add_task("optimize", "function=7", &mut er, &mut fb)
        .unwrap();
    assert_eq!(er.get_min_outputs(), 2);

    let mut er = EnvReqs::new();
    lib.add_task("royal-road-wd", "length=12,block_count=3", &mut er, &mut fb)
        .unwrap();
    assert_eq!(er.get_min_outputs(), 12);
    assert_eq!(er.get_min_inputs(), 0);
    assert!(fb.messages.is_empty());
}

#[test]
fn bad_arguments_report_cpp_messages() {
    let mut lib = TaskLib::new();
    let mut er = EnvReqs::new();

    let mut fb = Feedback::new();
    assert_eq!(lib.add_task("matchstr", "", &mut er, &mut fb), None);
    assert_eq!(
        fb.errors(),
        vec![
            "required argument 'string' was not found",
            "unknown/unprocessed task entry 'matchstr'"
        ]
    );

    let mut fb = Feedback::new();
    assert_eq!(
        lib.add_task("mult", "halflife=2,bogus=1", &mut er, &mut fb),
        None
    );
    assert_eq!(
        fb.errors(),
        vec![
            "unrecognized argument: 'bogus'",
            "unknown/unprocessed task entry 'mult'"
        ]
    );

    let mut fb = Feedback::new();
    assert_eq!(lib.add_task("mult", "threshold=3", &mut er, &mut fb), None);
    assert_eq!(
        fb.errors(),
        vec![
            "required argument 'halflife' was not found",
            "unknown/unprocessed task entry 'mult'"
        ]
    );
    assert_eq!(lib.get_size(), 0);
}

/// The Logic3in targets compiled in must equal the ids in the C++ source.
#[test]
fn logic3_targets_match_cpp() {
    let src = cpp_task_lib();
    let mut lib = TaskLib::new();
    let mut er = EnvReqs::new();
    let mut fb = Feedback::new();
    let mut checked = 0;
    let marker = "double cTaskLib::Task_Logic3in_";
    let mut rest = src.as_str();
    while let Some(i) = rest.find(marker) {
        rest = &rest[i + marker.len()..];
        let suffix: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect();
        let end = rest.find("\n}").unwrap();
        let body = &rest[..end];
        let call = body.find("ctx.GetLogicId()").unwrap() + "ctx.GetLogicId()".len();
        let paren = body[call..].find(')').unwrap() + call;
        let targets: Vec<i32> = body[call..paren]
            .split(',')
            .filter_map(|t| t.trim().parse().ok())
            .collect();
        let name = format!("logic_3{}", suffix);
        let id = lib.add_task(&name, "", &mut er, &mut fb).unwrap();
        match lib.get_task(id).unwrap().kind() {
            TaskKind::Logic3(t) => assert_eq!(t, &targets[..], "{}", name),
            k => panic!("{} registered as {:?}", name, k),
        }
        checked += 1;
    }
    assert_eq!(checked, 68);
}

/// Every task named in a real environment file registers, with its real
/// argument string (`REACTION <name> <task>[:args] ...`).
#[test]
fn real_environment_files_register() {
    let root = repo_root();
    let mut files = Vec::new();
    collect_env_files(&root.join("avida-core/tests"), &mut files);
    collect_env_files(&root.join("avida-core/support/config"), &mut files);
    assert!(files.len() >= 75, "found {} env files", files.len());

    let mut lib = TaskLib::new();
    let mut seen = BTreeSet::new();
    for f in &files {
        let text = fs::read_to_string(f).unwrap_or_default();
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("");
            let mut words = line.split_whitespace();
            if !words
                .next()
                .is_some_and(|w| w.eq_ignore_ascii_case("reaction"))
            {
                continue;
            }
            let _rname = words.next();
            let Some(trigger) = words.next() else {
                continue;
            };
            let (task, info) = match trigger.find(':') {
                Some(i) => (&trigger[..i], &trigger[i + 1..]),
                None => (trigger, ""),
            };
            let mut er = EnvReqs::new();
            let mut fb = Feedback::new();
            let id = lib.add_task(task, info, &mut er, &mut fb);
            assert!(
                id.is_some(),
                "{}: '{}' / '{}' -> {:?}",
                f.display(),
                task,
                info,
                fb.messages
            );
            seen.insert(task.to_string());
        }
    }
    for t in [
        "not",
        "nand",
        "optimize",
        "sort_inputs",
        "live-on-patch-id",
        "matchprodstr",
        "perfect_strings",
    ] {
        assert!(seen.contains(t), "no real use of {}", t);
    }
}

fn collect_env_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_env_files(&p, out);
        } else if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
            if n.contains("env") && !n.ends_with(".dat") {
                out.push(p);
            }
        }
    }
}
