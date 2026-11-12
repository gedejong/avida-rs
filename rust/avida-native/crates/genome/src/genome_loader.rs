//! Port of `Avida::Util::LoadGenomeDetailFile`
//! (avida-core/source/util/GenomeLoader.cc): load a `.org` file (one
//! instruction name per line, optional `#inst_set` / `#hw_type`
//! directives) into a [`Genome`].

use crate::cstr::{apto_trim, as_int};
use crate::feedback::Feedback;
use crate::genome::Genome;
use crate::inst_set_registry::InstSetRegistry;
use crate::sequence::InstructionSequence;
use avida_config::InitFile;

/// `GetCustomDirectives().Get(name)` of a loaded file.
fn custom_directive<'a>(file: &'a InitFile, name: &str) -> Option<&'a str> {
    file.custom_directives()
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

/// `Util::LoadGenomeDetailFile(fname, wdir, hwm, feedback, specified_instset)`.
///
/// The instruction set is, in order of preference: `specified_instset`
/// (if non-empty), the file's `#inst_set` directive (Apto-trimmed), or the
/// registry's default set. A `#hw_type` directive must match that set's
/// hardware type.
///
/// Returns:
/// - `Ok(Some(genome))` on success;
/// - `Ok(None)` when the load failed; the reasons are in `feedback`
///   (`unable to open file '...'.`, `invalid instruction set '...' defined
///   in organism '...'`, `hardware type mismatch in organism '...': is = N,
///   org = M`), or no instruction set is loaded at all (C++ would crash);
/// - `Err(line)` for an unknown instruction name: `cInstSet::GetInst`
///   prints `line` to stderr and calls `exit(1)`, so the caller must do the
///   same. (Because of that exit, the C++ fallback messages `unable to load
///   organism` / `unknown instruction: .. (best match: ..)` are dead code
///   and not ported.)
pub fn load_genome_detail_file(
    fname: &str,
    wdir: &str,
    hwm: &InstSetRegistry,
    feedback: &mut Feedback,
    specified_instset: &str,
) -> Result<Option<Genome>, String> {
    // cInitFile input_file(fname, wdir, feedback, &custom_directives)
    let custom_directives = ["inst_set".to_string(), "hw_type".to_string()];
    let input_file = InitFile::with_feedback(fname, wdir, feedback, Some(&custom_directives), None);
    if !input_file.was_opened() {
        return Ok(None);
    }

    let mut specified_instset = specified_instset.to_string();
    let directive_inst_set = custom_directive(&input_file, "inst_set");

    let is = if !specified_instset.is_empty() || directive_inst_set.is_some() {
        if specified_instset.is_empty() {
            specified_instset = directive_inst_set.unwrap_or("").to_string();
        }
        let specified_instset = apto_trim(&specified_instset);
        if hwm.is_inst_set(&specified_instset) {
            match hwm.get_inst_set(&specified_instset) {
                Some(is) => is,
                None => return Ok(None),
            }
        } else {
            feedback.error(format!(
                "invalid instruction set '{}' defined in organism '{}'",
                specified_instset, fname
            ));
            return Ok(None);
        }
    } else {
        match hwm.default_inst_set() {
            Some(is) => is,
            None => return Ok(None),
        }
    };

    if let Some(hw) = custom_directive(&input_file, "hw_type") {
        let hw_type = as_int(hw);
        if is.hardware_type() != hw_type {
            feedback.error(format!(
                "hardware type mismatch in organism '{}': is = {}, org = {}",
                fname,
                is.hardware_type(),
                hw_type
            ));
            return Ok(None);
        }
    }

    let hw_type = is.hardware_type();
    let inst_set = is.inst_set_name().to_string();
    let mut new_seq = InstructionSequence::with_size(input_file.num_lines() as i32);
    for line_num in 0..input_file.num_lines() {
        let cur_line = input_file.get_line(line_num as i32);
        new_seq.set(line_num as i32, is.get_inst(&cur_line)?);
    }

    Ok(Some(Genome::with_inst_set(hw_type, &inst_set, new_seq)))
}
