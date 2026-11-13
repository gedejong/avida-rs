//! Rendering a map layer into an RGBA8 image, one pixel per cell.

use crate::ancestry::decode;
use crate::legend::{Legend, Scale};
use crate::model::{LayerId, MapFrame};
use crate::palette::{Palette, Rgba, EMPTY, OKABE_ITO, OTHER_GREY};

/// The legend [`render_rgba`] uses for a layer: the range of its finite
/// values on `scale`. `None` for the categorical layers
/// ([`LayerId::is_categorical`]), for
/// a layer that was not captured, and for a layer without finite values.
pub fn layer_legend(map: &MapFrame, layer: LayerId, scale: Scale) -> Option<Legend> {
    if layer.is_categorical() {
        return None;
    }
    Legend::from_values(map.layer(layer)?, scale)
}

/// Renders one layer as `w * h` RGBA8 pixels, row-major from cell 0 (top
/// left, `x = cell % w`). Empty cells, NaN values and layers that were not
/// captured are transparent ([`EMPTY`]).
///
/// [`LayerId::Genotype`] is categorical: the genotypes in
/// [`MapFrame::top_genotypes`] get [`Palette::category`] colours by rank,
/// all others grey. Every other layer is coloured by
/// [`Palette::sample`] over [`layer_legend`]; a categorical palette then
/// quantises the range into its colours.
pub fn render_rgba(map: &MapFrame, layer: LayerId, palette: &Palette, scale: Scale) -> Vec<u8> {
    match layer_legend(map, layer, scale) {
        Some(legend) => render_rgba_with_legend(map, layer, palette, &legend),
        None if layer == LayerId::Genotype => render_genotypes(map, palette),
        None if layer == LayerId::Ancestry => render_families(map),
        None => vec![0; map.num_cells() * 4],
    }
}

/// [`render_rgba`] with a caller-chosen legend, e.g. one fixed across
/// frames so colours do not shift as the population changes.
/// [`LayerId::Genotype`] ignores the legend.
pub fn render_rgba_with_legend(
    map: &MapFrame,
    layer: LayerId,
    palette: &Palette,
    legend: &Legend,
) -> Vec<u8> {
    if layer == LayerId::Genotype {
        return render_genotypes(map, palette);
    }
    if layer == LayerId::Ancestry {
        return render_families(map);
    }
    let n = map.num_cells();
    let Some(values) = map.layer(layer) else {
        return vec![0; n * 4];
    };
    let mut out = Vec::with_capacity(n * 4);
    for &v in values.iter().take(n) {
        let c = if v.is_nan() {
            EMPTY
        } else {
            palette.sample(legend.normalize(f64::from(v)))
        };
        out.extend_from_slice(&c);
    }
    out.resize(n * 4, 0);
    out
}

fn render_genotypes(map: &MapFrame, palette: &Palette) -> Vec<u8> {
    let mut out = Vec::with_capacity(map.num_cells() * 4);
    for &g in map.genotype_id.iter() {
        let c = if g == -1 {
            EMPTY
        } else {
            palette.category(map.genotype_slot(g))
        };
        out.extend_from_slice(&c);
    }
    out
}

/// How much darker the first genotype of a family is drawn than its last.
const FAMILY_SHADE: f32 = 0.18;

/// The colour of a [`LayerId::Ancestry`] value: the family's slot in
/// Okabe and Ito's palette ([`OTHER_GREY`] without a family), darkened by
/// up to [`FAMILY_SHADE`] towards the start of the family so that its
/// branches show.
pub fn family_color(value: f32) -> Rgba {
    let Some((slot, within)) = decode(value) else {
        return EMPTY;
    };
    let base = slot.map_or(OTHER_GREY, |s| OKABE_ITO[s]);
    let keep = 1.0 - FAMILY_SHADE * (1.0 - within);
    let c = |v: u8| (f32::from(v) * keep).round() as u8;
    [c(base[0]), c(base[1]), c(base[2]), 255]
}

fn render_families(map: &MapFrame) -> Vec<u8> {
    let n = map.num_cells();
    let mut out = Vec::with_capacity(n * 4);
    if let Some(values) = map.layer(LayerId::Ancestry) {
        for &v in values.iter().take(n) {
            out.extend_from_slice(&family_color(v));
        }
    }
    out.resize(n * 4, 0);
    out
}
