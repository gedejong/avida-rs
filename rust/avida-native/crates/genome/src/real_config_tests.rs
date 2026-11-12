//! Tests against the real C++ instruction table and every real
//! instruction-set / organism file in the repository.
//!
//! The CPU instruction library is scraped at test time from the
//! `initInstLib` table of avida-core/source/cpu/cHardwareCPU.cc (names,
//! classes, flags, descriptions and the NOP table), so these tests need no
//! hardware crate.

use crate::feedback::{ErrorMessages, Feedback};
use crate::genome_loader::load_genome_detail_file;
use crate::inst_lib::{inst_flag, BehavClass, InstLib, InstLibEntry, InstructionClass};
use crate::inst_set::{InstCodeConfig, InstSet, INST_CODE_INSTNUM};
use crate::inst_set_registry::{convert_legacy_inst_set_file, InstSetRegistry};
use crate::instruction::Instruction;
use crate::sequence::InstructionSequence;
use avida_apto::AvidaRng;
use avida_config::InitFile;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

fn avida_core() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core")
}

/// Split the argument list of a C++ call at top-level commas.
fn split_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut depth = 0;
    let mut in_str = false;
    for c in s.chars() {
        if in_str {
            cur.push(c);
            if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                cur.push(c);
            }
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => args.push(std::mem::take(&mut cur).trim().to_string()),
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        args.push(cur.trim().to_string());
    }
    args
}

/// The argument text of `prefix(...)` on `line`.
fn call_args(line: &str, prefix: &str) -> Option<Vec<String>> {
    let start = line.find(prefix)? + prefix.len();
    let rest = &line[start..];
    // find the matching ')'
    let mut depth = 1;
    let mut in_str = false;
    for (i, c) in rest.char_indices() {
        if in_str {
            if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(split_args(&rest[..i]));
                }
            }
            _ => {}
        }
    }
    None
}

fn unquote(s: &str) -> String {
    s.trim().trim_matches('"').to_string()
}

fn parse_class(s: &str) -> InstructionClass {
    match s.trim() {
        "INST_CLASS_NOP" => InstructionClass::Nop,
        "INST_CLASS_FLOW_CONTROL" => InstructionClass::FlowControl,
        "INST_CLASS_CONDITIONAL" => InstructionClass::Conditional,
        "INST_CLASS_ARITHMETIC_LOGIC" => InstructionClass::ArithmeticLogic,
        "INST_CLASS_DATA" => InstructionClass::Data,
        "INST_CLASS_ENVIRONMENT" => InstructionClass::Environment,
        "INST_CLASS_LIFECYCLE" => InstructionClass::Lifecycle,
        "INST_CLASS_OTHER" => InstructionClass::Other,
        other => panic!("unknown instruction class {}", other),
    }
}

fn parse_flags(s: &str) -> u32 {
    let mut flags = 0;
    for part in s.replace(['(', ')'], "").split('|') {
        flags |= match part.trim() {
            "0" => 0,
            "nInstFlag::DEFAULT" => inst_flag::DEFAULT,
            "nInstFlag::NOP" => inst_flag::NOP,
            "nInstFlag::LABEL" => inst_flag::LABEL,
            "nInstFlag::STALL" => inst_flag::STALL,
            "nInstFlag::SLEEP" => inst_flag::SLEEP,
            "nInstFlag::PROMOTER" => inst_flag::PROMOTER,
            "nInstFlag::TERMINATOR" => inst_flag::TERMINATOR,
            "nInstFlag::IMMEDIATE_VALUE" => inst_flag::IMMEDIATE_VALUE,
            other => panic!("unknown flag {}", other),
        };
    }
    flags
}

fn parse_bclass(s: &str) -> BehavClass {
    match s.trim() {
        "BEHAV_CLASS_INPUT" => BehavClass::Input,
        "BEHAV_CLASS_ACTION" => BehavClass::Action,
        "BEHAV_CLASS_COPY" => BehavClass::Copy,
        "BEHAV_CLASS_NONE" => BehavClass::None,
        "BEHAV_CLASS_BREAK" => BehavClass::Break,
        "BEHAV_CLASS_END_GENE" => BehavClass::EndGene,
        other => panic!("unknown behaviour class {}", other),
    }
}

/// `cHardwareCPU::initInstLib()` rebuilt from the C++ source.
fn cpu_inst_lib() -> Arc<InstLib> {
    static LIB: OnceLock<Arc<InstLib>> = OnceLock::new();
    LIB.get_or_init(|| {
        let src = std::fs::read_to_string(avida_core().join("source/cpu/cHardwareCPU.cc"))
            .expect("cHardwareCPU.cc");
        let body_start = src
            .find("cHardwareCPU::initInstLib(void)")
            .expect("initInstLib");
        let body = &src[body_start..];

        let n_start = body.find("s_n_array[] = {").unwrap();
        let n_end = n_start + body[n_start..].find("};").unwrap();
        let mut nops = Vec::new();
        for line in body[n_start..n_end].lines() {
            if let Some(args) = call_args(line, "cNOPEntryCPU(") {
                let reg = match args[1].as_str() {
                    "REG_AX" => 0,
                    "REG_BX" => 1,
                    "REG_CX" => 2,
                    other => panic!("register {}", other),
                };
                nops.push((unquote(&args[0]), reg));
            }
        }

        let f_start = body.find("s_f_array[] = {").unwrap();
        let f_end = f_start + body[f_start..].find("\n  };").unwrap();
        let mut entries = Vec::new();
        for line in body[f_start..f_end].lines() {
            let t = line.trim_start();
            if !t.starts_with("tInstLibEntry<tMethod>(\"") {
                continue;
            }
            let args = call_args(t, "tInstLibEntry<tMethod>(").expect("entry args");
            let name = unquote(&args[0]);
            let class = args
                .get(2)
                .map(|a| parse_class(a))
                .unwrap_or(InstructionClass::Other);
            let flags = args.get(3).map(|a| parse_flags(a)).unwrap_or(0);
            let desc = args.get(4).map(|a| unquote(a)).unwrap_or_default();
            let bclass = args
                .get(5)
                .map(|a| parse_bclass(a))
                .unwrap_or(BehavClass::None);
            entries.push(InstLibEntry::new(&name, class, flags, &desc, bclass));
        }
        assert_eq!(entries.last().unwrap().name(), "NULL");
        let null_inst = entries.len() as i32 - 1;
        Arc::new(InstLib::new(entries, nops, 0, null_inst))
    })
    .clone()
}

fn cpu_only(hw: i32) -> Option<Arc<InstLib>> {
    if hw == 0 {
        Some(cpu_inst_lib())
    } else {
        None
    }
}

fn read_lines(dir: &Path, file: &str) -> Vec<String> {
    let f = InitFile::new(file, &dir.to_string_lossy(), None, None);
    assert!(
        f.was_opened(),
        "{}/{}: {:?}",
        dir.display(),
        file,
        f.feedback()
    );
    (0..f.num_lines()).map(|i| f.get_line(i as i32)).collect()
}

/// Keep only the `INSTSET` blocks (and their `INST` lines) with
/// `hw_type=0`.
fn hw0_blocks(lines: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut keep = false;
    for l in lines {
        if l.starts_with("INSTSET ") {
            keep = l.contains("hw_type=0");
        } else if !l.starts_with("INST ") {
            continue;
        }
        if keep {
            out.push(l.clone());
        }
    }
    out
}

fn load(lines: &[String], cfg: InstCodeConfig) -> (InstSetRegistry, Feedback, bool) {
    let mut reg = InstSetRegistry::new();
    let mut fb = Feedback::new();
    let ok = reg.load_inst_sets(lines, cfg, &mut cpu_only, &mut fb);
    (reg, fb, ok)
}

/// Every opcode's per-instruction data must agree with the library entry
/// it maps to.
fn check_consistency(is: &InstSet) {
    let lib = is.inst_lib().clone();
    for op in 0..is.size() {
        let inst = Instruction::new(op);
        let fun = is.lib_function_index(inst);
        let e = lib.get(fun);
        assert_eq!(is.name(op), e.name());
        assert_eq!(is.flags(inst), e.flags());
        assert_eq!(is.is_label(inst), e.is_label());
        assert_eq!(is.should_stall(inst), e.should_stall());
        assert_eq!(is.should_sleep(inst), e.should_sleep());
        assert_eq!(is.is_promoter(inst), e.is_promoter());
        assert_eq!(is.is_terminator(inst), e.is_terminator());
        assert_eq!(is.is_immediate_value(inst), e.is_immediate_value());
        assert_eq!(
            is.get_inst(e.name())
                .ok()
                .map(|i| is.name_of(i).to_string()),
            Some(e.name().to_string())
        );
        assert!(is.inst_in_set(e.name()));
        if is.is_nop(inst) {
            assert!(e.is_nop());
            assert_eq!(is.nop_mod(inst), lib.nop_mod(fun as u32));
        }
    }
    // The mutation index lists every opcode with nonzero redundancy, in
    // order, weighted by redundancy.
    let mi = is.mutation_index();
    let expected: Vec<(i32, f64)> = (0..is.size())
        .filter(|op| is.redundancy(Instruction::new(*op)) != 0)
        .map(|op| (op, is.redundancy(Instruction::new(op)) as f64))
        .collect();
    assert_eq!(mi.size() as usize, expected.len());
    let mut cum = 0.0;
    for (k, (op, w)) in expected.iter().enumerate() {
        assert_eq!(mi.value(k as i32), *op);
        assert_eq!(mi.weight(k as i32), *w);
        assert_eq!(mi.find_position(cum + w / 2.0), *op);
        cum += w;
    }
    assert_eq!(mi.total_weight(), cum);
}

#[test]
fn scraped_cpu_library() {
    let lib = cpu_inst_lib();
    assert_eq!(lib.num_nops(), 3);
    assert_eq!((lib.nop_name(2), lib.nop_mod(2)), ("nop-C", 2));
    assert_eq!(lib.name(0), "nop-A");
    assert!(lib.get(0).is_nop() && lib.get(0).is_default());
    assert!(!lib.get(lib.get_index("nop-X")).is_nop());
    assert!(lib.get(lib.get_index("IO")).should_stall());
    assert_eq!(
        lib.get(lib.get_index("h-copy")).class(),
        InstructionClass::Lifecycle
    );
    assert_eq!(
        lib.get(lib.get_index("label")).class(),
        InstructionClass::FlowControl
    );
    assert_eq!(lib.inst_null(), lib.size() - 1);
    assert_eq!(lib.get_index("NULL"), lib.inst_null());
    assert!(lib.size() > 500);
    // `error: unknown instruction 'X' (Best match = 'sense-unit')` is what
    // cbuild/work/avida prints for each of these names.
    for name in ["nope", "zzzzzzzz", "h-cpy", "nop-Q", "IOO"] {
        assert_eq!(lib.near_match(name), "sense-unit", "{}", name);
    }
}

#[test]
fn support_instset_heads() {
    let dir = avida_core().join("support/config");
    let (reg, fb, ok) = load(
        &read_lines(&dir, "instset-heads.cfg"),
        InstCodeConfig::default(),
    );
    assert!(ok, "{:?}", fb);
    assert_eq!(fb.num_messages(), 0);
    assert_eq!(reg.num_inst_sets(), 1);
    assert!(reg.is_inst_set("heads_default") && reg.is_inst_set("(default)"));
    assert!(!reg.is_inst_set("heads_sex"));
    let is = reg.get_inst_set("heads_default").unwrap();
    assert!(std::ptr::eq(is, reg.default_inst_set().unwrap()));
    assert_eq!(
        (is.inst_set_name(), is.hardware_type()),
        ("heads_default", 0)
    );
    assert_eq!((is.size(), is.num_nops()), (26, 3));
    assert_eq!((is.stack_size(), is.uops_per_cycle()), (10, 20));
    for op in 0..3 {
        assert!(is.is_nop(Instruction::new(op)));
        assert_eq!(is.nop_mod(Instruction::new(op)), op);
    }
    assert!(!is.is_nop(Instruction::new(3)));
    assert_eq!(is.get_inst("h-copy"), Ok(Instruction::new(21)));
    assert_eq!(is.get_inst("h-search").unwrap().symbol(), "z");
    assert_eq!(
        is.get_inst("h-cpy"),
        Err("Error: Unknown instruction 'h-cpy'.  Exiting...".to_string())
    );
    assert_eq!(is.find_best_match("h-cpy"), "h-copy");
    assert!(is.should_stall(is.get_inst("IO").unwrap()));
    assert!(is.should_stall(is.get_inst("h-divide").unwrap()));
    assert!(!is.should_stall(is.get_inst("h-copy").unwrap()));
    assert!(!is.has_costs() && !is.has_ft_costs() && !is.has_female_costs());
    assert_eq!(is.inst_default(), Instruction::new(0));
    assert_eq!(is.inst_error(), Instruction::new(255));
    assert!(!is.is_immediate_value(is.inst_error()));
    assert_eq!(is.cost(Instruction::new(5)), 0);
    assert_eq!(is.redundancy(Instruction::new(5)), 1);
    assert_eq!(is.instruction_code(Instruction::new(5)), 0);
    check_consistency(is);

    // GetRandomInst: one GetDouble(total) draw, uniform over 26 opcodes.
    let mut a = AvidaRng::new(42);
    let mut b = AvidaRng::new(42);
    let mut seen = [false; 26];
    for _ in 0..2000 {
        let inst = is.random_inst(&mut a);
        let expect = (b.get_double() * 26.0) as i32;
        assert_eq!(inst.op(), expect);
        seen[inst.op() as usize] = true;
    }
    assert!(seen.iter().all(|s| *s));
    assert_eq!(a.get_double(), b.get_double());

    let seq = InstructionSequence::from_string("wzca");
    assert_eq!(
        is.save_instruction_sequence(&seq),
        "h-alloc\nh-search\nnop-C\nnop-A\n"
    );
}

#[test]
fn support_instset_heads_sex() {
    let dir = avida_core().join("support/config");
    let (reg, fb, ok) = load(
        &read_lines(&dir, "instset-heads-sex.cfg"),
        InstCodeConfig::default(),
    );
    assert!(ok, "{:?}", fb);
    let is = reg.default_inst_set().unwrap();
    assert_eq!(is.inst_set_name(), "heads_sex");
    assert_eq!((is.size(), is.num_nops()), (26, 3));
    assert!(is.inst_in_set("divide-sex"));
    check_consistency(is);
}

/// Every config file under avida-core/tests/*/config/ with a `hw_type=0`
/// INSTSET block loads cleanly.
#[test]
fn all_test_instsets_hw0() {
    let tests_dir = avida_core().join("tests");
    let mut n_files = 0;
    let mut n_sets = 0;
    let mut dirs: Vec<_> = std::fs::read_dir(&tests_dir).unwrap().flatten().collect();
    dirs.sort_by_key(|d| d.file_name());
    for d in dirs {
        let cfg_dir = d.path().join("config");
        let Ok(files) = std::fs::read_dir(&cfg_dir) else {
            continue;
        };
        let mut files: Vec<_> = files.flatten().collect();
        files.sort_by_key(|f| f.file_name());
        for f in files {
            let raw = std::fs::read_to_string(f.path()).unwrap_or_default();
            if !raw
                .lines()
                .any(|l| l.trim_start().starts_with("INSTSET") && l.contains("hw_type=0"))
            {
                continue;
            }
            let fname = f.file_name().to_string_lossy().to_string();
            let lines = hw0_blocks(&read_lines(&cfg_dir, &fname));
            let (reg, fb, ok) = load(&lines, InstCodeConfig::default());
            assert!(ok, "{}: {:?}", f.path().display(), fb);
            assert_eq!(fb.num_errors(), 0, "{}: {:?}", f.path().display(), fb);
            assert!(reg.num_inst_sets() >= 1);
            for is in reg.inst_sets() {
                assert!(is.size() > 0);
                check_consistency(is);
            }
            n_files += 1;
            n_sets += reg.num_inst_sets();
        }
    }
    assert!(n_files >= 30, "only {} files", n_files);
    assert!(n_sets >= n_files);
}

/// `INST_SET_LOAD_LEGACY 1` tests: convert their legacy file and load it.
#[test]
fn all_test_legacy_instsets() {
    let tests_dir = avida_core().join("tests");
    let mut n = 0;
    for d in std::fs::read_dir(&tests_dir).unwrap().flatten() {
        let cfg_dir = d.path().join("config");
        if !cfg_dir.join("avida.cfg").exists() {
            continue;
        }
        let cfg = read_lines(&cfg_dir, "avida.cfg");
        // `-set NAME VALUE` arguments from test_list override avida.cfg.
        let test_list = std::fs::read_to_string(d.path().join("test_list")).unwrap_or_default();
        let args: Vec<&str> = test_list
            .lines()
            .find_map(|l| l.trim_start().strip_prefix("args"))
            .and_then(|l| l.trim_start().strip_prefix('='))
            .unwrap_or("")
            .split_whitespace()
            .collect();
        let mut overrides: Vec<(String, String)> = Vec::new();
        for (i, a) in args.iter().enumerate() {
            if *a == "-set" && i + 2 < args.len() {
                overrides.push((args[i + 1].to_string(), args[i + 2].to_string()));
            }
        }
        let setting = |name: &str| {
            if let Some((_, v)) = overrides.iter().rev().find(|(k, _)| k == name) {
                return Some(v.clone());
            }
            cfg.iter()
                .rev()
                .find_map(|l| l.strip_prefix(&format!("{} ", name)).map(|v| v.to_string()))
        };
        if setting("INST_SET_LOAD_LEGACY").as_deref() != Some("1") {
            continue;
        }
        let inst_set = setting("INST_SET").unwrap_or_else(|| "-".into());
        let hw_type: i32 = setting("HARDWARE_TYPE")
            .map(|v| v.parse().unwrap())
            .unwrap_or(0);
        let mut list = Vec::new();
        let mut fb = Feedback::new();
        let conv = convert_legacy_inst_set_file(
            &inst_set,
            hw_type,
            &cfg_dir.to_string_lossy(),
            &mut list,
            &mut fb,
        );
        assert!(conv.success, "{}: {:?}", cfg_dir.display(), fb);
        if inst_set == "-" {
            assert_eq!(conv.inst_set_setting.as_deref(), Some("instset-heads.cfg"));
            assert_eq!(
                fb.messages()[0].1,
                "using default instruction set: instset-heads.cfg"
            );
        }
        assert!(list[0].starts_with("INSTSET "));
        let (reg, fb, ok) = load(&list, InstCodeConfig::default());
        assert!(ok, "{}: {:?}", cfg_dir.display(), fb);
        let is = reg.default_inst_set().unwrap();
        assert!(is.size() > 0, "{}", cfg_dir.display());
        check_consistency(is);
        if cfg_dir.ends_with("optimize_multivar_lim_res/config") {
            // IObuf-add0 1 / IObuf-add1 1 / repro 0
            assert_eq!(is.size(), 3);
            assert_eq!(is.redundancy(Instruction::new(2)), 0);
            assert_eq!(is.mutation_index().size(), 2);
        }
        n += 1;
    }
    assert!(n >= 30, "only {} legacy configs", n);
}

#[test]
fn legacy_conversion_format() {
    let dir = avida_core().join("tests/analyze_dumplandscape/config");
    let mut list = Vec::new();
    let mut fb = Feedback::new();
    let conv = convert_legacy_inst_set_file("-", 0, &dir.to_string_lossy(), &mut list, &mut fb);
    assert!(conv.success);
    assert_eq!(list[0], "INSTSET instset-heads.cfg:hw_type=0");
    assert_eq!(
        list[1],
        "INST nop-A:redundancy=1:cost=0:initial_cost=0:energy_cost=0:prob_fail=0.000000:addl_time_cost=0:res_cost=0.000000:fem_res_cost=0.000000:post_cost=0"
    );

    let mut fb = Feedback::new();
    let conv =
        convert_legacy_inst_set_file("missing.cfg", 0, &dir.to_string_lossy(), &mut list, &mut fb);
    assert!(!conv.success);
    assert_eq!(
        fb.error_messages(),
        vec!["unable to open file 'missing.cfg'."]
    );
    assert_eq!(fb.num_errors(), 0); // QUIRK: Append does not count

    let mut fb = Feedback::new();
    let conv = convert_legacy_inst_set_file("-", 4, &dir.to_string_lossy(), &mut list, &mut fb);
    assert!(!conv.success);
    assert_eq!(
        fb.error_messages(),
        vec!["unknown/unsupported HARDWARE_TYPE specified"]
    );
}

#[test]
fn redundancy_and_costs() {
    // avida_GA_sex: repro-sex and repro have redundancy 0.
    let dir = avida_core().join("tests/avida_GA_sex/config");
    let (reg, fb, ok) = load(
        &hw0_blocks(&read_lines(&dir, "avida.cfg")),
        InstCodeConfig::default(),
    );
    assert!(ok, "{:?}", fb);
    let is = reg.get_inst_set("instset-heads.cfg").unwrap();
    assert_eq!(is.size(), 4);
    assert_eq!(is.num_nops(), 0);
    assert_eq!(is.redundancy(is.get_inst("repro").unwrap()), 0);
    let mi = is.mutation_index();
    assert_eq!((mi.size(), mi.total_weight()), (2, 2.0));
    assert_eq!((mi.value(0), mi.value(1)), (0, 1));
    let mut rng = AvidaRng::new(1);
    for _ in 0..200 {
        assert!(is.random_inst(&mut rng).op() < 2);
    }

    // mating_types: female costs.
    let dir = avida_core().join("tests/mating_types/config");
    let (reg, fb, ok) = load(
        &hw0_blocks(&read_lines(&dir, "instset-heads-mating-types.cfg")),
        InstCodeConfig::default(),
    );
    assert!(ok, "{:?}", fb);
    let is = reg.default_inst_set().unwrap();
    let div = is.get_inst("div-sex-mating-type").unwrap();
    assert_eq!((is.female_cost(div), is.choosy_female_cost(div)), (5, 5));
    assert!(is.has_female_costs() && is.has_choosy_female_costs());
    assert!(!is.has_costs());
}

#[test]
fn inst_set_load_rules() {
    let lines = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();

    // INST_CODE handling, cost flags, negative redundancy, NULL, bad nops.
    let (reg, fb, ok) = load(
        &lines(&[
            "INSTSET a:hw_type=0:stack_size=7",
            "INST nop-A:inst_code=101",
            "INST nop-B:cost=2:initial_cost=1:energy_cost=3:res_cost=0.5:post_cost=2:bonus_cost=1",
            "INST inc:redundancy=-1",
            "INST dec:redundancy=2.9:prob_fail=0.25",
            "INST nop-C",
            "INST NULL",
            "INST nope",
            "INST push:inst_code=12",
            "INST pop:frob=1",
        ]),
        InstCodeConfig::default(),
    );
    assert!(!ok);
    assert_eq!(
        fb.messages()
            .iter()
            .map(|(_, m)| m.as_str())
            .collect::<Vec<_>>(),
        vec![
            "instruction 'inc' has negative redundancy, ignoring...",
            "invalid NOP placement, all NOPs must be listed first",
            "invalid use of NULL instruction",
            // Verified against cbuild/work/avida (Apto::NearMatch with the
            // buggy Apto::EditDistance and hash-map key order).
            "unknown instruction 'nope' (Best match = 'sense-unit')",
            "invalid character in instruction code, must be 0 or 1",
            "unrecognized argument: 'frob'",
        ]
    );
    // loadInstSet fails -> nothing registered
    assert_eq!(reg.num_inst_sets(), 0);

    // Same set without the fatal lines.
    let (_reg, fb, ok) = load(
        &lines(&[
            "INSTSET a:hw_type=0:stack_size=7",
            "INST nop-A:inst_code=101",
            "INST nop-B:cost=2:initial_cost=1:energy_cost=3:res_cost=0.5:post_cost=2:bonus_cost=1",
            "INST dec:redundancy=2.9:prob_fail=0.25",
            "INST push:inst_code=1x1",
        ]),
        InstCodeConfig::default(),
    );
    assert!(!ok);
    assert_eq!(
        fb.error_messages(),
        vec!["invalid character in instruction code, must be 0 or 1"]
    );

    let (mut reg, fb, ok) = load(
        &lines(&[
            "INSTSET a:hw_type=0:stack_size=7",
            "INST nop-A:inst_code=101",
            "INST nop-B:cost=2:initial_cost=1:energy_cost=3:res_cost=0.5:post_cost=2:bonus_cost=1",
            "INST dec:redundancy=2.9:prob_fail=0.25",
            "INST push",
        ]),
        InstCodeConfig {
            inst_code_length: 2,
            inst_code_default_type: INST_CODE_INSTNUM,
        },
    );
    assert!(ok, "{:?}", fb);
    let is = reg.get_inst_set("a").unwrap();
    assert_eq!(is.stack_size(), 7);
    assert_eq!(is.instruction_code(Instruction::new(0)), 0b10); // first 2 chars
    assert_eq!(is.instruction_code(Instruction::new(3)), 3); // INSTNUM quirk: plain id
    let b = Instruction::new(1);
    assert_eq!(
        (
            is.cost(b),
            is.ft_cost(b),
            is.energy_cost(b),
            is.post_cost(b)
        ),
        (2, 1, 3, 2)
    );
    assert_eq!((is.res_cost(b), is.bonus_cost(b)), (0.5, 1.0));
    assert!(is.has_costs() && is.has_ft_costs() && is.has_energy_costs() && is.has_res_costs());
    assert!(is.has_post_costs() && is.has_bonus_costs() && !is.has_fem_res_costs());
    let d = Instruction::new(2);
    assert_eq!((is.redundancy(d), is.prob_fail(d)), (2, 0.25));
    assert_eq!(is.num_nops(), 2);
    assert_eq!(is.mutation_index().total_weight(), 5.0);

    let is = reg.get_inst_set_mut("a").unwrap();
    let null = is.activate_null_inst();
    assert_eq!((null.op(), is.size(), is.name(4)), (4, 5, "NULL"));
    assert_eq!(is.activate_null_inst(), null);
    is.set_prob_fail(d, 0.5);
    is.set_redundancy(d, 4);
    assert_eq!(is.prob_fail(d), 0.5);
    assert_eq!(is.mutation_index().size(), 5); // QUIRK: appended

    // INSTSET-level errors.
    let (reg, fb, ok) = load(
        &lines(&[
            "INST nop-A",
            "INSTSET :hw_type=0",
            "INSTSET b",
            "INST nop-A",
            "INSTSET c:hw_type=7",
            "INST nop-A",
            "INSTSET d:hw_type=0:uops_per_cycle=3",
            "INST nop-A",
            "some other line",
            "INSTSET d:hw_type=0",
            "INST nop-B",
        ]),
        InstCodeConfig::default(),
    );
    assert!(!ok);
    assert_eq!(
        fb.error_messages(),
        vec![
            "disassociated INST definition, no INSTSET defined",
            "instruction set name not found",
            "required argument 'hw_type' was not found",
            "disassociated INST definition, no INSTSET defined",
            "unknown/unsupported hw_type specified for instset 'c'",
        ]
    );
    // The duplicate name maps to the later set.
    assert_eq!(reg.num_inst_sets(), 2);
    assert_eq!(reg.inst_set_id("d"), 1);
    assert_eq!(reg.inst_set(0).uops_per_cycle(), 3);
    assert_eq!(reg.get_inst_set("d").unwrap().name(0), "nop-B");
    assert_eq!(reg.get_inst_set("(default)").unwrap().name(0), "nop-A");
    assert!(reg.get_inst_set("zz").is_none());
}

#[test]
fn org_files_and_spop_parity() {
    // heads_default_100u: Inject default-classic.org with the test's inst
    // set; the injected ancestor (genotype 1) is in detail-100.spop.
    let dir = avida_core().join("tests/heads_default_100u/config");
    let wdir = dir.to_string_lossy().to_string();
    let (reg, fb, ok) = load(
        &hw0_blocks(&read_lines(&dir, "avida.cfg")),
        InstCodeConfig::default(),
    );
    assert!(ok, "{:?}", fb);

    let mut fb = Feedback::new();
    let g = load_genome_detail_file("default-classic.org", &wdir, &reg, &mut fb, "")
        .expect("no unknown instructions")
        .expect("loaded");
    assert_eq!(fb.num_messages(), 0);
    assert_eq!((g.hardware_type(), g.inst_set()), (0, "heads_default"));
    let spop = std::fs::read_to_string(
        avida_core().join("tests/heads_default_100u/expected/data/detail-100.spop"),
    )
    .unwrap();
    let ancestor = spop.lines().find(|l| l.starts_with("1 div:ext ")).unwrap();
    let cols: Vec<&str> = ancestor.split_whitespace().collect();
    assert_eq!(g.representation().as_string(), cols[16]);
    assert_eq!(g.as_string(), format!("0,heads_default,{}", cols[16]));

    // Errors.
    let mut fb = Feedback::new();
    assert_eq!(
        load_genome_detail_file("nope.org", &wdir, &reg, &mut fb, ""),
        Ok(None)
    );
    assert_eq!(fb.error_messages(), vec!["unable to open file 'nope.org'."]);

    let mut fb = Feedback::new();
    assert_eq!(
        load_genome_detail_file("default-classic.org", &wdir, &reg, &mut fb, "other"),
        Ok(None)
    );
    assert_eq!(
        fb.error_messages(),
        vec!["invalid instruction set 'other' defined in organism 'default-classic.org'"]
    );

    // support/config default-heads.org: #inst_set heads_default, #hw_type 0
    let sdir = avida_core().join("support/config");
    let (reg2, _, ok) = load(
        &read_lines(&sdir, "instset-heads.cfg"),
        InstCodeConfig::default(),
    );
    assert!(ok);
    let mut fb = Feedback::new();
    let g = load_genome_detail_file(
        "default-heads.org",
        &sdir.to_string_lossy(),
        &reg2,
        &mut fb,
        "",
    )
    .unwrap()
    .unwrap();
    assert_eq!(g.representation().size(), 100);
    assert!(g.representation().as_string().starts_with("wzcag"));
    let org_lines = read_lines(&sdir, "default-heads.org");
    let saved = reg2
        .default_inst_set()
        .unwrap()
        .save_instruction_sequence(g.representation());
    assert_eq!(
        saved,
        org_lines
            .iter()
            .map(|l| format!("{}\n", l))
            .collect::<String>()
    );

    // "(default)" is accepted by name.
    let mut fb = Feedback::new();
    let g2 = load_genome_detail_file(
        "default-heads.org",
        &sdir.to_string_lossy(),
        &reg2,
        &mut fb,
        " (default) ",
    )
    .unwrap()
    .unwrap();
    assert_eq!(g2, g);

    // A sex inst set with the right names but the org says hw_type 0: fine;
    // heads_sex is not loaded here, so it is invalid.
    let mut fb = Feedback::new();
    assert_eq!(
        load_genome_detail_file(
            "default-heads-sex.org",
            &sdir.to_string_lossy(),
            &reg2,
            &mut fb,
            ""
        ),
        Ok(None)
    );
    assert_eq!(
        fb.error_messages(),
        vec!["invalid instruction set 'heads_sex' defined in organism 'default-heads-sex.org'"]
    );

    // Hardware type mismatch: org file with #hw_type 2.
    let tmp = std::env::temp_dir().join(format!("avida-genome-test-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("bad.org"), "#hw_type 2\nnop-A\nh-cpy\n").unwrap();
    let mut fb = Feedback::new();
    assert_eq!(
        load_genome_detail_file("bad.org", &tmp.to_string_lossy(), &reg2, &mut fb, ""),
        Ok(None)
    );
    assert_eq!(
        fb.error_messages(),
        vec!["hardware type mismatch in organism 'bad.org': is = 0, org = 2"]
    );
    std::fs::write(tmp.join("bad2.org"), "nop-A # c\n  h-cpy  \n").unwrap();
    let mut fb = Feedback::new();
    assert_eq!(
        load_genome_detail_file("bad2.org", &tmp.to_string_lossy(), &reg2, &mut fb, ""),
        Err("Error: Unknown instruction 'h-cpy'.  Exiting...".to_string())
    );
    let _ = std::fs::remove_dir_all(&tmp);

    // Create(): genome -> inst set validation.
    assert_eq!(reg2.inst_set_for_genome(&g).map(|(id, _)| id), Some(0));
    let mut other = g.clone();
    other.set_property("instset", "nope");
    assert!(reg2.inst_set_for_genome(&other).is_none());
    let wrong_hw =
        crate::genome::Genome::with_inst_set(2, "heads_default", g.representation().clone());
    assert!(reg2.inst_set_for_genome(&wrong_hw).is_none());
}
