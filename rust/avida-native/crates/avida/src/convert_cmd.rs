//! `avida convert`: the command that turns a folder of the old
//! configuration format into one experiment file (see
//! `avida_config::convert`).

use std::path::{Path, PathBuf};

use avida_config::convert::{convert_folder, convert_section_file, Options};

pub const USAGE: &str = "\
usage: avida convert [DIR] [options]
       avida convert --section environment|events|instruction_sets FILE [-o OUT]

Writes DIR/avida.yaml (DIR is the current folder when left out) from the old
avida.cfg of DIR with its environment, events and instruction-set files.

  -c FILE          the old configuration file (default avida.cfg)
  -def NAME VALUE  a value for '#include NAME=...' and '$NAME' in the old files
  -set NAME VALUE  an old setting changed before writing (for example
                   '-set EVENT_FILE events-long.cfg' to take other events)
  -o FILE          where to write (default DIR/avida.yaml; '-' for stdout)
  --full           write every setting with its description, not only the
                   ones that differ from their defaults
  --remove-old     delete the old files that were read (inside DIR)
  --force          overwrite an existing output file

With --section, one old environment, events or instruction-set file becomes
a YAML file holding that section, which an experiment file can name
('events: events-long.yaml').
";

/// Runs `avida convert ARGS` (the arguments after `convert`); returns the
/// exit status.
pub fn run(args: &[String], cwd: &str) -> i32 {
    match run_inner(args, cwd) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {}", e.trim_end());
            1
        }
    }
}

fn run_inner(args: &[String], cwd: &str) -> Result<(), String> {
    let mut opts = Options::default();
    let mut dir: Option<String> = None;
    let mut out: Option<String> = None;
    let mut section: Option<(String, String)> = None;
    let mut remove_old = false;
    let mut force = false;
    let mut i = 0;
    let need = |i: usize, n: usize, what: &str| -> Result<(), String> {
        if i + n < args.len() {
            Ok(())
        } else {
            Err(format!("'{what}' needs {n} value(s)\n{USAGE}"))
        }
    };
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            "-c" => {
                need(i, 1, a)?;
                opts.config_file = args[i + 1].clone();
                i += 2;
            }
            "-def" => {
                need(i, 2, a)?;
                opts.defs.push((args[i + 1].clone(), args[i + 2].clone()));
                i += 3;
            }
            "-set" => {
                need(i, 2, a)?;
                opts.sets.retain(|(k, _)| *k != args[i + 1]);
                opts.sets.push((args[i + 1].clone(), args[i + 2].clone()));
                i += 3;
            }
            "-o" => {
                need(i, 1, a)?;
                out = Some(args[i + 1].clone());
                i += 2;
            }
            "--section" => {
                need(i, 2, a)?;
                section = Some((args[i + 1].clone(), args[i + 2].clone()));
                i += 3;
            }
            "--full" => {
                opts.full = true;
                i += 1;
            }
            "--remove-old" => {
                remove_old = true;
                i += 1;
            }
            "--force" => {
                force = true;
                i += 1;
            }
            _ if a.starts_with('-') => return Err(format!("unknown option '{a}'\n{USAGE}")),
            _ => {
                if dir.is_some() {
                    return Err(format!("more than one folder given\n{USAGE}"));
                }
                dir = Some(a.to_string());
                i += 1;
            }
        }
    }
    let dir_path = match &dir {
        Some(d) if Path::new(d).is_absolute() => PathBuf::from(d),
        Some(d) => Path::new(cwd).join(d),
        None => PathBuf::from(cwd),
    };
    let dir_str = dir_path.to_string_lossy().into_owned();

    if let Some((name, file)) = section {
        let text = convert_section_file(&name, &file, &dir_str, &opts.defs)?;
        let target = out.unwrap_or_else(|| {
            let stem = file.strip_suffix(".cfg").unwrap_or(&file);
            format!("{stem}.yaml")
        });
        return write(&dir_path, &target, &text, force);
    }

    let conv = convert_folder(&dir_str, &opts)?;
    for n in &conv.notes {
        eprintln!("note: {n}");
    }
    let target = out.unwrap_or_else(|| avida_config::experiment::DEFAULT_FILE.to_string());
    write(&dir_path, &target, &conv.yaml, force)?;
    if target != "-" {
        eprintln!(
            "wrote {} from {}",
            dir_path.join(&target).display(),
            conv.files_read.join(", ")
        );
    }
    if remove_old {
        for f in &conv.files_read {
            let p = Path::new(f);
            if p.is_absolute() || f.contains("..") {
                eprintln!("note: kept {f} (outside the folder)");
                continue;
            }
            std::fs::remove_file(dir_path.join(p))
                .map_err(|e| format!("cannot remove {f}: {e}"))?;
        }
    }
    Ok(())
}

fn write(dir: &Path, target: &str, text: &str, force: bool) -> Result<(), String> {
    if target == "-" {
        print!("{text}");
        return Ok(());
    }
    let path = if Path::new(target).is_absolute() {
        PathBuf::from(target)
    } else {
        dir.join(target)
    };
    if path.exists() && !force {
        return Err(format!("{} exists (--force overwrites it)", path.display()));
    }
    std::fs::write(&path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
}
