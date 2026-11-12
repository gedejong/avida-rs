//! Errors of analyze mode.

use std::fmt;

/// Why an analyze command stopped.
#[derive(Debug, Clone, PartialEq)]
pub enum AnalyzeError {
    /// The C++ calls `exit(code)` here (after printing its message, which
    /// has already been written to the error stream). The driver should
    /// flush its streams and exit with this code.
    Exit(i32),
    /// The command or value needs a part of Avida this crate does not port
    /// (test CPU, world, action library). `cpp` names the C++ function.
    Unsupported { what: String, cpp: &'static str },
    /// The C++ has undefined behaviour or crashes here (NULL dereference,
    /// out-of-range index, endless loop). Fails loudly instead.
    Fatal(String),
    /// The world or the test CPU stopped: an `Err` of `avida-hardware` (an
    /// unported feature reached while running, or a C++ `exit()`), or a
    /// start-up failure of the world. The message is the one the lower
    /// layer produced (`unsupported: ...` for missing ports).
    World(String),
}

impl AnalyzeError {
    /// An [`AnalyzeError::Unsupported`] for `what`, implemented in C++ by `cpp`.
    pub fn unsupported(what: impl Into<String>, cpp: &'static str) -> Self {
        AnalyzeError::Unsupported {
            what: what.into(),
            cpp,
        }
    }

    /// An [`AnalyzeError::Fatal`].
    pub fn fatal(msg: impl Into<String>) -> Self {
        AnalyzeError::Fatal(msg.into())
    }

    /// An [`AnalyzeError::World`] from a lower layer's `Err(String)`.
    pub fn world(msg: impl Into<String>) -> Self {
        AnalyzeError::World(msg.into())
    }
}

impl fmt::Display for AnalyzeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnalyzeError::Exit(c) => write!(f, "analyze mode exited with code {c}"),
            AnalyzeError::Unsupported { what, cpp } => {
                write!(f, "unsupported analyze command '{what}' (C++ {cpp})")
            }
            AnalyzeError::Fatal(m) => write!(f, "analyze mode: {m}"),
            AnalyzeError::World(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for AnalyzeError {}

impl From<std::io::Error> for AnalyzeError {
    fn from(e: std::io::Error) -> Self {
        AnalyzeError::Fatal(format!("I/O error: {e}"))
    }
}

/// Result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, AnalyzeError>;
