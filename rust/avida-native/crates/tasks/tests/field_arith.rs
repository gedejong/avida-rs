//! `field_arith` (port addition): each function and both modes against a
//! plain per-field reference, chance level, `thresh`, argument errors,
//! `planes` with too few outputs, the environment requirements.

use avida_tasks::{EnvReqs, Feedback, IoBuffer, TaskContext, TaskLib};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }
}

fn load(args: &str) -> Result<(TaskLib, usize, EnvReqs), Vec<String>> {
    let mut lib = TaskLib::new();
    let mut er = EnvReqs::new();
    let mut fb = Feedback::new();
    match lib.add_task("field_arith", args, &mut er, &mut fb) {
        Some(id) => Ok((lib, id, er)),
        None => Err(fb.errors().into_iter().map(String::from).collect()),
    }
}

fn eval(lib: &TaskLib, id: usize, input: &IoBuffer, output: &IoBuffer) -> f64 {
    let mut ctx = TaskContext::simple(input, output);
    lib.setup_tests(&mut ctx);
    ctx.set_task_entry(id);
    lib.test_output_pure(&mut ctx).expect("pure task")
}

/// Inputs as the organism's buffer holds them: `b` read last (newest).
fn inputs(a: u32, b: u32) -> IoBuffer {
    let mut i = IoBuffer::new(3);
    i.add(0x0f0f_0f0f);
    i.add(a as i32);
    i.add(b as i32);
    i
}

fn outputs(cap: usize, values: &[u32]) -> IoBuffer {
    let mut o = IoBuffer::new(cap);
    for &v in values {
        o.add(v as i32);
    }
    o
}

/// One field's operands and result, by plain arithmetic, no masks.
fn field_result(func: &str, w: u32, a: u32, b: u32) -> u32 {
    let m = (1u32 << w) - 1;
    let (a, b) = (a & m, b & m);
    match func {
        "xor" => a ^ b,
        "and" => a & b,
        "or" => a | b,
        _ => a + b,
    }
}

struct Ref {
    func: &'static str,
    w: u32,
    r: u32,
    stride: u32,
    n: u32,
}

impl Ref {
    fn new(func: &'static str, w: u32, stride: Option<u32>) -> Ref {
        let r = if func == "add" || func == "halfadd" {
            w + 1
        } else {
            w
        };
        let stride = stride.unwrap_or(r);
        Ref {
            func,
            w,
            r,
            stride,
            n: 32 / stride,
        }
    }
    fn args(&self, mode: &str, stride: Option<u32>, extra: &str) -> String {
        let mut s = format!("fn={},bits={},mode={}", self.func, self.w, mode);
        if let Some(st) = stride {
            s += &format!(",stride={}", st);
        }
        s + extra
    }
    /// The result bits of field `k` for operands `a`, `b`.
    fn result(&self, k: u32, a: u32, b: u32) -> u32 {
        let sh = k * self.stride;
        field_result(self.func, self.w, a >> sh, b >> sh)
    }
    /// The word a packed output should be.
    fn packed(&self, a: u32, b: u32) -> u32 {
        (0..self.n).fold(0, |acc, k| {
            acc | (self.result(k, a, b) << (k * self.stride))
        })
    }
    /// The `r` plane outputs, least significant plane first.
    fn planes(&self, a: u32, b: u32) -> Vec<u32> {
        (0..self.r)
            .map(|i| {
                (0..self.n).fold(0, |acc, k| {
                    acc | (((self.result(k, a, b) >> i) & 1) << (k * self.stride))
                })
            })
            .collect()
    }
    /// Reference fraction of compared packed bits right.
    fn packed_right(&self, out: u32, a: u32, b: u32) -> (u32, u32) {
        let mut right = 0;
        for k in 0..self.n {
            let res = self.result(k, a, b);
            for i in 0..self.r {
                let pos = k * self.stride + i;
                right += u32::from((out >> pos) & 1 == (res >> i) & 1);
            }
        }
        (right, self.n * self.r)
    }
    fn planes_right(&self, outs: &[u32], a: u32, b: u32) -> (u32, u32) {
        let mut right = 0;
        for k in 0..self.n {
            let res = self.result(k, a, b);
            for i in 0..self.r {
                let pos = k * self.stride;
                right += u32::from((outs[i as usize] >> pos) & 1 == (res >> i) & 1);
            }
        }
        (right, self.n * self.r)
    }
}

fn quality(right: u32, total: u32) -> f64 {
    f64::from((2 * right).saturating_sub(total)) / f64::from(total)
}

const CASES: &[(&str, u32)] = &[
    ("xor", 1),
    ("xor", 2),
    ("xor", 3),
    ("xor", 4),
    ("and", 1),
    ("and", 3),
    ("and", 4),
    ("or", 2),
    ("or", 3),
    ("halfadd", 1),
    ("add", 1),
    ("add", 2),
    ("add", 3),
    ("add", 4),
];

#[test]
fn exact_answers_earn_full_quality_in_both_modes() {
    let mut rng = Rng(0x1234_5678_9abc_def1);
    for &(func, w) in CASES {
        for stride in [None, Some(8), Some(5)] {
            let rf = Ref::new(func, w, stride);
            if rf.stride < rf.r {
                continue;
            }
            let (lib, packed_id, _) = load(&rf.args("packed", stride, "")).unwrap();
            let (lib_p, planes_id, _) = load(&rf.args("planes", stride, "")).unwrap();
            for _ in 0..200 {
                let (a, b) = (rng.next(), rng.next());
                let out = rf.packed(a, b);
                assert_eq!(
                    eval(&lib, packed_id, &inputs(a, b), &outputs(1, &[out])),
                    1.0,
                    "{func} w={w} stride={stride:?} a={a:#x} b={b:#x}"
                );
                // Pad bits and operand garbage above the result are free.
                let noisy = out
                    | (rng.next()
                        & !{
                            let mut m = 0;
                            for k in 0..rf.n {
                                m |= ((1u32 << rf.r) - 1) << (k * rf.stride);
                            }
                            m
                        });
                assert_eq!(
                    eval(&lib, packed_id, &inputs(a, b), &outputs(1, &[noisy])),
                    1.0
                );
                let planes = rf.planes(a, b);
                let noisy_planes: Vec<u32> = planes
                    .iter()
                    .map(|&p| {
                        let junk = rng.next();
                        let keep: u32 = (0..rf.n).fold(0, |m, k| m | (1 << (k * rf.stride)));
                        (p & keep) | (junk & !keep)
                    })
                    .collect();
                for ps in [&planes, &noisy_planes] {
                    assert_eq!(
                        eval(
                            &lib_p,
                            planes_id,
                            &inputs(a, b),
                            &outputs(rf.r as usize, ps)
                        ),
                        1.0,
                        "{func} w={w} planes stride={stride:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn quality_follows_the_reference_on_random_outputs() {
    let mut rng = Rng(0xdead_beef_cafe_f00d);
    for &(func, w) in CASES {
        let rf = Ref::new(func, w, None);
        let (lib, pid, _) = load(&rf.args("packed", None, ",thresh=0")).unwrap();
        let (lib_p, qid, _) = load(&rf.args("planes", None, ",thresh=0")).unwrap();
        let mut sum = 0.0;
        let trials = 400;
        for _ in 0..trials {
            let (a, b) = (rng.next(), rng.next());
            // Start from the right answer and corrupt random bits.
            let corrupt = rng.next() & rng.next();
            let out = rf.packed(a, b) ^ corrupt;
            let (right, total) = rf.packed_right(out, a, b);
            let got = eval(&lib, pid, &inputs(a, b), &outputs(1, &[out]));
            assert_eq!(got, quality(right, total), "{func} w={w} packed");
            let mut planes = rf.planes(a, b);
            for p in planes.iter_mut() {
                *p ^= rng.next() & rng.next();
            }
            let (right, total) = rf.planes_right(&planes, a, b);
            let got = eval(&lib_p, qid, &inputs(a, b), &outputs(rf.r as usize, &planes));
            assert_eq!(got, quality(right, total), "{func} w={w} planes");
            // Pure noise: chance level.
            let noise = rng.next();
            let (right, total) = rf.packed_right(noise, a, b);
            sum += quality(right, total);
        }
        let mean = sum / f64::from(trials);
        assert!(mean < 0.12, "{func} w={w}: mean noise quality {mean}");
    }
}

#[test]
fn the_documented_add_with_stride_four_is_the_nibble_expression() {
    let mut rng = Rng(7);
    let (lib, id, _) = load("fn=add,bits=2,stride=4").unwrap();
    for _ in 0..200 {
        let (a, b) = (rng.next(), rng.next());
        let want = ((a & 0x3333_3333).wrapping_add(b & 0x3333_3333)) & 0x7777_7777;
        assert_eq!(eval(&lib, id, &inputs(a, b), &outputs(1, &[want])), 1.0);
        assert!(eval(&lib, id, &inputs(a, b), &outputs(1, &[want ^ 1])) < 1.0);
    }
    // The default stride is the result width: ten 3-bit fields.
    let (lib, id, _) = load("fn=add,bits=2").unwrap();
    let (a, b) = (0x3fff_ffffu32, 0x1555_5555u32);
    let mut want = 0u32;
    for k in 0..10 {
        let (x, y) = ((a >> (3 * k)) & 3, (b >> (3 * k)) & 3);
        want |= (x + y) << (3 * k);
    }
    assert_eq!(eval(&lib, id, &inputs(a, b), &outputs(1, &[want])), 1.0);
    // The two top bits of the word are no field.
    assert_eq!(
        eval(&lib, id, &inputs(a, b), &outputs(1, &[want ^ 0xc000_0000])),
        1.0
    );
}

#[test]
fn halfadd_is_add_with_one_bit_and_a_newer_operand_does_not_matter() {
    let mut rng = Rng(99);
    let (l1, h, _) = load("fn=halfadd,bits=1,mode=planes").unwrap();
    let (l2, a2, _) = load("fn=add,bits=1,mode=planes").unwrap();
    let (l3, h3, _) = load("fn=halfadd").unwrap();
    for _ in 0..100 {
        let (a, b) = (rng.next(), rng.next());
        let planes = [rng.next(), rng.next()];
        let i1 = inputs(a, b);
        let o = outputs(2, &planes);
        assert_eq!(eval(&l1, h, &i1, &o), eval(&l2, a2, &i1, &o));
        // Swapped operands give the same expectation (commutative).
        assert_eq!(eval(&l1, h, &inputs(b, a), &o), eval(&l1, h, &i1, &o));
        let o1 = outputs(1, &[planes[0]]);
        assert_eq!(
            eval(&l3, h3, &i1, &o1),
            eval(&load("fn=add,bits=1").unwrap().0, 0, &i1, &o1)
        );
    }
    // Which input is a and which b: only the two newest are read.
    let (lib, id, _) = load("fn=xor,bits=1").unwrap();
    let mut i = IoBuffer::new(3);
    i.add(0x1111_1111);
    i.add(0x0000_00ff);
    i.add(0x0000_0f0f);
    assert_eq!(eval(&lib, id, &i, &outputs(1, &[0x0000_0ff0])), 1.0);
}

#[test]
fn planes_are_least_significant_first_and_ordered_by_age() {
    let (lib, id, _) = load("fn=halfadd,mode=planes").unwrap();
    // Sum bit then carry bit, at bit 0 of every 2-bit field.
    let (a, b) = (0x5555_5555u32, 0x5555_5555u32);
    let sum_plane = 0; // 1 + 1 = 0b10 in every field
    let carry_plane = 0x5555_5555;
    let i = inputs(a, b);
    assert_eq!(
        eval(&lib, id, &i, &outputs(2, &[sum_plane, carry_plane])),
        1.0
    );
    // Reversed order: both planes wrong everywhere.
    assert_eq!(
        eval(&lib, id, &i, &outputs(2, &[carry_plane, sum_plane])),
        0.0
    );
    // Older outputs beyond the planes do not matter.
    assert_eq!(
        eval(&lib, id, &i, &outputs(3, &[7, sum_plane, carry_plane])),
        1.0
    );
}

#[test]
fn planes_with_too_few_outputs_or_inputs_earn_nothing() {
    let (lib, id, er) = load("fn=add,bits=2,mode=planes").unwrap();
    assert_eq!(er.get_min_outputs(), 3);
    let i = inputs(0x1111_1111, 0x2222_2222);
    let planes = Ref::new("add", 2, None).planes(0x1111_1111, 0x2222_2222);
    assert_eq!(eval(&lib, id, &i, &outputs(3, &planes)), 1.0);
    // Two outputs given to a buffer of 3: the oldest plane was never output.
    assert_eq!(eval(&lib, id, &i, &outputs(3, &planes[1..])), 0.0);
    // A buffer too small for the planes stores fewer than it needs.
    assert_eq!(eval(&lib, id, &i, &outputs(2, &planes)), 0.0);
    assert_eq!(eval(&lib, id, &i, &outputs(3, &[])), 0.0);
    // Fewer than two inputs.
    let mut one = IoBuffer::new(3);
    one.add(0x1111_1111);
    assert_eq!(eval(&lib, id, &one, &outputs(3, &planes)), 0.0);
    assert_eq!(eval(&lib, id, &IoBuffer::new(3), &outputs(3, &planes)), 0.0);
    let (lib, id, _) = load("fn=xor").unwrap();
    assert_eq!(eval(&lib, id, &one, &outputs(1, &[0x1111_1111])), 0.0);
}

#[test]
fn thresh_cuts_low_quality_and_defaults_to_full_credit_only() {
    // 32 one-bit fields of xor: flip 8 of 32 bits for f = 0.75, q = 0.5.
    let (a, b) = (0xaaaa_5555u32, 0x0f0f_0f0fu32);
    let right = a ^ b;
    let wrong = right ^ 0x0000_00ff;
    let i = inputs(a, b);
    let q = |extra: &str| {
        let (lib, id, _) = load(&format!("fn=xor,bits=1{extra}")).unwrap();
        eval(&lib, id, &i, &outputs(1, &[wrong]))
    };
    assert_eq!(q(",thresh=0"), 0.5);
    assert_eq!(q(",thresh=0.5"), 0.5);
    assert_eq!(q(",thresh=0.51"), 0.0);
    assert_eq!(q(""), 0.0); // default thresh 1.0
    let (lib, id, _) = load("fn=xor,bits=1").unwrap();
    assert_eq!(eval(&lib, id, &i, &outputs(1, &[right])), 1.0);
    // Exactly at the threshold counts: q = 0.25 at f = 0.625.
    let wrong12 = right ^ 0x0000_0fff;
    let (lib, id, _) = load("fn=xor,bits=1,thresh=0.25").unwrap();
    assert_eq!(eval(&lib, id, &i, &outputs(1, &[wrong12])), 0.25);
    // All bits wrong and half the bits wrong both earn nothing.
    assert_eq!(eval(&lib, id, &i, &outputs(1, &[!right])), 0.0);
    assert_eq!(eval(&lib, id, &i, &outputs(1, &[right ^ 0xffff])), 0.0);
}

#[test]
fn colon_and_comma_separators_both_parse_and_case_is_ignored() {
    let (_, _, a) = load("fn=add:bits=3:mode=planes").unwrap();
    let (_, _, b) = load("FN=ADD,bits=3,Mode=Planes").unwrap();
    assert_eq!(a.get_min_outputs(), 4);
    assert_eq!(b.get_min_outputs(), 4);
}

#[test]
fn argument_errors_are_reported() {
    let cases: &[(&str, &str)] = &[
        ("", "required argument 'fn'"),
        ("bits=2", "required argument 'fn'"),
        ("fn=mul", "unknown fn 'mul'"),
        ("fn=xor,mode=serial", "unknown mode 'serial'"),
        ("fn=xor,bits=0", "exceeds its defined range"),
        ("fn=xor,bits=5", "bits=5 is not in 1..4"),
        ("fn=halfadd,bits=2", "halfadd needs bits=1"),
        ("fn=add,bits=2,stride=2", "stride=2 is not in 3..32"),
        ("fn=add,bits=2,stride=33", "stride=33"),
        ("fn=add,thresh=1.5", "thresh=1.5"),
        ("fn=add,thresh=-0.1", "thresh=-0.1"),
        ("fn=add,colour=red", "unrecognized argument"),
    ];
    for (args, want) in cases {
        match load(args) {
            Ok(_) => {
                // `bits=0` is rejected by the range check of the schema or
                // by ours; either way it must not load.
                panic!("{args:?} loaded");
            }
            Err(errs) => {
                if *args == "fn=xor,bits=0" {
                    assert!(!errs.is_empty());
                } else {
                    assert!(
                        errs.iter().any(|e| e.contains(want)),
                        "{args:?}: {errs:?} lacks {want:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn environment_requirements_and_distinct_entries() {
    let (_, _, er) = load("fn=add,bits=2").unwrap();
    assert_eq!(
        er.get_min_outputs(),
        1,
        "packed needs only the default buffer"
    );
    let (_, _, er) = load("fn=xor,bits=4,mode=planes").unwrap();
    assert_eq!(er.get_min_outputs(), 4);
    assert_eq!(er.get_min_inputs(), 2);
    let (_, _, er) = load("fn=add,bits=4,mode=planes").unwrap();
    assert_eq!(er.get_min_outputs(), 5);
    // Entries with arguments are never merged.
    let mut lib = TaskLib::new();
    let mut er = EnvReqs::new();
    let mut fb = Feedback::new();
    let a = lib.add_task("field_arith", "fn=add,bits=2", &mut er, &mut fb);
    let b = lib.add_task("field_arith", "fn=add,bits=3", &mut er, &mut fb);
    assert_eq!((a, b), (Some(0), Some(1)));
    assert!(lib.is_organism_local(0));
    assert_eq!(lib.get_task(1).unwrap().field_spec().unwrap().bits(), 3);
    // The larger of two plane counts wins.
    lib.add_task("field_arith", "fn=add,bits=1,mode=planes", &mut er, &mut fb);
    lib.add_task("field_arith", "fn=xor,bits=1,mode=planes", &mut er, &mut fb);
    assert_eq!(er.get_min_outputs(), 2);
}

#[test]
fn naive_outputs_score_above_chance_on_the_packed_rungs() {
    // Inputs as the environment makes them (a fixed top byte). The result
    // bits are unbalanced and a quarter of them are fixed by the top
    // bytes, so a threshold of 0.25 pays outputs that compute nothing;
    // ARCHITECTURE.md quotes these figures.
    let mut rng = Rng(0xfeed_0042);
    let (half, hid, _) = load("fn=halfadd,thresh=0").unwrap();
    let (add2, aid, _) = load("fn=add,bits=2,thresh=0").unwrap();
    let naive: [fn(u32, u32) -> u32; 7] = [
        |_, _| 0,
        |_, _| !0,
        |a, _| a,
        |_, b| b,
        |a, b| a ^ b,
        |a, b| a & b,
        |a, b| a | b,
    ];
    let trials = 600;
    for (name, lib, id) in [("halfadd", &half, hid), ("add2", &add2, aid)] {
        let (mut zero, mut xor, mut top_hits) = (0.0, 0.0, 0);
        for _ in 0..trials {
            let a = (0x0f << 24) | (rng.next() & 0xff_ffff);
            let b = (0x33 << 24) | (rng.next() & 0xff_ffff);
            let i = inputs(a, b);
            let q = |f: fn(u32, u32) -> u32| eval(lib, id, &i, &outputs(1, &[f(a, b)]));
            zero += q(naive[0]);
            xor += q(naive[4]);
            let best = naive.iter().map(|&f| q(f)).fold(0.0, f64::max);
            top_hits += usize::from(best >= 0.75);
        }
        let (zero, xor) = (zero / trials as f64, xor / trials as f64);
        assert!((0.4..0.5).contains(&xor), "{name}: a ^ b scores {xor}");
        if name == "halfadd" {
            assert!((0.2..0.3).contains(&zero), "{name}: 0 scores {zero}");
        } else {
            assert!(zero < 0.1, "{name}: 0 scores {zero}");
        }
        assert!(
            top_hits * 50 < trials,
            "{name}: naive >= 0.75 in {top_hits}"
        );
    }
}
