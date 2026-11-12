//! Hand-built frames and statuses for the unit tests.

use std::sync::Arc;

use avida_session::{Frame, LayerId, RunState, Speed, Status};
use avida_viewmodel::{
    EnvView, GenotypeCells, GenotypeRow, Header, MapFrame, TaskView, UpdateProgress, UpdateRow,
};

/// A 3x2 world: cells 0, 1 and 3 hold genotypes 7, 8 and 7; one task; the
/// fitness layer captured.
pub fn frame_with_map() -> Frame {
    let map = MapFrame {
        w: 3,
        h: 2,
        genotype_id: vec![7, 8, -1, 7, -1, -1].into(),
        top_genotypes: vec![7, 8].into(),
        layers: vec![(
            LayerId::Fitness,
            Arc::from(vec![1.0, 4.0, f32::NAN, 2.0, f32::NAN, f32::NAN]),
        )],
        num_tasks: 1,
        task_bits: vec![1, 0, 0, 0, 0, 0].into(),
        words_per_cell: 1,
        top_counts: vec![2, 1].into(),
        genotype_cells: vec![
            GenotypeCells {
                id: 7,
                first_cell: 0,
                cells: 2,
            },
            GenotypeCells {
                id: 8,
                first_cell: 1,
                cells: 1,
            },
        ]
        .into(),
    };
    let mut layers = LayerId::FIXED.to_vec();
    layers.push(LayerId::Task(0));
    Frame {
        update: 12,
        header: Header {
            world_x: 3,
            world_y: 2,
            random_seed: 1,
            inst_set: "heads_default".into(),
            done: false,
            progress: UpdateProgress::default(),
            last_finished_update: 12,
            stats: UpdateRow::default(),
        },
        map: Some(map),
        dominant: None,
        genotypes: vec![GenotypeRow {
            rank: 0,
            id: 7,
            parent_id: -1,
            name: "100-aaaaa".into(),
            abundance: 2,
            depth: 0,
            update_born: 0,
            genome_len: 100,
            threshold: true,
            ave_fitness: 1.5,
            ave_merit: 1.0,
            ave_gestation: 300.0,
            last_births: 0,
        }],
        inspect: None,
        env: EnvView {
            resources: Vec::new(),
            reactions: Vec::new(),
            tasks: vec![TaskView {
                name: "nand".into(),
                desc: "Nand".into(),
                last_count: 1,
            }],
        },
        inst_names: ["nop-A", "nop-B", "nop-C", "h-copy", "h-divide"]
            .map(String::from)
            .to_vec()
            .into(),
        layers,
        lineage: Vec::new(),
    }
}

pub fn status(state: RunState, update: i32) -> Status {
    Status {
        state,
        update,
        slice: None,
        speed: Speed::Max,
        run_to: None,
        run_until: None,
        follow_offspring: false,
        mid_slice: false,
    }
}
