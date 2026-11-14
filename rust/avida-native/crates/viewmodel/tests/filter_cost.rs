//! What a frame costs on a 160 x 160 world when the filter asks for the
//! property layers. It prints numbers, so it is left out of the default
//! run: `cargo test --release -p avida-viewmodel --test filter_cost -- --ignored --nocapture`.

mod common;

use std::time::Instant;

use avida_viewmodel::{capture, LayerId, Prop, Subscription};

fn median_ms(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

#[test]
#[ignore = "a measurement; run it explicitly in a release build"]
fn capture_with_the_filter_layers_on_a_160_by_160_world() {
    let dir = common::fresh_config("heads_default_100u", "filter_cost");
    let cfg = dir.join("avida.cfg");
    let text = std::fs::read_to_string(&cfg).unwrap();
    std::fs::write(&cfg, format!("{text}\nWORLD_X 160\nWORLD_Y 160\n")).unwrap();
    let events = dir.join("events.cfg");
    let text = std::fs::read_to_string(&events).unwrap();
    std::fs::write(
        &events,
        text.replace(
            "u begin Inject default-classic.org",
            "u begin InjectAll filename=default-classic.org",
        )
        .replace("u 100 Exit", "u 8 Exit")
        .replace("u 100 SavePopulation", ""),
    )
    .unwrap();
    let mut world = common::build_world(&dir);
    let mut out = Vec::new();
    common::run_with_stops(&mut world, 8, &mut out, |_| {});
    eprintln!("organisms: {}", world.pop.num_organisms);

    let sub = |layers: Vec<LayerId>| Subscription {
        layers,
        inspect_cell: None,
        genotypes_top_n: 100,
        lineage_of: None,
    };
    let mut props: Vec<LayerId> = vec![LayerId::Genotype];
    props.extend(Prop::ALL.iter().map(|p| p.layer()));
    let cases = [
        ("genotype layer only", sub(vec![LayerId::Genotype])),
        ("fitness layer only", sub(vec![LayerId::Fitness])),
        ("genotype and the eight property layers", sub(props.clone())),
        ("... and the ancestry layer", {
            let mut l = props.clone();
            l.push(LayerId::Ancestry);
            sub(l)
        }),
    ];
    for (name, s) in cases {
        let times: Vec<f64> = (0..30)
            .map(|_| {
                let t0 = Instant::now();
                let f = capture(&world, &s);
                let ms = t0.elapsed().as_secs_f64() * 1000.0;
                assert!(f.map.is_some());
                ms
            })
            .collect();
        eprintln!("capture, {name:<42} median {:>6.2} ms", median_ms(times));
    }
}
