//! Recording a run and playing it back: a recorded run computes what an
//! unrecorded one does, the file holds the frames a session shows, a file
//! cut mid-record is readable up to its last complete record, the player
//! seeks and reports its place, and `avida-rec` records headless (also in
//! the parallel execution mode) and stops cleanly on SIGTERM.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use avida_session::recording::{Recording, Tail, MAGIC, VERSION};
use avida_session::{
    Command, Frame, MapFrameCompare, RecordSpec, RunState, Session, SessionEvent, UpdateRow,
};

use common::{assert_same_data, endless_config, fresh_config, plain_run, timeout, Harness};

/// Updates between recorded frames in the shared run.
const EVERY: u32 = 20;

struct Recorded {
    dir: PathBuf,
    file: PathBuf,
    /// What the session showed, paused at update 40 with every layer.
    reference_40: Arc<Frame>,
}

/// One recorded run of `heads_default_100u`, shared by the tests: frames at
/// 0, 20, 40, 60, 80 and 100, and the session's own frame at update 40.
fn recorded() -> &'static Recorded {
    static RUN: OnceLock<Recorded> = OnceLock::new();
    RUN.get_or_init(|| {
        let dir = fresh_config("recording_run");
        let file = dir.join("run.avrec");
        let mut h = Harness::spawn(&dir, &[]);
        let mut spec = RecordSpec::new(&file);
        spec.every = EVERY;
        h.send(Command::Record(spec));
        h.wait_event(|e| matches!(e, SessionEvent::RecordingStarted(_)));
        h.send(Command::RunToUpdate(40));
        h.wait_event(|e| *e == SessionEvent::Paused { update: 40 });
        // The recorder's own subscription: every layer, a table of 100.
        let layers = h.session.latest_frame().unwrap().layers.clone();
        h.send(Command::Subscribe(avida_session::Subscription {
            layers,
            inspect_cell: None,
            genotypes_top_n: avida_session::DEFAULT_RECORD_TOP_N,
            lineage_of: None,
        }));
        h.wait_until(|s| {
            s.latest_frame()
                .is_some_and(|f| f.map.as_ref().is_some_and(|m| m.layers.len() > 10))
        });
        let reference_40 = h.session.latest_frame().unwrap();
        h.send(Command::Run);
        h.wait_event(|e| matches!(e, SessionEvent::Finished { .. }));
        h.send(Command::StopRecording);
        h.wait_event(|e| matches!(e, SessionEvent::RecordingStopped { .. }));
        let status = h.session.recording_status().unwrap();
        assert_eq!(status.frames, 6, "{status:?}");
        assert!(!status.active && status.error.is_none());
        Recorded {
            dir,
            file,
            reference_40,
        }
    })
}

#[test]
fn a_recorded_run_writes_what_an_unrecorded_run_writes() {
    let rec = recorded();
    let plain = fresh_config("recording_plain");
    plain_run(&plain);
    assert_same_data(&rec.dir.join("data"), &plain.join("data"));
}

#[test]
fn the_file_holds_the_frames_the_session_showed() {
    let rec = recorded();
    let mut r = Recording::open(&rec.file).unwrap();
    assert_eq!(r.len(), 6);
    assert_eq!(r.tail(), Tail::Clean);
    let updates: Vec<i32> = (0..r.len()).map(|i| r.update_of(i).unwrap()).collect();
    assert_eq!(updates, [0, 20, 40, 60, 80, 100]);
    assert_eq!(r.meta().every, EVERY);
    assert_eq!((r.meta().world_x, r.meta().world_y), (60, 60));

    let got = r.frame(2).unwrap();
    let want = &rec.reference_40;
    assert_eq!(got.update, want.update);
    assert_eq!(got.header, want.header);
    assert_eq!(got.dominant, want.dominant);
    assert_eq!(got.genotypes, want.genotypes);
    assert_eq!(got.env, want.env);
    assert_eq!(got.inst_names, want.inst_names);
    assert_eq!(got.layers, want.layers);
    let (a, b) = (got.map.as_ref().unwrap(), want.map.as_ref().unwrap());
    assert!(
        a.same_as(b),
        "the decoded map differs from the captured one"
    );
    assert!(got.inspect.is_none() && got.lineage.is_empty());
    // The statistics row alone reads the same.
    let row: UpdateRow = r.row(2).unwrap();
    assert_eq!(row, want.header.stats);
}

#[test]
fn a_file_cut_inside_a_record_opens_up_to_the_last_complete_one() {
    let rec = recorded();
    let bytes = fs::read(&rec.file).unwrap();
    let full = Recording::open(&rec.file).unwrap();
    assert_eq!(full.len(), 6);
    let header_end = 16 + u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let dir = rec.dir.join("cut");
    fs::create_dir_all(&dir).unwrap();
    let cut = dir.join("cut.avrec");
    let mut sizes = Vec::new();
    for n in [
        bytes.len() - 1,
        bytes.len() * 9 / 10,
        bytes.len() / 2,
        header_end + 3,
        header_end,
    ] {
        fs::write(&cut, &bytes[..n]).unwrap();
        let mut r = Recording::open(&cut).unwrap_or_else(|e| panic!("cut at {n}: {e}"));
        assert!(r.len() < 6, "cut at {n} of {}", bytes.len());
        if n == header_end {
            assert_eq!(r.tail(), Tail::Clean);
        } else {
            assert!(matches!(r.tail(), Tail::Cut { .. }), "{:?}", r.tail());
        }
        for i in 0..r.len() {
            r.frame(i).unwrap();
        }
        sizes.push(r.len());
    }
    assert_eq!(
        sizes[0], 5,
        "one byte short of the end loses the last record only"
    );
    assert_eq!(&sizes[3..], &[0, 0]);
    // A cut inside the header is an error, not a panic.
    fs::write(&cut, &bytes[..12]).unwrap();
    assert!(Recording::open(&cut).is_err());
    fs::write(&cut, &bytes[..30]).unwrap();
    assert!(Recording::open(&cut).is_err());
}

#[test]
fn damaged_bytes_give_errors_not_panics() {
    let rec = recorded();
    let bytes = fs::read(&rec.file).unwrap();
    let path = rec.dir.join("damaged.avrec");
    // Flip a byte at many places; whatever opens must not panic.
    let step = bytes.len() / 60;
    for at in (0..bytes.len()).step_by(step.max(1)) {
        let mut b = bytes.clone();
        b[at] ^= 0x5a;
        fs::write(&path, &b).unwrap();
        if let Ok(mut r) = Recording::open(&path) {
            for i in 0..r.len() {
                let _ = r.frame(i);
                let _ = r.row(i);
            }
        }
    }
}

#[test]
fn foreign_and_future_files_are_refused_with_a_reason() {
    let rec = recorded();
    let path = rec.dir.join("foreign.avrec");
    fs::write(&path, b"not a recording at all").unwrap();
    let e = Recording::open(&path).err().unwrap().to_string();
    assert!(e.contains("not an Avida recording"), "{e}");
    assert!(Session::play(&path).is_err());

    let mut bytes = fs::read(&rec.file).unwrap();
    assert_eq!(&bytes[..8], MAGIC);
    bytes[8..12].copy_from_slice(&(VERSION + 1).to_le_bytes());
    fs::write(&path, &bytes).unwrap();
    let e = Recording::open(&path).err().unwrap().to_string();
    assert!(e.contains(&format!("version {}", VERSION + 1)), "{e}");
    let e = Session::play(&path).err().unwrap();
    assert!(e.contains("version"), "{e}");
}

/// A playback session with a wake channel.
struct Player {
    session: Session,
    wake: mpsc::Receiver<()>,
}

impl Player {
    fn open(path: &Path) -> Player {
        let (tx, wake) = mpsc::channel();
        let session = Session::play_with_wake(
            path,
            Box::new(move || {
                let _ = tx.send(());
            }),
        )
        .unwrap_or_else(|e| panic!("play: {e}"));
        Player { session, wake }
    }

    fn wait(&self, what: &str, cond: impl Fn(&Session) -> bool) {
        let deadline = Instant::now() + timeout();
        while !cond(&self.session) {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            let _ = self.wake.recv_timeout(Duration::from_millis(20));
        }
    }

    fn wait_index(&self, i: usize) {
        self.wait(&format!("record {i}"), |s| {
            s.playback_info().is_some_and(|p| p.index == i)
        });
    }
}

#[test]
fn the_player_seeks_steps_and_reports_where_it_is() {
    let rec = recorded();
    let p = Player::open(&rec.file);
    let info = p.session.playback_info().unwrap();
    assert_eq!((info.frames, info.index, info.update), (6, 0, 0));
    assert_eq!((info.first_update, info.last_update), (0, 100));
    assert_eq!(info.cut_tail_bytes, 0);
    assert_eq!(p.session.state(), RunState::Paused);
    assert_eq!(p.session.latest_frame().unwrap().update, 0);
    assert_eq!(p.session.setup().config_file, rec.file);
    let (reset, rows) = p.session.drain_rows_reset();
    assert!(!reset);
    assert_eq!(rows.iter().map(|r| r.update).collect::<Vec<_>>(), [0]);

    // Seek to an update: the frame at or before it.
    p.session.send(Command::RunToUpdate(50));
    p.wait_index(2);
    assert_eq!(p.session.latest_frame().unwrap().update, 40);
    assert_eq!(p.session.status().update, 40);
    let (reset, rows) = p.session.drain_rows_reset();
    assert!(!reset);
    assert_eq!(rows.iter().map(|r| r.update).collect::<Vec<_>>(), [20, 40]);

    // Back one frame: the chart restarts with the rows up to it.
    p.session.send(Command::StepSlice);
    p.wait_index(1);
    let (reset, rows) = p.session.drain_rows_reset();
    assert!(reset);
    assert_eq!(rows.iter().map(|r| r.update).collect::<Vec<_>>(), [0, 20]);

    p.session.send(Command::StepUpdate);
    p.wait_index(2);
    let (reset, rows) = p.session.drain_rows_reset();
    assert!(!reset);
    assert_eq!(rows.iter().map(|r| r.update).collect::<Vec<_>>(), [40]);

    // Out of range seeks clamp.
    p.session.send(Command::SeekFrame(999));
    p.wait_index(5);
    assert_eq!(p.session.latest_frame().unwrap().update, 100);
    p.session.send(Command::RunToUpdate(-5));
    p.wait_index(0);

    // What needs a simulation says so; a subscription is just ignored.
    p.session
        .send(Command::Subscribe(avida_session::Subscription::default()));
    p.session.send(Command::SavePopulation);
    p.session.send(Command::Restart { args: Vec::new() });
    let refusals = std::cell::Cell::new(0);
    p.wait("the refusals", |s| {
        refusals.set(
            refusals.get()
                + s.drain_events()
                    .iter()
                    .filter(|e| matches!(e, SessionEvent::Error(m) if m.contains("playback")))
                    .count(),
        );
        refusals.get() >= 2
    });
    assert_eq!(refusals.get(), 2, "a subscription is not an error");
    assert_eq!(p.session.exit_code(), None);

    // Play from the start to the end: it pauses on the last frame.
    p.session.send(Command::Run);
    p.wait("the end of the recording", |s| {
        s.state() == RunState::Paused && s.playback_info().is_some_and(|i| i.index == 5)
    });
    // At the end Run plays it again.
    p.session.send(Command::Run);
    p.wait("the replay", |s| {
        s.state() == RunState::Running || s.playback_info().is_some_and(|i| i.index < 5)
    });
    p.session.send(Command::Pause);
    p.wait("the pause", |s| s.state() == RunState::Paused);
}

#[test]
fn a_cut_recording_plays_up_to_its_last_complete_frame() {
    let rec = recorded();
    let bytes = fs::read(&rec.file).unwrap();
    let path = rec.dir.join("cut-play.avrec");
    fs::write(&path, &bytes[..bytes.len() - 7]).unwrap();
    let p = Player::open(&path);
    let info = p.session.playback_info().unwrap();
    assert_eq!(info.frames, 5);
    assert!(info.cut_tail_bytes > 0);
    assert_eq!(info.last_update, 80);
}

#[test]
fn a_recording_that_is_still_being_written_is_followed_on_refresh() {
    let rec = recorded();
    let bytes = fs::read(&rec.file).unwrap();
    let path = rec.dir.join("growing.avrec");
    let full = Recording::open(&rec.file).unwrap();
    assert_eq!(full.len(), 6);
    // Two records' worth first, then the rest appended.
    let mut r = Recording::open(&rec.file).unwrap();
    let _ = r.refresh();
    let cut = bytes.len() * 2 / 5;
    fs::write(&path, &bytes[..cut]).unwrap();
    let p = Player::open(&path);
    let before = p.session.playback_info().unwrap().frames;
    assert!(before < 6);
    fs::write(&path, &bytes).unwrap();
    p.session.send(Command::SeekFrame(999));
    p.wait("the whole recording", |s| {
        s.playback_info()
            .is_some_and(|i| i.frames == 6 && i.index == 5)
    });
}

fn avida_rec() -> Process {
    Process::new(env!("CARGO_BIN_EXE_avida-rec"))
}

#[test]
fn avida_rec_records_headless_in_the_parallel_mode() {
    let dir = fresh_config("recording_headless");
    let file = dir.join("parallel.avrec");
    let out = avida_rec()
        .args(["--record"])
        .arg(&file)
        .args(["--record-every", "25", "--progress", "0"])
        .arg(&dir)
        .args([
            "-set",
            "EXECUTION_MODE",
            "1",
            "-set",
            "EXECUTION_THREADS",
            "2",
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{:?}\n{stderr}", out.status);
    assert!(stderr.contains("recorded 5 frames"), "{stderr}");
    let mut r = Recording::open(&file).unwrap();
    assert_eq!(r.len(), 5);
    assert_eq!(r.update_of(4), Some(100));
    assert_eq!(r.meta().every, 25);
    assert!(r.meta().args.iter().any(|a| a == "EXECUTION_MODE"));
    let last = r.frame(4).unwrap();
    assert!(last.header.stats.orgs > 1);
    assert!(last.map.unwrap().occupied() > 1);
}

#[test]
fn avida_rec_leaves_a_complete_file_when_terminated() {
    let dir = endless_config("recording_sigterm");
    let file = dir.join("endless.avrec");
    let mut child = avida_rec()
        .args(["--record"])
        .arg(&file)
        .args(["--record-every", "2", "--progress", "0"])
        .arg(&dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + timeout();
    loop {
        if Recording::open(&file).is_ok_and(|r| r.len() >= 3) {
            break;
        }
        assert!(Instant::now() < deadline, "no frames were recorded");
        if let Some(status) = child.try_wait().unwrap() {
            panic!("avida-rec ended early: {status}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let killed = Process::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(128 + 15), "{status:?}");
    let mut r = Recording::open(&file).unwrap();
    assert_eq!(r.tail(), Tail::Clean);
    assert!(r.len() >= 3);
    for i in 0..r.len() {
        r.frame(i).unwrap();
    }
    // Updates ascend, and the last is a whole update, not a multiple chosen
    // by the interval: the stop wrote its own frame.
    let updates: Vec<i32> = (0..r.len()).map(|i| r.update_of(i).unwrap()).collect();
    assert!(updates.windows(2).all(|w| w[0] < w[1]), "{updates:?}");
}
