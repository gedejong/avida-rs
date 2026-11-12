//! Ports of `cPopulation::Kaboom` (both overloads) and
//! `cPopulation::SpawnDeme` (avida-core/source/main/cPopulation.cc at
//! 47f13dadb), which the heads CPU reaches through
//! `cPopulationInterface::Kaboom` / `SpawnDeme` from `explode`,
//! `explode1`-`explode5`, `smart-explode`, `agg-SA`, `coop-SA` and
//! `spawn-deme`.
//!
//! The exploding organism is the one being run, so it is not in its cell:
//! its state comes in as `org`, and every lookup of an organism in a cell
//! answers with `org` for its own cell (which a small torus can reach
//! through several offsets).

use avida_apto::fmt::{fmt_g, fmt_g_showpoint};
use avida_hardware::organism::{transmission, OrgSource, OrgState};

use crate::deme_compete::CloneOrigin;
use crate::driver::verbosity;
use crate::topology::grid_neighbor;
use crate::world::World;

/// `genome_temp[i] != ref_genome[i]` counted over the neighbour's genome
/// string, as `(fewest, most)` mismatches. `Apto::String::operator[]` does
/// not check bounds: one position past the reference reads its NUL (a
/// mismatch), positions beyond that read heap memory after the string,
/// which may or may not match; they count as matches in the first value
/// and as mismatches in the second.
fn hamming_bounds(temp: &[u8], reference: &[u8]) -> (i32, i32) {
    let (mut lo, mut hi) = (0, 0);
    for (i, &c) in temp.iter().enumerate() {
        match reference.get(i) {
            Some(&r) => {
                if c != r {
                    lo += 1;
                    hi += 1;
                }
            }
            None if i == reference.len() => {
                lo += 1;
                hi += 1;
            }
            None => hi += 1,
        }
    }
    (lo, hi)
}

/// `diff > distance` when the bytes past the reference string cannot
/// change the answer; `Err` when they decide it.
fn more_distant(temp: &[u8], reference: &[u8], distance: i32) -> Result<bool, String> {
    let (lo, hi) = hamming_bounds(temp, reference);
    if (lo > distance) != (hi > distance) {
        return Err(
            "unsupported: kaboom kin test of a genome longer than the exploding one \
                    (C++: cPopulation::Kaboom reads past the end of the reference genome string)"
                .to_string(),
        );
    }
    Ok(lo > distance)
}

impl World {
    /// `std::cout << v` from inside an organism's step. The status line
    /// of each finished update (printed with `ios::showpoint` and a sticky
    /// precision, see [`World::cout_double`]) precedes the steps of the
    /// next update, so the first update's steps still print `%g`.
    fn step_cout_double(&self, v: f64) -> String {
        if self.stats.update() >= 1 && self.cfg.verbosity > verbosity::SILENT {
            let precision = if self.cfg.verbosity >= verbosity::DEBUG {
                4
            } else {
                7
            };
            fmt_g_showpoint(v, precision)
        } else {
            fmt_g(v, 6)
        }
    }

    /// The exploding organism's cell must still hold it (the C++ reads
    /// `in_cell.GetOrganism()`, NULL once it was killed).
    fn kaboom_check_alive(&self, cell: i32) -> Result<(), String> {
        if !self.is_occupied(cell) {
            return Err("unsupported: Kaboom of an organism no longer in its cell \
                        (C++: cPopulation::Kaboom dereferences NULL)"
                .to_string());
        }
        Ok(())
    }

    /// The genome string of the organism in `cell` (`org` for `own`).
    fn cell_genome_string(&self, cell: i32, own: i32, org: &OrgState) -> Option<Vec<u8>> {
        if cell == own {
            return Some(org.genome().representation().as_string().into_bytes());
        }
        self.pop.orgs[cell as usize]
            .as_ref()
            .map(|o| o.state.genome().representation().as_string().into_bytes())
    }

    /// `organism->SystematicsGroup("genotype")->ID()` of the organism in
    /// `cell` (the running organism's membership stays with its cell while
    /// it lives).
    fn cell_genotype_id(&self, cell: i32) -> Result<i32, String> {
        let m = self
            .pop
            .membership
            .get(cell as usize)
            .ok_or("Kaboom: cell out of range")?;
        self.systematics
            .genotype_of(m)
            .and_then(|g| self.systematics.group_id(g))
            .ok_or_else(|| {
                "unsupported: Kaboom of an organism without a genotype (C++: NULL dereference)"
                    .to_string()
            })
    }

    /// `cPopulation::Kaboom(in_cell, ctx, distance)`: kill every organism
    /// in the `KABOOM_RADIUS` square around `cell` (the cell itself
    /// included) that is not kin, then the exploding organism `org`.
    /// Distance 0 compares genotypes; otherwise kin are those whose genome
    /// string differs in at most `distance` symbols. A negative distance
    /// (`explode` with both settings adjustable) kills the exploding
    /// organism inside the loop already.
    pub fn kaboom(&mut self, cell: i32, org: &mut OrgState, distance: i32) -> Result<(), String> {
        self.stats.inc_kaboom();
        self.stats.add_ham_distance(distance);
        self.kaboom_check_alive(cell)?;
        let ref_genome = org.genome().representation().as_string().into_bytes();
        let bgid = self.cell_genotype_id(cell)?;

        let radius = self.cfg.kaboom_radius;
        let (world_x, world_y) = (self.pop.world_x, self.pop.world_y);
        for i in -radius..=radius {
            for j in -radius..=radius {
                let death_cell = grid_neighbor(cell, world_x, world_y, i, j);

                // do we actually have something to kill?
                if !self.is_occupied(death_cell) {
                    continue;
                }

                if distance == 0 {
                    let temp_id = self.cell_genotype_id(death_cell)?;
                    if temp_id != bgid {
                        self.kill_organism(death_cell, Some(&mut *org));
                        self.stats.inc_kaboom_kills();
                    }
                } else {
                    let genome_temp = self
                        .cell_genome_string(death_cell, cell, org)
                        .ok_or("Kaboom: occupied cell without an organism")?;
                    if more_distant(&genome_temp, &ref_genome, distance)? {
                        self.stats.inc_kaboom_kills();
                        self.kill_organism(death_cell, Some(&mut *org));
                    }
                }
            }
        }
        self.kill_organism(cell, Some(org));
        Ok(())
    }

    /// `cPopulation::Kaboom(in_cell, ctx, distance, effect)`: the
    /// suicidal-altruism variant. Neighbours (not the centre offset) that
    /// are not kin have their merit scaled by `effect` when it is below 1
    /// (killed when that leaves no merit); kin have it scaled up when
    /// `effect` is above 1. The kin/non-kin counts of organisms with and
    /// without the `agg-SA`/`coop-SA` symbol go to the stats, and the
    /// exploding organism dies. The C++ prints its merit arithmetic to
    /// stdout.
    pub fn kaboom_effect(
        &mut self,
        cell: i32,
        org: &mut OrgState,
        distance: i32,
        effect: f64,
    ) -> Result<(), String> {
        self.stats.inc_kaboom();
        self.stats.add_ham_distance(distance);
        self.kaboom_check_alive(cell)?;
        let ref_genome = org.genome().representation().as_string().into_bytes();
        let mut agg_inst = b'Z';
        let mut coop_inst = b'Z';
        let is = self.hw.inst_set_for_genome(org.genome())?;
        // `GetInst` prints the error to stderr and exits(1) for a missing
        // name.
        let first = |name: &str| -> Result<u8, String> {
            let inst = is.get_inst(name)?;
            Ok(inst.symbol().as_bytes().first().copied().unwrap_or(0))
        };
        let found = if effect < 1.0 {
            first("agg-SA").map(|s| agg_inst = s)
        } else {
            first("coop-SA").map(|s| coop_inst = s)
        };
        found.map_err(|line| self.exit_with_stderr_line(line))?;
        let radius = self.cfg.kaboom_radius;
        let (world_x, world_y) = (self.pop.world_x, self.pop.world_y);

        let mut sa_kin_count = 0;
        let mut sa_notkin_count = 0;
        let mut nsa_kin_count = 0;
        let mut nsa_notkin_count = 0;

        for i in -radius..=radius {
            for j in -radius..=radius {
                if i == 0 && j == 0 {
                    continue;
                }
                let death_cell = grid_neighbor(cell, world_x, world_y, i, j);

                // do we actually have something to kill?
                if !self.is_occupied(death_cell) {
                    continue;
                }

                let genome_temp = self
                    .cell_genome_string(death_cell, cell, org)
                    .ok_or("Kaboom: occupied cell without an organism")?;
                let not_kin = more_distant(&genome_temp, &ref_genome, distance)?;
                let sa_org = genome_temp.iter().any(|&c| c == agg_inst || c == coop_inst);

                // Is the SA org correctly ID'd by Hamming distance
                match (sa_org, not_kin) {
                    (true, false) => sa_kin_count += 1,
                    (true, true) => sa_notkin_count += 1,
                    (false, false) => nsa_kin_count += 1,
                    (false, true) => nsa_notkin_count += 1,
                }

                let running_here = death_cell == cell;
                let cur_merit = if running_here {
                    org.phenotype.merit.get_double()
                } else {
                    self.pop.orgs[death_cell as usize]
                        .as_ref()
                        .map_or(0.0, |o| o.state.phenotype.merit.get_double())
                };
                if not_kin && effect < 1.0 {
                    self.stats.inc_kaboom_kills();
                    // Hurting competitors
                    let mut text = format!("before {}\n", self.step_cout_double(cur_merit));
                    let new_merit = cur_merit * effect;
                    text.push_str(&format!("effect is {}\n", self.step_cout_double(effect)));
                    text.push_str(&format!(
                        "new shoudl be {}\n",
                        self.step_cout_double(new_merit)
                    ));
                    self.action_state.stdout.push_str(&text);
                    if new_merit <= 0.0 {
                        self.kill_organism(death_cell, Some(&mut *org));
                        let occupied = i32::from(self.is_occupied(death_cell));
                        self.action_state
                            .stdout
                            .push_str(&format!("dead: {occupied}\n"));
                    } else {
                        self.kaboom_update_merit(death_cell, cell, org, new_merit)?;
                        let after = if running_here {
                            org.phenotype.merit.get_double()
                        } else {
                            self.pop.orgs[death_cell as usize]
                                .as_ref()
                                .map_or(0.0, |o| o.state.phenotype.merit.get_double())
                        };
                        let line = format!("after {}\n", self.step_cout_double(after));
                        self.action_state.stdout.push_str(&line);
                    }
                } else if !not_kin && effect > 1.0 {
                    // Helping kin
                    self.kaboom_update_merit(death_cell, cell, org, cur_merit * effect)?;
                    self.stats.inc_kaboom_kills();
                }
            }
        }
        self.stats.inc_sa_kin(sa_kin_count);
        self.stats.inc_sa_not_kin(sa_notkin_count);
        self.stats.inc_nsa_kin(nsa_kin_count);
        self.stats.inc_nsa_not_kin(nsa_notkin_count);
        self.kill_organism(cell, Some(org));
        Ok(())
    }

    /// `org_temp->UpdateMerit(ctx, new_merit)` for the organism in
    /// `target` (`cPopulation::UpdateMerit(cell, merit)`). A merit of 0
    /// kills it and then dereferences the empty cell in C++; that is the
    /// `Err`.
    fn kaboom_update_merit(
        &mut self,
        target: i32,
        own: i32,
        org: &mut OrgState,
        new_merit: f64,
    ) -> Result<(), String> {
        if new_merit <= 0.0 {
            return Err(
                "unsupported: Kaboom raising the merit of an organism with merit 0 \
                        (C++: cPopulation::UpdateMerit kills it, then dereferences NULL)"
                    .to_string(),
            );
        }
        let state = if target == own { Some(org) } else { None };
        self.update_merit_of(target, state, new_merit);
        Ok(())
    }

    /// `cPopulation::SpawnDeme(deme1_id, ctx, deme2_id = -1)`: empty a
    /// random other deme and inject into one of its cells a clone of a
    /// random organism of deme `deme1_id` (`org` answers for its own
    /// cell). With a single deme, or none of `deme1_id`'s cells occupied,
    /// the C++ loops forever; those are the `Err`.
    pub fn spawn_deme(
        &mut self,
        deme1_id: i32,
        own_cell: i32,
        org: &mut OrgState,
    ) -> Result<(), String> {
        let num_demes = self.pop.demes.len() as u32;
        let d1 = usize::try_from(deme1_id)
            .ok()
            .filter(|&d| d < self.pop.demes.len())
            .ok_or("unsupported: SpawnDeme of a missing deme (C++: assert)")?;
        if num_demes < 2 {
            return Err("unsupported: spawn-deme with a single deme \
                        (C++: cPopulation::SpawnDeme never finds a second deme)"
                .to_string());
        }

        // If the second argument is a -1, choose a deme at random.
        let mut deme2_id = -1i64;
        while deme2_id == -1 || deme2_id == i64::from(deme1_id) {
            deme2_id = i64::from(self.rng.get_uint(num_demes));
        }
        let d2 = deme2_id as usize;

        // Determine the cell to copy from.
        let size1 = self.pop.demes[d1].size();
        if !(0..size1).any(|p| self.is_occupied(self.pop.demes[d1].cell_id(p))) {
            return Err("unsupported: spawn-deme from an empty deme \
                        (C++: cPopulation::SpawnDeme searches it forever)"
                .to_string());
        }
        let mut cell1_id = self.pop.demes[d1].cell_id(self.rng.get_uint(size1 as u32) as i32);
        while !self.is_occupied(cell1_id) {
            cell1_id = self.pop.demes[d1].cell_id(self.rng.get_uint(size1 as u32) as i32);
        }

        // Clear out existing cells in target deme.
        let size2 = self.pop.demes[d2].size();
        for i in 0..size2 {
            let c = self.pop.demes[d2].cell_id(i);
            self.kill_organism(c, Some(&mut *org));
        }

        // And do the spawning.
        let cell2_id = self.pop.demes[d2].cell_id(self.rng.get_uint(size2 as u32) as i32);
        let origin = if cell1_id == own_cell {
            CloneOrigin::of(org)
        } else {
            let o = self.pop.orgs[cell1_id as usize]
                .as_ref()
                .ok_or("SpawnDeme: occupied cell without an organism")?;
            CloneOrigin::of(&o.state)
        };
        if cell2_id < 0 || cell2_id as usize >= self.pop.cells.len() {
            return Err(
                "unsupported: InjectClone into a nonexistent cell (C++: assert)".to_string(),
            );
        }
        if self.cfg.use_avatars != 0 {
            return Err("unsupported: InjectClone with USE_AVATARS (C++: \
                        cPopulation::PlaceAvatar for the clone)"
                .to_string());
        }
        self.inject_clone_from(
            cell2_id,
            origin,
            OrgSource::new(transmission::DUPLICATION, "", false),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hamming_bounds_reads_the_nul_then_unknown_bytes() {
        assert_eq!(hamming_bounds(b"abc", b"abc"), (0, 0));
        assert_eq!(hamming_bounds(b"abd", b"abc"), (1, 1));
        // Shorter neighbour: only its own positions are compared.
        assert_eq!(hamming_bounds(b"ab", b"abc"), (0, 0));
        // One longer: the reference's NUL never matches a symbol.
        assert_eq!(hamming_bounds(b"abcd", b"abc"), (1, 1));
        // Two longer: the second extra byte is past the string.
        assert_eq!(hamming_bounds(b"abcde", b"abc"), (1, 2));
    }

    #[test]
    fn unknown_bytes_stop_only_when_they_decide_the_kin_test() {
        assert_eq!(more_distant(b"abcde", b"abc", 5), Ok(false));
        assert_eq!(more_distant(b"xbcde", b"abc", 0), Ok(true));
        assert!(more_distant(b"abcde", b"abc", 1).is_err());
    }
}
