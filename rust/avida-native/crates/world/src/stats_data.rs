//! `cStats::m_data_manager` (the `tDataManager<cStats>` entries registered
//! in `cStats::setupProvidedData`) and `cStats::PrintDataFile`, which prints
//! a row of named entries (`cDataManager_Base::PrintRow`), at 47f13dadb.

use avida_apto::fmt::fmt_double;
use avida_apto::OutputManager;
use avida_config::string_util::{compress_whitespace, pop};

use crate::stats::Stats;

/// The value of one data entry (`cFlexVar` of an `int` or `double`
/// getter).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EntryValue {
    Int(i32),
    Double(f64),
}

impl EntryValue {
    /// `fp << cFlexVar`: `ostream <<` of the held value.
    pub fn print(self) -> String {
        match self {
            EntryValue::Int(v) => v.to_string(),
            EntryValue::Double(v) => fmt_double(v),
        }
    }
}

impl Stats {
    /// `m_data_manager.Get(name)`: the description and current value of a
    /// registered entry. Names are case-sensitive.
    pub fn data_entry(&self, name: &str) -> Option<(&'static str, EntryValue)> {
        use EntryValue::{Double, Int};
        Some(match name {
            // Time Stats
            "update" => ("Update", Int(self.m_update)),
            "generation" => ("Generation", Double(self.sum_generation.average())),
            // Population Level Stats
            "num_resamplings" => (
                "Total Number of resamplings this time step",
                Int(self.num_resamplings),
            ),
            "num_failedResamplings" => (
                "Total Number of divide commands that reached the resampling hard-cap this time step",
                Int(self.num_failed_resamplings),
            ),
            // Current Counts...
            "num_births" => ("Count of Births in Population", Int(self.num_births)),
            "cumulative_births" => ("Total Births over Time", Int(self.cumulative_births)),
            "num_deaths" => ("Count of Deaths in Population", Int(self.num_deaths)),
            "breed_in" => ("Count of Non-Breed-True Births", Int(self.num_breed_in)),
            "breed_true" => ("Count of Breed-True Births", Int(self.num_breed_true)),
            "bred_true" => (
                "Count of Organisms that have Bred True",
                Int(self.num_breed_true_creatures),
            ),
            "num_cpus" => ("Count of Organisms in Population", Int(self.num_creatures)),
            "num_parasites" => ("Count of Parasites in Population", Int(self.num_parasites)),
            "threads" => ("Count of Threads in Population", Int(self.m_num_threads)),
            "num_no_birth" => (
                "Count of Childless Organisms",
                Int(self.num_no_birth_creatures),
            ),
            // Total Counts...
            "tot_cpus" => ("Total Organisms ever in Population", Int(self.tot_organisms)),
            // Some Average Data...
            "ave_repro_rate" => (
                "Average Repro-Rate (1/Gestation)",
                Double(self.sum_repro_rate.average()),
            ),
            "ave_merit" => ("Average Merit", Double(self.sum_merit.average())),
            "ave_age" => ("Average Age", Double(self.sum_creature_age.average())),
            "ave_memory" => ("Average Memory Used", Double(self.sum_mem_size.average())),
            "ave_neutral" => (
                "Average Neutral Metric",
                Double(self.sum_neutral_metric.average()),
            ),
            "ave_lineage" => (
                "Average Lineage Label",
                Double(self.sum_lineage_label.average()),
            ),
            "ave_gest" => ("Average Gestation Time", Double(self.sum_gestation.average())),
            "ave_fitness" => ("Average Fitness", Double(self.sum_fitness.average())),
            "ave_copy_length" => ("Average Copied Length", Double(self.sum_copy_size.average())),
            "ave_exe_length" => ("Average Executed Length", Double(self.sum_exe_size.average())),
            "ave_speculative" => (
                "Averate Speculative Instructions",
                Double(if self.m_spec_num != 0 {
                    f64::from(self.m_spec_total) / f64::from(self.m_spec_num)
                } else {
                    0.0
                }),
            ),
            "speculative_waste" => ("Speculative Execution Waste", Int(self.m_spec_waste)),
            // Maximums
            "max_fitness" => ("Maximum Fitness in Population", Double(self.max_fitness)),
            "max_merit" => ("Maximum Merit in Population", Double(self.max_merit)),
            // Minimums
            "min_fitness" => ("Minimum Fitness in Population", Double(self.min_fitness)),
            _ => return None,
        })
    }

    /// `cStats::PrintDataFile(filename, format, sep)`: one row of the
    /// entries named in `format` (separated by `sep`), after a legend on
    /// first use. Unknown names are skipped with a message on stderr, which
    /// is returned in the second element (`cDataManager_Base::PrintRow`
    /// prints it with `cerr`).
    pub fn print_data_file(
        &self,
        out: &mut OutputManager,
        filename: &str,
        format: &str,
        sep: char,
    ) -> Result<String, String> {
        let mut errors = String::new();
        let df = out.static_with_path(filename)?;
        let mut row_entries = format.to_string();
        compress_whitespace(&mut row_entries);

        // If we haven't output the header for this file yet, do so.
        if !df.header_done() {
            // Make sure its space seperated...
            let format_line: String = row_entries
                .chars()
                .map(|c| if c == sep { ' ' } else { c })
                .collect();
            df.write_raw_comment("#filetype population_data");
            df.write_raw_comment(&format!("#format {format_line}"));

            // Setup the human-readable description...
            let mut header_entries = row_entries.clone();
            df.write_comment(" ");
            df.write_comment("Legend:");
            while !header_entries.is_empty() {
                let cur_entry = pop(&mut header_entries, sep);
                if let Some((desc, _)) = self.data_entry(&cur_entry) {
                    df.write_column_desc(desc, "");
                }
            }
            df.endl();
        }

        let mut line = String::new();
        while !row_entries.is_empty() {
            let cur_entry = pop(&mut row_entries, sep);
            match self.data_entry(&cur_entry) {
                Some((_, value)) => {
                    line.push_str(&value.print());
                    line.push(' ');
                }
                None => {
                    errors.push_str(&format!(
                        "Data manager unable to find entry '{cur_entry}'\n"
                    ));
                }
            }
        }
        line.push('\n');
        let fp = df.ofstream();
        fp.write_all(line.as_bytes())
            .and_then(|()| fp.flush())
            .map_err(|e| e.to_string())?;
        Ok(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_have_cpp_descriptions() {
        let mut s = Stats::new(vec![], vec![], 0, 0.0);
        s.m_update = 20;
        s.num_creatures = 5;
        assert_eq!(
            s.data_entry("update"),
            Some(("Update", EntryValue::Int(20)))
        );
        assert_eq!(
            s.data_entry("num_cpus"),
            Some(("Count of Organisms in Population", EntryValue::Int(5)))
        );
        assert_eq!(
            s.data_entry("ave_fitness").map(|e| e.0),
            Some("Average Fitness")
        );
        assert!(s.data_entry("Update").is_none());
        assert_eq!(EntryValue::Double(0.24864864864).print(), "0.248649");
        assert_eq!(EntryValue::Double(0.0).print(), "0");
    }

    #[test]
    fn print_data_file_writes_legend_then_rows() {
        let dir = std::env::temp_dir().join(format!("avida_stats_data_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut out = OutputManager::new(dir.to_str().unwrap());
        let mut s = Stats::new(vec![], vec![], 0, 0.0);
        s.m_update = 0;
        s.num_creatures = 1;
        let errs = s
            .print_data_file(&mut out, "p.dat", "update,generation,num_cpus,bogus", ',')
            .unwrap();
        assert_eq!(errs, "Data manager unable to find entry 'bogus'\n");
        s.m_update = 20;
        s.print_data_file(&mut out, "p.dat", "update,generation,num_cpus", ',')
            .unwrap();
        out.flush_all();
        drop(out);
        let text = std::fs::read_to_string(dir.join("p.dat")).unwrap();
        assert_eq!(
            text,
            "#filetype population_data\n#format update generation num_cpus bogus\n#  \n\
             # Legend:\n#  1: Update\n#  2: Generation\n#  3: Count of Organisms in Population\n\
             \n\n0 0 1 \n20 0 1 \n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
