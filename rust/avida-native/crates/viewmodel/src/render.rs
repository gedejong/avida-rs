//! Rendering a map layer into an RGBA8 image, one pixel per cell.

use crate::legend::{Legend, Scale};
use crate::model::{LayerId, MapFrame};
use crate::palette::{Palette, EMPTY};

/// The legend [`render_rgba`] uses for a layer: the range of its finite
/// values on `scale`. `None` for [`LayerId::Genotype`] (categorical), for
/// a layer that was not captured, and for a layer without finite values.
pub fn layer_legend(map: &MapFrame, layer: LayerId, scale: Scale) -> Option<Legend> {
    if layer == LayerId::Genotype {
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
