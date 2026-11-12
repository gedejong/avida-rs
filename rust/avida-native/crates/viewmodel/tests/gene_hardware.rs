//! BCR and GP8 organisms in the view model: every thread's heads (with the
//! memory space and position of each), registers, stacks and state, and
//! the gene memories next to the genome and the offspring spaces.

mod common;

use avida_viewmodel::{capture, ip_sites, OrganismView, Subscription};

use common::{append_events, build_world, gene_config, run_with_stops};

const CELL: i32 = 12;

/// Runs the two-gene organism for `updates` updates and captures it.
fn organism(hw_type: i32, name: &str, updates: i32) -> (avida_world::world::World, OrganismView) {
    let dir = gene_config(hw_type, name);
    append_events(&dir, &format!("u {updates} Exit\n"));
    let mut w = build_world(&dir);
    let mut sink = Vec::new();
    run_with_stops(&mut w, updates, &mut sink, |_| {});
    let sub = Subscription {
        inspect_cell: Some(CELL),
        ..Subscription::default()
    };
    let org = capture(&w, &sub).inspect.expect("the organism lives");
    (w, org)
}

fn check(hw_type: i32, name: &str, regs: usize, heads: usize) {
    let (w, org) = organism(hw_type, name, 4);
    let cpu = &org.cpu;
    assert_eq!(cpu.hw_type, hw_type);
    assert_eq!(cpu.threads.len(), 2, "one thread per gene");
    assert_eq!(cpu.head_names.len(), heads);
    assert_eq!(
        &cpu.head_names[..5],
        ["IP", "Read", "Write", "Flow", "Flow2"]
    );
    assert_eq!(cpu.head_names[heads - 1], format!("Flow{}", heads - 3));
    for (t, th) in cpu.threads.iter().enumerate() {
        assert_eq!(th.heads.len(), heads, "thread {t}");
        assert_eq!(th.regs.len(), regs, "thread {t} registers of its own");
        assert_eq!(th.stacks.len(), 2, "a local and a global stack");
        assert!(!th.status.is_empty(), "thread {t} says what it does");
        // Every head names a space the view carries.
        for h in &th.heads {
            assert!(cpu.space(h.mem_space).is_some(), "head in {}", h.mem_space);
        }
    }
    // The threads run in the genes: ids from 16 up, labelled by promoter.
    let ip0 = cpu.threads[0].heads[0];
    let ip1 = cpu.threads[1].heads[0];
    assert_eq!((ip0.mem_space, ip1.mem_space), (16, 17));
    assert_eq!(cpu.space(0).unwrap().label, "genome");
    assert_eq!(cpu.space(16).unwrap().label, "gene 0, label AB");
    assert_eq!(cpu.space(17).unwrap().label, "gene 1, label BB");
    assert_eq!(cpu.space(0).unwrap().memory, cpu.memory);
    assert_eq!(cpu.space(0).unwrap().memory.len(), 23);
    assert!(cpu.space(16).unwrap().memory.len() >= 8);
    // `set-memory` made another space, named by its register letter.
    let extra = cpu.spaces.iter().find(|s| s.label == "memory C");
    assert!(
        extra.is_some(),
        "spaces: {:?}",
        cpu.spaces
            .iter()
            .map(|s| (&s.label, s.id))
            .collect::<Vec<_>>()
    );
    // Executed sites are flagged in the gene's own memory.
    assert!(cpu
        .space(16)
        .unwrap()
        .memory
        .iter()
        .any(|c| c.flags & avida_viewmodel::mem_flags::EXECUTED != 0));
    // The session's breakpoint check sees both IPs, in their gene spaces.
    let sites = ip_sites(&w, CELL);
    assert_eq!(sites, vec![(16, ip0.pos), (17, ip1.pos)]);
    // The IP is a site of its space.
    assert!(usize::try_from(ip0.pos).unwrap() < cpu.space(16).unwrap().memory.len());
}

#[test]
fn a_bcr_organism_exposes_threads_heads_registers_and_gene_memories() {
    check(5, "bcr_view", 12, 12);
}

#[test]
fn a_gp8_organism_exposes_threads_heads_registers_and_gene_memories() {
    check(4, "gp8_view", 8, 8);
}

#[test]
fn a_waiting_thread_says_what_it_waits_for() {
    let (_, org) = organism(5, "bcr_wait", 2);
    let status = &org.cpu.threads[1].status;
    assert!(
        status.contains("waiting for LX > 2 into BX"),
        "status: {status}"
    );
    assert!(status.contains("label BB"), "status: {status}");
}
