//! Unit tests of the presentation helpers: palettes, legends, histograms,
//! decimation and RGBA rendering, on hand-built data.

use std::sync::Arc;

use avida_viewmodel::{
    decimate_min_max, format_number, layer_legend, render_rgba, render_rgba_with_legend, series_of,
    GenotypeCells, Histogram, LayerId, Legend, MapFrame, Palette, Scale, UpdateRow,
    CATEGORICAL_SLOTS, EMPTY, OKABE_ITO, OTHER_GREY, UNCLASSIFIED,
};

// ---------------------------------------------------------------------------
// Palettes
// ---------------------------------------------------------------------------

#[test]
fn viridis_and_cividis_tables_match_matplotlib_end_points() {
    // matplotlib: viridis(0) = #440154, viridis(1) = #fde725,
    // viridis(0.5) = #21918c; cividis(0) = (0, 34, 78), cividis(1) =
    // (254, 232, 56).
    assert_eq!(Palette::Viridis.sample(0.0), [0x44, 0x01, 0x54, 255]);
    assert_eq!(Palette::Viridis.sample(1.0), [0xfd, 0xe7, 0x25, 255]);
    assert_eq!(Palette::Viridis.sample(0.5), [0x21, 0x91, 0x8c, 255]);
    assert_eq!(Palette::Cividis.sample(0.0), [0, 34, 78, 255]);
    assert_eq!(Palette::Cividis.sample(1.0), [254, 232, 56, 255]);
    assert_eq!(Palette::Viridis.table().len(), 256);
    assert_eq!(Palette::Cividis.table().len(), 256);
}

#[test]
fn sequential_maps_brighten_monotonically() {
    // Both maps are designed to increase in lightness; a rough luma must
    // never fall by more than rounding.
    for p in [Palette::Viridis, Palette::Cividis] {
        let luma = |c: &[u8; 3]| {
            0.2126 * f64::from(c[0]) + 0.7152 * f64::from(c[1]) + 0.0722 * f64::from(c[2])
        };
        for pair in p.table().windows(2) {
            assert!(luma(&pair[1]) + 1.0 >= luma(&pair[0]), "{p:?} {pair:?}");
        }
        assert!(luma(&p.table()[255]) > luma(&p.table()[0]) + 150.0);
    }
}

#[test]
fn sampling_clamps_and_maps_nan_to_transparent() {
    let p = Palette::Viridis;
    assert_eq!(p.sample(-3.0), p.sample(0.0));
    assert_eq!(p.sample(7.0), p.sample(1.0));
    assert_eq!(p.sample(f64::NAN), EMPTY);
    // matplotlib's lookup: entry floor(t * 256), capped at 255.
    let t = p.table();
    assert_eq!(p.sample(0.999), [t[255][0], t[255][1], t[255][2], 255]);
    assert_eq!(p.sample(1.0 / 256.0), [t[1][0], t[1][1], t[1][2], 255]);
    let lut = p.lut256();
    assert_eq!(lut.len(), 256);
    for (i, c) in lut.iter().enumerate() {
        assert_eq!(&c[..3], &t[i][..]);
        assert_eq!(c[3], 255);
    }
}

#[test]
fn okabe_ito_slots_then_grey() {
    let p = Palette::OkabeIto;
    assert!(p.is_categorical());
    assert_eq!(CATEGORICAL_SLOTS, 7);
    assert_eq!(p.category(Some(0)), [0xE6, 0x9F, 0x00, 255]); // orange
    assert_eq!(p.category(Some(1)), [0x56, 0xB4, 0xE9, 255]); // sky blue
    assert_eq!(p.category(Some(6)), [0xCC, 0x79, 0xA7, 255]); // reddish purple
    let grey = [OTHER_GREY[0], OTHER_GREY[1], OTHER_GREY[2], 255];
    assert_eq!(p.category(Some(7)), grey);
    assert_eq!(p.category(None), grey);
    // No black (it would read as an empty cell) and all distinct.
    for (i, a) in OKABE_ITO.iter().enumerate() {
        assert_ne!(*a, [0, 0, 0]);
        for b in &OKABE_ITO[i + 1..] {
            assert_ne!(a, b);
        }
    }
    // A categorical palette quantises a continuous position into its bins.
    assert_eq!(p.sample(0.0), p.category(Some(0)));
    assert_eq!(p.sample(1.0), p.category(Some(6)));
    // A sequential palette spreads the genotype slots over its range.
    assert_eq!(
        Palette::Viridis.category(Some(0)),
        Palette::Viridis.sample(0.0)
    );
    assert_eq!(
        Palette::Viridis.category(Some(6)),
        Palette::Viridis.sample(1.0)
    );
    assert_eq!(Palette::Viridis.category(None), grey);
}

// ---------------------------------------------------------------------------
// Legends
// ---------------------------------------------------------------------------

fn labels(l: &Legend) -> Vec<&str> {
    l.ticks.iter().map(|t| t.label.as_str()).collect()
}

#[test]
fn linear_legend_uses_1_2_5_steps_inside_the_range() {
    let l = Legend::new(0.0, 100.0, Scale::Linear);
    assert_eq!(labels(&l), ["0", "20", "40", "60", "80", "100"]);
    assert_eq!(l.ticks[0].pos, 0.0);
    assert_eq!(l.ticks[5].pos, 1.0);
    assert!((l.ticks[1].pos - 0.2).abs() < 1e-12);

    let l = Legend::new(0.13, 0.87, Scale::Linear);
    assert_eq!(
        labels(&l),
        ["0.2", "0.3", "0.4", "0.5", "0.6", "0.7", "0.8"]
    );
    for t in &l.ticks {
        assert!(t.value >= l.min && t.value <= l.max);
        assert!((l.normalize(t.value) - t.pos).abs() < 1e-12);
    }

    let l = Legend::new(-3.0, 7.0, Scale::Linear);
    assert_eq!(labels(&l), ["-2", "0", "2", "4", "6"]);
}

#[test]
fn legend_normalizes_and_inverts() {
    let l = Legend::new(10.0, 20.0, Scale::Linear);
    assert_eq!(l.normalize(15.0), 0.5);
    assert_eq!(l.normalize(5.0), 0.0);
    assert_eq!(l.normalize(25.0), 1.0);
    assert!(l.normalize(f64::NAN).is_nan());
    assert_eq!(l.value_at(0.25), 12.5);
    // Reversed bounds are swapped.
    assert_eq!(Legend::new(20.0, 10.0, Scale::Linear).min, 10.0);
}

#[test]
fn log_legend_ticks_decades_and_maps_logarithmically() {
    let l = Legend::new(0.01, 1000.0, Scale::Log);
    assert_eq!(l.scale, Scale::Log);
    assert_eq!(labels(&l), ["0.01", "0.1", "1", "10", "100", "1000"]);
    assert!((l.normalize(1.0) - 0.4).abs() < 1e-12);
    assert!((l.value_at(0.4) - 1.0).abs() < 1e-12);
    assert_eq!(l.normalize(0.0), 0.0);
    assert_eq!(l.normalize(-5.0), 0.0);

    // Less than two decades: 1-2-5 ticks.
    let l = Legend::new(3.0, 60.0, Scale::Log);
    assert_eq!(labels(&l), ["5", "10", "20", "50"]);

    // Many decades are thinned to at most eight ticks.
    let l = Legend::new(1e-10, 1e10, Scale::Log);
    assert!(l.ticks.len() <= 8);
    assert_eq!(l.ticks[0].label, "1e-10");

    // A log scale without a positive lower bound is linear.
    assert_eq!(Legend::new(0.0, 10.0, Scale::Log).scale, Scale::Linear);
}

#[test]
fn legend_from_values_skips_empty_cells() {
    let v = [f32::NAN, 2.0, 8.0, f32::NAN, 4.0];
    let l = Legend::from_values(&v, Scale::Linear).unwrap();
    assert_eq!((l.min, l.max), (2.0, 8.0));
    // Log ignores non-positive values for the range.
    let v = [0.0, 0.5, 50.0, f32::NAN];
    let l = Legend::from_values(&v, Scale::Log).unwrap();
    assert_eq!((l.min, l.max, l.scale), (0.5, 50.0, Scale::Log));
    // ... and degrades to linear when there are none.
    let l = Legend::from_values(&[0.0, -1.0], Scale::Log).unwrap();
    assert_eq!((l.min, l.max, l.scale), (-1.0, 0.0, Scale::Linear));
    assert!(Legend::from_values(&[f32::NAN], Scale::Linear).is_none());
    assert!(Legend::from_values(&[], Scale::Linear).is_none());
}

#[test]
fn degenerate_legend_has_one_tick_in_the_middle() {
    let l = Legend::new(3.0, 3.0, Scale::Linear);
    assert!(l.is_degenerate());
    assert_eq!(labels(&l), ["3"]);
    assert_eq!(l.normalize(3.0), 0.5);
    let l = Legend::new(2.5, 2.5, Scale::Log);
    assert_eq!(labels(&l), ["2.5"]);
}

#[test]
fn numbers_are_formatted_compactly() {
    assert_eq!(format_number(0.0), "0");
    assert_eq!(format_number(42.0), "42");
    assert_eq!(format_number(-7.0), "-7");
    assert_eq!(format_number(0.25), "0.25");
    assert_eq!(format_number(4.56789), "4.568");
    assert_eq!(format_number(1234.5678), "1235");
    assert_eq!(format_number(1e7), "1e7");
    assert_eq!(format_number(1.5e-6), "1.5e-6");
}

// ---------------------------------------------------------------------------
// Histograms
// ---------------------------------------------------------------------------

#[test]
fn linear_histogram_counts_finite_values() {
    let v = [
        0.0,
        1.0,
        2.0,
        3.0,
        4.0,
        5.0,
        6.0,
        7.0,
        8.0,
        9.0,
        10.0,
        f32::NAN,
    ];
    let h = Histogram::new(&v, 5, Scale::Linear);
    assert_eq!((h.min, h.max), (0.0, 10.0));
    // The top edge belongs to the last bin.
    assert_eq!(h.counts, vec![2, 2, 2, 2, 3]);
    assert_eq!(h.skipped, 1);
    assert_eq!(h.total(), 11);
    assert_eq!(h.max_count(), 3);
    let edges = h.bin_edges();
    assert_eq!(edges.len(), 6);
    for (e, want) in edges.iter().zip([0.0, 2.0, 4.0, 6.0, 8.0, 10.0]) {
        assert!((e - want).abs() < 1e-12);
    }
    assert_eq!(h.bin_of(1.999), Some(0));
    assert_eq!(h.bin_of(2.0), Some(1));
    assert_eq!(h.bin_of(10.5), None);
}

#[test]
fn log_histogram_bins_by_decade() {
    let v = [1.0, 5.0, 20.0, 50.0, 100.0, 0.0, -1.0];
    let h = Histogram::new(&v, 2, Scale::Log);
    assert_eq!(h.scale, Scale::Log);
    assert_eq!((h.min, h.max), (1.0, 100.0));
    assert_eq!(h.counts, vec![2, 3]);
    assert_eq!(h.skipped, 2);
    let e = h.bin_edges();
    assert!((e[1] - 10.0).abs() < 1e-9);
}

#[test]
fn histogram_edge_cases() {
    // One distinct value sits in the middle of a unit-wide range.
    let h = Histogram::new(&[4.0, 4.0, 4.0], 3, Scale::Linear);
    assert_eq!((h.min, h.max), (3.5, 4.5));
    assert_eq!(h.counts, vec![0, 3, 0]);
    // Nothing usable.
    let h = Histogram::new(&[f32::NAN, f32::INFINITY], 4, Scale::Linear);
    assert_eq!(h.total(), 0);
    assert_eq!(h.skipped, 2);
    // Zero bins means one.
    assert_eq!(
        Histogram::new(&[1.0, 2.0], 0, Scale::Linear).counts,
        vec![2]
    );
    // Fixed range drops what falls outside.
    let h = Histogram::with_range(&[-1.0, 0.5, 2.0], 2, 0.0, 1.0, Scale::Linear);
    assert_eq!((h.counts.clone(), h.skipped), (vec![0, 1], 2));
    // A log histogram of non-positive values is linear.
    assert_eq!(
        Histogram::new(&[0.0, -2.0], 2, Scale::Log).scale,
        Scale::Linear
    );
}

// ---------------------------------------------------------------------------
// Decimation
// ---------------------------------------------------------------------------

fn xs(n: usize) -> Vec<f64> {
    (0..n).map(|i| i as f64).collect()
}

#[test]
fn short_series_pass_through() {
    let x = xs(5);
    let y = vec![3.0, 1.0, 4.0, 1.0, 5.0];
    let d = decimate_min_max(&x, &y, 10);
    assert_eq!(d.len(), 5);
    assert_eq!(d[2], (2.0, 4.0));
}

#[test]
fn decimation_keeps_ends_and_extremes_in_order() {
    let n = 1000;
    let x = xs(n);
    let mut y: Vec<f64> = (0..n).map(|i| (i as f64 / 50.0).sin()).collect();
    y[500] = 9.0; // a spike
    y[700] = -9.0; // a dip
    let d = decimate_min_max(&x, &y, 40);
    assert!(d.len() <= 40);
    assert_eq!(d.first(), Some(&(0.0, y[0])));
    assert_eq!(d.last(), Some(&(999.0, y[999])));
    assert!(d.contains(&(500.0, 9.0)));
    assert!(d.contains(&(700.0, -9.0)));
    assert!(d.windows(2).all(|p| p[0].0 < p[1].0), "x must increase");
    // The decimated range equals the full range.
    let lo = d.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let hi = d.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    assert_eq!((lo, hi), (-9.0, 9.0));
}

#[test]
fn decimation_edge_cases() {
    let x = xs(10);
    let y: Vec<f64> = (0..10).map(f64::from).collect();
    assert!(decimate_min_max(&x, &y, 0).is_empty());
    assert_eq!(decimate_min_max(&x, &y, 1), vec![(9.0, 9.0)]);
    assert_eq!(decimate_min_max(&x, &y, 3), vec![(0.0, 0.0), (9.0, 9.0)]);
    // Monotone data: each bucket's min and max are its first and last.
    let d = decimate_min_max(&x, &y, 6);
    assert_eq!(d.len(), 6);
    assert_eq!(d[0], (0.0, 0.0));
    assert_eq!(d[5], (9.0, 9.0));
    // Non-finite values are dropped; lengths are read up to the shorter.
    let y = [1.0, f64::NAN, 3.0];
    assert_eq!(decimate_min_max(&x, &y, 10), vec![(0.0, 1.0), (2.0, 3.0)]);
    assert!(decimate_min_max(&[], &[], 10).is_empty());
}

#[test]
fn update_rows_turn_into_series() {
    let rows: Vec<UpdateRow> = (0..3)
        .map(|u| UpdateRow {
            update: u * 10,
            ave_fitness: f64::from(u) * 0.5,
            ..UpdateRow::default()
        })
        .collect();
    let (x, y) = series_of(&rows, |r| r.ave_fitness);
    assert_eq!(x, vec![0.0, 10.0, 20.0]);
    assert_eq!(y, vec![0.0, 0.5, 1.0]);
}

// ---------------------------------------------------------------------------
// RGBA rendering
// ---------------------------------------------------------------------------

/// A 3x2 map: cells 0..2 hold genotypes 7, 7, 9; cell 3 is empty; cell 4
/// genotype 11 (outside the top list); cell 5 an unclassified organism.
fn test_map() -> MapFrame {
    let nan = f32::NAN;
    MapFrame {
        w: 3,
        h: 2,
        genotype_id: Arc::from(vec![7, 7, 9, -1, 11, UNCLASSIFIED]),
        top_genotypes: Arc::from(vec![7, 9]),
        layers: vec![
            (
                LayerId::Fitness,
                Arc::from(vec![1.0, 10.0, 100.0, nan, 0.0, 5.5]),
            ),
            (
                LayerId::Resource(0),
                Arc::from(vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0]),
            ),
        ],
        num_tasks: 2,
        task_bits: Arc::from(vec![0b01, 0b11, 0, 0, 0b10, 0]),
        words_per_cell: 1,
        top_counts: Arc::from(vec![2, 1]),
        families: Arc::from(Vec::new()),
        genotype_cells: Arc::from(vec![
            GenotypeCells {
                id: UNCLASSIFIED,
                first_cell: 5,
                cells: 1,
            },
            GenotypeCells {
                id: 7,
                first_cell: 0,
                cells: 2,
            },
            GenotypeCells {
                id: 9,
                first_cell: 2,
                cells: 1,
            },
            GenotypeCells {
                id: 11,
                first_cell: 4,
                cells: 1,
            },
        ]),
    }
}

fn px(buf: &[u8], cell: usize) -> [u8; 4] {
    buf[cell * 4..cell * 4 + 4].try_into().unwrap()
}

#[test]
fn genotype_layer_uses_categories_by_rank() {
    let map = test_map();
    assert_eq!(map.occupied(), 5);
    assert_eq!(map.genotype_slot(9), Some(1));
    assert_eq!(map.genotype_slot(11), None);
    assert_eq!(map.cells_of(7), 2);
    assert_eq!(map.cells_of(8), 0);
    assert_eq!(map.first_cell_of(11), Some(4));
    assert_eq!(map.first_cell_of(UNCLASSIFIED), Some(5));
    assert_eq!(map.first_cell_of(-1), None);
    let p = Palette::OkabeIto;
    let buf = render_rgba(&map, LayerId::Genotype, &p, Scale::Linear);
    assert_eq!(buf.len(), 6 * 4);
    assert_eq!(px(&buf, 0), p.category(Some(0)));
    assert_eq!(px(&buf, 1), p.category(Some(0)));
    assert_eq!(px(&buf, 2), p.category(Some(1)));
    assert_eq!(px(&buf, 3), EMPTY);
    assert_eq!(px(&buf, 4), p.category(None));
    assert_eq!(px(&buf, 5), p.category(None));
    assert!(layer_legend(&map, LayerId::Genotype, Scale::Linear).is_none());
}

#[test]
fn continuous_layer_spans_its_range() {
    let map = test_map();
    let p = Palette::Viridis;
    let buf = render_rgba(&map, LayerId::Resource(0), &p, Scale::Linear);
    assert_eq!(px(&buf, 0), p.sample(0.0));
    assert_eq!(px(&buf, 5), p.sample(1.0));
    // Resources exist in empty cells.
    assert_eq!(px(&buf, 3), p.sample(0.6));

    // Log fitness: 1, 10, 100 are evenly spaced; 0 sits at the bottom;
    // the empty cell is transparent.
    let buf = render_rgba(&map, LayerId::Fitness, &p, Scale::Log);
    let legend = layer_legend(&map, LayerId::Fitness, Scale::Log).unwrap();
    assert_eq!(
        (legend.min, legend.max, legend.scale),
        (1.0, 100.0, Scale::Log)
    );
    assert!((legend.normalize(10.0) - 0.5).abs() < 1e-12);
    assert_eq!(px(&buf, 0), p.sample(0.0));
    assert_eq!(px(&buf, 1), p.sample(legend.normalize(10.0)));
    assert_eq!(px(&buf, 2), p.sample(1.0));
    assert_eq!(px(&buf, 3), EMPTY);
    assert_eq!(px(&buf, 4), p.sample(0.0));
}

#[test]
fn fixed_legend_and_missing_layers() {
    let map = test_map();
    let p = Palette::Cividis;
    let legend = Legend::new(0.0, 10.0, Scale::Linear);
    let buf = render_rgba_with_legend(&map, LayerId::Resource(0), &p, &legend);
    assert_eq!(px(&buf, 5), p.sample(0.5));
    // A layer that was not captured renders transparent.
    let buf = render_rgba(&map, LayerId::Merit, &p, Scale::Linear);
    assert_eq!(buf, vec![0; 6 * 4]);
    // Task bits.
    assert!(map.task_done(0, 0) && !map.task_done(0, 1));
    assert!(map.task_done(1, 0) && map.task_done(1, 1));
    assert!(map.task_done(4, 1));
    assert!(!map.task_done(4, 2), "task out of range");
}
