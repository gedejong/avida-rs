//! Structured population save/load (`cPopulation::SavePopulation`,
//! `LoadPopulation`, `SaveStructuredSystematicsGroup`).

mod common;

use avida_config::InitFile;
use avida_genome::Genome;
use avida_systematics::spop::{
    genotype_hints, load_population_genotypes, read_population, read_structured_group, LoadOptions,
    OrganismSaveData, PopulationSave, SaveOptions, StructuredGroupSave,
};
use avida_systematics::{
    Arbiter, CladeArbiter, ClassificationHints, GenotypeArbiter, GroupMembership, Manager,
    RoleClassificationHints, Source, TransmissionType, UnitInfo,
};
use common::*;

const WORLD_CELLS: usize = 60 * 60;

fn repo_file(rel: &str) -> String {
    let p = format!("{}/../../../../{}", env!("CARGO_MANIFEST_DIR"), rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

fn testdata(name: &str) -> String {
    let p = format!("{}/testdata/{}", env!("CARGO_MANIFEST_DIR"), name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

fn genotype_manager() -> Manager {
    let mut m = Manager::new();
    assert!(m.register_arbiter(Arbiter::genotype(GenotypeArbiter::new(
        "genotype",
        3,
        false,
        &[]
    ))));
    m
}

fn plain_org(cell: i32, lineage: i32) -> OrganismSaveData {
    OrganismSaveData {
        cpu_cycles_used: 0,
        lineage_label: lineage,
        opinion_group: -1,
        forage_target: -1,
        birth_cell: cell,
        av_cell_id: -1,
        av_birth_cell: -1,
        parent_ft: -1,
        had_parent_teacher: false,
        parent_merit: 1.0,
    }
}

/// `LoadPopulation` (no offsets, no extra columns) into an empty world:
/// returns the occupant of each cell (membership, lineage label).
fn load(mgr: &mut Manager, text: &str) -> Vec<Option<(GroupMembership, i32)>> {
    let file = InitFile::from_text(text, ".");
    let opts = LoadOptions::default();
    let mut pop = read_population(&file, "detail-100.spop", &opts);
    load_population_genotypes(mgr.genotypes_mut().unwrap(), &mut pop).unwrap();
    let mut cells: Vec<Option<(GroupMembership, i32)>> = vec![None; WORLD_CELLS];
    for p in pop.placements(0, 0, &opts).unwrap() {
        let tmp = &pop.genotypes[p.genotype];
        let g = tmp.group.unwrap();
        let arb = mgr.genotypes().unwrap();
        let id = arb.genotype(g).unwrap().id();
        // Genome mg(tmp.bg->Properties().Get("genome"))
        let genome = Genome::from_string(&arb.property(g, "genome").string_value());
        let u = UnitInfo {
            source: &tmp.source,
            genome: &genome,
            generation: 0,
        };
        let mut m = GroupMembership::new();
        mgr.classify_new_unit(&u, Some(&genotype_hints(id)), &mut m);
        let cell = p.cell_id as usize;
        if let Some((old, _)) = cells[cell].take() {
            mgr.remove_unit(&old);
        }
        cells[cell] = Some((m, p.lineage_label));
    }
    cells
}

/// `SavePopulation` with the defaults of the action (historic rows on).
fn save(mgr: &Manager, cells: &[Option<(GroupMembership, i32)>]) -> String {
    let mut s = PopulationSave::new(SaveOptions {
        save_historic: true,
        ..SaveOptions::default()
    });
    for (cell, occ) in cells.iter().enumerate() {
        if let Some((m, lineage)) = occ {
            let g = mgr.genotype_of(m).unwrap();
            s.add_organism(mgr, g, cell as i32, &plain_org(cell as i32, *lineage));
        }
    }
    let (mut df, out) = mem_file();
    s.write(mgr, &mut df);
    drop(df);
    out.text()
}

/// testdata/detail-100-reloaded.spop is the reference binary's
/// `SavePopulation` right after `LoadPopulation detail-100.spop` (events
/// `u begin LoadPopulation detail-100.spop`, `u begin SavePopulation`,
/// `u begin Exit` in a copy of avida-core/tests/heads_default_100u/config).
#[test]
fn reload_of_detail_100_matches_reference_binary() {
    let original = repo_file("avida-core/tests/heads_default_100u/expected/data/detail-100.spop");
    let mut mgr = genotype_manager();
    let cells = load(&mut mgr, &original);
    let saved = save(&mgr, &cells);
    let reference = testdata("detail-100-reloaded.spop");
    assert_eq!(without_timestamp(&saved), without_timestamp(&reference));

    // Loading our own output again and saving it is stable.
    let mut mgr2 = genotype_manager();
    let cells2 = load(&mut mgr2, &saved);
    assert_eq!(
        without_timestamp(&save(&mgr2, &cells2)),
        without_timestamp(&saved)
    );

    // The dominant genotype after the load (PrintDominantData's group).
    let dom = mgr.dominant_genotype().unwrap();
    assert_eq!(mgr.num_units(dom), Some(7));
    assert_eq!(mgr.group_id(dom), Some(1));
    assert!(mgr
        .property(dom, "name")
        .string_value()
        .starts_with("100-aaa"));
    assert_eq!(mgr.property(dom, "ave_metabolic_rate").double_value(), 0.0);
}

#[test]
fn detail_100_genotype_rows_sort_and_remap() {
    let original = repo_file("avida-core/tests/heads_default_100u/expected/data/detail-100.spop");
    let file = InitFile::from_text(&original, ".");
    let opts = LoadOptions::default();
    let mut pop = read_population(&file, "x.spop", &opts);
    assert!(pop.structured);
    assert_eq!(pop.genotypes.len(), 35);
    // Descending file ids (no parasites).
    let file_ids: Vec<i32> = pop.genotypes.iter().map(|g| g.id_num).collect();
    let mut sorted = file_ids.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(file_ids, sorted);
    let g1 = pop.genotypes.iter().find(|g| g.id_num == 1).unwrap();
    assert_eq!(g1.num_cpus, 7);
    assert_eq!(g1.cells, vec![0, 1, 58, 117, 3421, 3480, 3599]);
    assert_eq!(g1.offsets, vec![388, 388, 65, 66, 330, 362, 329]);
    assert_eq!(g1.source.transmission_type, TransmissionType::Division);
    assert!(g1.source.external);
    assert_eq!(g1.source.arguments, "x.spop");

    let mut arb = GenotypeArbiter::new("genotype", 3, false, &[]);
    load_population_genotypes(&mut arb, &mut pop).unwrap();
    // Loaded oldest first: file id 1 becomes 1, file id 3 becomes 2, ...
    let by_file = |fid: i32| {
        let t = pop.genotypes.iter().find(|g| g.id_num == fid).unwrap();
        arb.genotype(t.group.unwrap()).unwrap()
    };
    assert_eq!(by_file(1).id(), 1);
    assert_eq!(by_file(3).id(), 2);
    // File genotype 23 had parent 14 (loaded as 11).
    assert_eq!(by_file(14).id(), 11);
    assert_eq!(by_file(23).parent_str(), "11");
    assert_eq!(by_file(23).source().as_string(), "div:ext");
    assert_eq!(arb.historic().len(), 35);
}

#[test]
fn save_options_select_columns() {
    let mut mgr = genotype_manager();
    let src = div(true);
    let gn = genome(ANCESTOR);
    let u = UnitInfo {
        source: &src,
        genome: &gn,
        generation: 0,
    };
    let mut m1 = GroupMembership::new();
    let mut m2 = GroupMembership::new();
    mgr.classify_new_unit(&u, None, &mut m1);
    mgr.classify_new_unit(&u, None, &mut m2);
    let psrc = Source::new(TransmissionType::Horizontal, "lbl", false);
    let pg = genome("rucavutycasvab");
    let pu = UnitInfo {
        source: &psrc,
        genome: &pg,
        generation: 0,
    };
    let mut mp = GroupMembership::new();
    mgr.classify_new_unit(&pu, None, &mut mp);
    let g = mgr.genotype_of(&m1).unwrap();
    let p = mgr.genotype_of(&mp).unwrap();
    assert_ne!(g, p);

    let org = |cell: i32| OrganismSaveData {
        cpu_cycles_used: 10 + cell,
        lineage_label: 2,
        opinion_group: 4,
        forage_target: 5,
        birth_cell: 6,
        av_cell_id: 7,
        av_birth_cell: 8,
        parent_ft: 9,
        had_parent_teacher: true,
        parent_merit: 2.5,
    };
    let run = |opts: SaveOptions| -> Vec<String> {
        let mut s = PopulationSave::new(opts);
        s.add_parasite(&mgr, p, 3);
        s.add_organism(&mgr, g, 3, &org(3));
        s.add_organism(&mgr, g, 4, &org(4));
        let (mut df, out) = mem_file();
        s.write(&mgr, &mut df);
        drop(df);
        out.text()
            .lines()
            .filter(|l| !l.starts_with('#') && !l.is_empty())
            .map(|l| {
                // Drop the 17 genotype columns.
                l.splitn(18, ' ').nth(17).unwrap_or("").to_string()
            })
            .collect()
    };
    // Map order: id 1 (bucket 1) before id 2 (bucket 2).
    assert_eq!(
        run(SaveOptions::default()),
        vec!["3,4 13,14 2,2 ", "3  -1 "]
    );
    assert_eq!(
        run(SaveOptions {
            save_groupings: true,
            ..Default::default()
        }),
        vec!["3,4 13,14 2,2 4,4 5,5 6,6 ", "3  -1 -1 -1 0 "]
    );
    // Without USE_AVATARS only the first organism of a genotype records its
    // avatar cells.
    assert_eq!(
        run(SaveOptions {
            save_avatars: true,
            ..Default::default()
        }),
        vec!["3,4 13,14 2,2 7,-1 8,-1 ", "3  -1 -1 -1 "]
    );
    assert_eq!(
        run(SaveOptions {
            save_avatars: true,
            use_avatars: true,
            ..Default::default()
        }),
        vec!["3,4 13,14 2,2 7,7 8,8 ", "3  -1 -1 -1 "]
    );
    assert_eq!(
        run(SaveOptions {
            save_rebirth: true,
            use_avatars: true,
            ..Default::default()
        }),
        vec![
            "3,4 13,14 2,2 4,4 5,5 6,6 7,7 8,8 9,9 1,1 2.500000,2.500000 ",
            "3  -1 -1 -1 0 -1 -1 -1 0 1.000000 "
        ]
    );
}

#[test]
fn structured_group_save_and_read() {
    let mut mgr = genotype_manager();
    assert!(mgr.register_arbiter(Arbiter::Clade(CladeArbiter::new("clade"))));
    assert!(!mgr.register_arbiter(Arbiter::Clade(CladeArbiter::new("clade"))));
    let src = div(true);
    let gn = genome(ANCESTOR);
    let u = UnitInfo {
        source: &src,
        genome: &gn,
        generation: 0,
    };
    let mut hints = RoleClassificationHints::new();
    let mut ch = ClassificationHints::new();
    ch.insert("name".into(), "founders".into());
    hints.insert("clade".into(), ch);
    let mut cells = Vec::new();
    for _ in 0..3 {
        let mut m = GroupMembership::new();
        mgr.classify_new_unit(&u, Some(&hints), &mut m);
        assert_eq!(m.len(), 2);
        cells.push(m);
    }
    let clade_role = mgr.arbiter_for_role("clade").unwrap();
    let mut s = StructuredGroupSave::new();
    for (i, m) in cells.iter().enumerate() {
        s.add_organism(&mgr, m.group(clade_role).unwrap(), i as i32 * 5, 0);
    }
    let (mut df, out) = mem_file();
    s.write(&mgr, &mut df);
    drop(df);
    let text = without_timestamp(&out.text());
    assert_eq!(
        text,
        "#filetype systematics_data\n#format name cells \n# Structured Systematics Group Save\n\
         #  1: Clade Name\n#  2: Occupied Cell IDs\n\nfounders 0,5,10 \n"
    );
    let lines = read_structured_group(&InitFile::from_text(&text, "."));
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].cells, vec![0, 5, 10]);
    assert_eq!(lines[0].props["name"], "founders");

    // LoadStructuredSystematicsGroup into a fresh clade arbiter.
    let mut mgr2 = genotype_manager();
    mgr2.register_arbiter(Arbiter::Clade(CladeArbiter::new("clade")));
    let role = mgr2.arbiter_for_role("clade").unwrap();
    let grp = mgr2.legacy_load(role, &lines[0].props).unwrap().unwrap();
    for _ in &lines[0].cells {
        let joined = mgr2.group_classify_new_unit(grp, &u, None).unwrap();
        assert_eq!(joined, grp);
    }
    assert_eq!(mgr2.num_units(grp), Some(3));
}

#[test]
fn sexual_self_classification_pools_parent_groups_by_role() {
    let mut mgr = genotype_manager();
    mgr.register_arbiter(Arbiter::Clade(CladeArbiter::new("clade")));
    let src = div(true);
    let ga = genome(ANCESTOR);
    let gb = genome(&mutate(30, 'z'));
    let mut hints = RoleClassificationHints::new();
    let mut ch = ClassificationHints::new();
    ch.insert("name".into(), "c".into());
    hints.insert("clade".into(), ch);
    let mut pa = GroupMembership::new();
    let mut pb = GroupMembership::new();
    mgr.classify_new_unit(
        &UnitInfo {
            source: &src,
            genome: &ga,
            generation: 0,
        },
        Some(&hints),
        &mut pa,
    );
    mgr.classify_new_unit(
        &UnitInfo {
            source: &src,
            genome: &gb,
            generation: 0,
        },
        Some(&hints),
        &mut pb,
    );
    // Offspring with a third genome and two parents.
    let child_src = div(false);
    let gc = genome(&mutate(31, 'z'));
    let cu = UnitInfo {
        source: &child_src,
        genome: &gc,
        generation: 1,
    };
    let mut child = GroupMembership::new();
    mgr.self_classify(&cu, &[&pa, &pb], &mut child);
    let roles: Vec<&str> = child
        .groups()
        .iter()
        .map(|&g| mgr.role_of(g).unwrap())
        .collect();
    assert_eq!(roles, vec!["genotype", "clade"]);
    let cg = child.groups()[0];
    assert_eq!(mgr.property(cg, "parents").string_value(), "1,2");
    assert_eq!(mgr.depth(cg), Some(1));
    let clade = child.groups()[1];
    assert_eq!(mgr.num_units(clade), Some(3));
    // Sexual ancestry: siblings share parents.
    assert_eq!(
        mgr.sexual_ancestry(cg)
            .map(|s| (s.ancestor_id(0), s.ancestor_id(1))),
        Some((1, 2))
    );

    // Gestation reaches the genotype; death leaves both groups.
    mgr.handle_gestation(
        &child,
        &avida_systematics::GestationInfo {
            last_gestation_time: 4.0,
            ..Default::default()
        },
    );
    assert_eq!(mgr.property(cg, "ave_gestation_time").double_value(), 4.0);
    mgr.remove_unit(&child);
    assert_eq!(mgr.num_units(clade), Some(2));
    assert_eq!(mgr.group_id(cg), None);

    // Asexual: each parent group classifies with itself.
    let mut child2 = GroupMembership::new();
    mgr.self_classify(&cu, &[&pa], &mut child2);
    assert_eq!(child2.len(), 2);
    assert_eq!(
        mgr.property(child2.groups()[0], "parents").string_value(),
        "1"
    );

    mgr.perform_update(0);
    mgr.update_provided_values(1);
    assert_eq!(
        mgr.provided_value("systematics.genotype.current")
            .map(|v| v.int_value()),
        Some(3)
    );
    assert_eq!(
        mgr.provided_value("systematics.clade.current")
            .map(|v| v.int_value()),
        Some(1)
    );
}
