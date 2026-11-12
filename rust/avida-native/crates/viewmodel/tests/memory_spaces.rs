//! Memory spaces of a TransSMT organism hosting a parasite: the snapshot
//! carries every space, each thread's heads say which space they are in, and
//! the parasite's code is listed under its own space.

mod common;

use avida_viewmodel::{capture, ip_sites, Subscription};

use common::{append_events, build_world, fresh_config, run_with_stops};

const TEST: &str = "parasites_limited_resources";

#[test]
fn a_hosted_parasite_has_its_own_listed_memory_space() {
    let dir = fresh_config(TEST, "memory_spaces");
    let cfg = std::fs::read_to_string(dir.join("avida.cfg")).unwrap();
    let cfg: String = cfg
        .lines()
        .map(|l| {
            if l.starts_with("RANDOM_SEED") {
                "RANDOM_SEED 7\n".to_string()
            } else {
                format!("{l}\n")
            }
        })
        .collect();
    std::fs::write(dir.join("avida.cfg"), cfg).unwrap();
    append_events(&dir, "u 640 Exit\n");
    let mut w = build_world(&dir);
    let mut sink = Vec::new();
    let mut found = None;
    run_with_stops(&mut w, 640, &mut sink, |w| {
        if w.stats.update() < 640 {
            return;
        }
        for cell in 0..w.pop.cells.len() as i32 {
            let sub = Subscription {
                inspect_cell: Some(cell),
                ..Subscription::default()
            };
            let Some(org) = capture(w, &sub).inspect else {
                continue;
            };
            if org.cpu.threads.iter().any(|t| t.heads[0].mem_space != 0) {
                found = Some((cell, org));
                break;
            }
        }
    });
    let (cell, org) = found.expect("an organism with a parasite thread at update 640");
    let cpu = &org.cpu;
    assert_eq!(cpu.hw_type, 2);
    assert_eq!(cpu.spaces[0].id, 0);
    assert_eq!(cpu.spaces[0].label, "genome");
    assert_eq!(cpu.spaces[0].memory, cpu.memory, "space 0 is the genome");
    let parasite = cpu
        .threads
        .iter()
        .map(|t| t.heads[0])
        .find(|h| h.mem_space != 0)
        .unwrap();
    let space = cpu
        .space(parasite.mem_space)
        .expect("the IP's space is carried");
    assert_eq!(space.label, "parasite");
    assert!(
        usize::try_from(parasite.pos).unwrap() < space.memory.len(),
        "the IP is a site of its space"
    );
    assert!(
        space.memory.len() >= 8,
        "a parasite's code, not an empty space"
    );
    // Every head of every thread names a space the snapshot carries.
    for t in &cpu.threads {
        for h in &t.heads {
            assert!(
                cpu.space(h.mem_space).is_some(),
                "head in space {}",
                h.mem_space
            );
        }
    }
    // A label is only given for what the hardware knows: `offspring` needs
    // a thread of the host whose write head is in the space, `parasite` a
    // thread that is owned by one; any other space is called `space N`.
    for sp in &cpu.spaces {
        let host_writes = cpu
            .threads
            .iter()
            .filter(|t| t.heads[0].mem_space == 0)
            .any(|t| t.heads[2].mem_space == sp.id);
        match sp.label.as_str() {
            "genome" => assert_eq!(sp.id, 0),
            "parasite" => assert!(cpu.threads.iter().any(|t| t.heads[0].mem_space == sp.id)),
            "offspring" => {
                assert!(host_writes || cpu.threads.iter().any(|t| t.heads[2].mem_space == sp.id))
            }
            other => assert_eq!(other, format!("space {}", sp.id)),
        }
    }
    // The session's breakpoint check sees the parasite's IP in its space.
    assert!(ip_sites(&w, cell).contains(&(parasite.mem_space, parasite.pos)));
    eprintln!(
        "cell {cell}: spaces {:?}",
        cpu.spaces
            .iter()
            .map(|s| (s.id, s.label.clone(), s.memory.len()))
            .collect::<Vec<_>>()
    );
}
