//! Sorting the genotype table.

use std::cmp::Ordering;

use avida_viewmodel::GenotypeRow;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum GenoColumn {
    Name,
    Abundance,
    Fitness,
    Merit,
    Gestation,
    Depth,
    Length,
    Born,
}

impl GenoColumn {
    pub const ALL: [GenoColumn; 8] = [
        GenoColumn::Name,
        GenoColumn::Abundance,
        GenoColumn::Fitness,
        GenoColumn::Merit,
        GenoColumn::Gestation,
        GenoColumn::Depth,
        GenoColumn::Length,
        GenoColumn::Born,
    ];

    pub fn title(self) -> &'static str {
        match self {
            GenoColumn::Name => "Genotype",
            GenoColumn::Abundance => "Count",
            GenoColumn::Fitness => "Fitness",
            GenoColumn::Merit => "Merit",
            GenoColumn::Gestation => "Gestation",
            GenoColumn::Depth => "Depth",
            GenoColumn::Length => "Length",
            GenoColumn::Born => "Born",
        }
    }

    pub fn tooltip(self) -> &'static str {
        match self {
            GenoColumn::Name => "Genotype name: genome length and a unique suffix.",
            GenoColumn::Abundance => "Living organisms of this genotype.",
            GenoColumn::Fitness => "Average fitness of its organisms.",
            GenoColumn::Merit => "Average merit of its organisms.",
            GenoColumn::Gestation => "Average gestation time in CPU cycles.",
            GenoColumn::Depth => "Depth in the phylogeny (genotypes back to the founder).",
            GenoColumn::Length => "Genome length in instructions.",
            GenoColumn::Born => "Update at which the genotype first appeared.",
        }
    }

    /// Numbers read best largest first; names alphabetically.
    pub fn default_descending(self) -> bool {
        !matches!(self, GenoColumn::Name)
    }

    fn compare(self, a: &GenotypeRow, b: &GenotypeRow) -> Ordering {
        // NaN (no organisms measured yet) sorts below every number.
        let key = |x: f64| if x.is_nan() { f64::NEG_INFINITY } else { x };
        let f = |x: f64, y: f64| key(x).total_cmp(&key(y));
        match self {
            GenoColumn::Name => a.name.cmp(&b.name),
            GenoColumn::Abundance => a.abundance.cmp(&b.abundance),
            GenoColumn::Fitness => f(a.ave_fitness, b.ave_fitness),
            GenoColumn::Merit => f(a.ave_merit, b.ave_merit),
            GenoColumn::Gestation => f(a.ave_gestation, b.ave_gestation),
            GenoColumn::Depth => a.depth.cmp(&b.depth),
            GenoColumn::Length => a.genome_len.cmp(&b.genome_len),
            GenoColumn::Born => a.update_born.cmp(&b.update_born),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GenoSort {
    pub column: GenoColumn,
    pub descending: bool,
}

impl Default for GenoSort {
    fn default() -> Self {
        GenoSort {
            column: GenoColumn::Abundance,
            descending: true,
        }
    }
}

impl GenoSort {
    /// A click on a column header: reverse the order of the sorted column,
    /// or sort by another column in its natural direction.
    pub fn click(&mut self, column: GenoColumn) {
        if self.column == column {
            self.descending = !self.descending;
        } else {
            self.column = column;
            self.descending = column.default_descending();
        }
    }
}

/// Row indices in display order. Ties keep abundance rank order.
pub fn sorted_indices(rows: &[GenotypeRow], sort: GenoSort) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..rows.len()).collect();
    idx.sort_by(|&i, &j| {
        let o = sort.column.compare(&rows[i], &rows[j]);
        let o = if sort.descending { o.reverse() } else { o };
        o.then(rows[i].rank.cmp(&rows[j].rank))
    });
    idx
}

/// One instruction of a genome listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenomeSite {
    pub symbol: String,
    /// The instruction name, when the names of the instruction set are
    /// known.
    pub name: Option<String>,
}

/// A genome symbol string (`GenotypeView::genome`) as instructions,
/// parsed the way the genome crate parses it, named from `names`
/// (`Frame::inst_names`, indexed by opcode) when given.
pub fn genome_listing(genome: &str, names: Option<&[String]>) -> Vec<GenomeSite> {
    let seq = avida_genome::sequence::InstructionSequence::from_string(genome);
    (0..seq.size())
        .map(|i| {
            let inst = seq.get(i);
            let op = usize::try_from(inst.op()).unwrap_or(usize::MAX);
            GenomeSite {
                symbol: inst.symbol(),
                name: names.and_then(|n| n.get(op)).cloned(),
            }
        })
        .collect()
}

/// What a lineage ([`avida_session::Frame::lineage`]: a genotype, then
/// its parent, back to the founder) amounts to.
#[derive(Clone, Debug, PartialEq)]
pub struct LineageSummary {
    /// Genotypes in the chain, the traced one included.
    pub genotypes: usize,
    /// Of those, the ones without living organisms.
    pub extinct: usize,
    /// Updates from the founder's birth to the traced genotype's.
    pub span: i32,
    /// Fitness of the traced genotype over the founder's, when both are
    /// known and the founder's is above zero.
    pub fitness_gain: Option<f64>,
    /// The chain stops before an injected genotype (it was cut at
    /// `LINEAGE_MAX`, or an ancestor is no longer recorded).
    pub truncated: bool,
}

pub fn lineage_summary(lineage: &[GenotypeRow]) -> Option<LineageSummary> {
    let traced = lineage.first()?;
    let founder = lineage.last()?;
    let ok = |f: f64| f.is_finite() && f > 0.0;
    Some(LineageSummary {
        genotypes: lineage.len(),
        extinct: lineage.iter().filter(|g| !g.is_alive()).count(),
        span: traced.update_born - founder.update_born.max(0),
        // A genotype that has not finished a gestation yet reads 0: no
        // ratio then.
        fitness_gain: (ok(founder.ave_fitness) && ok(traced.ave_fitness))
            .then(|| traced.ave_fitness / founder.ave_fitness),
        truncated: founder.parent_id >= 0,
    })
}

/// `[update born, average fitness]` along a lineage, founder first,
/// leaving out genotypes whose fitness is unknown.
pub fn lineage_fitness(lineage: &[GenotypeRow]) -> Vec<[f64; 2]> {
    lineage
        .iter()
        .rev()
        .filter(|g| g.ave_fitness.is_finite())
        .map(|g| [f64::from(g.update_born.max(0)), g.ave_fitness])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(rank: usize, name: &str, abundance: i32, fitness: f64) -> GenotypeRow {
        GenotypeRow {
            rank,
            id: rank as i32,
            parent_id: -1,
            name: name.into(),
            abundance,
            depth: 0,
            update_born: 0,
            genome_len: 100,
            threshold: false,
            ave_fitness: fitness,
            ave_merit: 0.0,
            ave_gestation: 0.0,
            last_births: 0,
        }
    }

    #[test]
    fn lineages_are_summarised_founder_to_traced() {
        let mut traced = row(0, "100-aaaac", 4, 2.0);
        traced.update_born = 90;
        traced.parent_id = 2;
        let mut mid = row(5, "100-aaaab", 0, f64::NAN);
        mid.id = 2;
        mid.parent_id = 1;
        mid.update_born = 40;
        let mut founder = row(9, "100-aaaaa", 1, 0.5);
        founder.id = 1;
        let lineage = vec![traced, mid, founder];
        let s = lineage_summary(&lineage).unwrap();
        assert_eq!(s.genotypes, 3);
        assert_eq!(s.extinct, 1);
        assert_eq!(s.span, 90);
        assert_eq!(s.fitness_gain, Some(4.0));
        assert!(!s.truncated);
        assert_eq!(lineage_fitness(&lineage), vec![[0.0, 0.5], [90.0, 2.0]]);
        assert!(lineage_summary(&[]).is_none());
        assert!(lineage_summary(&lineage[..2]).unwrap().truncated);
        let mut unmeasured = lineage.clone();
        unmeasured[0].ave_fitness = 0.0;
        assert_eq!(lineage_summary(&unmeasured).unwrap().fitness_gain, None);
    }

    #[test]
    fn sorts_by_column_and_toggles_direction() {
        let rows = vec![
            row(0, "100-b", 9, 0.1),
            row(1, "100-a", 4, f64::NAN),
            row(2, "099-c", 4, 0.9),
        ];
        let mut s = GenoSort::default();
        assert_eq!(sorted_indices(&rows, s), vec![0, 1, 2]);
        s.click(GenoColumn::Name);
        assert!(!s.descending);
        assert_eq!(sorted_indices(&rows, s), vec![2, 1, 0]);
        s.click(GenoColumn::Name);
        assert_eq!(sorted_indices(&rows, s), vec![0, 1, 2]);
        s.click(GenoColumn::Fitness);
        assert!(s.descending);
        let order = sorted_indices(&rows, s);
        assert_eq!(order[0], 2);
    }

    #[test]
    fn genomes_are_listed_with_names_when_known() {
        let names: Vec<String> = ["nop-A", "nop-B", "nop-C"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let sites = genome_listing("cab", Some(&names));
        assert_eq!(sites.len(), 3);
        assert_eq!(sites[0].symbol, "c");
        assert_eq!(sites[0].name.as_deref(), Some("nop-C"));
        let unnamed = genome_listing("z", Some(&names));
        assert_eq!(unnamed[0].name, None);
        assert_eq!(genome_listing("ab", None)[1].name, None);
    }
}
