//! The `avida-s` main (`avida-core/source/targets/avida-s/main.cc`).

use crate::dump::DumpVisitor;
use crate::interp::Interpreter;
use crate::library::{register_core_lib, register_native_stubs, ASLibrary};
use crate::output;
use crate::parser::Parser;
use crate::semantic::SemanticVisitor;
use crate::symtab::SymbolTable;
use crate::types::{AS_EXIT_FAIL_PARSE, AS_EXIT_FAIL_SEMANTIC, AS_EXIT_FILE_NOT_FOUND};

/// The library of `main()`: `RegisterASCoreLib`, `RegisterASAvidaLib` and
/// `RegisterASAnalyzeLib`. The native object functions are signature-only
/// stand-ins (see [`register_native_stubs`]).
pub fn default_library() -> ASLibrary {
    let mut lib = ASLibrary::new();
    register_core_lib(&mut lib);
    register_native_stubs(&mut lib);
    lib
}

/// The banner `Avida::PrintVersionBanner()` stands for: the repository has no
/// such function any more; `primitive.cc` prints `cout << Version::Banner() << endl`.
pub fn banner_output() -> String {
    avida_config::cmdline::banner_output()
}

/// `main()` of `avida-s`. Returns the process exit code (the C++ calls
/// `exit()` on every path).
pub fn run(lib: &ASLibrary, script_name: &str, banner: bool) -> i32 {
    if banner {
        output::write_str(&banner_output());
        output::flush();
    }

    let src = match std::fs::read(script_name) {
        Ok(s) => s,
        Err(_) => {
            output::err_write(b"error: unable to open script\n");
            return AS_EXIT_FILE_NOT_FOUND;
        }
    };

    let mut parser = Parser::new();
    if parser.parse(script_name, src) {
        let tree = match parser.extract_tree() {
            Some(t) => t,
            None => return AS_EXIT_FAIL_PARSE,
        };

        let mut dump = DumpVisitor::new();
        dump.visit(&tree);
        output::endl();

        let global = SymbolTable::new_ref();
        let mut semantic = SemanticVisitor::new(lib, global.clone(), &tree);
        semantic.visit(&tree);
        semantic.post_check();

        if !semantic.was_successful() {
            output::err_write(b"error: semantics check failed\n");
            return AS_EXIT_FAIL_SEMANTIC;
        }

        let mut interp = Interpreter::new(global);
        interp.interpret(&tree)
    } else {
        output::err_write(b"error: parse failed\n");
        AS_EXIT_FAIL_PARSE
    }
}
