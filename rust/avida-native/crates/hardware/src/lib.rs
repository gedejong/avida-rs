//! Port of the virtual hardware layer (avida-core/source/cpu and
//! main/cOrganism at 47f13dadb). See `ARCHITECTURE.md` for the
//! `Organism`/`Hardware`/`OrgInterface` split.
//!
//! Hardware types 0 (`cHardwareCPU`, the heads CPU), 2
//! (`cHardwareTransSMT`, see [`hardware_transsmt`]), 3
//! (`cHardwareExperimental`, see [`hardware_experimental`]), 4
//! (`cHardwareGP8`, see [`hardware_gp8`]) and 5 (`cHardwareBCR`, see
//! [`hardware_bcr`]) are ported.
//!
//! # Loud start-up failures
//!
//! The following settings or inputs return
//! `Err("unsupported: SETTING=value (C++: file:symbol)")` instead of
//! silently diverging:
//!
//! * `UOPS_PER_CYCLE` above 255 for hardware types 4 and 5 (the C++ 8-bit
//!   micro-op counter never reaches it and loops forever);
//! * a hardware type the C++ has no hardware for;
//! * instruction-set entries whose `Inst_*` handler is not ported, a
//!   structural check: every handler table is complete (see
//!   [`hardware_cpu_insts::unported_methods`]).
//!
//! Run-time `Err`s are reserved for C++ `exit()` calls (`rotate-label`
//! outside grid/torus geometry, `throw`/`goto` without their marker
//! instruction, `cInstSet::GetInst` of a missing name) and for paths the
//! C++ leaves undefined (a NULL or out-of-range dereference, a division by
//! zero, a merit of 0 handed to `UpdateMerit`), each documented at its
//! site. One port limit is left, also as an `Err`: a `MAX_PREY_BT` kill
//! requested from a test CPU (see [`hardware_experimental_insts_env`]).
//!
//! An organism that faces its own cell (in a torus one cell wide or high,
//! or as an avatar in a bounded single row) is its own neighbour, as in
//! the C++: [`org_interface::OrgInterface::with_neighbor`] answers
//! [`org_interface::Faced::SelfOrg`] and the neighbour's reads and writes go
//! to the running organism's own state
//! ([`org_interface::with_neighbor_state`]).
//!
//! # Undefined behaviour in the C++
//!
//! Where the C++ reads out of range or relies on undefined arithmetic, the
//! port picks a deterministic substitute and documents it at the site:
//! signed overflow wraps; `INT_MIN % -1` is 0; out-of-range resource and
//! resource-bin indices read as 0 (the reference binary segfaults on
//! `rotate-uphill`, `get-res-stored` and `h-copy-res` in a test CPU); a
//! thread killed while `SingleProcess` holds a reference to it is handled
//! through `thread_realloc`; `PrintFinalStatus` of an organism that died
//! before dividing prints an empty offspring memory where the C++
//! dereferences NULL. Where the reference binary compiled `abs(x) % n`
//! as an unsigned remainder (hardware types 4 and 5: forage-target
//! folding, look-around directions, `rotate-x`), the port uses
//! `x.unsigned_abs()`, so `INT_MIN` folds as 2^31 as it does there.

// `eprintln!` and `println!` print through `avida_apto::diag`, which a
// front end can capture (the std macros' output when nothing captures).
macro_rules! eprintln {
    ($($arg:tt)*) => {
        ::avida_apto::diag::stderr(format_args!("{}\n", format_args!($($arg)*)))
    };
}
macro_rules! println {
    ($($arg:tt)*) => {
        ::avida_apto::diag::stdout(format_args!("{}\n", format_args!($($arg)*)))
    };
}

pub mod code_label;
pub mod cpu_inst_lib;
pub mod cpu_stack;
pub mod hardware;
pub mod hardware_bcr;
pub mod hardware_bcr_insts;
mod hardware_bcr_insts_env;
pub mod hardware_cpu;
pub mod hardware_cpu_insts;
mod hardware_cpu_insts_donate;
mod hardware_cpu_insts_energy;
mod hardware_cpu_insts_group;
mod hardware_cpu_insts_misc;
mod hardware_cpu_insts_move;
mod hardware_cpu_insts_msg;
mod hardware_cpu_insts_res;
pub mod hardware_experimental;
pub mod hardware_experimental_insts;
mod hardware_experimental_insts_env;
/// `cOrganism::SetForageTarget`, which `cPopulation::Inject` also calls.
pub use hardware_experimental_insts_env::org_set_forage_target;
pub mod hardware_experimental_lib;
pub mod hardware_gp8;
pub mod hardware_gp8_insts;
pub mod hardware_manager;
pub mod hardware_transsmt;
pub mod hardware_transsmt_insts;
pub mod head_cpu;
pub mod mini_trace;
pub mod nhardware;
pub mod org_interface;
pub mod org_sensor;
pub mod organism;
pub mod task_adapter;
pub mod test_cpu;
pub mod tracer;

pub use code_label::CodeLabel;
pub use cpu_stack::CpuStack;
pub use hardware::Hardware;
pub use hardware_cpu::HardwareCpu;
pub use hardware_manager::HardwareManager;
pub use hardware_transsmt::HardwareTransSmt;
pub use head_cpu::HeadCpu;
pub use org_interface::{OrgInterface, WorldServices};
pub use organism::{OrgState, Organism};
pub use tracer::{HardwareStatusPrinter, HardwareTracer, StringTracer};
