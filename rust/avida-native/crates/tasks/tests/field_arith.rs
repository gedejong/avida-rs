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

#[test]
fn width_limits_the_fields_to_the_low_bits() {
    // add, two-bit operands, three-bit fields, the low 24 bits only: eight
    // fields, and nothing above bit 23 is compared.
    let (lib, id, _) = load("fn=add:bits=2:mode=packed:width=24").unwrap();
    let (graded, gid, _) = load("fn=add:bits=2:mode=packed:width=24:thresh=0").unwrap();
    let mut rng = Rng(0x5EED);
    for _ in 0..200 {
        let (a, b) = (rng.next(), rng.next());
        let mut want = 0u32;
        for k in 0..8 {
            want |= (((a >> (3 * k)) & 3) + ((b >> (3 * k)) & 3)) << (3 * k);
        }
        let junk = rng.next() & 0xFF00_0000;
        assert_eq!(
            eval(&lib, id, &inputs(a, b), &outputs(1, &[want | junk])),
            1.0
        );
        // One wrong bit below bit 24 costs 2 of the 24 compared bits.
        let q = eval(&graded, gid, &inputs(a, b), &outputs(1, &[want ^ 1]));
        assert!((q - 22.0 / 24.0).abs() < 1e-12, "{q}");
        assert_eq!(eval(&lib, id, &inputs(a, b), &outputs(1, &[want ^ 1])), 0.0);
    }
    // With pre-masked operands the two-round carry expression is the sum.
    for _ in 0..200 {
        let (a, b) = (rng.next() & 0xFF6D_B6DB, rng.next() & 0xFF6D_B6DB);
        let (s1, c1) = (a ^ b, (a & b) << 1);
        let (s2, c2) = (s1 ^ c1, (s1 & c1) << 1);
        assert_eq!(eval(&lib, id, &inputs(a, b), &outputs(1, &[s2 | c2])), 1.0);
    }
    // The planes rung needs no mask at all.
    let (lib, id, _) = load("fn=add:bits=2:mode=planes:width=24").unwrap();
    for _ in 0..200 {
        let (a, b) = (rng.next(), rng.next());
        let (s1, c1) = (a ^ b, (a & b) << 1);
        let (s2, c2) = (s1 ^ c1, (s1 & c1) << 1);
        let outs = [s1, s2 >> 1, (c1 | c2) >> 2];
        assert_eq!(eval(&lib, id, &inputs(a, b), &outputs(3, &outs)), 1.0);
    }
    let errs = load("fn=add:bits=2:width=2").unwrap_err();
    assert!(errs.iter().any(|e| e.contains("width=2")), "{errs:?}");
}

#[test]
fn carry_and_partial_are_the_steps_of_the_two_bit_adder() {
    let (carry, cid, _) = load("fn=carry:bits=2:mode=packed:width=24").unwrap();
    let (partial, pid, _) = load("fn=partial:bits=2:mode=packed:width=24").unwrap();
    let (add, aid, _) = load("fn=add:bits=2:mode=packed:width=24").unwrap();
    let mut rng = Rng(0xC0FFEE);
    for _ in 0..300 {
        // Operands as INPUT_AND_MASK 0xFF6DB6DB hands them out.
        let (a, b) = (rng.next() & 0xFF6D_B6DB, rng.next() & 0xFF6D_B6DB);
        let (s1, c1) = (a ^ b, (a & b) << 1);
        let (s2, c2) = (s1 ^ c1, (s1 & c1) << 1);
        let i = inputs(a, b);
        assert_eq!(eval(&carry, cid, &i, &outputs(1, &[c1])), 1.0);
        assert_eq!(eval(&partial, pid, &i, &outputs(1, &[s2])), 1.0);
        assert_eq!(eval(&add, aid, &i, &outputs(1, &[s2 | c2])), 1.0);
        // Per field, against plain arithmetic.
        for k in 0..8 {
            let (x, y) = ((a >> (3 * k)) & 3, (b >> (3 * k)) & 3);
            assert_eq!((c1 >> (3 * k)) & 7, (x & y) << 1);
            assert_eq!(((s2 | c2) >> (3 * k)) & 7, x + y);
        }
    }
}

/// The logic steps on the two computed words, from which `partial` is
/// composed the way `xor` is composed from `nand`, `and` and `or`.
#[test]
fn the_xc_steps_compose_partial() {
    let task = |f: &str| load(&format!("fn={f}:bits=2:mode=packed:width=24")).unwrap();
    let (nand, nid, _) = task("nand_xc");
    let (and, aid, _) = task("and_xc");
    let (or, oid, _) = task("or_xc");
    let (andn_xc, xid, _) = task("andn_xc");
    let (andn_cx, cid, _) = task("andn_cx");
    let (partial, pid, _) = task("partial");
    let (add, sid, _) = task("add");
    let mut rng = Rng(0xBADC0DE);
    for _ in 0..300 {
        let (a, b) = (rng.next() & 0xFF6D_B6DB, rng.next() & 0xFF6D_B6DB);
        let (x, c) = (a ^ b, (a & b) << 1);
        let i = inputs(a, b);
        let one = |v: u32| outputs(1, &[v]);
        // The bits above `width` are not judged: a plain 32-bit nand passes.
        assert_eq!(eval(&nand, nid, &i, &one(!(x & c))), 1.0);
        assert_eq!(eval(&and, aid, &i, &one(x & c)), 1.0);
        assert_eq!(eval(&or, oid, &i, &one(x | c)), 1.0);
        assert_eq!(eval(&andn_xc, xid, &i, &one(x & !c)), 1.0);
        assert_eq!(eval(&andn_cx, cid, &i, &one(c & !x)), 1.0);
        // partial from the steps, three ways.
        for v in [(x | c) & !(x & c), (x & !c) | (c & !x), x ^ c] {
            assert_eq!(eval(&partial, pid, &i, &one(v)), 1.0);
        }
        // and_xc is the second round's carry before its shift.
        assert_eq!(eval(&add, sid, &i, &one((x ^ c) | ((x & c) << 1))), 1.0);
        // Neither computed word alone is one of the steps, unless they agree.
        if x & c & 0xFF_FFFF != 0 {
            assert!(eval(&or, oid, &i, &one(x ^ c)) < 1.0);
            assert!(eval(&andn_xc, xid, &i, &one(x)) < 1.0);
        }
    }
    let errs = load("fn=nor_xc:bits=2").unwrap_err();
    assert!(errs.iter().any(|e| e.contains("nand_xc")), "{errs:?}");
}

/// `expect=1` only marks the entry (the quality it computes is the same);
/// the library answers per entry.
#[test]
fn expect_marks_the_entry_and_nothing_else() {
    let (plain, id, _) = load("fn=xor:bits=2").unwrap();
    assert!(!plain.task_expected(id));
    assert!(!plain.has_expected());
    let (marked, id, _) = load("fn=xor:bits=2:expect=1").unwrap();
    assert!(marked.task_expected(id));
    assert!(marked.has_expected());
    assert!(!marked.task_expected(id + 1), "no such entry");
    let (off, id, _) = load("fn=xor:bits=2:expect=0").unwrap();
    assert!(!off.task_expected(id));
    let Err(errors) = load("fn=xor:bits=2:expect=2") else {
        panic!("expect=2 loaded");
    };
    assert!(errors.iter().any(|e| e.contains("expect=2")), "{errors:?}");
    let input = inputs(0x1234, 0x0f0f);
    let out = outputs(1, &[0x1234 ^ 0x0f0f]);
    assert_eq!(
        eval(&plain, 0, &input, &out),
        eval(&marked, 0, &input, &out)
    );
}

/// `add3`: the sum of the three newest inputs, `w + 2` result bits per field.
/// With `bits=3`, `stride=5`, `width=24` (inputs masked to `0xFF739CE7`) it is
/// what an evolved four-bit adder computes when applied a second time, to its
/// own sum and the next input.
#[test]
fn add3_sums_the_three_newest_inputs() {
    let three = |a: u32, b: u32, c: u32| {
        let mut i = IoBuffer::new(3);
        i.add(c as i32);
        i.add(a as i32);
        i.add(b as i32);
        i
    };
    let (add3, id3, er) = load("fn=add3:bits=3:stride=5:mode=packed:width=24").unwrap();
    let (add, id2, _) = load("fn=add:bits=3:stride=5:mode=packed:width=24").unwrap();
    assert_eq!(er.get_min_inputs(), 3, "add3 reads three inputs");
    let mut rng = Rng(0xADD3);
    let one = |v: u32| outputs(1, &[v]);
    for _ in 0..300 {
        let m = 0xFF73_9CE7;
        let (a, b, c) = (rng.next() & m, rng.next() & m, rng.next() & m);
        let field = |v: u32, k: u32| (v >> (5 * k)) & 7;
        // Plain arithmetic per field: the sum of the low 24 bits is the sum
        // of the fields, since no field sum (at most 21) leaves its field.
        let s = (a & 0xFF_FFFF) + (b & 0xFF_FFFF) + (c & 0xFF_FFFF);
        for k in 0..4 {
            assert_eq!((s >> (5 * k)) & 31, field(a, k) + field(b, k) + field(c, k));
        }
        let i = three(a, b, c);
        assert_eq!(eval(&add3, id3, &i, &one(s)), 1.0);
        // The adder applied twice: (a + b) + c, in any order of operands.
        let ab = ((a & 0xFF_FFFF) + (b & 0xFF_FFFF)) & 0xFF_FFFF;
        assert_eq!(eval(&add3, id3, &i, &one(ab + (c & 0xFF_FFFF))), 1.0);
        assert_eq!(eval(&add3, id3, &three(c, a, b), &one(s)), 1.0);
        // The two-operand sum is right on add and, unless c adds nothing, not on add3.
        assert_eq!(eval(&add, id2, &i, &one(ab)), 1.0);
        if c & 0x73_9CE7 != 0 {
            assert!(eval(&add3, id3, &i, &one(ab)) < 1.0);
        }
    }
    // Two stored inputs are not enough.
    let mut two = IoBuffer::new(3);
    two.add(1);
    two.add(2);
    assert_eq!(eval(&add3, id3, &two, &one(3)), 0.0);
    // The default stride is w + 2; a smaller stride is refused.
    let (_, _, _) = load("fn=add3:bits=2").unwrap();
    let errs = load("fn=add3:bits=3:stride=4").unwrap_err();
    assert!(errs.iter().any(|e| e.contains("stride=4")), "{errs:?}");
    let errs = load("fn=sub:bits=2").unwrap_err();
    assert!(errs.iter().any(|e| e.contains("add3")), "{errs:?}");
}

/// `add4`: the sum of the four newest inputs, `w + 2` result bits per field:
/// the adder applied three times, each time to its own sum and the next input.
#[test]
fn add4_sums_the_four_newest_inputs() {
    let four = |a: u32, b: u32, c: u32, d: u32| {
        let mut i = IoBuffer::new(4);
        i.add(d as i32);
        i.add(c as i32);
        i.add(a as i32);
        i.add(b as i32);
        i
    };
    let (add4, id4, er) = load("fn=add4:bits=3:stride=5:mode=packed:width=24").unwrap();
    let (add3, id3, _) = load("fn=add3:bits=3:stride=5:mode=packed:width=24").unwrap();
    assert_eq!(er.get_min_inputs(), 4, "add4 reads four inputs");
    let mut rng = Rng(0xADD4);
    let one = |v: u32| outputs(1, &[v]);
    let low = |v: u32| v & 0xFF_FFFF;
    for _ in 0..300 {
        let m = 0xFF73_9CE7;
        let (a, b, c, d) = (
            rng.next() & m,
            rng.next() & m,
            rng.next() & m,
            rng.next() & m,
        );
        let field = |v: u32, k: u32| (v >> (5 * k)) & 7;
        // No field sum (at most 28) leaves its five bits.
        let s = low(a) + low(b) + low(c) + low(d);
        for k in 0..4 {
            assert_eq!(
                (s >> (5 * k)) & 31,
                field(a, k) + field(b, k) + field(c, k) + field(d, k)
            );
        }
        let i = four(a, b, c, d);
        assert_eq!(eval(&add4, id4, &i, &one(s)), 1.0);
        // ((a + b) + c) + d, and the operands in another order.
        let abc = low(low(low(a) + low(b)) + low(c));
        assert_eq!(eval(&add4, id4, &i, &one(abc + low(d))), 1.0);
        assert_eq!(eval(&add4, id4, &four(d, c, b, a), &one(s)), 1.0);
        // The three-operand sum of the same inputs is right on add3 and, unless
        // d adds nothing, not on add4.
        assert_eq!(eval(&add3, id3, &i, &one(abc)), 1.0);
        if d & 0x73_9CE7 != 0 {
            assert!(eval(&add4, id4, &i, &one(abc)) < 1.0);
        }
    }
    // Three stored inputs are not enough.
    let mut three = IoBuffer::new(4);
    three.add(1);
    three.add(2);
    three.add(4);
    assert_eq!(eval(&add4, id4, &three, &one(7)), 0.0);
    let errs = load("fn=add4:bits=3:stride=4").unwrap_err();
    assert!(errs.iter().any(|e| e.contains("stride=4")), "{errs:?}");
    let errs = load("fn=sub:bits=2").unwrap_err();
    assert!(errs.iter().any(|e| e.contains("add3 to add8")), "{errs:?}");
}

/// `add5` to `add8`: the sum of the `n` newest inputs in `w + 3` bits. With
/// `bits=3`, `stride=6`, `width=24` (inputs masked to `0xFF1C71C7`, four
/// fields) a sum of up to eight operands (at most 56) stays in its field.
#[test]
fn add5_to_add8_sum_the_newest_inputs() {
    let newest = |v: &[u32]| {
        // v[0] is input[0], the newest; add them oldest first.
        let mut i = IoBuffer::new(v.len());
        for &x in v.iter().rev() {
            i.add(x as i32);
        }
        i
    };
    let one = |v: u32| outputs(1, &[v]);
    let low = |v: u32| v & 0xFF_FFFF;
    let m = 0xFF1C_71C7;
    let mut rng = Rng(0xADD8);
    for n in 5..=8usize {
        let (task, id, er) =
            load(&format!("fn=add{n}:bits=3:stride=6:mode=packed:width=24")).unwrap();
        let (below, idb, _) = load(&format!(
            "fn=add{}:bits=3:stride=6:mode=packed:width=24",
            n - 1
        ))
        .unwrap();
        assert_eq!(er.get_min_inputs(), n as i32, "add{n} reads {n} inputs");
        for _ in 0..200 {
            let v: Vec<u32> = (0..n).map(|_| rng.next() & m).collect();
            let s: u32 = v.iter().map(|&x| low(x)).sum();
            for k in 0..4 {
                let f: u32 = v.iter().map(|&x| (x >> (6 * k)) & 7).sum();
                assert_eq!((s >> (6 * k)) & 63, f, "no field sum leaves its field");
            }
            let i = newest(&v);
            assert_eq!(eval(&task, id, &i, &one(s)), 1.0);
            // The sum of the n - 1 newest is right on the rung below and, unless
            // the oldest input adds nothing, not on this one.
            let s1: u32 = v[..n - 1].iter().map(|&x| low(x)).sum();
            assert_eq!(eval(&below, idb, &i, &one(s1)), 1.0);
            if v[n - 1] & 0x1C_71C7 != 0 {
                assert!(eval(&task, id, &i, &one(s1)) < 1.0);
            }
        }
        // n - 1 stored inputs are not enough.
        let short = newest(&vec![1; n - 1]);
        assert_eq!(eval(&task, id, &short, &one(n as u32 - 1)), 0.0);
        // Five to eight 3-bit operands need six result bits: stride 5 is refused.
        let errs = load(&format!("fn=add{n}:bits=3:stride=5")).unwrap_err();
        assert!(errs.iter().any(|e| e.contains("stride=5")), "{errs:?}");
    }
    // add3 and add4 keep w + 2 result bits: stride 5 holds them.
    assert!(load("fn=add4:bits=3:stride=5").is_ok());
    let errs = load("fn=add9:bits=3:stride=7").unwrap_err();
    assert!(errs.iter().any(|e| e.contains("add9")), "{errs:?}");
}
