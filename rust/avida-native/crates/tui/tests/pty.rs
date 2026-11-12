//! Drives the real binary in a pseudo-terminal (`tests/pty_driver.py`, which
//! needs only Python's standard library) and checks start-up, a few keys and
//! that the terminal is given back on every way out. Skips, with a note on
//! stderr, when `python3` or a pty is not available.

mod common;

use std::path::PathBuf;
use std::process::Command;

fn run(scenario: &str) -> Option<String> {
    let dir: PathBuf = common::fresh_config(&format!("pty-{scenario}"));
    let driver = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/pty_driver.py");
    let out = match Command::new("python3")
        .arg(&driver)
        .arg(env!("CARGO_BIN_EXE_avida-tui"))
        .arg(&dir)
        .arg(scenario)
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            eprintln!("skipping the pty test: python3 is not available ({e})");
            return None;
        }
    };
    if out.status.code() == Some(77) {
        eprintln!("skipping the pty test: no pseudo-terminal here");
        return None;
    }
    assert!(
        out.status.success(),
        "driver failed: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

fn field(json: &str, key: &str) -> String {
    let pat = format!("\"{key}\": ");
    let at = json
        .find(&pat)
        .unwrap_or_else(|| panic!("no {key} in {json}"))
        + pat.len();
    json[at..]
        .split([',', '}'])
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

fn assert_given_back(j: &str) {
    assert_eq!(field(j, "alt_on"), "1", "{j}");
    assert!(field(j, "alt_off").parse::<u32>().unwrap() >= 1, "{j}");
    assert_eq!(field(j, "ends_on_main_screen"), "true", "{j}");
    assert_eq!(field(j, "ends_with_cursor_shown"), "true", "{j}");
    assert_eq!(field(j, "mouse_on"), "1", "{j}");
    assert_eq!(field(j, "mouse_ends_off"), "true", "{j}");
}

#[test]
fn starts_responds_to_keys_and_quits_cleanly_in_a_real_terminal() {
    let Some(j) = run("quit") else { return };
    assert_eq!(field(&j, "started"), "true", "{j}");
    assert_eq!(field(&j, "stats_screen"), "true", "{j}");
    assert_eq!(field(&j, "running"), "true", "{j}");
    assert_eq!(field(&j, "exit"), "0", "{j}");
    assert_eq!(field(&j, "asked_cursor_position"), "0", "{j}");
    assert_given_back(&j);
    // Frames are drawn as differences: the screen is cleared at most when
    // it starts, never once per frame.
    assert!(field(&j, "full_clears").parse::<u32>().unwrap() <= 2, "{j}");
}

#[test]
fn sigterm_gives_the_terminal_back() {
    let Some(j) = run("sigterm") else { return };
    assert_eq!(field(&j, "started"), "true", "{j}");
    assert_eq!(field(&j, "exit"), "143", "{j}");
    assert_given_back(&j);
}

#[test]
fn a_panic_gives_the_terminal_back_before_the_message() {
    let Some(j) = run("panic") else { return };
    assert_eq!(field(&j, "exit"), "101", "{j}");
    assert_given_back(&j);
    assert_eq!(field(&j, "panic_text_after_restore"), "true", "{j}");
}

/// The terminal going away (the pty master closed while a run is going at
/// max speed) must end the process quickly with a failure status, not leave
/// it spinning. The driver kills and reaps the child by pid if it does not.
fn assert_exits_when_the_terminal_goes_away(scenario: &str) {
    let Some(j) = run(scenario) else { return };
    assert_eq!(field(&j, "running"), "true", "{j}");
    let exit = field(&j, "exit");
    assert!(
        exit != "null" && exit != "0",
        "did not exit with failure: {j}"
    );
    assert!(
        field(&j, "exit_seconds").parse::<f64>().unwrap() < 10.0,
        "{j}"
    );
}

#[test]
fn a_hang_up_ends_the_process() {
    assert_exits_when_the_terminal_goes_away("hangup");
}

#[test]
fn a_dead_terminal_ends_the_process_even_when_sighup_is_blocked() {
    assert_exits_when_the_terminal_goes_away("hangup_masked");
}

/// Quitting or a signal while a run goes at max speed (the terminal still
/// there): the process ends quickly with `exit` and gives the terminal
/// back.
fn assert_ends_while_running(scenario: &str, exit: &str) {
    let Some(j) = run(scenario) else { return };
    assert_eq!(field(&j, "running"), "true", "{j}");
    assert_eq!(field(&j, "exit"), exit, "{j}");
    assert!(
        field(&j, "exit_seconds").parse::<f64>().unwrap() < 10.0,
        "{j}"
    );
    assert_given_back(&j);
}

#[test]
fn quitting_while_running_at_max_speed_exits_cleanly() {
    assert_ends_while_running("quit_running", "0");
}

#[test]
fn sigterm_while_running_at_max_speed_gives_the_terminal_back() {
    assert_ends_while_running("sigterm_running", "143");
}

#[test]
fn sigint_while_running_at_max_speed_gives_the_terminal_back() {
    assert_ends_while_running("sigint_running", "130");
}

/// SIGHUP without a hang-up (`kill -HUP` from elsewhere): the terminal is
/// still there, so it is given back.
#[test]
fn sighup_with_the_terminal_still_there_gives_it_back() {
    assert_ends_while_running("sighup_running", "129");
}

/// A signal ends the process within about a second (the exit watchdog's
/// grace) with the status the signal asks for, also when the interface
/// cannot make progress. `seconds` are counted from the signal (for
/// `hangup_stuck_sighup`, from the hang-up that sends it).
fn assert_signal_ends_it(scenario: &str, exit: &str) -> String {
    let Some(j) = run(scenario) else {
        return String::new();
    };
    assert_eq!(field(&j, "exit"), exit, "{j}");
    assert!(
        field(&j, "exit_seconds").parse::<f64>().unwrap() < 4.0,
        "{j}"
    );
    j
}

/// The test hook makes the interface spin on the dead terminal the way it
/// used to (reading crossterm with no hang-up check), so only the
/// watchdog can end it.
#[test]
fn the_hang_ups_sighup_ends_a_process_stuck_on_the_dead_terminal() {
    assert_signal_ends_it("hangup_stuck_sighup", "129");
}

#[test]
fn sigterm_ends_a_process_stuck_on_a_dead_terminal() {
    let j = assert_signal_ends_it("hangup_stuck_sigterm", "143");
    if !j.is_empty() {
        assert_eq!(field(&j, "alive_after_hangup"), "true", "{j}");
    }
}

#[test]
fn sigint_ends_a_process_stuck_on_a_dead_terminal() {
    let j = assert_signal_ends_it("hangup_stuck_sigint", "130");
    if !j.is_empty() {
        assert_eq!(field(&j, "alive_after_hangup"), "true", "{j}");
    }
}

/// A terminal that is there but not read (the pty master open, its buffer
/// full): the interface blocks writing a frame, and SIGTERM still ends the
/// process.
#[test]
fn sigterm_ends_a_process_blocked_writing_to_an_unread_terminal() {
    let j = assert_signal_ends_it("blocked_output_sigterm", "143");
    if !j.is_empty() {
        assert_eq!(field(&j, "running"), "true", "{j}");
    }
}
