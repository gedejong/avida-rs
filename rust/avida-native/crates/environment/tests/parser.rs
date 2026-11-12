//! The environment-file parser (`cEnvironment::Load`, `LoadLine` and the
//! `Load*` functions) and the reaction/resource modifiers: values, defaults
//! and the exact C++ feedback text on errors.

use avida_apto::AvidaRng;
use avida_config::config::Config;
use avida_config::feedback::{Feedback, FeedbackKind};
use avida_environment::{geometry, proc_type, Environment, Resource, SimpleTaskLib};
use std::path::PathBuf;

fn new_env(config: &Config) -> Environment<SimpleTaskLib> {
    Environment::new(config, SimpleTaskLib::new())
}

/// Load `line` into a fresh environment; the error and every message.
fn fail(line: &str, config: &Config) -> (String, Vec<String>) {
    let mut env = new_env(config);
    let mut fb = Feedback::new();
    let err = env.load_line(line, &mut fb, config).unwrap_err();
    let msgs = fb.messages().iter().map(|(_, m)| m.clone()).collect();
    (err, msgs)
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("avida_env_parser_{}_{}", tag, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn keyword_and_reaction_errors() {
    let c = Config::new();
    let (err, msgs) = fail("MUTATION point 0.1", &c);
    assert_eq!(err, "unknown environment keyword 'MUTATION'");
    assert_eq!(msgs, vec!["unknown environment keyword 'MUTATION'"]);

    let (err, msgs) = fail("REACTION X not process:bogus=1", &c);
    assert_eq!(
        msgs,
        vec![
            "unknown process variable 'bogus' in reaction 'X'",
            "failed in loading reaction-process...",
            "failed in loading 'REACTION'",
        ]
    );
    assert_eq!(err, "failed in loading 'REACTION'");

    let (_, msgs) = fail("REACTION X not process:type=bogus", &c);
    assert_eq!(
        msgs[0],
        "unknown reaction process type 'bogus' found in 'X'"
    );
    let (_, msgs) = fail("REACTION X not process:value=abc", &c);
    assert_eq!(msgs[0], "in reaction 'X' process, value set to non-number");
    let (_, msgs) = fail("REACTION X not process:value=", &c);
    assert_eq!(
        msgs[0],
        "no value given for 'value' in reaction 'X' process"
    );
    let (_, msgs) = fail("REACTION X not process:=3", &c);
    assert_eq!(
        msgs[0],
        "no variable povided to set to '3' in 'reaction 'X' process'"
    );
    let (_, msgs) = fail("REACTION X not process:resource=nope", &c);
    assert_eq!(
        msgs[0],
        "in reaction 'X' process, 'resource' setting of 'nope' not found"
    );
    let (_, msgs) = fail("REACTION X not process:phenplastbonus=some", &c);
    assert_eq!(
        msgs[0],
        "invalid setting for phenplastbonus in reaction 'X'"
    );
    let (_, msgs) = fail("REACTION X not process:germline=2", &c);
    assert_eq!(msgs[0], "in reaction 'X' process, germline set to non-bool");
    let (_, msgs) = fail("REACTION X not requisite:bogus=1", &c);
    assert_eq!(
        msgs,
        vec![
            "unknown requisite variable 'bogus' in reaction 'X'",
            "failed in loading reaction-requisite...",
            "failed in loading 'REACTION'",
        ]
    );
    let (_, msgs) = fail("REACTION X not requisite:max_count=x", &c);
    assert_eq!(
        msgs[0],
        "in reaction 'X' requisite, max_count set to non-integer"
    );
    let (_, msgs) = fail("REACTION X not other:a=1", &c);
    assert_eq!(msgs[0], "unknown entry type 'other' in reaction 'X'");
    let (_, msgs) = fail("REACTION", &c);
    assert_eq!(msgs[0], "each reaction must include a name and trigger");
    let (_, msgs) = fail("REACTION X :a=1", &c);
    assert_eq!(msgs[0], "unknown/unprocessed task entry ''");
}

#[test]
fn reaction_definitions() {
    let c = Config::new();
    let mut env = new_env(&c);
    let mut fb = Feedback::new();
    // A requisite may only name a known reaction; a bare name pre-declares.
    let (_, msgs) = fail("REACTION A not requisite:reaction=B", &c);
    assert_eq!(
        msgs[0],
        "in reaction 'A' requisite, 'reaction' setting of 'B' not found"
    );
    for l in [
        "REACTION B",
        "REACTION A not process:value=1.5:type=mult:max=2:min=0.5:frac=0.25 \
         process:value=2:type=energy requisite:reaction=B:min_count=1:divide_only=2",
        "REACTION B not process:value=1",
        "reaction C nand",
        "REACTION A nand process:value=7",
        "REACTION D sort_inputs:size=3",
        "REACTION E sort_inputs:size=3",
    ] {
        if let Err(e) = env.load_line(l, &mut fb, &c) {
            panic!("{}: {}\n{}", l, e, fb.render());
        }
    }
    // Same argument-less trigger: same task; argument tasks never merge.
    assert_eq!(env.num_tasks(), 4);
    assert_eq!(env.reaction_lib().reaction(0).name(), "B");
    assert_eq!(env.reaction_lib().reaction(0).task, Some(0));
    assert_eq!(env.reaction_lib().reaction(3).task, Some(2));
    assert_eq!(env.reaction_lib().reaction(4).task, Some(3));
    // QUIRK: action-trigger ordering is the library size at definition time.
    let trig: Vec<_> = env
        .action_triggers()
        .iter()
        .map(|t| (t.id.as_str(), t.temp_ordering))
        .collect();
    assert_eq!(trig, vec![("not", 0), ("nand", 1), ("sort_inputs", 2)]);

    // Redefinition warns (not so for defining a pre-declared reaction) and
    // appends to the existing reaction.
    assert_eq!(fb.num_messages(), 1);
    assert_eq!(fb.message_type(0), FeedbackKind::Warning);
    assert_eq!(fb.message(0), "re-defining reaction 'A'");
    let a = env.reaction_lib().reaction(1);
    assert_eq!(a.task, Some(1));
    assert_eq!(a.processes.len(), 3);
    let p = &a.processes[0];
    assert_eq!(
        (
            p.value,
            p.proc_type,
            p.max_number,
            p.min_number,
            p.max_fraction
        ),
        (1.5, proc_type::MULT, 2.0, 0.5, 0.25)
    );
    assert_eq!(a.processes[1].proc_type, proc_type::ENERGY);
    let q = &a.requisites[0];
    assert_eq!(q.prior_reactions, vec![0]);
    assert_eq!(q.min_task_count, 1);
    assert_eq!(q.divide_only, 2);
    assert_eq!(env.reaction_lib().reaction(2).processes.len(), 0);

    assert!(env.set_reaction_task("C", "not"));
    assert_eq!(env.reaction_lib().reaction(2).task, Some(0));
    assert!(!env.set_reaction_task("C", "xor"));
    assert!(!env.set_reaction_task("Z", "not"));
}

#[test]
fn cellbox_requisite_validation() {
    let mut c = Config::new();
    c.world_x = 10;
    c.world_y = 10;
    let mut env = new_env(&c);
    env.load_line(
        "REACTION A not requisite:cellbox=1,2,3,4",
        &mut Feedback::new(),
        &c,
    )
    .unwrap();
    let b = env.reaction_lib().reaction(0).requisites[0].cell_box;
    assert_eq!((b.x, b.y, b.width, b.height), (1, 2, 3, 4));
    for (spec, msg) in [
        (
            "10,0",
            "cellbox requisite requires 0 >= < xx < WORLD_X for first argument",
        ),
        ("", "no value given for 'cellbox' in reaction 'A' requisite"),
        (
            "0,-1",
            "cellbox requisite requires 0 >= < yy < WORLD_Y for 2nd argument",
        ),
        // QUIRK: a box reaching the world's edge is rejected.
        (
            "0,0,10,1",
            "cellbox requisite requires 0 < width+xx < WORLD_X for 3rd argument",
        ),
        (
            "0,0,1,0",
            "cellbox requisite requires 0 < height+yy < WORLD_Y for 4th argument",
        ),
    ] {
        let (_, msgs) = fail(&format!("REACTION A not requisite:cellbox={}", spec), &c);
        assert_eq!(msgs[0], msg, "{}", spec);
    }
}

#[test]
fn resources() {
    let mut c = Config::new();
    c.world_x = 20;
    c.world_y = 20;
    let mut env = new_env(&c);
    let mut fb = Feedback::new();
    env.load_line(
        "RESOURCE a:initial=5:inflow=1.5:outflow=0.25:geometry=torus:inflowx=3:inflowy=0:\
         outflowx=0:outflowy=4:xdiffuse=0.5:ygravity=-1:deme=true \
         b",
        &mut fb,
        &c,
    )
    .unwrap();
    let a = env.resource_lib().resource(0);
    assert_eq!((a.initial, a.inflow, a.outflow), (5.0, 1.5, 0.25));
    assert_eq!(a.geometry, geometry::TORUS);
    // A single inflow/outflow point fills in X2/Y2 ...
    assert_eq!((a.inflow_x2, a.inflow_y2), (3, 0));
    assert_eq!(a.outflow_y2, 4);
    // QUIRK: ... but an outflow coordinate of 0 does not (`> 0`).
    assert_eq!(a.outflow_x2, Resource::NONE);
    assert_eq!((a.xdiffuse, a.ygravity), (0.5, -1.0));
    assert!(a.deme_resource);
    assert_eq!(env.resource_lib().num_deme_resources(), 1);
    let b = env.resource_lib().resource(1);
    assert_eq!(b.name(), "b");
    assert_eq!((b.initial, b.inflow, b.outflow), (0.0, 0.0, 0.0));
    assert_eq!(b.geometry, geometry::GLOBAL);
    assert_eq!((b.xdiffuse, b.ydiffuse), (1.0, 1.0));

    let (_, msgs) = fail("RESOURCE a:geometry=cube", &c);
    assert_eq!(msgs[0], "in resource 'a', cube unknown geometry");
    let (_, msgs) = fail("RESOURCE a:bogus=1", &c);
    assert_eq!(msgs[0], "unknown variable 'bogus' in resource 'a'");
    let (_, msgs) = fail("RESOURCE a:deme=maybe", &c);
    assert_eq!(msgs[0], "in resource 'a', maybe must be true or false");
    let (_, msgs) = fail("RESOURCE a:energy=true", &c);
    assert_eq!(
        msgs[0],
        "energy resources can not be used without the energy model"
    );
    let (_, msgs) = fail("RESOURCE", &c);
    assert_eq!(
        msgs,
        vec![
            "resource line with no resources listed",
            "failed in loading 'RESOURCE'"
        ]
    );

    // HGT: needs ENABLE_HGT and a plain global resource. QUIRK: inflowX1
    // defaults to 0, not -99, so it must be reset explicitly.
    let (_, msgs) = fail("RESOURCE frag:hgt=1", &c);
    assert_eq!(msgs[0], "misconfigured HGT resource: frag");
    let (_, msgs) = fail("RESOURCE frag:hgt=1:inflowx=-99", &c);
    assert_eq!(
        msgs[0],
        "resource configured to use HGT, but HGT not enabled"
    );
}

#[test]
fn cell_and_gradient_resources() {
    let mut c = Config::new();
    c.world_x = 10;
    c.world_y = 10;
    let mut env = new_env(&c);
    let mut fb = Feedback::new();
    env.load_line("CELL spot:5,7:initial=3:inflow=1", &mut fb, &c)
        .unwrap();
    env.load_line("CELL spot:7..9:outflow=0.5", &mut fb, &c)
        .unwrap();
    let r = env.resource_lib().resource(0);
    assert_eq!(r.geometry, geometry::GRID);
    assert_eq!(r.inflow_x1, Resource::NONE);
    let cells: Vec<_> = r
        .cell_list
        .iter()
        .map(|c| (c.cell_id, c.initial, c.inflow, c.outflow))
        .collect();
    // Cell 7 is updated in place (all three values), 8 and 9 appended.
    assert_eq!(
        cells,
        vec![
            (5, 3.0, 1.0, 0.0),
            (7, 0.0, 0.0, 0.5),
            (8, 0.0, 0.0, 0.5),
            (9, 0.0, 0.0, 0.5)
        ]
    );
    let (_, msgs) = fail("CELL spot:1:bogus=2", &c);
    assert_eq!(msgs[0], "unknown variable 'bogus' in resource 'spot'");

    env.load_line(
        "GRADIENT_RESOURCE g:peakx=5:peaky=6:height=10:spread=4:plateau=1.5:updatestep=100",
        &mut fb,
        &c,
    )
    .unwrap();
    let g = env.resource_lib().resource(1);
    assert!(g.isgradient);
    assert_eq!(g.geometry, geometry::GRID);
    assert_eq!((g.peakx, g.plateau, g.outflow), (5, 1.5, 1.0));
    let (_, msgs) = fail("GRADIENT_RESOURCE g:halo_anchor_x=x", &c);
    // (sic) the C++ label for halo_anchor_x.
    assert_eq!(
        msgs[0],
        "in gradient resource 'g', halo_halo_anchor_x set to non-integer"
    );
    let (_, msgs) = fail("GRADIENT_RESOURCE g:nope=1", &c);
    assert_eq!(msgs[0], "unknown variable 'nope' in gradient resource 'g'");
}

#[test]
fn state_grids() {
    let c = Config::new();
    let mut env = new_env(&c);
    env.load_line(
        "GRID g:width=2:height=2:initx=0:inity=1:initfacing=3:states=a,b=7:grid=a,a,b,a",
        &mut Feedback::new(),
        &c,
    )
    .unwrap();
    let g = env.state_grid(0);
    assert_eq!(g.name(), "g");
    assert_eq!(
        (g.width(), g.height(), g.initial_x(), g.initial_y()),
        (2, 2, 0, 1)
    );
    assert_eq!(g.initial_facing(), 3);
    assert_eq!(g.states(), &["a".to_string(), "b".to_string()]);
    // Sense values default to the 1-based load order.
    assert_eq!(g.sense_values(), &[1, 7]);
    // Rows are listed top (highest y) first: "a,a / b,a" puts b at (0, 0).
    assert_eq!(g.state_at(0, 0), 1);
    assert_eq!(g.state_at(1, 0), 0);
    assert_eq!(g.state_at(0, 1), 0);
    assert_eq!(g.sense_state_at(0, 0), 7);

    let base = "GRID g:width=2:height=1:initx=0:inity=0:initfacing=0";
    for (rest, msg) in [
        (
            ":states=a,a:grid=a,a",
            "duplicate state identifier for state grid g",
        ),
        (":states=:grid=a,a", "no states defined for state grid g"),
        (
            ":states=a:grid=a,c",
            "state identifier undefined for cell (0, 1) in state grid g",
        ),
        (
            ":states=a:grid=a",
            "grid definition size mismatch for state grid g",
        ),
        (
            ":states=a:grid=a,a,a",
            "grid definition size mismatch for state grid g",
        ),
    ] {
        let (_, msgs) = fail(&format!("{}{}", base, rest), &c);
        assert_eq!(msgs[0], msg, "{}", rest);
    }
    let (_, msgs) = fail(
        "GRID g:width=2:height=1:initx=2:inity=0:initfacing=0:states=a:grid=a,a",
        &c,
    );
    assert_eq!(
        msgs[0],
        "initx and inity must not exceed (width - 1) and (height - 1)"
    );
    let (_, msgs) = fail(
        "GRID g:width=1:height=1:initx=0:inity=0:initfacing=9:states=a:grid=a:extra=1",
        &c,
    );
    assert_eq!(msgs[0], "value of 'initfacing' exceeds its defined range");
    assert_eq!(msgs[1], "unrecognized argument: 'extra'");
}

#[test]
fn set_active_errors() {
    let c = Config::new();
    let (_, msgs) = fail("SET_ACTIVE REACTION nope", &c);
    assert_eq!(msgs[0], "unknown REACTION: 'nope'");
    let (_, msgs) = fail("SET_ACTIVE RESOURCE r 0", &c);
    assert_eq!(msgs[0], "cannot deactivate items of type RESOURCE");
    let mut env = new_env(&c);
    let mut fb = Feedback::new();
    env.load_line("SET_ACTIVE", &mut fb, &c).unwrap();
    assert_eq!(fb.message_type(0), FeedbackKind::Notification);
}

#[test]
fn load_files() {
    let c = Config::new();
    let dir = temp_dir("files");
    let d = dir.to_str().unwrap();
    std::fs::write(
        dir.join("env.cfg"),
        "# comment\nREACTION A not process:value=$V   # trailing\n\nREACTION B\n",
    )
    .unwrap();
    let mut env = new_env(&c);
    let mut fb = Feedback::new();
    let defs = vec![("V".to_string(), "3.5".to_string())];
    let err = env
        .load("env.cfg", d, &mut fb, Some(&defs), &c)
        .unwrap_err();
    assert_eq!(err, "pre-declared reaction 'B' never defined");
    assert_eq!(env.reaction_value_by_name("A"), 3.5);

    let mut env = new_env(&c);
    let mut fb = Feedback::new();
    let err = env.load("missing.cfg", d, &mut fb, None, &c).unwrap_err();
    assert_eq!(err, "failed to load environment 'missing.cfg'");

    std::fs::write(
        dir.join("bad.cfg"),
        "REACTION A not\nGROWTH x\nREACTION B nand\n",
    )
    .unwrap();
    let mut env = new_env(&c);
    let mut fb = Feedback::new();
    let err = env.load("bad.cfg", d, &mut fb, None, &c).unwrap_err();
    assert_eq!(err, "unknown environment keyword 'GROWTH'");
    // Loading stops at the failing line.
    assert_eq!(env.num_reactions(), 1);
}

#[test]
fn reaction_and_resource_modifiers() {
    let c = Config::new();
    let mut env = new_env(&c);
    let mut fb = Feedback::new();
    for l in [
        "RESOURCE r1 r2",
        "REACTION A not process:value=1:resource=r1 requisite:max_count=1",
        "REACTION B nand process:value=2",
        "REACTION C and process:value=3",
        "REACTION D orn",
    ] {
        env.load_line(l, &mut fb, &c).unwrap();
    }
    let mut rng = AvidaRng::new(11);
    assert!(env.set_reaction_value(&mut rng, "B", 5.0));
    assert_eq!(env.reaction_value(1), 5.0);
    assert!(!env.set_reaction_value(&mut rng, "Z", 5.0));
    assert!(env.set_reaction_value_mult("B", 2.0));
    assert_eq!(env.reaction_value_by_name("B"), 10.0);
    // A reaction without processes has value 0 and cannot be changed.
    assert_eq!(env.reaction_value_by_name("D"), 0.0);
    assert_eq!(env.reaction_value(99), 0.0);

    assert!(env.set_reaction_value(&mut rng, "ALL", 0.5));
    assert_eq!(env.reaction_value(0), 0.5);
    assert_eq!(env.reaction_value(2), 0.5);

    // RANDOM:n picks n distinct reactions with Random::Choose.
    let mut rng = AvidaRng::new(11);
    let mut shadow = AvidaRng::new(11);
    assert!(env.set_reaction_value(&mut rng, "RANDOM:2", 9.0));
    let mut chosen = vec![0; 2];
    shadow.choose(4, &mut chosen);
    for i in 0..3 {
        let expect = if chosen.contains(&i) { 9.0 } else { 0.5 };
        assert_eq!(env.reaction_value(i), expect, "reaction {}", i);
    }
    assert!(!env.set_reaction_value(&mut rng, "RANDOM:5", 1.0));

    assert!(env.set_reaction_inst("A", "nop-C"));
    assert_eq!(env.reaction_lib().reaction(0).processes[0].inst, "nop-C");
    assert!(env.set_reaction_max_task_count("A", 4));
    assert!(env.set_reaction_min_task_count("A", 2));
    assert!(env.set_reaction_max_count("A", 6));
    assert!(env.set_reaction_min_count("A", 3));
    let q = &env.reaction_lib().reaction(0).requisites[0];
    assert_eq!(
        (
            q.min_task_count,
            q.max_task_count,
            q.min_reaction_count,
            q.max_reaction_count
        ),
        (2, 4, 3, 6)
    );
    // No requisite to change on B.
    assert!(!env.set_reaction_max_count("B", 6));

    assert!(env.set_resource_inflow("r2", 4.0));
    assert!(env.set_resource_outflow("r2", 0.5));
    let r2 = env.resource_lib().resource(1);
    assert_eq!((r2.inflow, r2.outflow), (4.0, 0.5));
    assert!(!env.set_resource_inflow("r3", 1.0));

    assert!(env.change_resource(0, "r2", 0));
    assert_eq!(
        env.reaction_lib().reaction(0).processes[0].resource,
        Some(1)
    );
    assert!(env.change_resource(0, "none", 0));
    assert_eq!(env.reaction_lib().reaction(0).processes[0].resource, None);
    assert!(!env.change_resource(0, "r1", 3));
}

#[test]
fn attack_prey_targets_and_groups() {
    let mut c = Config::new();
    c.pred_prey_switch = -1;
    c.default_group = 4;
    let mut env = new_env(&c);
    assert_eq!(env.group_ids().iter().copied().collect::<Vec<_>>(), vec![4]);
    env.add_target_id(2, &c);
    env.add_target_id(0, &c);
    assert_eq!(env.attack_prey_ft_list(), &[-1, 0, 2]);
    env.add_target_id(-1, &c);
    assert_eq!(env.attack_prey_ft_list(), &[-1, 0, 2]);

    c.pred_prey_switch = 0;
    let mut env = new_env(&c);
    env.add_target_id(0, &c);
    assert_eq!(env.attack_prey_ft_list(), &[-3, -2, -1, 0]);
    env.add_target_id(-2, &c);
    assert_eq!(env.attack_prey_ft_list(), &[-3, -2, -1, 0]);
    assert!(env.is_target_id(-2) && !env.is_target_id(5));

    env.add_habitat(3);
    assert!(env.is_habitat(3) && !env.is_habitat(1));
    env.add_group_id(1);
    assert!(env.is_group_id(1));
}
