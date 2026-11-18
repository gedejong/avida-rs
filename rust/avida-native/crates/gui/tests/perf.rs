//! Frame costs on a 300 x 300 world full of organisms, and of the charts
//! with a long history. It prints numbers rather than asserting tight
//! limits, so it is left out of the default run; build it optimised:
//! `cargo test --release -p avida-gui --test perf -- --ignored --nocapture`.

mod common;

use std::time::{Duration, Instant};

use avida_gui::model::{Phase, SideTab, SpeedSetting};
use avida_gui::StartupArgs;
use avida_session::{LayerId, RunState, UpdateRow};
use common::{fresh_config, harness, settle, wait_until};
use egui::vec2;

type H = egui_kittest::Harness<'static, avida_gui::AvidaApp>;

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Median and 95th percentile of `samples`, in milliseconds.
fn summary(mut samples: Vec<f64>) -> (f64, f64) {
    samples.sort_by(f64::total_cmp);
    let at = |q: f64| samples[((samples.len() - 1) as f64 * q).round() as usize];
    (at(0.5), at(0.95))
}

/// One frame as the app pays for it on the CPU: the UI pass plus turning
/// its shapes into triangles. Kittest adds an AccessKit tree update that a
/// real window only pays for with a screen reader on; the GPU upload and
/// draw are not included.
fn frame(h: &mut H) -> f64 {
    let start = Instant::now();
    h.step();
    let out = h.output();
    let _ = h.ctx.tessellate(out.shapes.clone(), out.pixels_per_point);
    ms(start.elapsed())
}

/// Times `n` frames, each after `before` has run.
fn frames(h: &mut H, n: usize, mut before: impl FnMut(&mut H, usize)) -> (f64, f64) {
    let mut samples = Vec::with_capacity(n);
    for i in 0..n {
        before(h, i);
        samples.push(frame(h));
    }
    summary(samples)
}

fn report(what: &str, (median, p95): (f64, f64)) {
    eprintln!("{what:<58} median {median:>7.2} ms   p95 {p95:>7.2} ms");
}

#[test]
#[ignore = "a measurement; run it explicitly in a release build"]
fn frame_costs_on_a_300_by_300_world() {
    let dir = fresh_config("perf");
    let events = avida_config::convert::section_lines(&dir, "avida.yaml", "events").unwrap();
    avida_config::convert::replace_section(
        &dir,
        "avida.yaml",
        "events",
        &events
            .replace(
                "u begin Inject default-classic.org",
                "u begin InjectAll filename=default-classic.org",
            )
            .replace("u 100 Exit", "u 100000 Exit"),
    )
    .unwrap();
    let mut h = harness(
        StartupArgs {
            dir: Some(dir),
            avida_args: ["-set", "world.x", "300", "-set", "world.y", "300"]
                .map(String::from)
                .to_vec(),
            ..Default::default()
        },
        vec2(1440.0, 900.0),
    );
    wait_until(&mut h, "the world", |app| {
        let s = app.state();
        s.phase == Phase::Ready && s.frame.as_ref().is_some_and(|f| f.map.is_some())
    });
    h.state_mut().state_mut().run_to(3);
    wait_until(&mut h, "update 3", |app| {
        let s = app.state();
        s.run_state() == Some(RunState::Paused) && s.history.latest().is_some_and(|r| r.update >= 3)
    });
    let orgs = h.state().state().organisms().unwrap_or(0);
    eprintln!("world 300 x 300, {orgs} organisms");
    assert!(orgs > 80_000, "the world is full: {orgs}");
    settle(&mut h);

    // Paused, nothing changes.
    h.state_mut().state_mut().side_tab = SideTab::Stats;
    settle(&mut h);
    report("idle frame, stats tab", frames(&mut h, 60, |_, _| {}));

    // A new map image every frame: alternate two layers.
    let layers = [LayerId::Genotype, LayerId::Fitness];
    let mut renders = Vec::new();
    for i in 0..20 {
        h.state_mut().state_mut().set_layer(layers[i % 2]);
        // The new layer's values arrive with the next frame from the
        // session; wait for the image to change.
        let t0 = Instant::now();
        wait_until(&mut h, "the layer", |app| {
            app.map_image().is_some_and(|m| m.layer == layers[i % 2])
        });
        renders.push(ms(t0.elapsed()));
    }
    report(
        "layer switch until the new map is on screen",
        summary(renders),
    );

    // Zoomed in: hatching, grid lines, the overview map.
    h.state_mut().state_mut().viewport.zoom = 24.0;
    h.state_mut().state_mut().viewport.auto_fit = false;
    settle(&mut h);
    report(
        "idle frame, map zoomed with overview",
        frames(&mut h, 60, |_, _| {}),
    );
    h.state_mut().state_mut().viewport.auto_fit = true;

    // Running at full speed with the charts open.
    h.state_mut().state_mut().side_tab = SideTab::Charts;
    h.state_mut().state_mut().set_speed(SpeedSetting::default());
    h.state_mut().state_mut().toggle_run();
    wait_until(&mut h, "running", |app| {
        app.state().run_state() == Some(RunState::Running)
    });
    // Paced like a 60 Hz display for a few seconds, so that new updates
    // (a new map image, new chart rows) arrive between frames.
    let u0 = h.state().state().current_update().unwrap_or(0);
    let t0 = Instant::now();
    let (mut all, mut fresh) = (Vec::new(), Vec::new());
    while t0.elapsed() < Duration::from_secs(8) {
        let before = h.state().state().current_update();
        let cost = frame(&mut h);
        all.push(cost);
        if h.state().state().current_update() != before {
            fresh.push(cost);
        }
        std::thread::sleep(Duration::from_secs_f64(
            (1.0 / 60.0 - cost / 1000.0).max(0.0),
        ));
    }
    let updates = h.state().state().current_update().unwrap_or(0) - u0;
    let ups = f64::from(updates) / t0.elapsed().as_secs_f64();
    report("frame while running, charts tab", summary(all));
    if !fresh.is_empty() {
        report(
            &format!("  of which frames with a new update ({})", fresh.len()),
            summary(fresh),
        );
    }
    eprintln!("{:<58} {ups:.1} updates/s", "simulation speed meanwhile");
    h.state_mut().state_mut().toggle_run();
    wait_until(&mut h, "paused", |app| {
        app.state().run_state() == Some(RunState::Paused)
    });

    // Charts with a long history: 200,000 rows, ten more each frame.
    let last = h.state().state().history.latest().cloned().unwrap();
    let synth = |u: i32| UpdateRow {
        update: u,
        orgs: 90_000 + (u % 977),
        ave_fitness: 0.25 + f64::from(u % 101) / 1000.0,
        task_last_count: (0..9).map(|k| (u + k) % 500).collect(),
        ..last.clone()
    };
    let base = last.update + 1;
    h.state_mut()
        .state_mut()
        .history
        .extend((base..base + 200_000).map(synth));
    let first = frame(&mut h);
    eprintln!(
        "{:<58} {first:>7.2} ms",
        "first charts frame over 200,000 rows"
    );
    let mut next = base + 200_000;
    report(
        "charts frame, 200,000 rows, 10 new rows per frame",
        frames(&mut h, 120, |h, _| {
            h.state_mut()
                .state_mut()
                .history
                .extend((next..next + 10).map(synth));
            next += 10;
        }),
    );
    h.state_mut().state_mut().chart_tab = avida_gui::model::ChartTab::Tasks;
    settle(&mut h);
    report(
        "tasks chart, 200,000 rows, 10 new rows per frame",
        frames(&mut h, 120, |h, _| {
            h.state_mut()
                .state_mut()
                .history
                .extend((next..next + 10).map(synth));
            next += 10;
        }),
    );
}

/// What the filter costs on a 160 x 160 world (25,600 organisms): the
/// selection and the subset's row per frame, and a UI frame of each chart
/// view without and with a filter.
#[test]
#[ignore = "a measurement; run it explicitly in a release build"]
fn filtering_costs_on_a_160_by_160_world() {
    use avida_gui::model::ChartTab;
    use avida_viewmodel::{subset_row, Condition, Prop};

    let dir = fresh_config("perf-filter");
    let events = avida_config::convert::section_lines(&dir, "avida.yaml", "events").unwrap();
    avida_config::convert::replace_section(
        &dir,
        "avida.yaml",
        "events",
        &events
            .replace(
                "u begin Inject default-classic.org",
                "u begin InjectAll filename=default-classic.org",
            )
            .replace("u 100 Exit", "u 100000 Exit"),
    )
    .unwrap();
    let mut h = harness(
        StartupArgs {
            dir: Some(dir),
            avida_args: [
                "-set",
                "world.x",
                "160",
                "-set",
                "world.y",
                "160",
                "-set",
                "mutation.divide_ins_prob",
                "0.08",
                "-set",
                "mutation.divide_del_prob",
                "0.08",
            ]
            .map(String::from)
            .to_vec(),
            ..Default::default()
        },
        vec2(1440.0, 900.0),
    );
    wait_until(&mut h, "the world", |app| {
        let s = app.state();
        s.phase == Phase::Ready && s.frame.as_ref().is_some_and(|f| f.map.is_some())
    });
    h.state_mut().state_mut().run_to(12);
    wait_until(&mut h, "update 12", |app| {
        let s = app.state();
        s.run_state() == Some(RunState::Paused)
            && s.history.latest().is_some_and(|r| r.update >= 12)
    });
    eprintln!(
        "world 160 x 160, {} organisms",
        h.state().state().organisms().unwrap_or(0)
    );
    h.state_mut().state_mut().side_tab = SideTab::Charts;
    // A filter that needs every property layer and keeps part of the world.
    h.state_mut().state_mut().edit_filter(|f| {
        f.set(Condition::range(Prop::GenomeLen, 0.0, 1e9));
        f.set(Condition::Task {
            task: 0,
            performs: false,
        });
        f.set(Condition::region((10, 10), (120, 120)));
    });
    h.state_mut().state_mut().chart_tab = ChartTab::Scatter;
    wait_until(&mut h, "the layers", |app| {
        app.state()
            .frame
            .as_ref()
            .and_then(|f| f.map.as_ref())
            .is_some_and(|m| Prop::ALL.iter().all(|p| m.layer(p.layer()).is_some()))
    });
    settle(&mut h);

    // The per-frame work of the view-model.
    let frame_arc = h.state().state().frame.clone().unwrap();
    let map = frame_arc.map.as_ref().unwrap();
    let filter = h.state().state().brush.filter.clone();
    let mut select = Vec::new();
    let mut row = Vec::new();
    let mut points = Vec::new();
    for _ in 0..200 {
        let t0 = Instant::now();
        let sel = filter.select(map);
        select.push(ms(t0.elapsed()));
        let t0 = Instant::now();
        let _ = subset_row(frame_arc.update, map, &sel);
        row.push(ms(t0.elapsed()));
        let t0 = Instant::now();
        let p = avida_viewmodel::scatter_points(map, Prop::GenomeLen, Prop::Fitness);
        let _ = avida_viewmodel::thin_points(&p, Some(&sel), 60_000);
        points.push(ms(t0.elapsed()));
    }
    report("Filter::select, three conditions", summary(select));
    report("subset_row", summary(row));
    report("scatter_points + thin_points", summary(points));
    let matched = h.state().state().brush.selection().map_or(0, |s| s.matched);
    eprintln!("organisms matching the filter: {matched}");

    for (tab, name) in [
        (ChartTab::Scatter, "scatter"),
        (ChartTab::Histogram, "histogram"),
        (ChartTab::Tasks, "tasks"),
        (ChartTab::TimeSeries, "time series"),
        (ChartTab::OpenEnded, "open-ended"),
    ] {
        h.state_mut().state_mut().chart_tab = tab;
        settle(&mut h);
        report(
            &format!("idle frame, {name}, with the filter"),
            frames(&mut h, 60, |_, _| {}),
        );
    }
    h.state_mut().state_mut().clear_filter();
    for (tab, name) in [
        (ChartTab::Scatter, "scatter"),
        (ChartTab::Histogram, "histogram"),
        (ChartTab::Tasks, "tasks"),
    ] {
        h.state_mut().state_mut().chart_tab = tab;
        settle(&mut h);
        report(
            &format!("idle frame, {name}, without a filter"),
            frames(&mut h, 60, |_, _| {}),
        );
    }
    // The scatter plot while the run advances (a new frame every time).
    h.state_mut().state_mut().chart_tab = ChartTab::Scatter;
    h.state_mut().state_mut().edit_filter(|f| {
        f.set(Condition::range(Prop::GenomeLen, 0.0, 1e9));
    });
    settle(&mut h);
    h.state_mut().state_mut().set_speed(SpeedSetting::default());
    h.state_mut().state_mut().toggle_run();
    wait_until(&mut h, "running", |app| {
        app.state().run_state() == Some(RunState::Running)
    });
    let t0 = Instant::now();
    let mut all = Vec::new();
    while t0.elapsed() < Duration::from_secs(6) {
        let cost = frame(&mut h);
        all.push(cost);
        std::thread::sleep(Duration::from_secs_f64(
            (1.0 / 60.0 - cost / 1000.0).max(0.0),
        ));
    }
    report("frame while running, scatter with a filter", summary(all));
    h.state_mut().state_mut().toggle_run();
}
