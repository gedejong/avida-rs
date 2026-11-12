//! `avida-s`: runs `main.asl` from the current directory.

use avida_script::driver;
use avida_script::output;

fn main() {
    let lib = driver::default_library();
    let code = driver::run(&lib, "main.asl", true);
    output::exit(code);
}
