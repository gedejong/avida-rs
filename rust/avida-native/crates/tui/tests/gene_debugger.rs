//! The debugger on a live BCR and a live GP8 organism (two genes, one
//! thread each): the heads with their C++ names, the twelve (eight)
//! registers, the gene memories as spaces, the thread state, and the IP
//! marker moving on the right line while single-stepping.

mod common;

use avida_tui::app::App;
use common::*;

fn to_debugger() -> Vec<Step> {
    vec![
        Step::Until(
            "first frame",
            Box::new(|a| a.frame.as_ref().is_some_and(|f| f.map.is_some())),
        ),
        Step::Keys("."),
        Step::Until("one update", Box::new(|a| !a.history.is_empty())),
        Step::Keys("CU"),
        Step::Until(
            "watched organism",
            Box::new(|a| a.debugger.current.is_some() && a.debugger.ip().is_some()),
        ),
    ]
}

/// The position on the listing line carrying the IP marker `>`.
fn marked_line(screen: &str) -> Option<(i32, String)> {
    screen.lines().find_map(|l| {
        if l.contains("breakpoint") || l.contains("line ") {
            return None;
        }
        let i = l.find('>')?;
        let mut words = l[i + 1..].split_whitespace();
        let pos = words.next()?.parse().ok()?;
        let _symbol = words.next()?;
        Some((pos, words.next()?.to_string()))
    })
}

fn cycles(a: &App) -> u64 {
    a.debugger
        .current
        .as_ref()
        .map_or(0, |o| o.phenotype.lifetime_cpu_cycles)
}

fn debug_gene_organism(hw_type: i32, name: &str, regs: usize, waits: &'static str) {
    let dir = gene_config(hw_type, name);
    let mut steps = to_debugger();
    steps.push(Step::Check(Box::new(move |a, t| {
        let o = a.debugger.current.as_ref().unwrap();
        assert_eq!(o.cpu.hw_type, hw_type);
        assert_eq!(o.cpu.threads.len(), 2);
        assert_eq!(o.cpu.threads[0].regs.len(), regs);
        has(t, "thread 1/2");
        has(t, "IP ");
        has(t, "Fl2");
        has(t, "state label AB");
        let bad: Vec<_> = a
            .log
            .iter()
            .filter(|l| l.kind.is_problem())
            .map(|l| l.text.clone())
            .collect();
        assert!(bad.is_empty(), "log problems: {bad:?}");
        has(t, "Memory space 16 (gene 0, label AB)");
        assert!(marked_line(t).is_some(), "the IP is marked:\n{t}");
        // A step is one CPU cycle of several micro-operations here, and the
        // screen and the palette say so (the key help: `help::tests`).
        assert!(a.steps_run_micro_ops());
        has(t, "A step runs one CPU cycle");
        assert!(!t.contains("A step runs one instruction"), "{t}");
        assert!(a
            .palette_entries()
            .iter()
            .any(|it| it.title == avida_tui::actions::DEBUG_STEP_MICRO_OPS));
    })));
    steps.push(Step::Snap(format!("gene_{hw_type}_start")));
    // Single-step: the marker stays on the line of the IP, with the
    // instruction the memory holds there.
    for _ in 0..6 {
        steps.push(Step::Check(Box::new(|a, _| {
            // `Until` below compares against this count.
            let _ = cycles(a);
        })));
        steps.push(Step::Keys("i"));
        steps.push(Step::Until(
            "one instruction",
            Box::new(|a| a.status.as_ref().is_some_and(|s| s.run_until.is_none())),
        ));
        steps.push(Step::Check(Box::new(|a, t| {
            let ip = a.debugger.ip().unwrap();
            let space = a.debugger.ip_space().unwrap();
            assert_eq!(space, 16);
            let o = a.debugger.current.as_ref().unwrap();
            let want = o
                .cpu
                .cell_name(o.cpu.space(space).unwrap().memory.get(ip as usize))
                .to_string();
            let (pos, name) = marked_line(t).unwrap_or_else(|| panic!("no IP marker:\n{t}"));
            assert_eq!(pos, ip, "marker on the IP's line:\n{t}");
            assert!(
                want.starts_with(name.trim_end_matches('…')),
                "{name} vs {want}"
            );
        })));
    }
    // The other thread: `t` shows its heads, and its gene is listed.
    steps.push(Step::Keys("t"));
    steps.push(Step::Check(Box::new(|a, t| {
        assert_eq!(a.debugger.thread_index(), 1);
        has(t, "thread 2/2");
        has(t, waits);
        has(t, "Memory space 17 (gene 1, label BB)");
        assert!(marked_line(t).is_some(), "the IP is marked:\n{t}");
    })));
    steps.push(Step::Snap(format!("gene_{hw_type}_thread2")));
    // Cycling the space leaves follow mode and lists the genome.
    run_script(&dir, &["-s", "7"], 100, 40, steps, "100x40");
}

#[test]
fn a_live_bcr_organism_is_debugged_gene_by_gene() {
    debug_gene_organism(5, "dbg_bcr", 12, "waiting for LX > 2 into BX");
}

#[test]
fn a_live_gp8_organism_is_debugged_gene_by_gene() {
    debug_gene_organism(4, "dbg_gp8", 8, "waiting for HX > 2 into BX");
}
