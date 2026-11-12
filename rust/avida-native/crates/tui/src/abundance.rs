//! How many cells the most abundant genotypes held, frame by frame: the
//! data of the stacked abundance chart.
//!
//! Each frame with a map contributes one sample: the occupied cells and the
//! cell counts of the genotypes in the map's colour slots. The chart stacks
//! the genotypes that hold a slot now (so its colours match the map) and
//! puts everything else in an "other" band.

use avida_session::Frame;

/// Samples kept before every second one of the older half is dropped.
pub const MAX_SAMPLES: usize = 1200;

#[derive(Clone, Debug, PartialEq)]
pub struct Sample {
    pub update: i32,
    /// Occupied cells.
    pub total: u32,
    /// `(genotype id, cells)` of the slot genotypes, slot order.
    pub counts: Vec<(i32, u32)>,
}

impl Sample {
    pub fn count_of(&self, id: i32) -> u32 {
        self.counts
            .iter()
            .find(|&&(g, _)| g == id)
            .map_or(0, |&(_, n)| n)
    }
}

/// One band of the chart, bottom to top.
#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    /// `None` for the other genotypes.
    pub genotype: Option<i32>,
    /// The colour slot (`None` for other).
    pub slot: Option<usize>,
    /// Upper edge per sample (cumulative cells).
    pub top: Vec<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct AbundanceHistory {
    samples: Vec<Sample>,
}

impl AbundanceHistory {
    pub fn clear(&mut self) {
        self.samples.clear();
    }

    pub fn samples(&self) -> &[Sample] {
        &self.samples
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Takes in a frame with a map (others are skipped). A frame of the
    /// same update replaces the last sample; an earlier update starts over
    /// (a new run).
    pub fn record(&mut self, frame: &Frame) {
        let Some(map) = &frame.map else {
            return;
        };
        let counts: Vec<(i32, u32)> = map
            .top_genotypes
            .iter()
            .enumerate()
            .map(|(i, &g)| {
                let n = map
                    .top_counts
                    .get(i)
                    .copied()
                    .unwrap_or_else(|| map.cells_of(g));
                (g, n)
            })
            .collect();
        let sample = Sample {
            update: frame.update,
            total: u32::try_from(map.occupied()).unwrap_or(u32::MAX),
            counts,
        };
        match self.samples.last() {
            Some(last) if last.update == sample.update => {
                let n = self.samples.len();
                self.samples[n - 1] = sample;
            }
            Some(last) if last.update > sample.update => {
                self.samples.clear();
                self.samples.push(sample);
            }
            _ => self.samples.push(sample),
        }
        if self.samples.len() > MAX_SAMPLES {
            self.thin();
        }
    }

    /// Drops every second sample of the older half.
    fn thin(&mut self) {
        let half = self.samples.len() / 2;
        let mut i = 0;
        self.samples.retain(|_| {
            let keep = i >= half || i % 2 == 0;
            i += 1;
            keep
        });
    }

    /// The bands for the genotypes holding colour slots now (`slots`, slot
    /// order), plus other on top. Each band's `top` is cumulative.
    pub fn bands(&self, slots: &[i32]) -> Vec<Band> {
        let n = self.samples.len();
        let mut below = vec![0.0; n];
        let mut bands = Vec::with_capacity(slots.len() + 1);
        for (slot, &g) in slots.iter().enumerate() {
            let top: Vec<f64> = self
                .samples
                .iter()
                .zip(&below)
                .map(|(s, b)| b + f64::from(s.count_of(g)))
                .collect();
            below.clone_from(&top);
            bands.push(Band {
                genotype: Some(g),
                slot: Some(slot),
                top,
            });
        }
        let top: Vec<f64> = self
            .samples
            .iter()
            .zip(&below)
            .map(|(s, b)| f64::from(s.total).max(*b))
            .collect();
        bands.push(Band {
            genotype: None,
            slot: None,
            top,
        });
        bands
    }

    /// The largest occupied count (the chart's height).
    pub fn max_total(&self) -> u32 {
        self.samples.iter().map(|s| s.total).max().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(update: i32, total: u32, counts: &[(i32, u32)]) -> Sample {
        Sample {
            update,
            total,
            counts: counts.to_vec(),
        }
    }

    #[test]
    fn bands_stack_slot_genotypes_under_the_others() {
        let h = AbundanceHistory {
            samples: vec![sample(1, 10, &[(7, 6), (8, 1)]), sample(2, 12, &[(7, 4)])],
        };
        let bands = h.bands(&[7, 8, 99]);
        assert_eq!(bands.len(), 4);
        assert_eq!(bands[0].top, vec![6.0, 4.0]);
        assert_eq!(bands[1].top, vec![7.0, 4.0]);
        assert_eq!(bands[2].top, vec![7.0, 4.0], "absent genotype adds nothing");
        assert_eq!(bands[3].genotype, None);
        assert_eq!(bands[3].top, vec![10.0, 12.0]);
        assert_eq!(h.max_total(), 12);
    }

    #[test]
    fn thinning_keeps_the_newest_samples() {
        let mut h = AbundanceHistory {
            samples: (0..=MAX_SAMPLES as i32)
                .map(|u| sample(u, 1, &[]))
                .collect(),
        };
        h.thin();
        assert!(h.len() <= MAX_SAMPLES);
        assert_eq!(h.samples()[0].update, 0);
        assert_eq!(
            h.samples().last().map(|s| s.update),
            Some(MAX_SAMPLES as i32)
        );
        let n = h.len();
        assert_eq!(h.samples()[n - 1].update - h.samples()[n - 2].update, 1);
    }
}
