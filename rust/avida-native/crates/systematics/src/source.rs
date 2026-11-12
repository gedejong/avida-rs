//! `Avida::Systematics::Source` and `TransmissionType`
//! (avida-core/include/public/avida/systematics/Unit.h, Types.h;
//! avida-core/source/systematics/Unit.cc).

/// `Systematics::TransmissionType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum TransmissionType {
    /// `UNKNOWN` (0)
    #[default]
    Unknown = 0,
    /// `DIVISION` (1)
    Division = 1,
    /// `DUPLICATION` (2)
    Duplication = 2,
    /// `VERTICAL` (3)
    Vertical = 3,
    /// `HORIZONTAL` (4)
    Horizontal = 4,
}

impl TransmissionType {
    /// `(int)transmission_type`.
    pub fn as_int(self) -> i32 {
        self as i32
    }

    /// Parasite transmission (`VERTICAL` or `HORIZONTAL`).
    pub fn is_parasite(self) -> bool {
        matches!(
            self,
            TransmissionType::Vertical | TransmissionType::Horizontal
        )
    }
}

/// `Systematics::Source`: how a unit came to be.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Source {
    /// `transmission_type`
    pub transmission_type: TransmissionType,
    /// `external`: injected from outside rather than born.
    pub external: bool,
    /// `arguments` (e.g. a parasite's inject label).
    pub arguments: String,
}

impl Source {
    /// `Source(TransmissionType t, const Apto::String& a, bool e = false)`.
    pub fn new(transmission_type: TransmissionType, arguments: &str, external: bool) -> Self {
        Source {
            transmission_type,
            external,
            arguments: arguments.to_string(),
        }
    }

    /// `Source(const Apto::String& sourceString, const Apto::String& a)`:
    /// parses the `"div:int"` form written by [`Source::as_string`]. A string
    /// without `:` leaves the default (`UNKNOWN`, internal) and prints
    /// `incorrectly formatted Source string: <s>` to stderr, as the C++ does;
    /// an unknown type word also leaves `UNKNOWN`. The arguments are taken
    /// verbatim.
    pub fn from_strings(source_string: &str, arguments: &str) -> Self {
        let mut src = Source {
            transmission_type: TransmissionType::Unknown,
            external: false,
            arguments: arguments.to_string(),
        };
        let Some(pos) = source_string.find(':') else {
            eprintln!("incorrectly formatted Source string: {source_string}");
            return src;
        };
        let transmission = &source_string[..pos];
        let external = &source_string[pos + 1..];
        src.transmission_type = match transmission {
            "div" => TransmissionType::Division,
            "dup" => TransmissionType::Duplication,
            "horz" => TransmissionType::Horizontal,
            "vert" => TransmissionType::Vertical,
            _ => TransmissionType::Unknown,
        };
        src.external = external == "ext";
        src
    }

    /// `AsString()`: `"<type>:<ext|int>"`; the arguments are not included.
    pub fn as_string(&self) -> String {
        let t = match self.transmission_type {
            TransmissionType::Division => "div",
            TransmissionType::Duplication => "dup",
            TransmissionType::Horizontal => "horz",
            TransmissionType::Vertical => "vert",
            TransmissionType::Unknown => "unknown",
        };
        format!("{t}:{}", if self.external { "ext" } else { "int" })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for t in [
            TransmissionType::Unknown,
            TransmissionType::Division,
            TransmissionType::Duplication,
            TransmissionType::Vertical,
            TransmissionType::Horizontal,
        ] {
            for ext in [false, true] {
                let s = Source::new(t, "lbl", ext);
                let back = Source::from_strings(&s.as_string(), "lbl");
                assert_eq!(back, s);
            }
        }
        assert_eq!(
            Source::new(TransmissionType::Division, "", true).as_string(),
            "div:ext"
        );
        let bad = Source::from_strings("div", "x");
        assert_eq!(bad.transmission_type, TransmissionType::Unknown);
        assert!(!bad.external);
        assert_eq!(
            Source::from_strings("foo:ext", "").transmission_type,
            TransmissionType::Unknown
        );
    }
}
