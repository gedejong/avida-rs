//! The plottable columns of an [`UpdateRow`] and small text graphics.

use avida_session::UpdateRow;
use avida_viewmodel::{decimate_min_max, format_number};

/// A statistic recorded once per update.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Metric {
    Organisms,
    Genotypes,
    ThresholdGenotypes,
    AveFitness,
    MaxFitness,
    AveMerit,
    MaxMerit,
    AveGestation,
    AveGeneration,
    AveAge,
    AveGenomeSize,
    Births,
    Deaths,
    BreedTrue,
    Executed,
    Threads,
    TotalOrganisms,
}

impl Metric {
    pub const ALL: [Metric; 17] = [
        Metric::Organisms,
        Metric::Genotypes,
        Metric::ThresholdGenotypes,
        Metric::AveFitness,
        Metric::MaxFitness,
        Metric::AveMerit,
        Metric::MaxMerit,
        Metric::AveGestation,
        Metric::AveGeneration,
        Metric::AveAge,
        Metric::AveGenomeSize,
        Metric::Births,
        Metric::Deaths,
        Metric::BreedTrue,
        Metric::Executed,
        Metric::Threads,
        Metric::TotalOrganisms,
    ];

    /// The rows of the stats screen, in the order the C++ stats screen
    /// lists them.
    pub const STATS: [Metric; 13] = [
        Metric::Organisms,
        Metric::Genotypes,
        Metric::ThresholdGenotypes,
        Metric::AveFitness,
        Metric::MaxFitness,
        Metric::AveMerit,
        Metric::AveGestation,
        Metric::AveGenomeSize,
        Metric::AveGeneration,
        Metric::AveAge,
        Metric::Births,
        Metric::Deaths,
        Metric::BreedTrue,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Metric::Organisms => "Organisms",
            Metric::Genotypes => "Genotypes",
            Metric::ThresholdGenotypes => "Threshold genotypes",
            Metric::AveFitness => "Average fitness",
            Metric::MaxFitness => "Maximum fitness",
            Metric::AveMerit => "Average merit",
            Metric::MaxMerit => "Maximum merit",
            Metric::AveGestation => "Average gestation",
            Metric::AveGeneration => "Average generation",
            Metric::AveAge => "Average age",
            Metric::AveGenomeSize => "Average genome size",
            Metric::Births => "Births",
            Metric::Deaths => "Deaths",
            Metric::BreedTrue => "Breed-true births",
            Metric::Executed => "Instructions executed",
            Metric::Threads => "Threads",
            Metric::TotalOrganisms => "Organisms ever born",
        }
    }

    pub fn value(self, r: &UpdateRow) -> f64 {
        match self {
            Metric::Organisms => f64::from(r.orgs),
            Metric::Genotypes => f64::from(r.genotypes),
            Metric::ThresholdGenotypes => f64::from(r.threshold_genotypes),
            Metric::AveFitness => r.ave_fitness,
            Metric::MaxFitness => r.max_fitness,
            Metric::AveMerit => r.ave_merit,
            Metric::MaxMerit => r.max_merit,
            Metric::AveGestation => r.ave_gestation,
            Metric::AveGeneration => r.ave_generation,
            Metric::AveAge => r.ave_age,
            Metric::AveGenomeSize => r.ave_mem_size,
            Metric::Births => f64::from(r.births),
            Metric::Deaths => f64::from(r.deaths),
            Metric::BreedTrue => f64::from(r.breed_true),
            Metric::Executed => f64::from(r.executed),
            Metric::Threads => f64::from(r.threads),
            Metric::TotalOrganisms => f64::from(r.tot_organisms),
        }
    }

    pub fn format(self, v: f64) -> String {
        format_number(v)
    }
}

const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Samples `values` down to `width` points (min/max decimation keeps
/// spikes) and draws them as block characters scaled between their own
/// minimum and maximum. A flat series is a row of low bars.
pub fn sparkline(values: &[f64], width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let xs: Vec<f64> = (0..values.len()).map(|i| i as f64).collect();
    let pts = decimate_min_max(&xs, values, width);
    // decimate_min_max can return up to `width` points; resample evenly
    // when it returned fewer or more than we have columns for.
    let ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
    if ys.is_empty() {
        return String::new();
    }
    let cols: Vec<f64> = if ys.len() <= width {
        ys
    } else {
        (0..width).map(|i| ys[i * ys.len() / width]).collect()
    };
    let lo = cols.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = cols.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    cols.iter()
        .map(|&v| {
            if hi > lo {
                let t = (v - lo) / (hi - lo);
                BARS[((t * 7.0).round() as usize).min(7)]
            } else {
                BARS[0]
            }
        })
        .collect()
}

/// A horizontal bar of `width` cells filled to `frac` (clamped to 0..=1),
/// with eighth-block resolution.
pub fn hbar(frac: f64, width: usize) -> String {
    const PARTS: [char; 8] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉'];
    let frac = if frac.is_finite() {
        frac.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let eighths = (frac * width as f64 * 8.0).round() as usize;
    let full = eighths / 8;
    let mut s = "█".repeat(full.min(width));
    if full < width {
        let rem = eighths % 8;
        if rem > 0 {
            s.push(PARTS[rem]);
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparkline_scales_and_fits() {
        let s = sparkline(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0], 8);
        assert_eq!(s, "▁▂▃▄▅▆▇█");
        assert_eq!(sparkline(&[2.0, 2.0, 2.0], 5), "▁▁▁");
        assert!(
            sparkline(&(0..1000).map(f64::from).collect::<Vec<_>>(), 20)
                .chars()
                .count()
                <= 20
        );
        assert_eq!(sparkline(&[1.0], 0), "");
        assert_eq!(sparkline(&[], 4), "");
    }

    #[test]
    fn hbar_fills_proportionally() {
        assert_eq!(hbar(1.0, 4), "████");
        assert_eq!(hbar(0.5, 4), "██");
        assert_eq!(hbar(0.0, 4), "");
        assert_eq!(hbar(f64::NAN, 4), "");
        assert_eq!(hbar(0.125, 4), "▌");
    }

    #[test]
    fn metrics_read_rows() {
        let r = UpdateRow {
            orgs: 7,
            ave_fitness: 1.5,
            ..UpdateRow::default()
        };
        assert_eq!(Metric::Organisms.value(&r), 7.0);
        assert_eq!(Metric::AveFitness.value(&r), 1.5);
        for m in Metric::ALL {
            assert!(!m.label().is_empty());
        }
    }
}
