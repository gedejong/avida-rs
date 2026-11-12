//! A new experiment from Avida's default configuration.
//!
//! The files of `avida-core/support/config` (the configuration the C++
//! build installs next to the `avida` binary) are compiled into the
//! program, so a new experiment needs nothing on disk and works the same
//! from an app bundle. Only the top-level files are bundled: the
//! configuration, events and environment, every instruction set and the
//! matching ancestor organisms. `misc/` (alternative environments and
//! instruction sets) is left out.

use std::path::{Path, PathBuf};

/// One bundled file.
#[derive(Clone, Copy, Debug)]
pub struct BundledFile {
    pub name: &'static str,
    pub bytes: &'static [u8],
}

macro_rules! bundled {
    ($($name:literal),* $(,)?) => {
        &[$(BundledFile {
            name: $name,
            bytes: include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../../avida-core/support/config/",
                $name
            )),
        }),*]
    };
}

/// Avida's default configuration, as `avida-core/support/config` holds it.
pub const DEFAULT_CONFIG: &[BundledFile] = bundled!(
    "avida.cfg",
    "events.cfg",
    "environment.cfg",
    "analyze.cfg",
    "instset-heads.cfg",
    "default-heads.org",
    "instset-heads-sex.cfg",
    "default-heads-sex.org",
    "instset-smt.cfg",
    "default-smt.org",
    "instset-transsmt.cfg",
    "default-transsmt.org",
    "instset-gx.cfg",
    "default-gx.org",
    "instset-experimental.cfg",
    "experimental.org",
);

/// The folder name a new experiment gets inside a folder that is not
/// empty.
pub const FOLDER_NAME: &str = "avida-experiment";

/// Where a new experiment chosen at `chosen` goes: `chosen` itself when it
/// is empty or does not exist yet, otherwise a new `avida-experiment`
/// folder inside it (`avida-experiment-2`, ... when taken).
pub fn target_folder(chosen: &Path) -> PathBuf {
    let empty = match std::fs::read_dir(chosen) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => !chosen.exists(),
    };
    if empty {
        return chosen.to_path_buf();
    }
    (1..)
        .map(|n| {
            if n == 1 {
                chosen.join(FOLDER_NAME)
            } else {
                chosen.join(format!("{FOLDER_NAME}-{n}"))
            }
        })
        .find(|p| !p.exists())
        .unwrap_or_else(|| chosen.join(FOLDER_NAME))
}

/// Writes the default configuration into `dir` (created if needed) and
/// returns `dir`. Nothing is overwritten: when any of the files exists
/// already, nothing is written and the error says which.
pub fn create_experiment(dir: &Path) -> Result<PathBuf, String> {
    let taken: Vec<&str> = DEFAULT_CONFIG
        .iter()
        .filter(|f| dir.join(f.name).exists())
        .map(|f| f.name)
        .collect();
    if let Some(first) = taken.first() {
        let more = match taken.len() {
            1 => String::new(),
            2 => " and 1 other file".to_string(),
            n => format!(" and {} other files", n - 1),
        };
        return Err(format!(
            "{} already has {first}{more}. Choose an empty folder so nothing is overwritten.",
            dir.display()
        ));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("Could not create {}: {e}", dir.display()))?;
    for f in DEFAULT_CONFIG {
        let path = dir.join(f.name);
        std::fs::write(&path, f.bytes)
            .map_err(|e| format!("Could not write {}: {e}", path.display()))?;
    }
    Ok(dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("avida-gui-new-{}", std::process::id()))
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_bundle_holds_every_top_level_file_of_the_default_config() {
        let source =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core/support/config");
        let mut on_disk: Vec<String> = std::fs::read_dir(&source)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        on_disk.sort();
        let mut bundled: Vec<String> = DEFAULT_CONFIG.iter().map(|f| f.name.to_string()).collect();
        bundled.sort();
        assert_eq!(bundled, on_disk);
        for f in DEFAULT_CONFIG {
            assert_eq!(std::fs::read(source.join(f.name)).unwrap(), f.bytes);
        }
    }

    #[test]
    fn creates_the_files_and_never_overwrites() {
        let dir = scratch("create").join("exp");
        assert_eq!(create_experiment(&dir).unwrap(), dir);
        assert!(dir.join("avida.cfg").is_file());
        assert!(dir.join("default-heads.org").is_file());
        let err = create_experiment(&dir).unwrap_err();
        assert!(
            err.contains("already has avida.cfg and 15 other files"),
            "{err}"
        );
    }

    #[test]
    fn a_folder_that_is_not_empty_gets_a_new_subfolder() {
        let dir = scratch("target");
        assert_eq!(target_folder(&dir), dir, "missing: used as is");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(target_folder(&dir), dir, "empty: used as is");
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        assert_eq!(target_folder(&dir), dir.join(FOLDER_NAME));
        std::fs::create_dir_all(dir.join(FOLDER_NAME)).unwrap();
        assert_eq!(target_folder(&dir), dir.join("avida-experiment-2"));
    }
}
