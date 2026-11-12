//! Map layers: which exist, what they are called, how each is coloured,
//! and turning a [`avida_viewmodel::MapFrame`] into the image the map
//! shows.

use std::collections::BTreeMap;

use avida_session::{Frame, LayerId};
use avida_viewmodel::{
    layer_legend, render_rgba, render_rgba_with_legend, EnvView, Legend, Palette, Scale,
    UNCLASSIFIED,
};

use crate::model::vision::ColorVision;

/// A display name: "Fitness", "Task: NAND", "Resource: glucose".
pub fn layer_title(env: &EnvView, layer: LayerId) -> String {
    let fixed = match layer {
        LayerId::Genotype => "Genotype",
        LayerId::Fitness => "Fitness",
        LayerId::Merit => "Merit",
        LayerId::Gestation => "Gestation time",
        LayerId::Age => "Age",
        LayerId::Generation => "Generation",
        LayerId::GenomeLen => "Genome length",
        LayerId::Lineage => "Lineage label",
        LayerId::BreedTrue => "Breeds true",
        LayerId::Modified => "Modified",
        LayerId::Threads => "Threads",
        LayerId::TaskCount => "Tasks performed",
        LayerId::Task(k) => {
            return match env.tasks.get(k) {
                Some(t) if !t.desc.is_empty() => format!("Task: {}", t.desc),
                _ => format!(
                    "Task: {}",
                    env.layer_label(layer).trim_start_matches("task ")
                ),
            }
        }
        LayerId::Resource(_) => {
            return format!(
                "Resource: {}",
                env.layer_label(layer).trim_start_matches("resource ")
            )
        }
    };
    fixed.to_string()
}

/// The group a layer is listed under in the picker.
pub fn layer_group(layer: LayerId) -> &'static str {
    match layer {
        LayerId::Task(_) => "Tasks",
        LayerId::Resource(_) => "Resources",
        _ => "Organisms",
    }
}

/// What a layer shows (tooltips).
pub fn layer_description(layer: LayerId) -> &'static str {
    match layer {
        LayerId::Genotype => "The genotype of each organism; the seven most abundant get their own colour, the rest are grey.",
        LayerId::Fitness => "Merit divided by gestation time: how fast an organism turns CPU time into offspring.",
        LayerId::Merit => "The organism's share of CPU time, raised by the tasks it performs.",
        LayerId::Gestation => "CPU cycles the organism needed to produce its last offspring.",
        LayerId::Age => "Updates the organism has lived.",
        LayerId::Generation => "Ancestors between the founder and the organism.",
        LayerId::GenomeLen => "Instructions in the organism's genome.",
        LayerId::Lineage => "The lineage label inherited from the injected ancestor.",
        LayerId::BreedTrue => "1 when the organism is an exact copy of its parent.",
        LayerId::Modified => "1 when the organism's phenotype was modified.",
        LayerId::Threads => "CPU threads the organism runs.",
        LayerId::TaskCount => "Distinct tasks performed in the last gestation.",
        LayerId::Task(_) => "How often the organism performed this task in its last gestation.",
        LayerId::Resource(_) => "Amount of the resource in each cell (frozen at the last resource update).",
    }
}

/// How the map colours its layers. Scales and locked legend ranges are
/// remembered per layer.
#[derive(Clone, Debug, PartialEq)]
pub struct MapSettings {
    pub layer: LayerId,
    /// Palette for continuous layers; the genotype layer is always
    /// categorical (Okabe-Ito).
    pub palette: Palette,
    /// The map (and legend) as a colour-vision deficiency shows it.
    pub vision: ColorVision,
    scales: BTreeMap<LayerId, Scale>,
    locked: BTreeMap<LayerId, (f64, f64)>,
}

impl Default for MapSettings {
    fn default() -> Self {
        MapSettings {
            layer: LayerId::Genotype,
            palette: Palette::Viridis,
            vision: ColorVision::Normal,
            scales: BTreeMap::new(),
            locked: BTreeMap::new(),
        }
    }
}

impl MapSettings {
    pub fn scale(&self, layer: LayerId) -> Scale {
        self.scales
            .get(&layer)
            .copied()
            .unwrap_or_else(|| layer.default_scale())
    }

    pub fn set_scale(&mut self, layer: LayerId, scale: Scale) {
        if self.scale(layer) != scale {
            self.scales.insert(layer, scale);
            // A range locked on one scale says little on the other.
            self.locked.remove(&layer);
        }
    }

    /// Whether the scale choice means anything for this layer.
    pub fn scale_applies(layer: LayerId) -> bool {
        layer != LayerId::Genotype && !layer.is_boolean()
    }

    /// The palette used for `layer`.
    pub fn palette_for(&self, layer: LayerId) -> Palette {
        if layer == LayerId::Genotype {
            Palette::OkabeIto
        } else {
            self.palette
        }
    }

    pub fn is_locked(&self, layer: LayerId) -> bool {
        self.locked.contains_key(&layer)
    }

    /// Fixes the colour range of `layer` to `legend`'s.
    pub fn lock(&mut self, layer: LayerId, legend: &Legend) {
        self.locked.insert(layer, (legend.min, legend.max));
    }

    pub fn unlock(&mut self, layer: LayerId) {
        self.locked.remove(&layer);
    }

    /// The locked legend of `layer`, if any.
    pub fn locked_legend(&self, layer: LayerId) -> Option<Legend> {
        self.locked
            .get(&layer)
            .map(|&(lo, hi)| Legend::new(lo, hi, self.scale(layer)))
    }
}

/// A rendered map layer: one RGBA8 pixel per cell.
#[derive(Clone, Debug, PartialEq)]
pub struct MapImage {
    pub layer: LayerId,
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
    /// The legend the colours were mapped with (`None` for the
    /// categorical genotype layer and for a layer without values).
    pub legend: Option<Legend>,
    pub palette: Palette,
    pub vision: ColorVision,
    pub update: i32,
}

/// Renders the selected layer of the frame. `None` when the frame has no
/// map, or was captured for another layer (a subscription change still
/// in flight), so the caller keeps showing the previous image.
pub fn render_map(
    frame: &Frame,
    settings: &MapSettings,
    highlight: Option<i32>,
) -> Option<MapImage> {
    let map = frame.map.as_ref()?;
    let layer = settings.layer;
    if layer != LayerId::Genotype && map.layer(layer).is_none() {
        return None;
    }
    let palette = settings.palette_for(layer);
    let scale = settings.scale(layer);
    let (mut rgba, legend) = if layer == LayerId::Genotype {
        (render_rgba(map, layer, &palette, scale), None)
    } else {
        let legend = settings
            .locked_legend(layer)
            .or_else(|| layer_legend(map, layer, scale));
        let rgba = match &legend {
            Some(l) => render_rgba_with_legend(map, layer, &palette, l),
            None => render_rgba(map, layer, &palette, scale),
        };
        (rgba, legend)
    };
    if let Some(g) = highlight {
        dim_except(&mut rgba, &map.genotype_id, g);
    }
    settings.vision.apply(&mut rgba);
    Some(MapImage {
        layer,
        w: usize::try_from(map.w).unwrap_or(0),
        h: usize::try_from(map.h).unwrap_or(0),
        rgba,
        legend,
        palette,
        vision: settings.vision,
        update: frame.update,
    })
}

/// Alpha of the cells outside a highlighted genotype.
pub const DIMMED_ALPHA: u8 = 38;

/// Fades every pixel whose cell does not hold genotype `keep`.
pub fn dim_except(rgba: &mut [u8], genotype_ids: &[i32], keep: i32) {
    for (px, &g) in rgba.as_chunks_mut::<4>().0.iter_mut().zip(genotype_ids) {
        if g != keep {
            px[3] = px[3].min(DIMMED_ALPHA);
        }
    }
}

/// What the map tooltip says about one cell.
#[derive(Clone, Debug, PartialEq)]
pub struct CellInfo {
    pub cell: usize,
    pub x: usize,
    pub y: usize,
    /// `None` for an empty cell.
    pub genotype: Option<GenotypeLabel>,
    /// The selected layer's value (`None` for the genotype layer, empty
    /// cells of per-organism layers, and layers not captured yet).
    pub value: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenotypeLabel {
    pub id: i32,
    pub name: String,
    /// Index among the most abundant genotypes (its colour).
    pub slot: Option<usize>,
}

pub fn cell_info(frame: &Frame, layer: LayerId, cell: usize) -> Option<CellInfo> {
    let map = frame.map.as_ref()?;
    let w = usize::try_from(map.w).ok().filter(|&w| w > 0)?;
    let gid = *map.genotype_id.get(cell)?;
    let genotype = (gid != -1).then(|| GenotypeLabel {
        id: gid,
        name: genotype_name(frame, gid),
        slot: map.genotype_slot(gid),
    });
    let value = if layer == LayerId::Genotype {
        None
    } else {
        map.layer(layer)
            .and_then(|v| v.get(cell))
            .filter(|v| v.is_finite())
            .map(|&v| f64::from(v))
    };
    Some(CellInfo {
        cell,
        x: cell % w,
        y: cell / w,
        genotype,
        value,
    })
}

/// A genotype's name as listed in the frame (see [`shown_name`]), or
/// `genotype #id` when it is not among the listed ones.
pub fn genotype_name(frame: &Frame, gid: i32) -> String {
    if gid == UNCLASSIFIED {
        return "unclassified".to_string();
    }
    frame
        .genotypes
        .iter()
        .find(|g| g.id == gid)
        .map(|g| shown_name(&g.name, g.id))
        .or_else(|| {
            frame
                .dominant
                .as_ref()
                .filter(|d| d.row.id == gid)
                .map(|d| shown_name(&d.row.name, d.row.id))
        })
        .unwrap_or_else(|| format!("genotype #{gid}"))
}

/// How a genotype is named on screen. Genotypes below the threshold all
/// share Avida's placeholder name (`100-no_name`), so their id is added
/// to tell them apart: `100-no_name #12`.
pub fn shown_name(name: &str, id: i32) -> String {
    if name.is_empty() {
        format!("genotype #{id}")
    } else if name.ends_with("no_name") {
        format!("{name} #{id}")
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::fixtures::frame_with_map;

    #[test]
    fn unnamed_genotypes_are_told_apart_by_id() {
        assert_eq!(shown_name("100-no_name", 12), "100-no_name #12");
        assert_eq!(shown_name("100-aaaaa", 7), "100-aaaaa");
        assert_eq!(shown_name("", 3), "genotype #3");
    }

    #[test]
    fn layers_follow_the_environment() {
        let f = frame_with_map();
        assert_eq!(layer_title(&f.env, LayerId::Task(0)), "Task: Nand");
        assert_eq!(layer_title(&f.env, LayerId::GenomeLen), "Genome length");
        assert_eq!(layer_group(LayerId::Task(0)), "Tasks");
    }

    #[test]
    fn genotype_layer_is_categorical_and_dimming_keeps_one_genotype() {
        let f = frame_with_map();
        let s = MapSettings::default();
        let img = render_map(&f, &s, None).unwrap();
        assert_eq!((img.w, img.h), (3, 2));
        assert!(img.legend.is_none());
        assert_eq!(img.palette, Palette::OkabeIto);
        assert_eq!(img.rgba[3], 255);
        assert_eq!(img.rgba[2 * 4 + 3], 0, "empty cell is transparent");
        let dimmed = render_map(&f, &s, Some(8)).unwrap();
        assert_eq!(dimmed.rgba[3], DIMMED_ALPHA);
        assert_eq!(dimmed.rgba[4 + 3], 255);
        // The colour-vision preview changes colours, not alpha.
        let s = MapSettings {
            vision: ColorVision::Achromatopsia,
            ..MapSettings::default()
        };
        let grey = render_map(&f, &s, None).unwrap();
        assert_eq!(grey.rgba[0], grey.rgba[1]);
        assert_eq!(grey.rgba[1], grey.rgba[2]);
        assert_eq!(grey.rgba[2 * 4 + 3], 0);
        assert_eq!(grey.vision, ColorVision::Achromatopsia);
    }

    #[test]
    fn a_frame_for_another_layer_is_not_rendered() {
        let f = frame_with_map();
        let s = MapSettings {
            layer: LayerId::Merit,
            ..MapSettings::default()
        };
        assert!(render_map(&f, &s, None).is_none());
    }

    #[test]
    fn locked_legends_survive_new_values_and_reset_on_scale_change() {
        let f = frame_with_map();
        let mut s = MapSettings {
            layer: LayerId::Fitness,
            ..MapSettings::default()
        };
        assert_eq!(s.scale(LayerId::Fitness), Scale::Log);
        let img = render_map(&f, &s, None).unwrap();
        let legend = img.legend.unwrap();
        assert_eq!((legend.min, legend.max), (1.0, 4.0));
        s.lock(LayerId::Fitness, &Legend::new(0.5, 8.0, Scale::Log));
        let img = render_map(&f, &s, None).unwrap();
        assert_eq!(img.legend.as_ref().map(|l| l.max), Some(8.0));
        s.set_scale(LayerId::Fitness, Scale::Linear);
        assert!(!s.is_locked(LayerId::Fitness));
    }

    #[test]
    fn cell_info_names_the_genotype_and_reads_the_value() {
        let f = frame_with_map();
        let info = cell_info(&f, LayerId::Fitness, 3).unwrap();
        assert_eq!((info.x, info.y), (0, 1));
        assert_eq!(info.value, Some(2.0));
        let g = info.genotype.unwrap();
        assert_eq!((g.id, g.name.as_str(), g.slot), (7, "100-aaaaa", Some(0)));
        let empty = cell_info(&f, LayerId::Fitness, 2).unwrap();
        assert!(empty.genotype.is_none() && empty.value.is_none());
        assert_eq!(genotype_name(&f, 8), "genotype #8");
        assert!(cell_info(&f, LayerId::Fitness, 99).is_none());
        let map = f.map.as_ref().unwrap();
        assert_eq!(map.first_cell_of(7), Some(0));
        assert_eq!(map.cells_of(7), 2);
    }
}
