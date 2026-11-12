//! A plain-text rendering of a [`Frame`], for snapshot tests and logs.
//!
//! Numbers are printed with `%g` (six significant digits, via
//! `avida_apto::fmt`), so the text is stable across platforms. Each float
//! layer is summarised (count, range, mean) and fingerprinted with an
//! FNV-1a hash of its `f32` bit patterns, so any change to a single cell
//! shows up; the genotype map is drawn in full, one character per cell.

use std::fmt::Write;

use avida_apto::fmt::fmt_g;

use crate::model::{CpuView, Frame, GenotypeRow, LayerId, MapFrame, OrganismView};
use crate::palette::CATEGORICAL_SLOTS;

/// Memory sites per line of the CPU memory dump.
const MEM_ROW: usize = 64;

/// Flag values 1..63 as one character each (0 prints as `.`).
const FLAG_CHARS: &[u8; 64] = b".123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ@#";

fn g(v: f64) -> String {
    fmt_g(v, 6)
}

fn list<T: ToString>(v: &[T]) -> String {
    let items: Vec<String> = v.iter().map(ToString::to_string).collect();
    format!("[{}]", items.join(","))
}

/// FNV-1a over the bit patterns of the values.
fn fingerprint(values: &[f32]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for v in values {
        for b in v.to_bits().to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

/// The map character of a cell: `.` empty, `A`.. for the top genotypes in
/// abundance order, `+` for other genotypes, `?` for an unclassified
/// occupant.
pub fn genotype_char(map: &MapFrame, genotype_id: i32) -> char {
    match genotype_id {
        -1 => '.',
        id if id < -1 => '?',
        id => match map.genotype_slot(id) {
            Some(s) if s < CATEGORICAL_SLOTS => char::from(b'A' + s as u8),
            _ => '+',
        },
    }
}

/// Renders a frame as text. Every section is present; missing parts print
/// as `none`.
pub fn to_text(f: &Frame) -> String {
    let mut s = String::new();
    let h = &f.header;
    let r = &h.stats;
    let _ = writeln!(s, "frame update={}", f.update);
    let _ = writeln!(
        s,
        "world {}x{} seed={} inst_set={} done={}",
        h.world_x, h.world_y, h.random_seed, h.inst_set, h.done
    );
    let p = &h.progress;
    let _ = writeln!(
        s,
        "progress in_update={} slices={}/{} last_finished={} layers={} inst_names={}",
        p.in_update,
        p.slices_done,
        p.slices_total,
        h.last_finished_update,
        f.layers.len(),
        list(&f.inst_names)
    );
    let _ = writeln!(
        s,
        "pop orgs={} genotypes={} threshold={} births={} deaths={} breed_true={} executed={} threads={} tot_orgs={}",
        r.orgs,
        r.genotypes,
        r.threshold_genotypes,
        r.births,
        r.deaths,
        r.breed_true,
        r.executed,
        r.threads,
        r.tot_organisms
    );
    let _ = writeln!(
        s,
        "ave fitness={} merit={} gestation={} generation={} age={} mem_size={}",
        g(r.ave_fitness),
        g(r.ave_merit),
        g(r.ave_gestation),
        g(r.ave_generation),
        g(r.ave_age),
        g(r.ave_mem_size)
    );
    let _ = writeln!(
        s,
        "max fitness={} merit={}",
        g(r.max_fitness),
        g(r.max_merit)
    );
    s.push_str("tasks_last");
    for (k, c) in r.task_last_count.iter().enumerate() {
        let name = f.env.tasks.get(k).map_or("?", |t| t.name.as_str());
        let _ = write!(s, " {name}={c}");
    }
    s.push('\n');
    s.push_str("resources");
    if r.resources.is_empty() {
        s.push_str(" none");
    }
    for (i, q) in r.resources.iter().enumerate() {
        let name = f.env.resources.get(i).map_or("?", |x| x.name.as_str());
        let _ = write!(s, " {name}={}", g(*q));
    }
    s.push('\n');

    match &f.map {
        Some(map) => write_map(&mut s, f, map),
        None => s.push_str("map none\n"),
    }

    match &f.dominant {
        Some(d) => {
            let _ = writeln!(s, "dominant {}", row_text(&d.row));
            let _ = writeln!(
                s,
                "  copy_size={} exe_size={} repro_rate={} max_fitness={} breed_true={} breed_in={} parents={}",
                g(d.ave_copy_size),
                g(d.ave_exe_size),
                g(d.ave_repro_rate),
                g(d.max_fitness),
                d.last_breed_true,
                d.last_breed_in,
                if d.parents.is_empty() { "-" } else { &d.parents }
            );
            let _ = writeln!(s, "  genome={}", d.genome);
        }
        None => s.push_str("dominant none\n"),
    }

    let _ = writeln!(s, "genotypes n={}", f.genotypes.len());
    for row in &f.genotypes {
        let _ = writeln!(s, "  {}", row_text(row));
    }

    match &f.inspect {
        Some(o) => write_organism(&mut s, o),
        None => s.push_str("inspect none\n"),
    }

    let _ = writeln!(s, "lineage n={}", f.lineage.len());
    for row in &f.lineage {
        let _ = writeln!(s, "  {}", row_text(row));
    }

    s.push_str("env\n");
    for res in &f.env.resources {
        let opt = |v: Option<f64>| v.map_or("-".to_string(), g);
        let _ = writeln!(
            s,
            "  resource name={} geometry={} spatial={} quantity={} inflow={} outflow={}",
            res.name,
            res.geometry,
            res.spatial,
            g(res.quantity),
            opt(res.inflow),
            opt(res.outflow)
        );
    }
    for rx in &f.env.reactions {
        let task = rx
            .task
            .and_then(|t| f.env.tasks.get(t))
            .map_or("-", |t| t.name.as_str());
        let _ = writeln!(
            s,
            "  reaction name={} task={} value={} last={}",
            rx.name,
            task,
            g(rx.value),
            rx.last_count
        );
    }
    for t in &f.env.tasks {
        let _ = writeln!(
            s,
            "  task name={} desc={} last={}",
            t.name, t.desc, t.last_count
        );
    }
    s
}

fn row_text(r: &GenotypeRow) -> String {
    let rank = if r.rank == GenotypeRow::NOT_RANKED {
        "-".to_string()
    } else {
        r.rank.to_string()
    };
    format!(
        "#{} id={} parent={} name={} n={} depth={} born={} len={} threshold={} fitness={} merit={} gestation={} births={}",
        rank,
        r.id,
        r.parent_id,
        r.name,
        r.abundance,
        r.depth,
        r.update_born,
        r.genome_len,
        u8::from(r.threshold),
        g(r.ave_fitness),
        g(r.ave_merit),
        g(r.ave_gestation),
        r.last_births
    )
}

fn write_map(s: &mut String, f: &Frame, map: &MapFrame) {
    let _ = writeln!(
        s,
        "map {}x{} occupied={} top={} top_cells={} genotypes_on_map={}",
        map.w,
        map.h,
        map.occupied(),
        list(&map.top_genotypes),
        list(&map.top_counts),
        map.genotype_cells.len()
    );
    for c in map.genotype_cells.iter() {
        let _ = writeln!(
            s,
            "  genotype {} first_cell={} cells={}",
            c.id, c.first_cell, c.cells
        );
    }
    let w = map.w.max(1) as usize;
    for row in map.genotype_id.chunks(w) {
        s.push_str("  ");
        s.extend(row.iter().map(|&id| genotype_char(map, id)));
        s.push('\n');
    }
    for (id, values) in &map.layers {
        let mut n = 0usize;
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        let mut sum = 0.0;
        for &v in values.iter() {
            if v.is_finite() {
                let v = f64::from(v);
                n += 1;
                lo = lo.min(v);
                hi = hi.max(v);
                sum += v;
            }
        }
        let label = f.env.layer_label(*id);
        if n == 0 {
            let _ = writeln!(s, "layer {label} n=0 fnv={:016x}", fingerprint(values));
        } else {
            let _ = writeln!(
                s,
                "layer {label} n={n} min={} max={} mean={} fnv={:016x}",
                g(lo),
                g(hi),
                g(sum / n as f64),
                fingerprint(values)
            );
        }
    }
    s.push_str("tasks_done");
    for k in 0..map.num_tasks {
        let cells = (0..map.num_cells())
            .filter(|&c| map.task_done(c, k))
            .count();
        let name = f.env.layer_label(LayerId::Task(k));
        let name = name.strip_prefix("task ").unwrap_or(&name);
        let _ = write!(s, " {name}={cells}");
    }
    s.push('\n');
}

fn write_organism(s: &mut String, o: &OrganismView) {
    let _ = writeln!(
        s,
        "inspect cell={} xy=({},{}) org={} genotype={} ({}) lineage={} faces={}",
        o.cell,
        o.x,
        o.y,
        o.org_id,
        o.genotype_id,
        o.genotype_name,
        o.lineage_label,
        o.faced_cell.map_or("-".to_string(), |c| c.to_string())
    );
    let p = &o.phenotype;
    let _ = writeln!(
        s,
        "  phenotype merit={} fitness={} bonus={} gestation={} age={} generation={} len={} copied={} executed={}",
        g(p.merit),
        g(p.fitness),
        g(p.cur_bonus),
        p.gestation_time,
        p.age,
        p.generation,
        p.genome_length,
        p.copied_size,
        p.executed_size
    );
    let _ = writeln!(
        s,
        "  cycles={} time={} divides={} born={} breed_true={} modified={} multi_thread={} to_die={}",
        p.cpu_cycles_used,
        p.time_used,
        p.num_divides,
        p.birth_update,
        p.parent_true,
        p.is_modified,
        p.is_multi_thread,
        p.to_die
    );
    let _ = writeln!(
        s,
        "  tasks cur={} last={}",
        list(&p.cur_task_count),
        list(&p.last_task_count)
    );
    let _ = writeln!(
        s,
        "  reactions cur={} last={}",
        list(&p.cur_reaction_count),
        list(&p.last_reaction_count)
    );
    let hex = |v: &[i32]| {
        let items: Vec<String> = v.iter().map(|x| format!("{:08x}", *x as u32)).collect();
        format!("[{}]", items.join(","))
    };
    let _ = writeln!(
        s,
        "  io cell_inputs={} input={} output={}",
        hex(&o.cell_inputs),
        hex(&o.input_buf),
        hex(&o.output_buf)
    );
    write_cpu(s, &o.cpu);
}

fn write_cpu(s: &mut String, cpu: &CpuView) {
    let _ = writeln!(
        s,
        "  cpu inst_set={} hw_type={} insts={} threads={} cur={} mem={}",
        cpu.inst_set,
        cpu.hw_type,
        cpu.inst_names.len(),
        cpu.threads.len(),
        cpu.cur_thread,
        cpu.memory.len()
    );
    for (t, th) in cpu.threads.iter().enumerate() {
        let heads: Vec<String> = th
            .heads
            .iter()
            .map(|h| {
                if h.mem_space == 0 {
                    h.pos.to_string()
                } else {
                    format!("{}:{}", h.mem_space, h.pos)
                }
            })
            .collect();
        let _ = writeln!(
            s,
            "  thread {t} id={} regs={} heads={} active_stack={} read_label={} next_label={}",
            th.id,
            list(&th.regs),
            format_args!("[{}]", heads.join(",")),
            th.active_stack,
            if th.read_label.is_empty() {
                "-"
            } else {
                &th.read_label
            },
            if th.next_label.is_empty() {
                "-"
            } else {
                &th.next_label
            }
        );
        for (i, st) in th.stacks.iter().enumerate() {
            let _ = writeln!(s, "    stack{i}={}", list(st));
        }
    }
    // Head markers of the current thread: IP, read, write, flow.
    let marks = ['i', 'r', 'w', 'f'];
    let mut head_row = vec![' '; cpu.memory.len()];
    if let Some(th) = cpu.threads.get(cpu.cur_thread) {
        for (h, head) in th.heads.iter().enumerate() {
            let Ok(p) = usize::try_from(head.pos) else {
                continue;
            };
            if head.mem_space != 0 {
                continue;
            }
            if let Some(slot) = head_row.get_mut(p) {
                let m = marks.get(h).copied().unwrap_or('h');
                *slot = if *slot == ' ' { m } else { '*' };
            }
        }
    }
    for (row, chunk) in cpu.memory.chunks(MEM_ROW).enumerate() {
        let start = row * MEM_ROW;
        let syms: String = chunk.iter().map(|m| m.sym).collect();
        let flags: String = chunk
            .iter()
            .map(|m| {
                FLAG_CHARS
                    .get(usize::from(m.flags))
                    .map_or('!', |&b| char::from(b))
            })
            .collect();
        let heads: String = head_row[start..start + chunk.len()].iter().collect();
        let _ = writeln!(s, "  mem {start:4} {syms}");
        let _ = writeln!(s, "  flg {start:4} {flags}");
        let line = format!("  hds {start:4} {heads}");
        let _ = writeln!(s, "{}", line.trim_end());
    }
}
