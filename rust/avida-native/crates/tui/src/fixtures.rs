//! Hand-built frames and applications for the unit tests.

use std::sync::Arc;

use avida_session::{Frame, LayerId, RunState, Speed, Status};
use avida_viewmodel::{
    CpuView, EnvView, GenotypeCells, GenotypeRow, HeadView, Header, MapFrame, MemCell,
    MemSpaceView, OrganismView, PhenotypeView, TaskView, ThreadView, UpdateProgress, UpdateRow,
};

use crate::app::App;
use crate::color::ColorMode;

pub const INST: [&str; 5] = ["nop-A", "nop-B", "nop-C", "h-copy", "h-divide"];

pub fn genotype(rank: usize, id: i32, parent: i32, name: &str, abundance: i32) -> GenotypeRow {
    GenotypeRow {
        rank,
        id,
        parent_id: parent,
        name: name.into(),
        abundance,
        depth: id,
        update_born: id * 10,
        genome_len: 50 + id,
        threshold: true,
        ave_fitness: 1.0 + f64::from(id),
        ave_merit: 1.0,
        ave_gestation: 300.0,
        last_births: 0,
    }
}

/// A 3x2 world: cells 0, 1 and 3 hold genotypes 7, 8 and 7.
pub fn frame_with_map() -> Frame {
    let map = MapFrame {
        w: 3,
        h: 2,
        genotype_id: vec![7, 8, -1, 7, -1, -1].into(),
        top_genotypes: vec![7, 8].into(),
        layers: Vec::new(),
        num_tasks: 1,
        task_bits: vec![1, 0, 0, 0, 0, 0].into(),
        words_per_cell: 1,
        top_counts: vec![2, 1].into(),
        families: Arc::from(Vec::new()),
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
        genotypes: vec![
            genotype(0, 7, 3, "050-aaaaa", 2),
            genotype(1, 8, 7, "051-bbbbb", 1),
        ],
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
        inst_names: INST.map(String::from).to_vec().into(),
        layers: LayerId::FIXED.to_vec(),
        lineage: Vec::new(),
    }
}

/// An organism with `sites` memory cells, register AX = `ax`, IP at `ip`.
pub fn organism(id: i32, ax: i32, ip: i32, sites: usize) -> OrganismView {
    let memory: Vec<MemCell> = (0..sites)
        .map(|i| MemCell {
            op: (i % INST.len()) as u8,
            sym: 'a',
            name_idx: (i % INST.len()) as u16,
            flags: 0,
        })
        .collect();
    OrganismView {
        cell: 3,
        x: 0,
        y: 1,
        org_id: id,
        genotype_id: 7,
        genotype_name: "050-aaaaa".into(),
        lineage_label: 0,
        faced_cell: None,
        phenotype: PhenotypeView {
            merit: 1.0,
            fitness: 1.0,
            cur_bonus: 1.0,
            gestation_time: 0,
            age: 0,
            generation: 0,
            genome_length: sites as i32,
            copied_size: 0,
            executed_size: 0,
            cpu_cycles_used: ip,
            lifetime_cpu_cycles: u64::try_from(ip).unwrap_or(0),
            time_used: 0,
            num_divides: 0,
            birth_update: 0,
            parent_true: true,
            is_modified: false,
            is_multi_thread: false,
            to_die: false,
            cur_task_count: Vec::new(),
            last_task_count: Vec::new(),
            cur_reaction_count: Vec::new(),
            last_reaction_count: Vec::new(),
        },
        cell_inputs: Vec::new(),
        input_buf: vec![1, 2],
        output_buf: Vec::new(),
        cpu: CpuView {
            inst_set: "heads_default".into(),
            hw_type: 0,
            spaces: vec![MemSpaceView {
                id: 0,
                label: "genome".into(),
                memory: memory.clone(),
            }],
            memory,
            inst_names: INST.map(String::from).to_vec().into(),
            threads: vec![ThreadView {
                id: 0,
                regs: vec![ax, 0, 0],
                heads: vec![
                    HeadView {
                        pos: ip,
                        mem_space: 0,
                    },
                    HeadView {
                        pos: 0,
                        mem_space: 0,
                    },
                    HeadView {
                        pos: 0,
                        mem_space: 0,
                    },
                    HeadView {
                        pos: 0,
                        mem_space: 0,
                    },
                ],
                stacks: vec![vec![0; 10], vec![3, 4, 5]],
                active_stack: 0,
                read_label: String::new(),
                next_label: String::new(),
                status: String::new(),
            }],
            head_names: ["IP", "Read", "Write", "Flow"].map(String::from).to_vec(),
            cur_thread: 0,
        },
    }
}

/// A TransSMT-like organism: a host genome in space 0 (8 sites), an
/// offspring space 1 (4 sites) and a parasite in space 2 (6 sites), with
/// the host thread in space 0 and a parasite thread in space 2.
pub fn parasitised(id: i32, host_ip: i32, parasite_ip: i32) -> OrganismView {
    let mut o = organism(id, 1, host_ip, 8);
    let space = |id: i32, label: &str, n: usize, shift: usize| MemSpaceView {
        id,
        label: label.into(),
        memory: (0..n)
            .map(|i| MemCell {
                op: ((i + shift) % INST.len()) as u8,
                sym: 'p',
                name_idx: ((i + shift) % INST.len()) as u16,
                flags: 0,
            })
            .collect(),
    };
    o.cpu.hw_type = 2;
    o.cpu.spaces.push(space(1, "offspring", 4, 1));
    o.cpu.spaces.push(space(2, "parasite", 6, 2));
    let host = o.cpu.threads[0].clone();
    let mut par = host.clone();
    par.id = 1;
    par.heads = vec![
        HeadView {
            pos: parasite_ip,
            mem_space: 2,
        },
        HeadView {
            pos: 1,
            mem_space: 2,
        },
        HeadView {
            pos: 2,
            mem_space: 1,
        },
        HeadView {
            pos: 0,
            mem_space: 2,
        },
    ];
    o.cpu.threads = vec![host, par];
    o
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

pub fn app() -> App {
    let mut a = App::new(".".into(), vec![], ColorMode::TrueColor);
    a.take_commands();
    a
}

/// An application that has been shown `frame` with cell 3 selected.
pub fn app_with(frame: Frame) -> App {
    let mut a = app();
    a.apply_status(status(RunState::Paused, 12));
    a.select(Some(3));
    a.take_commands();
    a.apply_frame(Arc::new(frame));
    a
}

/// A frame whose cell 3 holds `org`.
pub fn frame_with_org(org: OrganismView) -> Frame {
    Frame {
        inspect: Some(org),
        ..frame_with_map()
    }
}
