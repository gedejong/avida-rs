//! The recording format: an append-only file of [`Frame`] snapshots that a
//! front end can play back later, and the reader and writer for it.
//!
//! # Layout
//!
//! All integers are little-endian.
//!
//! ```text
//! file    = "AVIDAREC" version:u32 header_len:u32 header record*
//! header  = what is constant across frames (see [`Meta`])
//! record  = body_len:u32 body
//! body    = update:i32 row_len:u32 row frame_raw_len:u32 frame
//! row     = zlib( UpdateRow )
//! frame   = zlib( header fields, map, dominant genotype, genotype list, environment )
//! ```
//!
//! Every record stands on its own: it is length-prefixed and compressed by
//! itself, so a run that was killed leaves a file readable up to its last
//! complete record, and a player can seek to any record. [`Recording::open`]
//! builds the index of record offsets by one scan that reads only the
//! eight bytes at the start of each record; there is no trailer. The row
//! (the statistics of the update) is a block of its own so that a chart can
//! be rebuilt from thousands of records without decompressing their maps.
//!
//! # What is recorded
//!
//! What a front end needs to draw that moment: the map (genotype ids, the
//! subscribed layers including the ancestry families, the task bits and the
//! genotype bookkeeping), the statistics row, the genotype table, the
//! dominant genotype and the environment summary. Not recorded: the
//! inspected organism ([`Frame::inspect`]) and the lineage
//! ([`Frame::lineage`]); a decoded frame has neither. The environment is in
//! every record, not in the file header, because its task and reaction
//! counts and resource amounts change from frame to frame. The table of
//! where each genotype lives ([`MapFrame::genotype_cells`]) follows from the
//! genotype of each cell and is derived when a record is read.
//!
//! # Size
//!
//! A record is a few megabytes of raw per-cell arrays that are mostly
//! repeats (empty cells, large clades, resources that vary slowly), so two
//! cheap steps carry most of the compression:
//!
//! * the bytes of each numeric array are stored plane by plane (all first
//!   bytes, then all second bytes, ...), which turns `f32` and `i32` arrays
//!   whose values share exponents and high bytes into long runs;
//! * a general compressor over the result: zlib (DEFLATE) from the pure
//!   Rust `miniz_oxide`. It is `unsafe`-free (the workspace forbids
//!   `unsafe` and a C dependency such as `zstd` would put a C toolchain into
//!   the build), already in the build through the image and PNG
//!   code, checks its stream with an Adler-32 so corruption is detected, and
//!   at the level used here compresses a 160x160 frame of 25 layers in a
//!   few tens of milliseconds, which a capture every 50 updates of a world
//!   that size hides. (Level 3 to 6 to 9 took a 60x60 frame from 43.6 to
//!   42.7 to 42.4 KB of layers.) `lz4_flex` is faster but has no entropy
//!   coding, and `ruzstd` has no strong compressor; neither was measured,
//!   because compression is a small part of the cost of a frame next to
//!   capturing it.
//!
//! # Errors
//!
//! Nothing here panics on a truncated or corrupt file: every length is
//! checked against what remains before anything is allocated, and failures
//! are [`RecordingError`]s. A damaged or cut tail is not an error when the
//! file is opened: the complete records before it are readable
//! ([`Recording::tail`]).

use std::fmt;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use avida_viewmodel::{
    EnvView, Family, Frame, GenotypeCells, GenotypeRow, GenotypeView, Header, LayerId, MapFrame,
    ReactionView, ResourceView, TaskView, UpdateProgress, UpdateRow,
};

/// First bytes of every recording.
pub const MAGIC: &[u8; 8] = b"AVIDAREC";

/// The format version this code writes and reads.
pub const VERSION: u32 = 1;

/// DEFLATE level of the records (miniz levels run 1 to 10).
const LEVEL: u8 = 6;

/// Largest header, record or array the reader accepts (guards allocation
/// on a corrupt length).
const MAX_RECORD: u64 = 1 << 31;
const MAX_HEADER: u32 = 1 << 24;
const MAX_CELLS: u64 = 1 << 26;

/// Why a recording could not be written or read.
#[derive(Debug)]
pub enum RecordingError {
    Io(io::Error),
    /// Not a recording: the first bytes are not [`MAGIC`].
    NotARecording,
    /// A recording of a version this code does not read.
    UnsupportedVersion(u32),
    /// The file ends inside its header.
    TruncatedHeader,
    /// The data of record `record` (0 for the header) is damaged.
    Corrupt {
        record: usize,
        why: String,
    },
    /// A record number past the last complete one.
    NoSuchRecord(usize),
}

impl fmt::Display for RecordingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecordingError::Io(e) => write!(f, "{e}"),
            RecordingError::NotARecording => write!(
                f,
                "this is not an Avida recording (it does not start with {})",
                String::from_utf8_lossy(MAGIC)
            ),
            RecordingError::UnsupportedVersion(v) => write!(
                f,
                "this recording has format version {v}; this program reads version {VERSION}"
            ),
            RecordingError::TruncatedHeader => {
                write!(f, "the recording ends inside its header")
            }
            RecordingError::Corrupt { record, why } => {
                write!(f, "record {record} is damaged: {why}")
            }
            RecordingError::NoSuchRecord(i) => write!(f, "the recording has no record {i}"),
        }
    }
}

impl std::error::Error for RecordingError {}

impl From<io::Error> for RecordingError {
    fn from(e: io::Error) -> Self {
        RecordingError::Io(e)
    }
}

/// What is constant across the frames of a recording.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meta {
    pub world_x: i32,
    pub world_y: i32,
    /// `RANDOM_SEED` as configured.
    pub random_seed: i32,
    pub inst_set: String,
    /// Instruction names of set 0, indexed by opcode.
    pub inst_names: Vec<String>,
    /// The layers every frame carries, [`LayerId::Genotype`] first: what a
    /// decoded [`Frame::layers`] lists.
    pub layers: Vec<LayerId>,
    /// The capture interval in updates the recorder was asked for.
    pub every: u32,
    /// How many genotypes the table of each frame lists at most.
    pub genotypes_top_n: u32,
    /// The `avida` command-line arguments of the run (information only).
    pub args: Vec<String>,
    /// The run's working directory (information only).
    pub work_dir: String,
}

impl Meta {
    /// The constants of a recording whose first frame is `first`.
    pub fn for_frame(
        first: &Frame,
        every: u32,
        genotypes_top_n: usize,
        args: Vec<String>,
        work_dir: String,
    ) -> Meta {
        let mut layers = Vec::new();
        if let Some(map) = &first.map {
            layers.push(LayerId::Genotype);
            layers.extend(map.layers.iter().map(|(l, _)| *l));
        }
        Meta {
            world_x: first.header.world_x,
            world_y: first.header.world_y,
            random_seed: first.header.random_seed,
            inst_set: first.header.inst_set.clone(),
            inst_names: first.inst_names.to_vec(),
            layers,
            every,
            genotypes_top_n: u32::try_from(genotypes_top_n).unwrap_or(u32::MAX),
            args,
            work_dir,
        }
    }
}

// --- encoding ---------------------------------------------------------------

#[derive(Default)]
struct Wr {
    buf: Vec<u8>,
}

impl Wr {
    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    fn bool(&mut self, v: bool) {
        self.buf.push(u8::from(v));
    }
    fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn f64(&mut self, v: f64) {
        self.u64(v.to_bits());
    }
    fn len(&mut self, n: usize) {
        self.u32(u32::try_from(n).unwrap_or(u32::MAX));
    }
    fn str(&mut self, s: &str) {
        self.len(s.len());
        self.buf.extend_from_slice(s.as_bytes());
    }
    fn opt_f64(&mut self, v: Option<f64>) {
        self.bool(v.is_some());
        self.f64(v.unwrap_or(0.0));
    }
    fn i32s(&mut self, v: &[i32]) {
        self.len(v.len());
        for &x in v {
            self.i32(x);
        }
    }
    fn f64s(&mut self, v: &[f64]) {
        self.len(v.len());
        for &x in v {
            self.f64(x);
        }
    }
    /// The bytes of `n` elements of `W` bytes, plane by plane.
    fn planes<const W: usize>(&mut self, n: usize, bytes: impl Fn(usize) -> [u8; W]) {
        self.buf.reserve(n * W);
        for plane in 0..W {
            for i in 0..n {
                self.buf.push(bytes(i)[plane]);
            }
        }
    }
    fn layer(&mut self, l: LayerId) {
        let (tag, idx) = layer_code(l);
        self.u8(tag);
        self.u32(idx);
    }
}

fn layer_code(l: LayerId) -> (u8, u32) {
    let small = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    match l {
        LayerId::Genotype => (0, 0),
        LayerId::Fitness => (1, 0),
        LayerId::Merit => (2, 0),
        LayerId::Gestation => (3, 0),
        LayerId::Age => (4, 0),
        LayerId::Generation => (5, 0),
        LayerId::GenomeLen => (6, 0),
        LayerId::Lineage => (7, 0),
        LayerId::Ancestry => (8, 0),
        LayerId::BreedTrue => (9, 0),
        LayerId::Modified => (10, 0),
        LayerId::Threads => (11, 0),
        LayerId::TaskCount => (12, 0),
        LayerId::Task(k) => (13, small(k)),
        LayerId::Resource(r) => (14, small(r)),
    }
}

fn layer_from(tag: u8, idx: u32) -> Option<LayerId> {
    Some(match tag {
        0 => LayerId::Genotype,
        1 => LayerId::Fitness,
        2 => LayerId::Merit,
        3 => LayerId::Gestation,
        4 => LayerId::Age,
        5 => LayerId::Generation,
        6 => LayerId::GenomeLen,
        7 => LayerId::Lineage,
        8 => LayerId::Ancestry,
        9 => LayerId::BreedTrue,
        10 => LayerId::Modified,
        11 => LayerId::Threads,
        12 => LayerId::TaskCount,
        13 => LayerId::Task(idx as usize),
        14 => LayerId::Resource(idx as usize),
        _ => return None,
    })
}

fn encode_row(r: &UpdateRow) -> Vec<u8> {
    let mut w = Wr::default();
    w.i32(r.update);
    w.i32(r.orgs);
    w.i32(r.genotypes);
    w.i32(r.threshold_genotypes);
    for v in [
        r.ave_fitness,
        r.ave_merit,
        r.ave_gestation,
        r.ave_generation,
        r.ave_age,
        r.ave_mem_size,
        r.max_fitness,
        r.max_merit,
    ] {
        w.f64(v);
    }
    for v in [
        r.births,
        r.deaths,
        r.breed_true,
        r.executed,
        r.threads,
        r.tot_organisms,
    ] {
        w.i32(v);
    }
    w.i32s(&r.task_last_count);
    w.f64s(&r.resources);
    w.buf
}

fn put_genotype_row(w: &mut Wr, g: &GenotypeRow) {
    w.u64(if g.rank == GenotypeRow::NOT_RANKED {
        u64::MAX
    } else {
        g.rank as u64
    });
    w.i32(g.id);
    w.i32(g.parent_id);
    w.str(&g.name);
    w.i32(g.abundance);
    w.i32(g.depth);
    w.i32(g.update_born);
    w.i32(g.genome_len);
    w.bool(g.threshold);
    w.f64(g.ave_fitness);
    w.f64(g.ave_merit);
    w.f64(g.ave_gestation);
    w.i32(g.last_births);
}

fn encode_frame(f: &Frame) -> Vec<u8> {
    let mut w = Wr::default();
    w.bool(f.header.done);
    w.bool(f.header.progress.in_update);
    w.i32(f.header.progress.slices_done);
    w.i32(f.header.progress.slices_total);
    w.i32(f.header.last_finished_update);

    match &f.map {
        None => w.bool(false),
        Some(m) => {
            w.bool(true);
            w.i32(m.w);
            w.i32(m.h);
            let n = m.genotype_id.len();
            w.planes::<4>(n, |i| m.genotype_id[i].to_le_bytes());
            w.i32s(&m.top_genotypes);
            w.len(m.layers.len());
            for (id, values) in &m.layers {
                w.layer(*id);
                w.planes::<4>(values.len(), |i| values[i].to_bits().to_le_bytes());
            }
            w.len(m.num_tasks);
            w.len(m.words_per_cell);
            w.planes::<8>(m.task_bits.len(), |i| m.task_bits[i].to_le_bytes());
            w.len(m.top_counts.len());
            for &c in m.top_counts.iter() {
                w.u32(c);
            }
            w.len(m.families.len());
            for fam in m.families.iter() {
                w.i32(fam.root);
                w.u32(fam.organisms);
                w.len(fam.slot);
            }
        }
    }

    match &f.dominant {
        None => w.bool(false),
        Some(d) => {
            w.bool(true);
            put_genotype_row(&mut w, &d.row);
            w.str(&d.genome);
            w.f64(d.ave_copy_size);
            w.f64(d.ave_exe_size);
            w.f64(d.ave_repro_rate);
            w.f64(d.max_fitness);
            w.i32(d.last_breed_true);
            w.i32(d.last_breed_in);
            w.str(&d.parents);
        }
    }

    w.len(f.genotypes.len());
    for g in &f.genotypes {
        put_genotype_row(&mut w, g);
    }

    w.len(f.env.resources.len());
    for r in &f.env.resources {
        w.str(&r.name);
        w.i32(r.geometry);
        w.bool(r.spatial);
        w.f64(r.quantity);
        w.opt_f64(r.inflow);
        w.opt_f64(r.outflow);
    }
    w.len(f.env.reactions.len());
    for r in &f.env.reactions {
        w.str(&r.name);
        w.bool(r.task.is_some());
        w.len(r.task.unwrap_or(0));
        w.f64(r.value);
        w.i32(r.last_count);
    }
    w.len(f.env.tasks.len());
    for t in &f.env.tasks {
        w.str(&t.name);
        w.str(&t.desc);
        w.i32(t.last_count);
    }
    w.buf
}

fn encode_meta(m: &Meta) -> Vec<u8> {
    let mut w = Wr::default();
    w.i32(m.world_x);
    w.i32(m.world_y);
    w.i32(m.random_seed);
    w.str(&m.inst_set);
    w.u32(m.every);
    w.u32(m.genotypes_top_n);
    w.len(m.layers.len());
    for &l in &m.layers {
        w.layer(l);
    }
    w.len(m.inst_names.len());
    for s in &m.inst_names {
        w.str(s);
    }
    w.len(m.args.len());
    for s in &m.args {
        w.str(s);
    }
    w.str(&m.work_dir);
    w.buf
}

// --- decoding ---------------------------------------------------------------

struct Rd<'a> {
    buf: &'a [u8],
    pos: usize,
}

type R<T> = Result<T, String>;

impl<'a> Rd<'a> {
    fn new(buf: &'a [u8]) -> Rd<'a> {
        Rd { buf, pos: 0 }
    }
    fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }
    fn take(&mut self, n: usize) -> R<&'a [u8]> {
        if n > self.remaining() {
            return Err("ends early".to_string());
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn arr<const N: usize>(&mut self) -> R<[u8; N]> {
        let mut a = [0u8; N];
        a.copy_from_slice(self.take(N)?);
        Ok(a)
    }
    fn u8(&mut self) -> R<u8> {
        Ok(self.take(1)?[0])
    }
    fn bool(&mut self) -> R<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            b => Err(format!("a flag holds {b}")),
        }
    }
    fn u32(&mut self) -> R<u32> {
        Ok(u32::from_le_bytes(self.arr()?))
    }
    fn i32(&mut self) -> R<i32> {
        Ok(i32::from_le_bytes(self.arr()?))
    }
    fn u64(&mut self) -> R<u64> {
        Ok(u64::from_le_bytes(self.arr()?))
    }
    fn f64(&mut self) -> R<f64> {
        Ok(f64::from_bits(self.u64()?))
    }
    fn opt_f64(&mut self) -> R<Option<f64>> {
        let some = self.bool()?;
        let v = self.f64()?;
        Ok(some.then_some(v))
    }
    /// A count of elements that each take at least `min` more bytes.
    fn count(&mut self, min: usize) -> R<usize> {
        let n = self.u32()? as usize;
        if n.checked_mul(min.max(1))
            .is_none_or(|b| b > self.remaining())
        {
            return Err(format!("a count of {n} exceeds the data"));
        }
        Ok(n)
    }
    fn str(&mut self) -> R<String> {
        let n = self.count(1)?;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| "text is not UTF-8".to_string())
    }
    fn i32s(&mut self) -> R<Vec<i32>> {
        let n = self.count(4)?;
        (0..n).map(|_| self.i32()).collect()
    }
    fn f64s(&mut self) -> R<Vec<f64>> {
        let n = self.count(8)?;
        (0..n).map(|_| self.f64()).collect()
    }
    fn planes<const W: usize>(&mut self, n: usize) -> R<Vec<[u8; W]>> {
        let bytes = n.checked_mul(W).filter(|&b| b <= self.remaining());
        let Some(bytes) = bytes else {
            return Err(format!("an array of {n} elements exceeds the data"));
        };
        let data = self.take(bytes)?;
        let mut out = vec![[0u8; W]; n];
        for (plane, chunk) in data.chunks_exact(n.max(1)).enumerate().take(W) {
            for (i, &b) in chunk.iter().enumerate() {
                out[i][plane] = b;
            }
        }
        Ok(out)
    }
    fn layer(&mut self) -> R<LayerId> {
        let tag = self.u8()?;
        let idx = self.u32()?;
        layer_from(tag, idx).ok_or_else(|| format!("unknown layer {tag}"))
    }
    fn done(&self) -> R<()> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(format!("{} unread bytes", self.remaining()))
        }
    }
}

fn decode_row(buf: &[u8]) -> R<UpdateRow> {
    let mut r = Rd::new(buf);
    let row = UpdateRow {
        update: r.i32()?,
        orgs: r.i32()?,
        genotypes: r.i32()?,
        threshold_genotypes: r.i32()?,
        ave_fitness: r.f64()?,
        ave_merit: r.f64()?,
        ave_gestation: r.f64()?,
        ave_generation: r.f64()?,
        ave_age: r.f64()?,
        ave_mem_size: r.f64()?,
        max_fitness: r.f64()?,
        max_merit: r.f64()?,
        births: r.i32()?,
        deaths: r.i32()?,
        breed_true: r.i32()?,
        executed: r.i32()?,
        threads: r.i32()?,
        tot_organisms: r.i32()?,
        task_last_count: r.i32s()?,
        resources: r.f64s()?,
    };
    r.done()?;
    Ok(row)
}

fn get_genotype_row(r: &mut Rd) -> R<GenotypeRow> {
    let rank = r.u64()?;
    Ok(GenotypeRow {
        rank: if rank == u64::MAX {
            GenotypeRow::NOT_RANKED
        } else {
            usize::try_from(rank).map_err(|_| "a rank is too large".to_string())?
        },
        id: r.i32()?,
        parent_id: r.i32()?,
        name: r.str()?,
        abundance: r.i32()?,
        depth: r.i32()?,
        update_born: r.i32()?,
        genome_len: r.i32()?,
        threshold: r.bool()?,
        ave_fitness: r.f64()?,
        ave_merit: r.f64()?,
        ave_gestation: r.f64()?,
        last_births: r.i32()?,
    })
}

/// [`MapFrame::genotype_cells`] from the genotype of each cell: every
/// genotype on the map by ascending id, with its lowest cell and its number
/// of cells. It is not recorded because it follows from the map, and as a
/// table of 12 bytes per genotype it was a fifth of a compressed frame.
fn derive_genotype_cells(genotype_id: &[i32]) -> Arc<[GenotypeCells]> {
    let mut occupied: Vec<(i32, u32)> = genotype_id
        .iter()
        .enumerate()
        .filter(|&(_, &g)| g != -1)
        .map(|(cell, &g)| (g, cell as u32))
        .collect();
    occupied.sort_unstable();
    let mut out: Vec<GenotypeCells> = Vec::new();
    for (id, cell) in occupied {
        match out.last_mut() {
            Some(last) if last.id == id => last.cells += 1,
            _ => out.push(GenotypeCells {
                id,
                first_cell: cell as i32,
                cells: 1,
            }),
        }
    }
    out.into()
}

fn decode_map(r: &mut Rd) -> R<MapFrame> {
    let (w, h) = (r.i32()?, r.i32()?);
    let cells = u64::try_from(w).ok().zip(u64::try_from(h).ok());
    let n = cells
        .and_then(|(w, h)| w.checked_mul(h))
        .filter(|&n| n <= MAX_CELLS)
        .ok_or_else(|| format!("a {w} x {h} map"))? as usize;
    let genotype_id: Arc<[i32]> = r
        .planes::<4>(n)?
        .into_iter()
        .map(i32::from_le_bytes)
        .collect();
    let top_genotypes: Arc<[i32]> = r.i32s()?.into();
    let nl = r.count(5)?;
    let mut layers = Vec::with_capacity(nl);
    for _ in 0..nl {
        let id = r.layer()?;
        let v: Arc<[f32]> = r
            .planes::<4>(n)?
            .into_iter()
            .map(|b| f32::from_bits(u32::from_le_bytes(b)))
            .collect();
        layers.push((id, v));
    }
    let num_tasks = r.u32()? as usize;
    let words_per_cell = r.u32()? as usize;
    let words = n
        .checked_mul(words_per_cell)
        .ok_or_else(|| "task bits overflow".to_string())?;
    let task_bits: Arc<[u64]> = r
        .planes::<8>(words)?
        .into_iter()
        .map(u64::from_le_bytes)
        .collect();
    let nc = r.count(4)?;
    let top_counts: Arc<[u32]> = (0..nc).map(|_| r.u32()).collect::<R<Vec<_>>>()?.into();
    let genotype_cells = derive_genotype_cells(&genotype_id);
    let nf = r.count(12)?;
    let families: Arc<[Family]> = (0..nf)
        .map(|_| {
            Ok(Family {
                root: r.i32()?,
                organisms: r.u32()?,
                slot: r.u32()? as usize,
            })
        })
        .collect::<R<Vec<_>>>()?
        .into();
    Ok(MapFrame {
        w,
        h,
        genotype_id,
        top_genotypes,
        layers,
        num_tasks,
        task_bits,
        words_per_cell,
        top_counts,
        genotype_cells,
        families,
    })
}

fn decode_meta(buf: &[u8]) -> R<Meta> {
    let mut r = Rd::new(buf);
    let world_x = r.i32()?;
    let world_y = r.i32()?;
    let random_seed = r.i32()?;
    let inst_set = r.str()?;
    let every = r.u32()?;
    let genotypes_top_n = r.u32()?;
    let nl = r.count(5)?;
    let layers = (0..nl).map(|_| r.layer()).collect::<R<Vec<_>>>()?;
    let ni = r.count(4)?;
    let inst_names = (0..ni).map(|_| r.str()).collect::<R<Vec<_>>>()?;
    let na = r.count(4)?;
    let args = (0..na).map(|_| r.str()).collect::<R<Vec<_>>>()?;
    let work_dir = r.str()?;
    r.done()?;
    Ok(Meta {
        world_x,
        world_y,
        random_seed,
        inst_set,
        inst_names,
        layers,
        every,
        genotypes_top_n,
        args,
        work_dir,
    })
}

// --- writing ----------------------------------------------------------------

/// Appends frames to a recording file.
pub struct RecordingWriter {
    file: File,
    path: PathBuf,
    frames: u64,
    bytes: u64,
    last_update: Option<i32>,
}

impl RecordingWriter {
    /// Creates (or truncates) `path` and writes the header.
    pub fn create(path: &Path, meta: &Meta) -> Result<RecordingWriter, RecordingError> {
        let mut file = File::create(path)?;
        let header = encode_meta(meta);
        let mut out = Vec::with_capacity(16 + header.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(
            &u32::try_from(header.len())
                .unwrap_or(u32::MAX)
                .to_le_bytes(),
        );
        out.extend_from_slice(&header);
        file.write_all(&out)?;
        Ok(RecordingWriter {
            file,
            path: path.to_path_buf(),
            frames: 0,
            bytes: out.len() as u64,
            last_update: None,
        })
    }

    /// Appends one record (one `write` call, so a kill leaves at worst one
    /// cut record at the end). Returns the bytes it took.
    pub fn append(&mut self, frame: &Frame) -> Result<u64, RecordingError> {
        let row = zlib(&encode_row(&frame.header.stats));
        let raw = encode_frame(frame);
        let raw_len = raw.len();
        let packed = zlib(&raw);
        let body_len = 4 + 4 + row.len() + 4 + packed.len();
        if body_len as u64 >= MAX_RECORD {
            return Err(RecordingError::Corrupt {
                record: self.frames as usize + 1,
                why: "the frame is too large to record".to_string(),
            });
        }
        let mut out = Vec::with_capacity(4 + body_len);
        out.extend_from_slice(&(body_len as u32).to_le_bytes());
        out.extend_from_slice(&frame.update.to_le_bytes());
        out.extend_from_slice(&(row.len() as u32).to_le_bytes());
        out.extend_from_slice(&row);
        out.extend_from_slice(&(raw_len as u32).to_le_bytes());
        out.extend_from_slice(&packed);
        self.file.write_all(&out)?;
        self.frames += 1;
        self.bytes += out.len() as u64;
        self.last_update = Some(frame.update);
        Ok(out.len() as u64)
    }

    /// Records written so far.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// File size so far.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// The update of the last record written.
    pub fn last_update(&self) -> Option<i32> {
        self.last_update
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn zlib(data: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(data, LEVEL)
}

// --- reading ----------------------------------------------------------------

/// What follows the last complete record of a recording.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tail {
    /// Nothing: the file ends after a record.
    Clean,
    /// Part of a record: the run was killed while writing it, or it is
    /// still being written.
    Cut { bytes: u64 },
    /// Bytes that cannot be a record (a damaged length).
    Damaged { bytes: u64 },
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    offset: u64,
    body_len: u32,
    update: i32,
}

/// An opened recording: the header and an index of the complete records.
pub struct Recording {
    file: File,
    path: PathBuf,
    meta: Meta,
    inst_names: Arc<[String]>,
    index: Vec<Entry>,
    /// Where the scan for records resumes.
    scanned_to: u64,
    tail: Tail,
}

impl Recording {
    /// Opens `path`, checks its header and indexes its records.
    pub fn open(path: &Path) -> Result<Recording, RecordingError> {
        let mut file = File::open(path)?;
        let mut head = [0u8; 16];
        let got = read_up_to(&mut file, &mut head)?;
        if got < MAGIC.len() || &head[..8] != MAGIC {
            return Err(RecordingError::NotARecording);
        }
        if got < 16 {
            return Err(RecordingError::TruncatedHeader);
        }
        let version = u32::from_le_bytes([head[8], head[9], head[10], head[11]]);
        if version != VERSION {
            return Err(RecordingError::UnsupportedVersion(version));
        }
        let header_len = u32::from_le_bytes([head[12], head[13], head[14], head[15]]);
        if header_len > MAX_HEADER {
            return Err(RecordingError::Corrupt {
                record: 0,
                why: "the header is too large".to_string(),
            });
        }
        let mut header = vec![0u8; header_len as usize];
        if read_up_to(&mut file, &mut header)? < header.len() {
            return Err(RecordingError::TruncatedHeader);
        }
        let meta =
            decode_meta(&header).map_err(|why| RecordingError::Corrupt { record: 0, why })?;
        let inst_names: Arc<[String]> = meta.inst_names.clone().into();
        let mut rec = Recording {
            file,
            path: path.to_path_buf(),
            meta,
            inst_names,
            index: Vec::new(),
            scanned_to: 16 + u64::from(header_len),
            tail: Tail::Clean,
        };
        rec.refresh()?;
        Ok(rec)
    }

    /// Indexes the records appended since the last scan (a recording that
    /// is still being written). Returns how many there are now.
    pub fn refresh(&mut self) -> Result<usize, RecordingError> {
        let file_len = self.file.metadata()?.len();
        let mut pos = self.scanned_to;
        self.tail = Tail::Clean;
        while pos < file_len {
            let left = file_len - pos;
            if left < 8 {
                self.tail = Tail::Cut { bytes: left };
                break;
            }
            self.file.seek(SeekFrom::Start(pos))?;
            let mut p = [0u8; 8];
            self.file.read_exact(&mut p)?;
            let body_len = u32::from_le_bytes([p[0], p[1], p[2], p[3]]);
            let update = i32::from_le_bytes([p[4], p[5], p[6], p[7]]);
            // update, row length, row, raw length, frame: at least 12 bytes.
            if body_len < 12 || u64::from(body_len) >= MAX_RECORD {
                self.tail = Tail::Damaged { bytes: left };
                break;
            }
            let end = pos + 4 + u64::from(body_len);
            if end > file_len {
                self.tail = Tail::Cut { bytes: left };
                break;
            }
            self.index.push(Entry {
                offset: pos,
                body_len,
                update,
            });
            pos = end;
        }
        self.scanned_to = pos;
        Ok(self.index.len())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn meta(&self) -> &Meta {
        &self.meta
    }

    /// Complete records.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// What follows the last complete record.
    pub fn tail(&self) -> Tail {
        self.tail
    }

    /// The update record `i` was captured at (from the index, no I/O).
    pub fn update_of(&self, i: usize) -> Option<i32> {
        self.index.get(i).map(|e| e.update)
    }

    /// The index of the last record captured at or before `update`
    /// (the first record when `update` precedes them all); `None` when
    /// there are no records. Record updates ascend.
    pub fn position_at_or_before(&self, update: i32) -> Option<usize> {
        if self.index.is_empty() {
            return None;
        }
        let after = self.index.partition_point(|e| e.update <= update);
        Some(after.saturating_sub(1))
    }

    fn body(&mut self, i: usize) -> Result<Vec<u8>, RecordingError> {
        let e = *self.index.get(i).ok_or(RecordingError::NoSuchRecord(i))?;
        self.file.seek(SeekFrom::Start(e.offset + 4))?;
        let mut body = vec![0u8; e.body_len as usize];
        self.file.read_exact(&mut body)?;
        Ok(body)
    }

    /// The statistics row of record `i`; reads only its first bytes.
    pub fn row(&mut self, i: usize) -> Result<UpdateRow, RecordingError> {
        let e = *self.index.get(i).ok_or(RecordingError::NoSuchRecord(i))?;
        let corrupt = |why: String| RecordingError::Corrupt { record: i, why };
        self.file.seek(SeekFrom::Start(e.offset + 8))?;
        let mut len = [0u8; 4];
        self.file.read_exact(&mut len)?;
        let row_len = u32::from_le_bytes(len);
        if u64::from(row_len) + 8 > u64::from(e.body_len) {
            return Err(corrupt("the row is longer than the record".to_string()));
        }
        let mut packed = vec![0u8; row_len as usize];
        self.file.read_exact(&mut packed)?;
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&packed, 1 << 24)
            .map_err(|e| corrupt(format!("the row does not decompress ({e:?})")))?;
        decode_row(&raw).map_err(corrupt)
    }

    /// Record `i` as a [`Frame`] (without an inspected organism or a
    /// lineage).
    pub fn frame(&mut self, i: usize) -> Result<Frame, RecordingError> {
        let corrupt = |why: String| RecordingError::Corrupt { record: i, why };
        let body = self.body(i)?;
        let mut r = Rd::new(&body);
        let update = r.i32().map_err(corrupt)?;
        let row_len = r.u32().map_err(corrupt)? as usize;
        let row_packed = r.take(row_len).map_err(corrupt)?;
        let raw_len = r.u32().map_err(corrupt)? as usize;
        let packed = r.take(r.remaining()).map_err(corrupt)?;
        if raw_len as u64 >= MAX_RECORD {
            return Err(corrupt("a frame of impossible size".to_string()));
        }
        let row_raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(row_packed, 1 << 24)
            .map_err(|e| corrupt(format!("the row does not decompress ({e:?})")))?;
        let stats = decode_row(&row_raw).map_err(corrupt)?;
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(packed, raw_len)
            .map_err(|e| corrupt(format!("the frame does not decompress ({e:?})")))?;
        if raw.len() != raw_len {
            return Err(corrupt("the frame has the wrong size".to_string()));
        }
        let mut r = Rd::new(&raw);
        let frame = self.decode_frame(&mut r, update, stats).map_err(corrupt)?;
        r.done().map_err(corrupt)?;
        Ok(frame)
    }

    fn decode_frame(&self, r: &mut Rd, update: i32, stats: UpdateRow) -> R<Frame> {
        let done = r.bool()?;
        let progress = UpdateProgress {
            in_update: r.bool()?,
            slices_done: r.i32()?,
            slices_total: r.i32()?,
        };
        let last_finished_update = r.i32()?;
        let map = if r.bool()? {
            Some(decode_map(r)?)
        } else {
            None
        };
        let dominant = if r.bool()? {
            let row = get_genotype_row(r)?;
            Some(GenotypeView {
                row,
                genome: r.str()?,
                ave_copy_size: r.f64()?,
                ave_exe_size: r.f64()?,
                ave_repro_rate: r.f64()?,
                max_fitness: r.f64()?,
                last_breed_true: r.i32()?,
                last_breed_in: r.i32()?,
                parents: r.str()?,
            })
        } else {
            None
        };
        let n = r.count(8)?;
        let genotypes = (0..n).map(|_| get_genotype_row(r)).collect::<R<Vec<_>>>()?;
        let n = r.count(8)?;
        let resources = (0..n)
            .map(|_| {
                Ok(ResourceView {
                    name: r.str()?,
                    geometry: r.i32()?,
                    spatial: r.bool()?,
                    quantity: r.f64()?,
                    inflow: r.opt_f64()?,
                    outflow: r.opt_f64()?,
                })
            })
            .collect::<R<Vec<_>>>()?;
        let n = r.count(8)?;
        let reactions = (0..n)
            .map(|_| {
                let name = r.str()?;
                let has_task = r.bool()?;
                let task = r.u32()? as usize;
                Ok(ReactionView {
                    name,
                    task: has_task.then_some(task),
                    value: r.f64()?,
                    last_count: r.i32()?,
                })
            })
            .collect::<R<Vec<_>>>()?;
        let n = r.count(8)?;
        let tasks = (0..n)
            .map(|_| {
                Ok(TaskView {
                    name: r.str()?,
                    desc: r.str()?,
                    last_count: r.i32()?,
                })
            })
            .collect::<R<Vec<_>>>()?;
        Ok(Frame {
            update,
            header: Header {
                world_x: self.meta.world_x,
                world_y: self.meta.world_y,
                random_seed: self.meta.random_seed,
                inst_set: self.meta.inst_set.clone(),
                done,
                progress,
                last_finished_update,
                stats,
            },
            map,
            dominant,
            genotypes,
            inspect: None,
            env: EnvView {
                resources,
                reactions,
                tasks,
            },
            inst_names: Arc::clone(&self.inst_names),
            layers: self.meta.layers.clone(),
            lineage: Vec::new(),
        })
    }
}

/// Reads until `buf` is full or the file ends; returns the bytes read.
fn read_up_to(file: &mut File, buf: &mut [u8]) -> io::Result<usize> {
    let mut got = 0;
    while got < buf.len() {
        match file.read(&mut buf[got..]) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(got)
}

/// Equality of two maps that treats NaN (an empty cell of a per-organism
/// layer) as equal to NaN, by comparing the bits of the float layers.
pub trait MapFrameCompare {
    fn same_as(&self, other: &MapFrame) -> bool;
}

impl MapFrameCompare for MapFrame {
    fn same_as(&self, other: &MapFrame) -> bool {
        let bits = |v: &[f32]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
        self.w == other.w
            && self.h == other.h
            && self.genotype_id == other.genotype_id
            && self.top_genotypes == other.top_genotypes
            && self.layers.len() == other.layers.len()
            && self
                .layers
                .iter()
                .zip(&other.layers)
                .all(|((a, x), (b, y))| a == b && bits(x) == bits(y))
            && self.num_tasks == other.num_tasks
            && self.task_bits == other.task_bits
            && self.words_per_cell == other.words_per_cell
            && self.top_counts == other.top_counts
            && self.genotype_cells == other.genotype_cells
            && self.families == other.families
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(update: i32, w: i32, h: i32) -> Frame {
        let n = (w * h) as usize;
        let ids: Vec<i32> = (0..n)
            .map(|i| match i % 5 {
                0 => -1,
                1 => 7,
                2 => 3,
                3 => -2,
                _ => 3,
            })
            .collect();
        let fit: Vec<f32> = (0..n)
            .map(|i| {
                if i % 5 == 0 {
                    f32::NAN
                } else {
                    i as f32 * 0.25
                }
            })
            .collect();
        let anc: Vec<f32> = (0..n).map(|i| (i % 9) as f32 + 0.5).collect();
        let row = UpdateRow {
            update,
            orgs: 4,
            genotypes: 3,
            threshold_genotypes: 1,
            ave_fitness: 1.5,
            ave_merit: -2.0,
            ave_gestation: 3.0,
            ave_generation: f64::INFINITY,
            ave_age: 0.1,
            ave_mem_size: 50.0,
            max_fitness: 9.0,
            max_merit: 8.0,
            births: 2,
            deaths: 1,
            breed_true: 1,
            executed: 900,
            threads: 4,
            tot_organisms: 17,
            task_last_count: vec![0, 2, 1],
            resources: vec![1.5, 0.0],
        };
        let genotype_cells = derive_genotype_cells(&ids);
        let genotype_row = |rank: usize, id: i32| GenotypeRow {
            rank,
            id,
            parent_id: -1,
            name: format!("{id:03}-aaaaa"),
            abundance: 2,
            depth: 1,
            update_born: 0,
            genome_len: 50,
            threshold: true,
            ave_fitness: 0.5,
            ave_merit: 100.0,
            ave_gestation: 400.0,
            last_births: 3,
        };
        Frame {
            update,
            header: Header {
                world_x: w,
                world_y: h,
                random_seed: 42,
                inst_set: "heads_default".into(),
                done: false,
                progress: UpdateProgress::default(),
                last_finished_update: update,
                stats: row,
            },
            map: Some(MapFrame {
                w,
                h,
                genotype_id: ids.into(),
                top_genotypes: vec![3, 7].into(),
                layers: vec![
                    (LayerId::Fitness, fit.into()),
                    (LayerId::Ancestry, anc.into()),
                    (LayerId::Task(1), vec![0.0; n].into()),
                ],
                num_tasks: 70,
                task_bits: (0..n * 2)
                    .map(|i| (i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15))
                    .collect(),
                words_per_cell: 2,
                top_counts: vec![9, 4].into(),
                genotype_cells,
                families: vec![
                    Family {
                        root: 3,
                        organisms: 9,
                        slot: 0,
                    },
                    Family {
                        root: 7,
                        organisms: 4,
                        slot: 2,
                    },
                ]
                .into(),
            }),
            dominant: Some(GenotypeView {
                row: genotype_row(0, 3),
                genome: "wzcagc".into(),
                ave_copy_size: 50.0,
                ave_exe_size: 49.0,
                ave_repro_rate: 0.01,
                max_fitness: 0.9,
                last_breed_true: 4,
                last_breed_in: 0,
                parents: "1,2".into(),
            }),
            genotypes: vec![genotype_row(0, 3), genotype_row(GenotypeRow::NOT_RANKED, 7)],
            inspect: None,
            env: EnvView {
                resources: vec![ResourceView {
                    name: "resNOT".into(),
                    geometry: 0,
                    spatial: false,
                    quantity: 12.5,
                    inflow: Some(1.0),
                    outflow: None,
                }],
                reactions: vec![ReactionView {
                    name: "NOT".into(),
                    task: Some(0),
                    value: 1.0,
                    last_count: 2,
                }],
                tasks: vec![TaskView {
                    name: "not".into(),
                    desc: "Not".into(),
                    last_count: 2,
                }],
            },
            inst_names: Arc::from(vec!["nop-A".to_string(), "h-copy".to_string()]),
            layers: vec![
                LayerId::Genotype,
                LayerId::Fitness,
                LayerId::Ancestry,
                LayerId::Task(1),
            ],
            lineage: Vec::new(),
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("avida-recording-unit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    fn write(path: &Path, frames: &[Frame]) {
        let meta = Meta::for_frame(
            &frames[0],
            10,
            100,
            vec!["-s".into(), "7".into()],
            "/x".into(),
        );
        let mut w = RecordingWriter::create(path, &meta).unwrap();
        for f in frames {
            w.append(f).unwrap();
        }
        assert_eq!(w.frames(), frames.len() as u64);
        assert_eq!(w.bytes(), std::fs::metadata(path).unwrap().len());
    }

    #[test]
    fn frames_round_trip() {
        let path = scratch("round.avrec");
        let frames = [frame(0, 6, 5), frame(10, 6, 5), frame(20, 6, 5)];
        write(&path, &frames);
        let mut r = Recording::open(&path).unwrap();
        assert_eq!(r.len(), 3);
        assert_eq!(r.tail(), Tail::Clean);
        assert_eq!(r.meta().args, ["-s", "7"]);
        assert_eq!(r.meta().layers, frames[0].layers);
        for (i, want) in frames.iter().enumerate() {
            let got = r.frame(i).unwrap();
            assert_eq!(got.update, want.update);
            assert_eq!(got.header, want.header);
            assert_eq!(got.dominant, want.dominant);
            assert_eq!(got.genotypes, want.genotypes);
            assert_eq!(got.env, want.env);
            assert_eq!(got.inst_names, want.inst_names);
            assert_eq!(got.layers, want.layers);
            assert!(got
                .map
                .as_ref()
                .unwrap()
                .same_as(want.map.as_ref().unwrap()));
            assert_eq!(r.row(i).unwrap(), want.header.stats);
        }
        assert!(matches!(r.frame(3), Err(RecordingError::NoSuchRecord(3))));
    }

    #[test]
    fn positions_are_found_by_update() {
        let path = scratch("seek.avrec");
        write(&path, &[frame(0, 3, 3), frame(10, 3, 3), frame(20, 3, 3)]);
        let r = Recording::open(&path).unwrap();
        assert_eq!(r.position_at_or_before(-3), Some(0));
        assert_eq!(r.position_at_or_before(0), Some(0));
        assert_eq!(r.position_at_or_before(15), Some(1));
        assert_eq!(r.position_at_or_before(20), Some(2));
        assert_eq!(r.position_at_or_before(999), Some(2));
    }

    #[test]
    fn every_cut_of_a_file_opens_or_errors_without_panicking() {
        let path = scratch("cuts.avrec");
        write(&path, &[frame(0, 4, 4), frame(10, 4, 4), frame(20, 4, 4)]);
        let bytes = std::fs::read(&path).unwrap();
        let cut = scratch("cuts-cut.avrec");
        let mut last = 0;
        for n in 0..=bytes.len() {
            std::fs::write(&cut, &bytes[..n]).unwrap();
            match Recording::open(&cut) {
                Ok(mut r) => {
                    assert!(r.len() >= last, "records only come back with more bytes");
                    last = r.len();
                    for i in 0..r.len() {
                        r.frame(i).unwrap();
                    }
                    if n == bytes.len() {
                        assert_eq!((r.len(), r.tail()), (3, Tail::Clean));
                    }
                }
                Err(e) => assert!(n < 16 + 200, "cut at {n}: {e}"),
            }
        }
        assert_eq!(last, 3);
    }

    #[test]
    fn a_future_version_and_a_foreign_file_are_rejected() {
        let path = scratch("version.avrec");
        write(&path, &[frame(0, 3, 3)]);
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[8..12].copy_from_slice(&99u32.to_le_bytes());
        std::fs::write(&path, &bytes).unwrap();
        let e = Recording::open(&path).err().unwrap();
        assert!(matches!(e, RecordingError::UnsupportedVersion(99)), "{e}");
        assert!(e.to_string().contains("version 99"));
        std::fs::write(&path, b"PK\x03\x04 something else entirely").unwrap();
        assert!(matches!(
            Recording::open(&path).err().unwrap(),
            RecordingError::NotARecording
        ));
    }

    #[test]
    fn a_damaged_record_is_an_error_and_the_others_still_read() {
        let path = scratch("damage.avrec");
        write(&path, &[frame(0, 4, 4), frame(10, 4, 4), frame(20, 4, 4)]);
        let clean = Recording::open(&path).unwrap();
        let second = clean.index[1];
        let mut bytes = std::fs::read(&path).unwrap();
        // The middle of the second record's frame block.
        let at = second.offset as usize + 4 + second.body_len as usize - 5;
        bytes[at] ^= 0xff;
        std::fs::write(&path, &bytes).unwrap();
        let mut r = Recording::open(&path).unwrap();
        assert_eq!(r.len(), 3);
        assert!(r.frame(0).is_ok());
        assert!(matches!(
            r.frame(1),
            Err(RecordingError::Corrupt { record: 1, .. })
        ));
        assert!(r.frame(2).is_ok());
    }

    #[test]
    fn a_huge_length_is_not_allocated() {
        let path = scratch("huge.avrec");
        write(&path, &[frame(0, 3, 3)]);
        let mut bytes = std::fs::read(&path).unwrap();
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
        bytes.extend_from_slice(&5i32.to_le_bytes());
        bytes.extend_from_slice(&[0; 64]);
        std::fs::write(&path, &bytes).unwrap();
        let r = Recording::open(&path).unwrap();
        assert_eq!(r.len(), 1);
        assert!(matches!(r.tail(), Tail::Damaged { .. }));
    }
}
