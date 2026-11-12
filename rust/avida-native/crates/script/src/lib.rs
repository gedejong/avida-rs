//! AvidaScript (ASL): lexer, parser, semantic checker, interpreter and the
//! `avida-s` driver. See `PORTING.md` of the workspace.
//!
//! Ported from `avida-core/source/script/` and
//! `avida-core/source/targets/avida-s/`.

// The C++ is transliterated statement by statement; these style lints would
// push the code away from the source it cites.
#![allow(
    clippy::needless_range_loop,
    clippy::result_unit_err,
    clippy::explicit_counter_loop,
    clippy::too_many_arguments,
    clippy::type_complexity
)]

pub mod ast;
pub mod driver;
pub mod dump;
pub mod interp;
pub mod lexer;
pub mod library;
pub mod native;
pub mod oldmap;
pub mod output;
pub mod parser;
pub mod semantic;
pub mod symtab;
pub mod types;
