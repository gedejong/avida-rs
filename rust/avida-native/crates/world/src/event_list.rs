//! Port of `cEventList` (avida-core/source/main/cEventList.{h,cc}; the line
//! parser follows the pre-FFI body at commit ef5c6d948).
//!
//! The list is generic over the action payload `A`. The C++ list holds a
//! `cWorld*` to read trigger values and to run actions; here the caller passes
//! both as closures over its own context `W`, so the world can
//! `std::mem::take` the list, process it against itself and put it back.

use avida_config::string_util as su;

/// C++: `cEventList::TRIGGER_BEGIN` (`DBL_MIN`).
pub const TRIGGER_BEGIN: f64 = f64::MIN_POSITIVE;
/// C++: `cEventList::TRIGGER_END` (`DBL_MAX`).
pub const TRIGGER_END: f64 = f64::MAX;
/// C++: `cEventList::TRIGGER_ALL`.
pub const TRIGGER_ALL: f64 = 0.0;
/// C++: `cEventList::TRIGGER_ONCE` (`DBL_MAX`).
pub const TRIGGER_ONCE: f64 = f64::MAX;

/// C++: `cEventList::eTriggerType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerType {
    Update,
    Generation,
    Immediate,
    Births,
    Undefined,
    BirthsInterrupt,
}

/// One parsed line of an events file, before the action is instantiated.
#[derive(Debug, Clone, PartialEq)]
pub struct EventSpec {
    pub trigger: TriggerType,
    pub start: f64,
    pub interval: f64,
    pub stop: f64,
    pub name: String,
    pub args: String,
}

/// C++: `cEventList::AddEventFileFormat`, up to the `AddEvent` call. The
/// error string is what C++ writes to `cerr` (without the trailing newline).
pub fn parse_event_line(in_line: &str) -> Result<EventSpec, String> {
    let mut cur_line = in_line.to_string();
    let mut start = TRIGGER_BEGIN;
    let interval;
    let stop;

    let mut cur_word = su::pop_word(&mut cur_line);
    let trigger = match cur_word.as_str() {
        "i" | "immediate" => {
            let name = su::pop_word(&mut cur_line);
            return Ok(EventSpec {
                trigger: TriggerType::Immediate,
                start: TRIGGER_BEGIN,
                interval: TRIGGER_ONCE,
                stop: TRIGGER_END,
                name,
                args: cur_line,
            });
        }
        "u" | "update" => TriggerType::Update,
        "g" | "generation" => TriggerType::Generation,
        "b" | "births" => TriggerType::Births,
        "o" | "org_id" => TriggerType::BirthsInterrupt,
        _ => return Err(format!("error: unrecognized event trigger '{cur_word}'")),
    };
    cur_word = su::pop_word(&mut cur_line);

    let mut timing_str = cur_word;
    let mut tmp = su::pop(&mut timing_str, ':');
    if su::is_number(&tmp) || tmp == "begin" {
        if tmp != "begin" {
            start = su::as_double(&tmp);
        }
        if timing_str.is_empty() {
            interval = TRIGGER_ONCE;
            stop = TRIGGER_END;
        } else {
            tmp = su::pop(&mut timing_str, ':');
            interval = match tmp.as_str() {
                "all" => TRIGGER_ALL,
                "once" => TRIGGER_ONCE,
                _ => su::as_double(&tmp),
            };
            stop = if timing_str.is_empty() || timing_str == "end" {
                TRIGGER_END
            } else {
                su::as_double(&timing_str)
            };
        }
        cur_word = su::pop_word(&mut cur_line);
    } else {
        return Err(format!("error: invalid event timing '{tmp}'"));
    }

    Ok(EventSpec {
        trigger,
        start,
        interval,
        stop,
        name: cur_word,
        args: cur_line,
    })
}

/// C++: `cEventList::cEventListEntry`.
#[derive(Debug, Clone)]
pub struct EventEntry<A> {
    pub action: A,
    pub name: String,
    pub args: String,
    pub trigger: TriggerType,
    pub start: f64,
    pub interval: f64,
    pub stop: f64,
    original_start: f64,
}

impl<A> EventEntry<A> {
    fn next_interval(&mut self) {
        self.start += self.interval;
    }

    fn reset(&mut self) {
        self.start = self.original_start;
    }

    /// The "ran past its stop" test shared by `Process` and
    /// `ProcessInterrupt`.
    fn is_exhausted(&self) -> bool {
        self.stop != TRIGGER_END
            && ((self.start > self.stop && self.interval > 0.0)
                || (self.start < self.stop && self.interval < 0.0))
    }
}

/// C++: `cEventList`.
#[derive(Debug, Clone)]
pub struct EventList<A> {
    entries: Vec<EventEntry<A>>,
    num_events: i32,
    birth_interrupt_queue: Vec<f64>,
}

impl<A> Default for EventList<A> {
    fn default() -> Self {
        EventList {
            entries: Vec::new(),
            num_events: 0,
            birth_interrupt_queue: Vec::new(),
        }
    }
}

impl<A> EventList<A> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of events ever added (C++ `m_num_events`; never decremented).
    pub fn num_events(&self) -> i32 {
        self.num_events
    }

    pub fn entries(&self) -> &[EventEntry<A>] {
        &self.entries
    }

    /// C++: `cEventList::AddEvent`, after the action has been created.
    /// `trigger_value` is `GetTriggerValue` for the current world state
    /// (see [`trigger_value`]).
    pub fn add_event(
        &mut self,
        spec: EventSpec,
        action: A,
        trigger_value: impl Fn(TriggerType) -> f64,
    ) {
        let trigger = spec.trigger;
        let start = spec.start;
        self.entries.push(EventEntry {
            action,
            name: spec.name,
            args: spec.args,
            trigger,
            start,
            interval: spec.interval,
            stop: spec.stop,
            original_start: start,
        });
        let idx = self.entries.len() - 1;
        self.sync_event(idx, trigger_value(trigger));
        if trigger == TriggerType::BirthsInterrupt {
            self.queue_birth_interrupt_event(start);
        }
        self.num_events += 1;
    }

    /// C++: `cEventList::Process`. `trigger_value(w, t)` must return what
    /// `GetTriggerValue(t)` would (use [`trigger_value`]); it is re-read for
    /// every entry because actions change the world.
    pub fn process<W>(
        &mut self,
        w: &mut W,
        trigger_value: impl Fn(&W, TriggerType) -> f64,
        mut run: impl FnMut(&mut W, &mut A),
    ) {
        let mut i = 0;
        while i < self.entries.len() {
            let trigger = self.entries[i].trigger;
            if trigger == TriggerType::Immediate {
                run(w, &mut self.entries[i].action);
                self.entries.remove(i);
                continue;
            }
            if trigger != TriggerType::BirthsInterrupt {
                let t_val = trigger_value(w, trigger);
                let (start, stop) = (self.entries[i].start, self.entries[i].stop);
                if t_val != f64::MAX
                    && (t_val >= start || start == TRIGGER_BEGIN)
                    && (t_val <= stop || stop == TRIGGER_END)
                {
                    run(w, &mut self.entries[i].action);
                    let interval = self.entries[i].interval;
                    if interval == TRIGGER_ALL {
                        // Keep it as it is.
                    } else if interval == TRIGGER_ONCE {
                        self.entries.remove(i);
                        continue;
                    } else {
                        self.entries[i].next_interval();
                    }
                    if self.entries[i].is_exhausted() {
                        self.entries.remove(i);
                        continue;
                    }
                }
            }
            i += 1;
        }
    }

    /// C++: `cEventList::ProcessInterrupt`.
    pub fn process_interrupt<W>(
        &mut self,
        w: &mut W,
        trigger_value: impl Fn(&W, TriggerType) -> f64,
        mut run: impl FnMut(&mut W, &mut A),
    ) {
        let mut i = 0;
        while i < self.entries.len() {
            if self.entries[i].trigger == TriggerType::BirthsInterrupt {
                let t_val = trigger_value(w, TriggerType::BirthsInterrupt);
                if t_val == self.entries[i].start {
                    run(w, &mut self.entries[i].action);
                    let interval = self.entries[i].interval;
                    if interval == TRIGGER_ALL {
                        // Keep it as it is.
                    } else if interval == TRIGGER_ONCE {
                        // C++ deletes the entry and then, with `entry == NULL`,
                        // falls into the `else` branch below and dereferences
                        // it (undefined behaviour). The only sane reading is
                        // "deleted, nothing queued".
                        self.entries.remove(i);
                        continue;
                    } else {
                        self.entries[i].next_interval();
                    }
                    if self.entries[i].is_exhausted() {
                        self.entries.remove(i);
                        continue;
                    }
                    let start = self.entries[i].start;
                    self.queue_birth_interrupt_event(start);
                }
            }
            i += 1;
        }
    }

    /// C++: `cEventList::Sync`.
    pub fn sync(&mut self, trigger_value: impl Fn(TriggerType) -> f64) {
        let mut i = 0;
        while i < self.entries.len() {
            let t_val = trigger_value(self.entries[i].trigger);
            if !self.sync_event(i, t_val) {
                i += 1;
            }
        }
    }

    /// C++: `cEventList::SyncEvent`. Returns true when the entry was deleted.
    fn sync_event(&mut self, idx: usize, t_val: f64) -> bool {
        let entry = &mut self.entries[idx];
        if entry.trigger == TriggerType::Immediate {
            return false;
        }
        if t_val > entry.stop {
            self.entries.remove(idx);
            return true;
        }
        if t_val > entry.start && entry.interval == TRIGGER_ONCE {
            self.entries.remove(idx);
            return true;
        }
        if t_val + entry.interval <= entry.start {
            entry.reset();
        }
        if entry.interval == TRIGGER_ALL {
            return false;
        }
        while t_val > entry.start {
            entry.next_interval();
        }
        false
    }

    /// C++: `cEventList::PrintEventList`.
    pub fn print_event_list(&self) -> String {
        let mut out = String::new();
        for entry in &self.entries {
            out.push_str(match entry.trigger {
                TriggerType::Update => "update ",
                TriggerType::Generation => "generation ",
                TriggerType::Immediate => "immediate ",
                TriggerType::Births => "births ",
                TriggerType::BirthsInterrupt => "birth_interrupt ",
                TriggerType::Undefined => "undefined ",
            });
            if entry.trigger != TriggerType::Immediate {
                if entry.start == TRIGGER_BEGIN {
                    out.push_str("begin");
                } else {
                    out.push_str(&su::format_ostream_double(entry.start));
                }
                out.push(':');
                if entry.interval == TRIGGER_ONCE {
                    out.push_str("once");
                } else if entry.interval == TRIGGER_ALL {
                    out.push_str("all");
                } else {
                    out.push_str(&su::format_ostream_double(entry.interval));
                }
                out.push(':');
                if entry.stop == TRIGGER_END {
                    out.push_str("end");
                } else {
                    out.push_str(&su::format_ostream_double(entry.stop));
                }
                out.push(' ');
            }
            out.push_str(&entry.name);
            out.push(' ');
            out.push_str(&entry.args);
            out.push('\n');
        }
        out
    }

    /// C++: `cEventList::QueueBirthInterruptEvent`
    fn queue_birth_interrupt_event(&mut self, t_val: f64) {
        if !self.birth_interrupt_queue.contains(&t_val) {
            self.birth_interrupt_queue.push(t_val);
        }
    }

    /// C++: `cEventList::CheckBirthInterruptQueue` (always false in C++).
    pub fn check_birth_interrupt_queue(&self, _t_val: f64) -> bool {
        false
    }

    /// C++: `cEventList::IsEventUpcoming`
    pub fn is_event_upcoming(&self, event_name: &str) -> bool {
        self.entries.iter().any(|e| e.name == event_name)
    }
}

/// C++: `cEventList::GetTriggerValue`, with the three stats it reads passed
/// in: current update, average generation, total creatures.
pub fn trigger_value(
    trigger: TriggerType,
    update: i32,
    ave_generation: f64,
    tot_creatures: i32,
) -> f64 {
    match trigger {
        TriggerType::Immediate => TRIGGER_BEGIN,
        TriggerType::Update => f64::from(update),
        TriggerType::Generation => ave_generation,
        TriggerType::BirthsInterrupt | TriggerType::Births => f64::from(tot_creatures),
        TriggerType::Undefined => TRIGGER_END,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(lines: &[&str], update: i32) -> EventList<String> {
        let mut l = EventList::new();
        for line in lines {
            let spec = parse_event_line(line).unwrap();
            let name = spec.name.clone();
            l.add_event(spec, name, |t| trigger_value(t, update, 0.0, 0));
        }
        l
    }

    fn run_update(l: &mut EventList<String>, update: i32) -> Vec<String> {
        let mut ran = Vec::new();
        l.process(
            &mut ran,
            |_, t| trigger_value(t, update, 0.0, 0),
            |ran, a| ran.push(a.clone()),
        );
        ran
    }

    #[test]
    fn parses_the_default_events_file_forms() {
        let s = parse_event_line("u begin Inject default-heads.org").unwrap();
        assert_eq!(s.trigger, TriggerType::Update);
        assert_eq!(s.start, TRIGGER_BEGIN);
        assert_eq!(s.interval, TRIGGER_ONCE);
        assert_eq!(s.stop, TRIGGER_END);
        assert_eq!(s.name, "Inject");
        assert_eq!(s.args, "default-heads.org");

        let s = parse_event_line("u 0:100:end PrintAverageData").unwrap();
        assert_eq!((s.start, s.interval, s.stop), (0.0, 100.0, TRIGGER_END));
        assert_eq!(s.args, "");

        let s = parse_event_line("g 5:all:20 Foo a b").unwrap();
        assert_eq!(s.trigger, TriggerType::Generation);
        assert_eq!((s.start, s.interval, s.stop), (5.0, TRIGGER_ALL, 20.0));
        assert_eq!(s.args, "a b");

        let s = parse_event_line("i Exit").unwrap();
        assert_eq!(s.trigger, TriggerType::Immediate);
        assert_eq!(s.name, "Exit");
    }

    #[test]
    fn rejects_bad_trigger_and_timing() {
        assert_eq!(
            parse_event_line("x 1 Foo").unwrap_err(),
            "error: unrecognized event trigger 'x'"
        );
        assert_eq!(
            parse_event_line("u soon Foo").unwrap_err(),
            "error: invalid event timing 'soon'"
        );
    }

    #[test]
    fn processes_once_repeating_and_bounded_events() {
        let mut l = list(
            &[
                "u begin Inject",
                "u 0:2:end Print",
                "u 1:1:2 Bounded",
                "u 3 Exit",
            ],
            -1,
        );
        assert_eq!(run_update(&mut l, 0), ["Inject", "Print"]);
        assert_eq!(run_update(&mut l, 1), ["Bounded"]);
        assert_eq!(run_update(&mut l, 2), ["Print", "Bounded"]);
        assert_eq!(run_update(&mut l, 3), ["Exit"]);
        assert_eq!(run_update(&mut l, 4), ["Print"]);
        assert_eq!(l.entries().len(), 1);
        assert_eq!(l.num_events(), 4);
    }

    #[test]
    fn prints_like_cpp() {
        let l = list(&["u begin Inject x.org", "u 0:100:end Print", "i Exit"], -1);
        assert_eq!(
            l.print_event_list(),
            "update begin:once:end Inject x.org\nupdate 0:100:end Print \nimmediate Exit \n"
        );
    }
}
