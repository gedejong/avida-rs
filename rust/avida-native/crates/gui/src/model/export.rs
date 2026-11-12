//! Exporting the accumulated rows as CSV and the current map as PNG.

use std::fmt::Write as _;
use std::io::Cursor;

use avida_session::{LayerId, UpdateRow};
use avida_viewmodel::{EnvView, GenotypeRow};

use crate::model::metrics::Metric;

/// The rows as CSV: `update`, every [`Metric`] by its key, then one column
/// per task (`task:<name>`) and per resource (`resource:<name>`). Names
/// come from `env` when given.
pub fn rows_csv(rows: &[UpdateRow], env: Option<&EnvView>) -> String {
    let num_tasks = rows
        .iter()
        .map(|r| r.task_last_count.len())
        .max()
        .unwrap_or(0);
    let num_res = rows.iter().map(|r| r.resources.len()).max().unwrap_or(0);
    let mut out = String::from("update");
    for m in Metric::ALL {
        out.push(',');
        out.push_str(m.key());
    }
    for k in 0..num_tasks {
        let name = env
            .and_then(|e| e.tasks.get(k))
            .map_or_else(|| k.to_string(), |t| t.name.clone());
        out.push(',');
        out.push_str(&csv_field(&format!("task:{name}")));
    }
    for r in 0..num_res {
        let name = env
            .and_then(|e| e.resources.get(r))
            .map_or_else(|| r.to_string(), |res| res.name.clone());
        out.push(',');
        out.push_str(&csv_field(&format!("resource:{name}")));
    }
    out.push('\n');
    for row in rows {
        let _ = write!(out, "{}", row.update);
        for m in Metric::ALL {
            let v = m.value(row);
            if m.is_count() {
                let _ = write!(out, ",{}", v as i64);
            } else {
                let _ = write!(out, ",{v}");
            }
        }
        for k in 0..num_tasks {
            match row.task_last_count.get(k) {
                Some(c) => {
                    let _ = write!(out, ",{c}");
                }
                None => out.push(','),
            }
        }
        for r in 0..num_res {
            match row.resources.get(r) {
                Some(v) => {
                    let _ = write!(out, ",{v}");
                }
                None => out.push(','),
            }
        }
        out.push('\n');
    }
    out
}

/// The genotype table as CSV, one genotype per line in abundance order:
/// what the genotypes tab lists, every column included.
pub fn genotypes_csv(rows: &[GenotypeRow]) -> String {
    let mut out = String::from(
        "rank,id,name,parent_id,organisms,ave_fitness,ave_merit,ave_gestation,genome_length,depth,update_born,established,last_births\n",
    );
    for g in rows {
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{},{},{},{},{},{},{}",
            g.rank,
            g.id,
            csv_field(&g.name),
            g.parent_id,
            g.abundance,
            g.ave_fitness,
            g.ave_merit,
            g.ave_gestation,
            g.genome_len,
            g.depth,
            g.update_born,
            u8::from(g.threshold),
            g.last_births
        );
    }
    out
}

/// Quotes a CSV field when it holds a comma, quote or line break.
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// An integer upscale for a `w` x `h` map so its long side is at least
/// 1024 pixels (at most 32x), so exported maps stay crisp.
pub fn export_scale(w: usize, h: usize) -> u32 {
    let long = w.max(h).max(1);
    (1024usize.div_ceil(long)).clamp(1, 32) as u32
}

/// Encodes RGBA8 cells as a PNG, each cell `scale` x `scale` pixels
/// (nearest neighbour). Empty cells stay transparent.
pub fn png_bytes(rgba: &[u8], w: usize, h: usize, scale: u32) -> Result<Vec<u8>, String> {
    if w == 0 || h == 0 || rgba.len() != w * h * 4 {
        return Err(format!(
            "a {w}x{h} map needs {} bytes of RGBA, got {}",
            w * h * 4,
            rgba.len()
        ));
    }
    let s = scale.max(1) as usize;
    let (pw, ph) = (w * s, h * s);
    let mut px = vec![0u8; pw * ph * 4];
    for y in 0..ph {
        for x in 0..pw {
            let src = ((y / s) * w + x / s) * 4;
            let dst = (y * pw + x) * 4;
            px[dst..dst + 4].copy_from_slice(&rgba[src..src + 4]);
        }
    }
    let img = image::RgbaImage::from_raw(pw as u32, ph as u32, px)
        .ok_or_else(|| "image buffer has the wrong size".to_string())?;
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| format!("PNG encoding failed: {e}"))?;
    Ok(out.into_inner())
}

/// `map-fitness-u120.png`
pub fn map_file_name(layer: LayerId, update: i32) -> String {
    let key: String = layer
        .key()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    format!("map-{}-u{update}.png", key.trim_matches('-'))
}

/// `stats-u120.csv`
pub fn csv_file_name(update: i32) -> String {
    format!("stats-u{update}.csv")
}

/// `genotypes-u120.csv`
pub fn genotypes_file_name(update: i32) -> String {
    format!("genotypes-u{update}.csv")
}

/// `charts-u120.png`
pub fn charts_file_name(update: i32) -> String {
    format!("charts-u{update}.png")
}

/// The part of a screenshot (`width` x `height` RGBA pixels) inside
/// `rect` (points, scaled by `pixels_per_point`), as RGBA rows. `None`
/// when the rectangle misses the image.
pub fn crop_rgba(
    rgba: &[u8],
    width: usize,
    height: usize,
    rect: [f32; 4],
    pixels_per_point: f32,
) -> Option<(Vec<u8>, usize, usize)> {
    let px = |v: f32, max: usize| ((v * pixels_per_point).round().max(0.0) as usize).min(max);
    let (x0, y0) = (px(rect[0], width), px(rect[1], height));
    let (x1, y1) = (px(rect[2], width), px(rect[3], height));
    if x1 <= x0 || y1 <= y0 || rgba.len() != width * height * 4 {
        return None;
    }
    let (w, h) = (x1 - x0, y1 - y0);
    let mut out = Vec::with_capacity(w * h * 4);
    for y in y0..y1 {
        let start = (y * width + x0) * 4;
        out.extend_from_slice(&rgba[start..start + w * 4]);
    }
    Some((out, w, h))
}

#[cfg(test)]
mod tests {
    use avida_viewmodel::{ResourceView, TaskView};

    use super::*;

    #[test]
    fn csv_has_a_header_and_one_line_per_row() {
        let rows: Vec<UpdateRow> = (0..3)
            .map(|u| UpdateRow {
                update: u,
                orgs: 10 + u,
                ave_fitness: 0.5,
                task_last_count: vec![u, 0],
                resources: vec![1.5],
                ..UpdateRow::default()
            })
            .collect();
        let env = EnvView {
            resources: vec![ResourceView {
                name: "glu,cose".into(),
                geometry: 0,
                spatial: false,
                quantity: 1.5,
                inflow: None,
                outflow: None,
            }],
            reactions: Vec::new(),
            tasks: vec![
                TaskView {
                    name: "not".into(),
                    desc: "Not".into(),
                    last_count: 0,
                },
                TaskView {
                    name: "nand".into(),
                    desc: "Nand".into(),
                    last_count: 0,
                },
            ],
        };
        let csv = rows_csv(&rows, Some(&env));
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 4);
        assert!(lines[0].starts_with("update,orgs,genotypes,"));
        assert!(lines[0].ends_with(",task:not,task:nand,\"resource:glu,cose\""));
        let cols = lines[0].split(',').count();
        assert!(lines[2].starts_with("1,11,0,"));
        assert!(lines[2].contains(",0.5,"));
        assert!(lines[2].ends_with(",1,0,1.5"));
        // The quoted header field holds one comma.
        assert_eq!(lines[2].split(',').count() + 1, cols);
    }

    #[test]
    fn the_genotype_table_is_exported_in_rank_order() {
        let rows = vec![GenotypeRow {
            rank: 0,
            id: 7,
            parent_id: -1,
            name: "100-aaaaa".into(),
            abundance: 12,
            depth: 0,
            update_born: -1,
            genome_len: 100,
            threshold: true,
            ave_fitness: 0.25,
            ave_merit: 97.0,
            ave_gestation: 389.0,
            last_births: 3,
        }];
        let csv = genotypes_csv(&rows);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("rank,id,name,"));
        assert_eq!(lines[1], "0,7,100-aaaaa,-1,12,0.25,97,389,100,0,-1,1,3");
        assert_eq!(lines[0].split(',').count(), lines[1].split(',').count());
    }

    #[test]
    fn a_screenshot_is_cropped_to_a_rectangle_in_points() {
        // 4 x 2 pixels, each pixel's red channel its index.
        let rgba: Vec<u8> = (0..8u8).flat_map(|i| [i, 0, 0, 255]).collect();
        let (px, w, h) = crop_rgba(&rgba, 4, 2, [0.5, 0.0, 1.5, 1.0], 2.0).unwrap();
        assert_eq!((w, h), (2, 2));
        assert_eq!(
            px.iter().step_by(4).copied().collect::<Vec<_>>(),
            vec![1, 2, 5, 6]
        );
        assert!(crop_rgba(&rgba, 4, 2, [3.0, 0.0, 9.0, 9.0], 2.0).is_none());
    }

    #[test]
    fn png_is_upscaled_and_decodes_back() {
        let rgba = vec![
            255, 0, 0, 255, 0, 255, 0, 255, //
            0, 0, 255, 255, 0, 0, 0, 0,
        ];
        let bytes = png_bytes(&rgba, 2, 2, 3).unwrap();
        let img = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(img.dimensions(), (6, 6));
        assert_eq!(img.get_pixel(5, 0).0, [0, 255, 0, 255]);
        assert_eq!(img.get_pixel(5, 5).0, [0, 0, 0, 0]);
        assert!(png_bytes(&rgba, 3, 2, 1).is_err());
    }

    #[test]
    fn scale_and_names() {
        assert_eq!(export_scale(60, 60), 18);
        assert_eq!(export_scale(2000, 10), 1);
        assert_eq!(export_scale(1, 1), 32);
        assert_eq!(map_file_name(LayerId::Task(3), 50), "map-task-3-u50.png");
        assert_eq!(csv_file_name(7), "stats-u7.csv");
        assert_eq!(genotypes_file_name(7), "genotypes-u7.csv");
        assert_eq!(charts_file_name(7), "charts-u7.png");
    }
}
