//! The ancestry layer on a real run.

mod common;

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use avida_viewmodel::{capture, render_rgba, LayerId, Palette, Scale, Subscription};

/// A run of the stock configuration, or of the configuration directory
/// in `ANCESTRY_CONFIG` (with `ANCESTRY_ENV`, a file in it, as its
/// environment).
fn stock_run(name: &str, updates: i32) -> avida_world::world::World {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("viewmodel")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    let config = std::env::var_os("ANCESTRY_CONFIG").map_or_else(
        || Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../avida-core/support/config"),
        std::path::PathBuf::from,
    );
    common::copy_dir(&config, &dir);
    if let Some(env) = std::env::var_os("ANCESTRY_ENV") {
        fs::copy(dir.join(env), dir.join("environment.cfg")).unwrap();
    }
    common::append_events(&dir, &format!("u {updates} Exit\n"));
    let mut world = common::build_world(&dir);
    let mut out = Vec::new();
    common::run_with_stops(&mut world, updates, &mut out, |_| {});
    world
}

fn ancestry_frame(world: &avida_world::world::World) -> avida_viewmodel::MapFrame {
    let sub = Subscription {
        layers: vec![LayerId::Genotype, LayerId::Ancestry],
        ..Subscription::default()
    };
    capture(world, &sub).map.expect("a map")
}

#[test]
fn every_organism_is_in_a_family_and_a_genotype_has_one_value() {
    let world = stock_run("ancestry", 300);
    let map = ancestry_frame(&world);
    let pos = map.layer(LayerId::Ancestry).expect("the ancestry layer");
    let mut by_genotype: HashMap<i32, f32> = HashMap::new();
    let mut organisms = 0u32;
    for (&g, &v) in map.genotype_id.iter().zip(pos) {
        if g == -1 {
            assert!(v.is_nan());
            continue;
        }
        organisms += 1;
        assert!(map.family_of_value(v).is_some(), "genotype {g}: {v}");
        assert_eq!(*by_genotype.entry(g).or_insert(v), v);
    }
    assert!(organisms > 30 && by_genotype.len() > 5);
    // One founder: everybody is in a family, and the families' sizes add
    // up to the population.
    assert!(!map.families.is_empty() && map.families.len() <= avida_viewmodel::FAMILY_SLOTS);
    assert_eq!(
        map.families.iter().map(|f| f.organisms).sum::<u32>(),
        organisms
    );
    assert!(map
        .families
        .windows(2)
        .all(|w| w[0].organisms >= w[1].organisms));
}

/// Writes the genotype and ancestry layers as raw RGBA for a look:
/// `ANCESTRY_UPDATES=3000 cargo test -p avida-viewmodel --test ancestry -- --ignored`.
#[test]
#[ignore = "writes images for design review; run it explicitly"]
fn dump_layers() {
    let updates = std::env::var("ANCESTRY_UPDATES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2000);
    let world = stock_run("ancestry_dump", updates);
    let started = std::time::Instant::now();
    let map = ancestry_frame(&world);
    println!(
        "capture of {}x{} with the ancestry layer: {:?}",
        map.w,
        map.h,
        started.elapsed()
    );
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("viewmodel");
    for (layer, palette, name) in [
        (LayerId::Genotype, Palette::OkabeIto, "genotype"),
        (LayerId::Ancestry, Palette::OkabeIto, "ancestry"),
    ] {
        let rgba = render_rgba(&map, layer, &palette, Scale::Linear);
        fs::write(dir.join(format!("{name}_{}x{}.rgba", map.w, map.h)), rgba).unwrap();
    }
    for f in map.families.iter() {
        println!(
            "family #{} slot {} organisms {}",
            f.root, f.slot, f.organisms
        );
    }
}
