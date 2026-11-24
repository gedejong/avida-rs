//! `sense-ahead-resource0` (a port addition, PORTING.md): the amount of
//! resource 0 in the faced cell. `sense-faced-resource0` reads the
//! organism's own cell, as the C++ does.
//!
//! A hand-written replicator steps when the cell ahead holds more food than
//! its own, then turns, sixteen times, and then eats. On a food hill it is
//! paid more than the same organism with `sense-faced-resource0`, which
//! compares its own cell with itself and never steps.

use std::fs;
use std::path::Path;
use std::process::Command;

fn section<'a>(text: &'a str, from: &str, to: &str) -> (&'a str, &'a str) {
    let (head, rest) = text.split_once(from).unwrap();
    let (_, tail) = rest.split_once(to).unwrap();
    (head, tail)
}

/// Average merit at the last update of a run with `sense` as the look.
fn average_merit(sense: &str) -> f64 {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("sense_ahead")
        .join(sense);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let stock = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core/support/config");
    let cfg = fs::read_to_string(stock.join("avida.yaml")).unwrap();
    let set = fs::read_to_string(stock.join("instset-heads.yaml")).unwrap();
    let mut insts: Vec<&str> = set
        .lines()
        .filter_map(|l| l.strip_prefix("      - "))
        .map(|l| l.split_whitespace().next().unwrap())
        .collect();
    insts.extend(["move", "rotate-right-one", "sense-resource0", sense]);
    let insts: String = insts.iter().map(|i| format!("      - {i}\n")).collect();

    let (head, _) = section(&cfg, "\ninstruction_sets:\n", "\nevents:\n");
    let head = head
        .replace("  x: 60\n", "  x: 20\n")
        .replace("  y: 60\n", "  y: 20\n")
        .replace("  population_cap: 0\n", "  population_cap: 100\n")
        .replace("  copy_prob: 0.0075\n", "  copy_prob: 0.0\n")
        .replace("  divide_ins_prob: 0.05\n", "  divide_ins_prob: 0.0\n")
        .replace("  divide_del_prob: 0.05\n", "  divide_del_prob: 0.0\n")
        .replace(
            "birth_method: neighborhood_random",
            "birth_method: population_random",
        );
    for want in [
        "x: 20",
        "population_cap: 100",
        "copy_prob: 0.0\n",
        "population_random",
    ] {
        assert!(head.contains(want), "the stock file has no line for {want}");
    }
    let rest = format!(
        "
instruction_sets:
  - name: heads_climb
    hardware_type: 0
    instructions:
{insts}
environment:
  resources:
    - {{name: food, geometry: grid, initial: 0, inflow: 40, outflow: 0.0005,
       inflowx1: 9, inflowx2: 10, inflowy1: 9, inflowy2: 10,
       outflowx1: 0, outflowx2: 19, outflowy1: 0, outflowy2: 19,
       xdiffuse: 1.0, ydiffuse: 1.0, xgravity: 0, ygravity: 0}}
  reactions:
    - name: EAT
      task: echo
      process: {{resource: food, value: 1.0, type: pow, frac: 0.01, max: 5.0, depletable: 0}}
      requisite: {{max_count: 1}}

events:
  - {{at: begin, action: Inject, args: climber.org}}
  - {{at: 0, every: 100, until: end, action: PrintAverageData}}
  - {{at: 1500, action: Exit}}
"
    );
    fs::write(dir.join("avida.yaml"), format!("{head}{rest}")).unwrap();

    let mut org = vec!["h-alloc", "h-search", "nop-C", "nop-A", "mov-head", "nop-C"];
    for _ in 0..16 {
        org.extend([sense, "swap", "sense-resource0", "if-less", "move", "nop-A"]);
        org.push("rotate-right-one");
    }
    org.extend([
        "IO", "IO", "h-search", "h-copy", "if-label", "nop-C", "nop-A",
    ]);
    org.extend(["h-divide", "mov-head", "nop-A", "nop-B"]);
    fs::write(dir.join("climber.org"), org.join("\n") + "\n").unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_avida"))
        .current_dir(&dir)
        .args(["-s", "1", "-set", "general.verbosity", "0"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let data = fs::read_to_string(dir.join("data/average.dat")).unwrap();
    let last = data.lines().rfind(|l| l.starts_with("1500 ")).unwrap();
    last.split(' ').nth(1).unwrap().parse().unwrap()
}

#[test]
fn looking_ahead_pays_on_a_hill_and_the_faced_variant_does_not_look() {
    let ahead = average_merit("sense-ahead-resource0");
    let own_cell = average_merit("sense-faced-resource0");
    assert!(
        ahead > 1.2 * own_cell,
        "average merit {ahead} with sense-ahead-resource0, {own_cell} with sense-faced-resource0"
    );
}
