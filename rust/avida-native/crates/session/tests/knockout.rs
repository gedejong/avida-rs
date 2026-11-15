//! `Command::KnockoutMap`: the knockout map of the organism in a cell,
//! computed on the simulation thread; it changes nothing the run writes, it
//! is refused for an organism that is gone, and a playback answers with the
//! reason it cannot.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use avida_session::{
    Command, KnockoutReply, KnockoutRequest, RecordSpec, Session, SessionEvent, SiteClass,
    Subscription, KNOCKOUT_PLAYBACK_REASON,
};
use common::{copy_dir, files_under, normalized, Harness};

const ARGS: [&str; 11] = [
    "-s",
    "7",
    "-def",
    "INST_SET",
    "instset-heads-modular.cfg",
    "-set",
    "WORLD_X",
    "5",
    "-set",
    "WORLD_Y",
    "5",
];

fn args() -> Vec<&'static str> {
    let mut a = ARGS.to_vec();
    a.extend(["-set", "COPY_MUT_PROB", "0.01"]);
    a
}

fn modular_config(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("session_knockout")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core/support/config"),
        &dir,
    );
    fs::write(
        dir.join("events.cfg"),
        "u begin Inject modular-subroutine.org 12\n\
         u 0:5:end PrintAverageData\n\
         u 0:5:end PrintCountData\n\
         u 40 Exit\n",
    )
    .unwrap();
    dir
}

/// The id of the organism in cell 12, after update 0 has run.
fn watch_centre(h: &mut Harness) -> i32 {
    h.send(Command::StepUpdate);
    h.wait_event(|e| matches!(e, SessionEvent::Paused { .. }));
    h.send(Command::Subscribe(Subscription {
        layers: vec![],
        inspect_cell: Some(12),
        genotypes_top_n: 5,
        lineage_of: None,
    }));
    h.wait_until(|s| s.latest_frame().is_some_and(|f| f.inspect.is_some()));
    h.session
        .latest_frame()
        .unwrap()
        .inspect
        .as_ref()
        .unwrap()
        .org_id
}

fn reply(h: &mut Harness) -> Arc<KnockoutReply> {
    match h.wait_event(|e| matches!(e, SessionEvent::KnockoutMap(_))) {
        SessionEvent::KnockoutMap(r) => r,
        _ => unreachable!(),
    }
}

#[test]
fn the_map_of_the_watched_organism_comes_back_as_an_event() {
    let dir = modular_config("map");
    let mut h = Harness::spawn(&dir, &args());
    let org_id = watch_centre(&mut h);
    h.send(Command::KnockoutMap(KnockoutRequest { cell: 12, org_id }));
    let r = reply(&mut h);
    assert_eq!(r.request, KnockoutRequest { cell: 12, org_id });
    let m = r.result.as_ref().expect("a map");
    assert_eq!(m.performed.len(), 3);
    let nand = m.inst_names.iter().position(|n| n == "nand").unwrap();
    assert_eq!(m.sites[nand].class, SiteClass::Shared);
    assert_eq!(m.sites[nand].lost, vec![0, 1, 2]);
    assert_eq!(m.measures().calls.unwrap().call_sites, 3);

    // An organism that is not there: a one-line reason.
    h.send(Command::KnockoutMap(KnockoutRequest {
        cell: 12,
        org_id: org_id + 1000,
    }));
    let r = reply(&mut h);
    let e = r.result.as_ref().expect_err("refused");
    assert!(
        e.contains("no longer in cell 12") && !e.contains('\n'),
        "{e}"
    );
    h.send(Command::KnockoutMap(KnockoutRequest { cell: 999, org_id }));
    assert!(reply(&mut h).result.is_err());
}

#[test]
fn asking_for_a_map_does_not_change_the_run() {
    let run = |name: &str, ask: bool| {
        let dir = modular_config(name);
        let mut h = Harness::spawn(&dir, &args());
        let org_id = watch_centre(&mut h);
        if ask {
            for _ in 0..2 {
                h.send(Command::KnockoutMap(KnockoutRequest { cell: 12, org_id }));
                assert!(reply(&mut h).result.is_ok());
            }
        }
        h.send(Command::Run);
        h.wait_event(|e| matches!(e, SessionEvent::Finished { .. }));
        dir
    };
    let (a, b) = (run("plain", false), run("asked", true));
    let (mut fa, mut fb) = (Vec::new(), Vec::new());
    files_under(&a.join("data"), &a, &mut fa);
    files_under(&b.join("data"), &b, &mut fb);
    assert_eq!(fa, fb);
    assert!(fa.len() >= 2);
    for f in &fa {
        assert!(
            normalized(&a.join(f)) == normalized(&b.join(f)),
            "{f:?} differs"
        );
    }
}

#[test]
fn a_playback_says_why_it_cannot() {
    let dir = modular_config("playback");
    let file = dir.join("run.avrec");
    {
        let mut h = Harness::spawn(&dir, &args());
        let mut spec = RecordSpec::new(&file);
        spec.every = 10;
        h.send(Command::Record(spec));
        h.wait_event(|e| matches!(e, SessionEvent::RecordingStarted(_)));
        h.send(Command::Run);
        h.wait_event(|e| matches!(e, SessionEvent::Finished { .. }));
        h.send(Command::StopRecording);
        h.wait_event(|e| matches!(e, SessionEvent::RecordingStopped { .. }));
    }
    let session = Session::play(&file).unwrap();
    assert!(session.send(Command::KnockoutMap(KnockoutRequest {
        cell: 12,
        org_id: 1
    })));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let events = session.drain_events();
        if let Some(SessionEvent::KnockoutMap(r)) = events
            .into_iter()
            .find(|e| matches!(e, SessionEvent::KnockoutMap(_)))
        {
            assert_eq!(r.result.as_ref().err().unwrap(), KNOCKOUT_PLAYBACK_REASON);
            assert!(!KNOCKOUT_PLAYBACK_REASON.contains('\n'));
            return;
        }
        assert!(std::time::Instant::now() < deadline, "no answer");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
