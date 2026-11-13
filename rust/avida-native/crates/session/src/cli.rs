//! Command-line options shared by the programs that record a run
//! (`avida-rec`, `avida-gui`, `avida-tui`): `--record FILE`,
//! `--record-every N`, `--record-layers LIST` and `--record-top N`.

use crate::{LayerId, RecordSpec, DEFAULT_RECORD_EVERY, DEFAULT_RECORD_TOP_N};

/// The usage lines of the recording options.
pub const RECORD_USAGE: &str = concat!(
    "    --record FILE         record the run to FILE\n",
    "    --record-every N      capture a frame every N updates (default 100)\n",
    "    --record-layers LIST  comma-separated map layers to record, such as\n",
    "                          genotype,fitness,ancestry,task[0],resource[1]\n",
    "                          (default: every layer the world has)\n",
    "    --record-top N        genotypes in the table of each frame (default 100)",
);

/// Removes `name VALUE` and `name=VALUE` from `args`, wherever they stand,
/// and returns the last value. `Err` when the value is missing.
pub fn take_option(args: &mut Vec<String>, name: &str) -> Result<Option<String>, String> {
    let mut value = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == name {
            if i + 1 >= args.len() {
                return Err(format!("{name} needs a value"));
            }
            value = Some(args.remove(i + 1));
            args.remove(i);
        } else if let Some(v) = args[i]
            .strip_prefix(name)
            .and_then(|rest| rest.strip_prefix('='))
        {
            value = Some(v.to_string());
            args.remove(i);
        } else {
            i += 1;
        }
    }
    Ok(value)
}

/// Removes the recording options from `args`. `Ok(None)` when `--record`
/// is absent (and no other recording option is given).
pub fn take_record_options(args: &mut Vec<String>) -> Result<Option<RecordSpec>, String> {
    let path = take_option(args, "--record")?;
    let every = take_option(args, "--record-every")?;
    let layers = take_option(args, "--record-layers")?;
    let top = take_option(args, "--record-top")?;
    let Some(path) = path else {
        if every.is_some() || layers.is_some() || top.is_some() {
            return Err(
                "--record-every, --record-layers and --record-top need --record FILE".into(),
            );
        }
        return Ok(None);
    };
    let mut spec = RecordSpec::new(path);
    spec.every = match every {
        None => DEFAULT_RECORD_EVERY,
        Some(v) => v.parse::<u32>().ok().filter(|&n| n >= 1).ok_or_else(|| {
            format!("--record-every: {v:?} is not a whole number of updates, at least 1")
        })?,
    };
    spec.genotypes_top_n = match top {
        None => DEFAULT_RECORD_TOP_N,
        Some(v) => v
            .parse::<usize>()
            .ok()
            .filter(|&n| n >= 1)
            .ok_or_else(|| format!("--record-top: {v:?} is not a whole number, at least 1"))?,
    };
    if let Some(list) = layers {
        spec.layers = Some(
            list.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| {
                    layer_from_key(s).ok_or_else(|| format!("--record-layers: unknown layer {s:?}"))
                })
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    Ok(Some(spec))
}

/// The layer a [`LayerId::key`] names (`fitness`, `task[3]`,
/// `resource[0]`).
pub fn layer_from_key(key: &str) -> Option<LayerId> {
    let indexed = |prefix: &str| -> Option<usize> {
        key.strip_prefix(prefix)?
            .strip_prefix('[')?
            .strip_suffix(']')?
            .parse()
            .ok()
    };
    if let Some(k) = indexed("task") {
        return Some(LayerId::Task(k));
    }
    if let Some(r) = indexed("resource") {
        return Some(LayerId::Resource(r));
    }
    LayerId::FIXED.iter().copied().find(|l| l.key() == key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn options_are_taken_wherever_they_stand() {
        let mut a = v(&["cfg", "--record", "x.rec", "-s", "7", "--record-every=50"]);
        let spec = take_record_options(&mut a).unwrap().unwrap();
        assert_eq!(a, v(&["cfg", "-s", "7"]));
        assert_eq!(spec.path.to_str(), Some("x.rec"));
        assert_eq!(spec.every, 50);
        assert_eq!(spec.layers, None);
    }

    #[test]
    fn bad_values_are_named() {
        assert!(take_record_options(&mut v(&["--record"])).is_err());
        assert!(take_record_options(&mut v(&["--record", "x", "--record-every", "0"])).is_err());
        assert!(take_record_options(&mut v(&["--record-every", "5"])).is_err());
        assert!(
            take_record_options(&mut v(&["--record", "x", "--record-layers", "nope"])).is_err()
        );
        assert_eq!(take_record_options(&mut v(&["-s", "1"])), Ok(None));
    }

    #[test]
    fn layer_names_round_trip() {
        for l in LayerId::FIXED
            .into_iter()
            .chain([LayerId::Task(3), LayerId::Resource(0)])
        {
            assert_eq!(layer_from_key(&l.key()), Some(l));
        }
        let mut a = v(&[
            "--record",
            "x",
            "--record-layers",
            "fitness, task[2],ancestry",
        ]);
        let spec = take_record_options(&mut a).unwrap().unwrap();
        assert_eq!(
            spec.layers,
            Some(vec![LayerId::Fitness, LayerId::Task(2), LayerId::Ancestry])
        );
    }
}
