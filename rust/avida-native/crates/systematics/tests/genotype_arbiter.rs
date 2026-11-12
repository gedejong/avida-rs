//! GenotypeArbiter behaviour, against the C++ control flow in
//! avida-core/source/systematics/GenotypeArbiter.cc and Genotype.cc.

mod common;

use std::collections::HashMap;

use avida_systematics::{
    ClassificationHints, GenotypeArbiter, GestationInfo, GroupHandle, PropertyValue, UnitInfo,
    EVENT_ADD_THRESHOLD, EVENT_REMOVE_THRESHOLD,
};
use common::*;

fn ids(a: &GenotypeArbiter, hs: &[GroupHandle]) -> Vec<i32> {
    hs.iter().map(|&h| a.genotype(h).unwrap().id()).collect()
}

/// A founder unit plus one internal child per entry of `children`
/// (each `(parent_index, sequence)`), classified the way organisms are.
struct Lineage {
    arb: GenotypeArbiter,
    units: Vec<GroupHandle>,
}

impl Lineage {
    fn new(threshold: i32) -> Self {
        let mut arb = GenotypeArbiter::new("genotype", threshold, false, &[]);
        let src = div(true);
        let g = genome(ANCESTOR);
        let u = UnitInfo {
            source: &src,
            genome: &g,
            generation: 0,
        };
        let h = arb.classify_new_unit(&u, None);
        Lineage {
            arb,
            units: vec![h],
        }
    }

    /// A birth from unit `parent`; returns the unit index.
    fn birth(&mut self, parent: usize, seq: &str) -> usize {
        let src = div(false);
        let g = genome(seq);
        let u = UnitInfo {
            source: &src,
            genome: &g,
            generation: 1,
        };
        let pg = self.units[parent];
        let h = self.arb.group_classify_new_unit(pg, &u, Some(&[pg]));
        self.units.push(h);
        self.units.len() - 1
    }

    fn kill(&mut self, unit: usize) {
        self.arb.remove_unit(self.units[unit]);
    }

    fn alive_ids(&self) -> Vec<i32> {
        let mut v: Vec<i32> = (1..100)
            .filter(|&id| self.arb.group(id).is_some())
            .collect();
        v.sort();
        v
    }
}

#[test]
fn same_genome_twice_is_one_genotype_with_two_units() {
    let mut l = Lineage::new(3);
    let g1 = l.units[0];
    {
        let x = l.arb.genotype(g1).unwrap();
        assert_eq!(x.id(), 1);
        assert_eq!(x.num_units(), 1);
        assert_eq!(x.parent_str(), "");
        assert_eq!(x.depth(), 0);
        // Founder of a new best size: threshold and named at once.
        assert!(x.is_threshold());
        assert_eq!(x.name(), "100-aaaaa");
        // External source: no breed-in.
        assert_eq!(x.breed_in().cur(), 0);
        assert_eq!(x.update_born(), -1);
    }
    let c = l.birth(0, ANCESTOR);
    assert_eq!(l.units[c], g1);
    let x = l.arb.genotype(g1).unwrap();
    assert_eq!((x.num_units(), x.total_units()), (2, 2));
    assert_eq!((x.births().cur(), x.breed_true().cur()), (1, 1));
    assert_eq!(x.active_reference_count(), 2);
    assert_eq!(l.arb.stats().tot_genotypes, 1);
    assert_eq!(l.arb.dominant(), Some(g1));

    // An unrelated injection of the same genome finds it by hash.
    let src = div(true);
    let gn = genome(ANCESTOR);
    let u = UnitInfo {
        source: &src,
        genome: &gn,
        generation: 0,
    };
    assert_eq!(l.arb.classify_new_unit(&u, None), g1);
    assert_eq!(l.arb.genotype(g1).unwrap().num_units(), 3);
}

#[test]
fn mutant_child_links_to_parent_and_names_at_threshold() {
    let mut l = Lineage::new(3);
    l.arb.set_event_recording(true);
    let g1 = l.units[0];
    l.birth(0, ANCESTOR); // g1 has 2 units
    let m = mutate(10, 'x');
    let c = l.birth(0, &m);
    let g2 = l.units[c];
    assert_ne!(g2, g1);
    {
        let x = l.arb.genotype(g2).unwrap();
        assert_eq!(x.id(), 2);
        assert_eq!(x.parents(), &[g1]);
        assert_eq!(x.parent_str(), "1");
        assert_eq!(x.depth(), 1);
        assert!(!x.is_threshold());
        assert_eq!(x.name(), "100-no_name");
        assert_eq!(x.breed_in().cur(), 1);
        assert_eq!(x.generation_born(), 1);
    }
    let p = l.arb.genotype(g1).unwrap();
    assert_eq!(p.passive_reference_count(), 1);
    assert_eq!((p.births().cur(), p.breed_out().cur()), (2, 1));

    // A second birth of the mutant genome from the ancestor joins g2 through
    // the hash search (NotifyNewUnit: breed-in again).
    let c2 = l.birth(0, &m);
    assert_eq!(l.units[c2], g2);
    assert_eq!(l.arb.genotype(g2).unwrap().breed_in().cur(), 2);
    // Equal sizes: the earlier best keeps the front of the size-2 list.
    assert_eq!(ids(&l.arb, &l.arb.active_list(2)), vec![1, 2]);
    assert_eq!(l.arb.dominant(), Some(g1));

    // Third unit: reaches THRESHOLD (3) and becomes the dominant genotype.
    l.birth(c2, &m);
    let x = l.arb.genotype(g2).unwrap();
    assert!(x.is_threshold());
    assert_eq!(x.name(), "100-aaaab");
    assert_eq!(l.arb.dominant(), Some(g2));
    assert_eq!(l.arb.best_size(), 3);
    assert_eq!(l.arb.stats().num_threshold, 2);
    assert_eq!(l.arb.stats().tot_threshold, 2);
    let ev: Vec<(i32, i32)> = l
        .arb
        .drain_events()
        .iter()
        .map(|e| (e.group_id, e.event))
        .collect();
    assert_eq!(ev, vec![(2, EVENT_ADD_THRESHOLD)]);

    // Iteration order: best size first, then smaller sizes.
    assert_eq!(ids(&l.arb, &l.arb.iter_groups()), vec![2, 1]);

    // Properties as print actions read them.
    assert_eq!(
        l.arb.property(g2, "name"),
        PropertyValue::Str("100-aaaab".into())
    );
    assert_eq!(l.arb.property(g2, "parents").string_value(), "1");
    assert!(l.arb.property(g2, "threshold").bool_value());
    assert_eq!(l.arb.property(g2, "fitness"), PropertyValue::Null);
    assert_eq!(l.arb.property(g2, "src_transmission_type").int_value(), 1);
    assert_eq!(
        l.arb.property(g2, "genome").string_value(),
        format!("0,heads_default,{m}")
    );
    assert!(l.arb.has_property(g2, "genome"));
    assert!(!l.arb.has_property(g2, "fitness"));
}

#[test]
fn names_count_per_genome_size_in_base_26() {
    let mut arb = GenotypeArbiter::new("genotype", 1, false, &[]);
    let src = div(true);
    let mut names = Vec::new();
    for k in 0..28 {
        // 28 distinct genomes of length 100.
        let seq = mutate(20 + k / 26, (b'a' + (k % 26) as u8) as char);
        let g = genome(&seq);
        let u = UnitInfo {
            source: &src,
            genome: &g,
            generation: 0,
        };
        let h = arb.classify_new_unit(&u, None);
        // A new genotype is only named on creation when it raises m_best;
        // the second unit runs AdjustGenotype, which names it (size >= 1).
        arb.classify_new_unit(&u, None);
        names.push(arb.genotype(h).unwrap().name().to_string());
    }
    assert_eq!(names[0], "100-aaaaa");
    assert_eq!(names[1], "100-aaaab");
    assert_eq!(names[25], "100-aaaaz");
    assert_eq!(names[26], "100-aaaba");
    assert_eq!(names[27], "100-aaabb");

    // A different length has its own counter.
    let short = genome("rucavutycasvab");
    let u = UnitInfo {
        source: &src,
        genome: &short,
        generation: 0,
    };
    let h = arb.classify_new_unit(&u, None);
    arb.classify_new_unit(&u, None);
    assert_eq!(arb.genotype(h).unwrap().name(), "014-aaaaa");
}

/// Founder g1 -> g2 -> g3, one unit each.
fn chain() -> Lineage {
    let mut l = Lineage::new(3);
    l.birth(0, &mutate(10, 'x'));
    l.birth(1, &mutate(11, 'y'));
    assert_eq!(l.alive_ids(), vec![1, 2, 3]);
    l
}

#[test]
fn removal_oldest_first_keeps_ancestors_historic() {
    let mut l = chain();
    l.arb.set_event_recording(true);
    l.arb.perform_update(9); // cur_update = 10
    l.kill(0);
    // g1 has a child: deactivated into the historic list.
    assert_eq!(ids(&l.arb, &l.arb.historic()), vec![1]);
    let g1 = l.arb.genotype(l.units[0]).unwrap();
    assert!(!g1.is_active() && !g1.is_threshold());
    assert_eq!(g1.update_deactivated(), 10);
    assert_eq!(
        l.arb
            .drain_events()
            .iter()
            .map(|e| (e.group_id, e.event))
            .collect::<Vec<_>>(),
        vec![(1, EVENT_REMOVE_THRESHOLD)]
    );
    l.kill(2);
    // g3 had no children: gone at once.
    assert_eq!(l.alive_ids(), vec![1, 2]);
    assert_eq!(ids(&l.arb, &l.arb.historic()), vec![1]);
    l.kill(1);
    // g2 dies without children; releasing g1 prunes it too.
    assert_eq!(l.alive_ids(), Vec::<i32>::new());
    assert!(l.arb.historic().is_empty());
    assert_eq!(l.arb.num_genotypes_alive(), 0);
    assert_eq!(l.arb.best_size(), 0);
    assert_eq!(l.arb.dominant(), None);
}

#[test]
fn removal_newest_first_prunes_immediately() {
    let mut l = chain();
    l.kill(2);
    assert_eq!(l.alive_ids(), vec![1, 2]);
    l.kill(1);
    assert_eq!(l.alive_ids(), vec![1]);
    assert_eq!(
        l.arb
            .genotype(l.units[0])
            .unwrap()
            .passive_reference_count(),
        0
    );
    l.kill(0);
    assert_eq!(l.alive_ids(), Vec::<i32>::new());
    assert!(l.arb.historic().is_empty());
}

#[test]
fn removal_middle_first_then_leaf_cascades() {
    let mut l = chain();
    l.arb.perform_update(4);
    l.kill(1);
    assert_eq!(ids(&l.arb, &l.arb.historic()), vec![2]);
    l.kill(0);
    // Push adds at the front of the historic list.
    assert_eq!(ids(&l.arb, &l.arb.historic()), vec![1, 2]);
    // Coalescence: g3 is the only living genotype.
    l.arb.update_provided_values(5);
    assert_eq!(l.arb.stats().num_genotypes, 1);
    assert_eq!(l.arb.stats().num_historic_genotypes, 2);
    assert_eq!(l.arb.stats().dom_id, 3);
    l.kill(2);
    assert_eq!(l.alive_ids(), Vec::<i32>::new());
    assert!(l.arb.historic().is_empty());
}

#[test]
fn historic_genotypes_without_references_are_pruned_at_update() {
    let mut arb = GenotypeArbiter::new("genotype", 3, false, &[]);
    let row = |id: &str, parents: &str| -> HashMap<String, String> {
        [
            ("id", id),
            ("src", "div:int"),
            ("src_args", "(none)"),
            ("parents", parents),
            ("num_units", "0"),
            ("total_units", "1"),
            ("length", "100"),
            ("merit", "97"),
            ("gest_time", "389"),
            ("fitness", "0.25"),
            ("gen_born", "2"),
            ("update_born", "20"),
            ("update_deactivated", "30"),
            ("depth", "1"),
            ("hw_type", "0"),
            ("inst_set", "heads_default"),
            ("sequence", ANCESTOR),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
    };
    let a = arb.legacy_load(&row("7", "(none)")).unwrap();
    let b = arb.legacy_load(&row("9", "1")).unwrap();
    assert_eq!(ids(&arb, &[a, b]), vec![1, 2]);
    assert_eq!(arb.genotype(b).unwrap().parents(), &[a]);
    assert_eq!(arb.genotype(a).unwrap().passive_reference_count(), 1);
    assert_eq!(ids(&arb, &arb.historic()), vec![2, 1]);
    assert!(arb.legacy_load(&row("11", "42")).is_err());

    // A passive reference (as cDeme holds) keeps b, and with it a.
    arb.add_passive_reference(b);
    arb.perform_update(0);
    assert_eq!(arb.current_update(), 1);
    assert_eq!(ids(&arb, &arb.historic()), vec![2, 1]);
    arb.remove_passive_reference(b);
    arb.perform_update(1);
    assert!(arb.historic().is_empty());
    assert_eq!(arb.num_genotypes_alive(), 0);
}

#[test]
fn hinted_classification_reactivates_historic_genotype() {
    let mut arb = GenotypeArbiter::new("genotype", 3, false, &[]);
    let props: HashMap<String, String> = [
        ("src", "div:int"),
        ("src_args", "(none)"),
        ("parents", "(none)"),
        ("update_born", "-1"),
        ("depth", "0"),
        ("hw_type", "0"),
        ("inst_set", "heads_default"),
        ("sequence", ANCESTOR),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let g = arb.legacy_load(&props).unwrap();
    let x = arb.genotype(g).unwrap();
    assert_eq!(x.name(), "001-no_name");
    assert!(x.source().external);
    assert_eq!(x.generation_born(), -1);

    // LoadPopulation injects with the file's source ("div:int" is internal).
    let src = div(false);
    let gn = genome(ANCESTOR);
    let u = UnitInfo {
        source: &src,
        genome: &gn,
        generation: 0,
    };
    let mut hints = ClassificationHints::new();
    hints.insert("id".into(), "1".into());
    assert_eq!(arb.classify_new_unit(&u, Some(&hints)), g);
    let x = arb.genotype(g).unwrap();
    assert!(x.is_active() && x.is_threshold());
    assert_eq!(x.update_deactivated(), -1);
    assert_eq!(x.name(), "100-aaaaa");
    assert_eq!((x.num_units(), x.total_units()), (1, 1));
    assert_eq!(x.breed_in().cur(), 1);
    assert!(arb.historic().is_empty());
    assert_eq!(arb.stats().tot_genotypes, 1);
    // The second hinted unit is found in the active lists.
    assert_eq!(arb.classify_new_unit(&u, Some(&hints)), g);
    assert_eq!(arb.genotype(g).unwrap().num_units(), 2);
    assert_eq!(arb.group(1), Some(g));
}

#[test]
fn birth_chamber_reference_keeps_size_zero_genotype_dominant() {
    let mut l = Lineage::new(3);
    let g1 = l.units[0];
    l.arb.add_active_reference(g1);
    l.kill(0);
    let x = l.arb.genotype(g1).unwrap();
    assert!(x.is_active());
    assert_eq!(x.num_units(), 0);
    assert_eq!(l.arb.best_size(), 0);
    assert_eq!(ids(&l.arb, &l.arb.active_list(0)), vec![1]);
    // Begin()->Next() on m_best == 0 reads the size-0 list.
    assert_eq!(l.arb.dominant(), Some(g1));
    assert_eq!(l.arb.best_genotype(), None);
    l.arb.remove_active_reference(g1);
    assert_eq!(l.arb.num_genotypes_alive(), 0);
    assert_eq!(l.arb.dominant(), None);
}

#[test]
fn gestation_and_legacy_save_row() {
    let mut l = Lineage::new(3);
    l.arb.perform_update(4); // born at 5
    let c = l.birth(0, &mutate(10, 'x'));
    l.birth(c, &mutate(11, 'y')); // keeps g2 referenced
    let g2 = l.units[c];
    l.arb.handle_unit_gestation(
        g2,
        &GestationInfo {
            last_copied_size: 100.0,
            last_executed_size: 98.0,
            last_gestation_time: 390.0,
            last_metabolic_rate: 97.0,
            last_fitness: 97.0 / 390.0,
        },
    );
    {
        let x = l.arb.genotype(g2).unwrap();
        assert_eq!(x.gestation_count().cur(), 1);
        assert_eq!(x.repro_rate().average(), 1.0 / 390.0);
        assert_eq!(
            l.arb.property(g2, "ave_exe_size"),
            PropertyValue::Double(98.0)
        );
        assert_eq!(
            l.arb.property(g2, "max_fitness").double_value(),
            97.0 / 390.0
        );
    }
    l.arb.perform_update(9); // deactivated at 10
    l.kill(c);
    let (mut df, out) = mem_file();
    l.arb.legacy_save(&mut df);
    drop(df);
    let expected = format!(
        "#format id src src_args parents num_units total_units length merit gest_time fitness gen_born update_born update_deactivated depth hw_type inst_set sequence \n\
         #  1: ID\n#  2: Source\n#  3: Source Args\n#  4: Parent ID(s)\n\
         #  5: Number of currently living organisms\n\
         #  6: Total number of organisms that ever existed\n#  7: Genome Length\n\
         #  8: Average Merit\n#  9: Average Gestation Time\n# 10: Average Fitness\n\
         # 11: Generation Born\n# 12: Update Born\n# 13: Update Deactivated\n\
         # 14: Phylogenetic Depth\n# 15: Hardware Type ID\n# 16: Inst Set Name\n\
         # 17: Genome Sequence\n\n\
         2 div:int (none) 1 0 1 100 97 390 0.248718 1 5 10 1 0 heads_default {} \n",
        mutate(10, 'x')
    );
    assert_eq!(out.text(), expected);
}

#[test]
fn provided_values_follow_update_provided_values() {
    let mut l = Lineage::new(2);
    l.birth(0, ANCESTOR);
    l.birth(0, ANCESTOR);
    let c = l.birth(0, &mutate(10, 'x'));
    l.arb.update_provided_values(3);
    let v = |n: &str| {
        l.arb
            .provided_value(&format!("systematics.genotype.{n}"))
            .unwrap()
    };
    assert_eq!(v("current"), PropertyValue::Int(2));
    assert_eq!(v("total"), PropertyValue::Int(2));
    assert_eq!(v("current_threshold"), PropertyValue::Int(1));
    assert_eq!(v("dominant_id"), PropertyValue::Int(1));
    assert_eq!(v("ave_abundance"), PropertyValue::Double(2.0));
    // Entropy of abundances 3 and 1.
    let (p, q) = (0.75_f64, 0.25_f64);
    assert_eq!(
        v("entropy"),
        PropertyValue::Double(-(p * p.ln()) + -(q * q.ln()))
    );
    // Ages weighted by abundance: (3 - -1) * 3 and (3 - -1) * 1.
    assert_eq!(v("ave_age"), PropertyValue::Double(4.0));
    assert_eq!(l.arb.provided_value("systematics.genotype.nope"), None);
    assert_eq!(
        l.arb
            .describe_provided_value("systematics.genotype.ave_size"),
        Some("Average ")
    );
    assert_eq!(l.arb.provided_ids().len(), 23);
    let _ = c;
}

#[test]
fn sexual_ancestry_and_test_metrics_are_attached_once() {
    let mut l = Lineage::new(3);
    let a = l.birth(0, &mutate(10, 'x'));
    let b = l.birth(a, &mutate(11, 'y'));
    let (g1, g2, g3) = (l.units[0], l.units[a], l.units[b]);
    assert_eq!(l.arb.phylo_distance(g3, g1), Some(2));
    assert_eq!(l.arb.phylo_distance(g2, g3), Some(1));
    let sa = l.arb.genotype(g3).unwrap().data().sexual_ancestry.unwrap();
    assert_eq!((sa.id(), sa.ancestor_id(0), sa.ancestor_id(2)), (3, 2, 1));
    assert!(l.arb.genotype(g1).unwrap().data().sexual_ancestry.is_some());

    let mut calls = 0;
    let m = l
        .arb
        .genome_test_metrics(g2, |gn| {
            calls += 1;
            avida_systematics::GenomeTestMetrics {
                is_viable: true,
                copied_size: gn.representation().size(),
                ..Default::default()
            }
        })
        .cloned();
    assert_eq!(m.unwrap().copied_size, 100);
    l.arb.genome_test_metrics(g2, |_| {
        calls += 1;
        Default::default()
    });
    assert_eq!(calls, 1);
}
