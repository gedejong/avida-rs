//! The `Process` half of the mating-type print actions of PrintActions.cc
//! at 47f13dadb (`cActionPrintMatingTypeHistogram`,
//! `cActionPrintBirthChamberMatingTypeHistogram`,
//! `cActionPrintMatingDisplayData`, `cActionPrintFemaleMatePreferenceData`,
//! `cActionPrintBirthChamber`).

use avida_phenotype::phenotype::MATING_TYPE_FEMALE;

use crate::birth_chamber::BirthEntry;
use crate::world::World;

/// `type_counts[mating_type + 1]++`; a mating type outside -1..=1 would
/// index past the C++ array and is not counted here.
fn bump(counts: &mut [i32], index: i32) {
    if let Some(c) = usize::try_from(index).ok().and_then(|i| counts.get_mut(i)) {
        *c += 1;
    }
}

impl World {
    /// `cActionPrintMatingTypeHistogram::Process`: living organisms per
    /// mating type.
    pub(crate) fn print_mating_type_histogram(&mut self, filename: &str) -> Result<(), String> {
        let mut type_counts = [0i32; 3];
        let update = self.stats.update();
        for org in self.pop.orgs.iter().flatten() {
            bump(&mut type_counts, org.state.phenotype.mating_type + 1);
        }
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Avida population mating type histogram");
        df.write_time_stamp();
        df.write(update, "Update");
        df.write(type_counts[0], "Mating type -1 (juvenile)");
        df.write(type_counts[1], "Mating type 0 (female)");
        df.write(type_counts[2], "Mating type 1 (male)");
        df.endl();
        Ok(())
    }

    /// `cActionPrintBirthChamberMatingTypeHistogram::Process`: waiting
    /// offspring per mating type.
    pub(crate) fn print_birth_chamber_mating_type_histogram(
        &mut self,
        filename: &str,
        hw_type: i32,
    ) -> Result<(), String> {
        let update = self.stats.update();
        // The C++ opens the file and writes the update column first; the
        // counts do not depend on the file.
        let type_counts = [
            self.birth_chamber_waiting_offspring_number(-1, hw_type),
            self.birth_chamber_waiting_offspring_number(0, hw_type),
            self.birth_chamber_waiting_offspring_number(1, hw_type),
        ];
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Avida birth chamber mating type histogram");
        df.write_time_stamp();
        df.write(update, "Update");
        df.write(type_counts[0], "Mating type -1 (juvenile)");
        df.write(type_counts[1], "Mating type 0 (female)");
        df.write(type_counts[2], "Mating type 1 (male)");
        df.endl();
        Ok(())
    }

    /// `cActionPrintMatingDisplayData::Process`: average current mating
    /// displays A and B per mating type.
    pub(crate) fn print_mating_display_data(&mut self, filename: &str) -> Result<(), String> {
        // [0-2] = display A values for juvenile/undefined mating type,
        // females, and males; [3-5] = display B values for each sex
        let mut display_sums = [0i32; 6];
        let mut mating_type_sums = [0i32; 3];
        let mut display_avgs = [0.0f64; 6];
        for org in self.pop.orgs.iter().flatten() {
            let ph = &org.state.phenotype;
            let t = ph.mating_type;
            bump(&mut mating_type_sums, t + 1);
            if let Some(s) = usize::try_from(t + 1)
                .ok()
                .and_then(|i| display_sums.get_mut(i))
            {
                *s = s.wrapping_add(ph.cur_mating_display_a);
            }
            if let Some(s) = usize::try_from(t + 4)
                .ok()
                .and_then(|i| display_sums.get_mut(i))
            {
                *s = s.wrapping_add(ph.cur_mating_display_b);
            }
        }
        for i in 0..6 {
            let n = mating_type_sums[i % 3];
            if n > 0 {
                display_avgs[i] = f64::from(display_sums[i]) / f64::from(n);
            }
        }

        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Avida population mating display data");
        df.write_time_stamp();
        df.write(update, "Update");
        df.write(
            display_avgs[0],
            "Avg mating display A for mating type -1 (undefined)",
        );
        df.write(
            display_avgs[1],
            "Avg mating display A for mating type 0 (female)",
        );
        df.write(
            display_avgs[2],
            "Avg mating display A for mating type 1 (male)",
        );
        df.write(
            display_avgs[3],
            "Avg mating display B for mating type -1 (undefined)",
        );
        df.write(
            display_avgs[4],
            "Avg mating display B for mating type 0 (female)",
        );
        df.write(
            display_avgs[5],
            "Avg mating display B for mating type 1 (male)",
        );
        df.endl();
        Ok(())
    }

    /// `cActionPrintFemaleMatePreferenceData::Process`: females per mate
    /// preference.
    pub(crate) fn print_female_mate_preference_data(
        &mut self,
        filename: &str,
    ) -> Result<(), String> {
        let mut mate_pref_sums = [0i32; 4];
        for org in self.pop.orgs.iter().flatten() {
            let ph = &org.state.phenotype;
            if ph.mating_type == MATING_TYPE_FEMALE {
                bump(&mut mate_pref_sums, ph.mate_preference);
            }
        }
        let update = self.stats.update();
        let df = self.out.static_with_path(filename)?;
        df.write_comment("Avida population female mate preference histogram");
        df.write_time_stamp();
        df.write(update, "Update");
        df.write(mate_pref_sums[0], "Random");
        df.write(mate_pref_sums[1], "Highest display A");
        df.write(mate_pref_sums[2], "Highest display B");
        df.write(mate_pref_sums[3], "Highest merit");
        df.endl();
        Ok(())
    }

    /// `cActionPrintBirthChamber::Process` and
    /// `cBirthMatingTypeGlobalHandler::PrintBirthChamber`: every valid
    /// waiting offspring, one phenotype string per line, in a new file.
    /// The other selection handlers print nothing.
    pub(crate) fn print_birth_chamber(
        &mut self,
        filename: &str,
        hw_type: i32,
    ) -> Result<(), String> {
        let filename = if filename.is_empty() {
            format!("birth_chamber/bc-{}.dat", self.stats.update())
        } else {
            filename.to_string()
        };
        let Some(lines) = self.birth_chamber_lines(hw_type) else {
            return Ok(());
        };
        let mut df = self.out.create_with_path(&filename)?;
        df.write_time_stamp();
        df.write_comment(BirthEntry::phenotype_string_format());
        df.endl();
        let mut text = String::new();
        for line in lines {
            text.push_str(&line);
            text.push('\n');
        }
        df.ofstream()
            .write_all(text.as_bytes())
            .map_err(|e| e.to_string())?;
        df.flush();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::bump;

    #[test]
    fn bump_ignores_out_of_range_types() {
        let mut c = [0; 3];
        bump(&mut c, 0);
        bump(&mut c, 2);
        bump(&mut c, 2);
        bump(&mut c, 3);
        bump(&mut c, -1);
        assert_eq!(c, [1, 0, 2]);
    }
}
