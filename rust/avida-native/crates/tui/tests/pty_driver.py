"""Runs avida-tui in a pseudo-terminal and reports what it wrote.

usage: pty_driver.py BINARY WORKDIR SCENARIO

SCENARIO is one of: quit, sigterm, panic, hangup, hangup_masked,
quit_running, sigterm_running, sigint_running, sighup_running,
hangup_stuck_sighup, hangup_stuck_sigterm, hangup_stuck_sigint,
blocked_output_sigterm.
The hangup scenarios start a run at max speed and close the pty master;
hangup_masked blocks SIGHUP in the child so only the input error is left.
The *_running scenarios start a run at max speed and then quit (q, y) or
send the signal with the terminal still there. Every scenario that runs at
max speed gives the world an events file without its Exit, so the run is
still going when the process is told to end.
The hangup_stuck_* scenarios set AVIDA_TUI_TEST_UNGUARDED_INPUT, which
makes the interface read the terminal with no hang-up check, so closing the
pty master leaves it spinning in crossterm: hangup_stuck_sighup relies on
the SIGHUP of the hang-up, the others block SIGHUP, check the process is
still alive a second after the close and then send SIGTERM or SIGINT.
blocked_output_sigterm runs at max speed, stops reading the master so the
child blocks writing to the pty, and sends SIGTERM.
Prints one JSON object on stdout. Exit status 77 means no pty is available
(the Rust test then skips). Standard library only.
"""
import fcntl
import json
import os
import pty
import select
import signal
import struct
import sys
import termios
import time

binary, workdir, scenario = sys.argv[1:4]
COLS, ROWS = 100, 30
DEADLINE = 40.0
HANGUPS = ("hangup", "hangup_masked")
RUNNING = {
    "quit_running": None,
    "sigterm_running": signal.SIGTERM,
    "sigint_running": signal.SIGINT,
    "sighup_running": signal.SIGHUP,
}
# The signal sent a second after the close (None: the hang-up's own SIGHUP).
STUCK = {
    "hangup_stuck_sighup": None,
    "hangup_stuck_sigterm": signal.SIGTERM,
    "hangup_stuck_sigint": signal.SIGINT,
}
BLOCKED = "blocked_output_sigterm"

argv = [binary]
if scenario in HANGUPS or scenario in RUNNING or scenario == BLOCKED:
    with open(os.path.join(workdir, "events.cfg")) as f:
        lines = [l for l in f if "Exit" not in l and "SavePopulation" not in l]
    with open(os.path.join(workdir, "events-endless.cfg"), "w") as f:
        f.writelines(lines)
    argv += [".", "-set", "EVENT_FILE", "events-endless.cfg"]

try:
    pid, fd = pty.fork()
except OSError:
    sys.exit(77)
if pid == 0:
    os.chdir(workdir)
    env = dict(os.environ, TERM="xterm-256color")
    env.pop("NO_COLOR", None)
    if scenario == "hangup_masked" or STUCK.get(scenario, None) is not None:
        signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGHUP})
    if scenario in STUCK:
        env["AVIDA_TUI_TEST_UNGUARDED_INPUT"] = "1"
    if scenario == "panic":
        env["AVIDA_TUI_TEST_PANIC"] = "1"
    os.execve(binary, argv, env)

fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
raw = b""
start = time.time()


def pump(seconds):
    global raw
    end = time.time() + seconds
    while time.time() < end:
        r, _, _ = select.select([fd], [], [], 0.05)
        if r:
            try:
                b = os.read(fd, 65536)
            except OSError:
                return False
            if not b:
                return False
            raw += b
    return True


def wait_for(needle, timeout=20.0):
    end = time.time() + timeout
    while time.time() < end:
        if needle in raw:
            return True
        if not pump(0.1):
            return needle in raw
    return needle in raw


def reap(timeout=10.0):
    end = time.time() + timeout
    while time.time() < end:
        pump(0.05)
        done, status = os.waitpid(pid, os.WNOHANG)
        if done:
            return status
    return None


def wait_exit(t0, master_open, limit=15.0):
    """Waits for the child to exit without reading the master, killing it
    if it is still there after `limit` seconds (generous: the bound
    separates "exits" from "spins for ever", and the test suite often runs
    on a heavily loaded machine). Prints the result with the seconds since
    `t0` and ends the driver."""
    status = None
    while time.time() - t0 < limit:
        done, st = os.waitpid(pid, os.WNOHANG)
        if done:
            status = st
            break
        time.sleep(0.02)
    if status is None:
        # A process that controls its terminal cannot finish exiting, even
        # after SIGKILL, until the terminal has taken its output: let go
        # of the master first.
        if master_open:
            os.close(fd)
        os.kill(pid, signal.SIGKILL)
        os.waitpid(pid, 0)
        result["exit"] = None
    else:
        result["exit"] = os.waitstatus_to_exitcode(status)
    result["exit_seconds"] = round(time.time() - t0, 2)
    print(json.dumps(result))
    sys.exit(0)


result = {}
result["started"] = wait_for(b"PAUSED")
if scenario == "quit":
    os.write(fd, b"s")
    result["stats_screen"] = wait_for(b"Population")
    os.write(fd, b"\x1b[B\x1b[6~\x1b[5~")  # arrows and paging do not crash
    os.write(fd, b" ")
    result["running"] = wait_for(b"RUNNING")
    os.write(fd, b" ")
    pump(0.3)
    # Ctrl-L redraws; it must not wait for a cursor position report (this
    # driver never answers one).
    os.write(fd, b"\x0c")
    pump(0.3)
    result["asked_cursor_position"] = raw.count(b"\x1b[6n")
    os.write(fd, b"q")
    pump(0.4)
    os.write(fd, b"y")
elif scenario == "sigterm":
    os.kill(pid, signal.SIGTERM)
elif scenario in RUNNING:
    os.write(fd, b" ")
    result["running"] = wait_for(b"RUNNING")
    os.write(fd, b"+" * 20)  # up the speed ladder to max
    pump(0.5)
    if RUNNING[scenario] is None:
        os.write(fd, b"q")
        pump(0.4)
        t0 = time.time()
        os.write(fd, b"y")
    else:
        t0 = time.time()
        os.kill(pid, RUNNING[scenario])
elif scenario in HANGUPS:
    os.write(fd, b" ")
    result["running"] = wait_for(b"RUNNING")
    os.write(fd, b"+" * 20)  # up the speed ladder to max
    pump(0.5)
    t0 = time.time()
    os.close(fd)
    wait_exit(t0, False)
elif scenario in STUCK:
    pump(0.3)
    t0 = time.time()
    os.close(fd)
    if STUCK[scenario] is not None:
        time.sleep(1.0)
        done, st = os.waitpid(pid, os.WNOHANG)
        result["alive_after_hangup"] = not done
        if done:
            result["exit"] = os.waitstatus_to_exitcode(st)
            result["exit_seconds"] = None
            print(json.dumps(result))
            sys.exit(0)
        t0 = time.time()
        os.kill(pid, STUCK[scenario])
    wait_exit(t0, False)
elif scenario == BLOCKED:
    os.write(fd, b" ")
    result["running"] = wait_for(b"RUNNING")
    os.write(fd, b"+" * 20)  # up the speed ladder to max
    pump(0.5)
    # Stop reading: the pty's buffer fills and the child's next write to
    # it blocks.
    time.sleep(1.5)
    unread = struct.unpack("i", fcntl.ioctl(fd, termios.FIONREAD, b"\0" * 4))[0]
    result["unread_bytes"] = unread
    t0 = time.time()
    os.kill(pid, signal.SIGTERM)
    wait_exit(t0, True)
# "panic" ends by itself after the first draw.
status = reap()
if scenario in RUNNING:
    result["exit_seconds"] = round(time.time() - t0, 2)
if status is None:
    os.kill(pid, signal.SIGKILL)
    os.waitpid(pid, 0)
    result["exit"] = None
else:
    result["exit"] = os.waitstatus_to_exitcode(status)
result["alt_on"] = raw.count(b"\x1b[?1049h")
result["alt_off"] = raw.count(b"\x1b[?1049l")
result["mouse_on"] = raw.count(b"\x1b[?1000h")
result["mouse_off"] = raw.count(b"\x1b[?1000l")
result["cursor_hidden"] = raw.count(b"\x1b[?25l")
result["cursor_shown"] = raw.count(b"\x1b[?25h")
# The last cursor visibility sequence and the last screen switch decide the
# state the terminal is left in.
result["ends_with_cursor_shown"] = raw.rfind(b"\x1b[?25h") > raw.rfind(b"\x1b[?25l")
result["ends_on_main_screen"] = raw.rfind(b"\x1b[?1049l") > raw.rfind(b"\x1b[?1049h")
result["mouse_ends_off"] = raw.rfind(b"\x1b[?1000l") > raw.rfind(b"\x1b[?1000h")
result["full_clears"] = raw.count(b"\x1b[2J")
result["panic_text_after_restore"] = (
    raw.rfind(b"panicked") > raw.find(b"\x1b[?1049l") if scenario == "panic" else None
)
result["seconds"] = round(time.time() - start, 1)
print(json.dumps(result))
