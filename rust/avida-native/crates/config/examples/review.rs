//! Prints what the reference `avida -r` prints after the banner (used to
//! cross-check against `cbuild/work/avida`). Usage: review <dir> [args...]
use avida_config::cmdline::{banner_output, process_cmdline};

fn main() {
    let mut a: Vec<String> = std::env::args().skip(1).collect();
    let dir = a.remove(0);
    let mut args = vec!["avida".to_string()];
    args.extend(a);
    let o = process_cmdline(&args, &dir);
    print!("{}{}", banner_output(), o.stdout);
    eprint!("{}", o.stderr);
    std::process::exit(o.exit.unwrap_or(0) & 0xff);
}
