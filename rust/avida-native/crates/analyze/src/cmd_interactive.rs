//! `INTERACTIVE` (`cAnalyze::CommandInteractive` -> `RunInteractive` and
//! `InteractiveLoadCommandList`, cAnalyze.cc at 47f13dadb): read commands
//! from standard input with a `>> ` prompt until `END`, `QUIT` or `EXIT`.
//! The same loop runs after the analyze file in interactive analyze mode
//! (`ANALYZE_MODE 2`, `Avida2Driver::Run`). Standard input can be replaced
//! with [`Analyze::set_input`].

use std::cell::RefCell;
use std::io::{BufRead, Write};
use std::rc::Rc;

use avida_config::{string_util as su, Feedback};

use crate::actions;
use crate::analyze::{Analyze, Command, Def};
use crate::error::{AnalyzeError, Result};
use crate::host::AnalyzeHost;

/// `cin.getline(text_input, 2048)`. The C++ loops are only left by the
/// right command, and a failed `getline` (end of input, or a line of 2048
/// characters or more) leaves the stream failed and the buffer empty, so
/// they would prompt forever; that is an error here.
fn get_line(input: &mut dyn BufRead) -> Result<String> {
    let mut buf = Vec::new();
    let n = input.read_until(b'\n', &mut buf)?;
    if n == 0 {
        return Err(AnalyzeError::fatal(
            "interactive analyze reached the end of standard input (the C++ prompts '>> ' forever)",
        ));
    }
    if buf.last() == Some(&b'\n') {
        buf.pop();
    }
    if buf.len() >= 2048 {
        return Err(AnalyzeError::fatal(
            "INTERACTIVE line of 2048 characters or more (cin.getline fails; the C++ prompts forever)",
        ));
    }
    // `cString(const char*)` stops at the first NUL.
    if let Some(p) = buf.iter().position(|&b| b == 0) {
        buf.truncate(p);
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Where `cin` reads from: the process's standard input, or a reader
/// given with [`Analyze::set_input`] (shared, so that an `INTERACTIVE`
/// run from inside an interactive session reads on from the same place).
pub(crate) type InputSource = Option<Rc<RefCell<Box<dyn BufRead>>>>;

impl Analyze {
    /// Read the commands of interactive mode from `input` instead of the
    /// process's standard input.
    pub fn set_input(&mut self, input: Box<dyn BufRead>) {
        self.input = Some(Rc::new(RefCell::new(input)));
    }

    /// `cin.getline(text_input, 2048)` from the current input.
    fn read_input_line(&self) -> Result<String> {
        match &self.input {
            Some(input) => get_line(&mut **input.borrow_mut()),
            None => get_line(&mut std::io::stdin().lock()),
        }
    }

    /// Whether `command` names a flow command (`FUNCTION`, `FOREACH`,
    /// `FORRANGE`), whose body follows up to `END`.
    fn is_flow_command(&self, host: &dyn AnalyzeHost, command: &str) -> bool {
        matches!(self.find_def(host, command), Some(Def::Builtin(b)) if b.is_flow())
    }

    /// `InteractiveLoadCommandList(clist)`: the body of a flow command,
    /// prompted with one more `>>` per nesting level.
    fn interactive_load_command_list(
        &mut self,
        host: &dyn AnalyzeHost,
        depth: usize,
        clist: &mut Vec<Command>,
    ) -> Result<()> {
        loop {
            write!(self.out, "{} ", ">>".repeat(depth + 1))?;
            self.out.flush()?;
            let mut cur_input = self.read_input_line()?;
            let command = su::pop_word(&mut cur_input);
            if command == "END" {
                return Ok(());
            }
            let mut body = Vec::new();
            if self.is_flow_command(host, &command) {
                self.interactive_load_command_list(host, depth + 1, &mut body)?;
            }
            clist.push(Command {
                name: command,
                args: cur_input,
                body,
            });
        }
    }

    /// `RunInteractive()`: run commands typed on standard input. Unlike a
    /// script, an unknown command or an action's error is reported and
    /// the session goes on.
    pub(crate) fn run_interactive(&mut self, host: &mut dyn AnalyzeHost) -> Result<()> {
        let saved_analyze = host.analyze_mode();
        host.set_analyze_mode(true);
        writeln!(self.out, "Entering interactive mode...")?;
        loop {
            write!(self.out, ">> ")?;
            self.out.flush()?;
            let mut cur_input = self.read_input_line()?;
            let command = su::pop_word(&mut cur_input);
            if command.is_empty() {
                continue;
            }
            if command == "END" || command == "QUIT" || command == "EXIT" {
                break;
            }
            let mut body = Vec::new();
            if self.is_flow_command(host, &command) {
                self.interactive_load_command_list(host, 1, &mut body)?;
            }
            let mut args = cur_input;
            self.pre_process_args(&mut args)?;
            match self.find_def(host, &command) {
                Some(Def::Builtin(b)) => self.run_builtin(host, b, args, &mut body)?,
                Some(Def::Action) => {
                    let mut fb = Feedback::new();
                    if let Some(native) = actions::native_action(&command) {
                        self.run_native_action(host, native, &args, &mut fb)?;
                    } else {
                        host.run_action(&command, &args, &mut fb, &mut *self.out)?;
                    }
                    for i in 0..fb.num_messages() {
                        writeln!(self.err, "{}", fb.format_message(i))?;
                    }
                }
                None => {
                    if !self.function_run(host, &command, args)? {
                        writeln!(self.err, "Error: Unknown command '{command}'.")?;
                    }
                }
            }
        }
        if !saved_analyze {
            host.set_analyze_mode(false);
        }
        Ok(())
    }
}
