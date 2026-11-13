//! Organism messaging on the population side: the messaging methods of
//! `cPopulationInterface` (avida-core/source/main/cPopulationInterface.cc
//! at 47f13dadb: the three `SendMessage` overloads, `BroadcastMessage`,
//! `BcastAlarm`, `SendFlash`), `cPopulationCell::GetNeighboringCells`, the
//! delivery to the receiving organism (`cOrganism::ReceiveMessage`,
//! `ReceiveFlash`, `moveIPtoAlarmLabel` through
//! [`avida_hardware::Organism`]), the deme message counters and the
//! messaging stats (`cStats::LogMessage`, `LogRetMessage`, `SentFlash`,
//! `PrintMessageData`).
//!
//! The receiver is borrowed out of its cell for the delivery and runs
//! against its own interface. The running organism is not in its cell; a
//! delivery to it (a cell connected to itself, `ALARM_SELF`, its own input
//! avatar under neural networking) is queued in
//! [`MessagingState`] and its hardware applies it as soon as the
//! interface call returns ([`avida_hardware::org_interface::SelfDelivery`]).
//!
//! `cStats::SentMessage` hands each delivered message to the message
//! predicates of [`crate::actions3`].
//!
//! With `NEURAL_NETWORKING` and `USE_AVATARS 2` a message goes out through
//! each output avatar of the sender (until one send succeeds) and reaches
//! every organism with an input avatar in the target cell; the target cell
//! is recorded as the message's `m_transCellID`.

use std::collections::BTreeSet;

use avida_hardware::org_interface::{OrgMessage, SelfDelivery};
use avida_hardware::organism::OrgUid;

use crate::pop_interface::PopulationInterface;
use crate::world::World;

/// `Avida::INVALID_CONFIG`
const INVALID_CONFIG: i32 = 2;

/// World-side messaging state.
#[derive(Debug, Default)]
pub struct MessagingState {
    /// `static const double drop_prob` of `SendMessage(msg, rcell)`:
    /// QUIRK: `NET_DROP_PROB` as it was the first time any organism sent a
    /// message; later `SetConfig` changes are not seen.
    drop_prob: Option<f64>,
    /// Deliveries to the running organism, waiting for its hardware to
    /// take them (`take_self_deliveries`).
    self_deliveries: Vec<SelfDelivery>,
    /// The length of the running (sending) organism's received-message
    /// buffer while it sends, counting the messages already queued for it
    /// during this send (`set_sender_recv_len`).
    pub running_recv_len: Option<usize>,
}

impl MessagingState {
    /// The queued deliveries to the running organism, oldest first.
    pub fn take_self_deliveries(&mut self) -> Vec<SelfDelivery> {
        std::mem::take(&mut self.self_deliveries)
    }
}

impl World {
    /// Is `cell` the cell of the organism being run (and still its)?
    fn is_running_cell(&self, cell: i32) -> bool {
        self.pop
            .running
            .as_ref()
            .is_some_and(|r| r.cell_id == cell && !r.removed)
    }

    /// Hand `d` to the organism in `cell` (which is occupied). An `Err` is
    /// the C++ run ending.
    fn deliver(&mut self, cell: i32, d: SelfDelivery) -> Result<(), String> {
        if self.is_running_cell(cell) {
            self.messaging.self_deliveries.push(d);
            return Ok(());
        }
        let c = usize::try_from(cell).map_err(|_| "message to a negative cell id")?;
        let Some(mut org) = self.pop.orgs.get_mut(c).and_then(Option::take) else {
            return Err("message to an empty cell (C++: NULL dereference)".to_string());
        };
        let r = {
            let mut iface = PopulationInterface {
                w: self,
                cell_id: cell,
            };
            match d {
                SelfDelivery::Message(msg) => org.receive_message(&mut iface, msg),
                SelfDelivery::Flash => org.receive_flash(),
                SelfDelivery::Alarm(jump_label) => {
                    org.move_ip_to_alarm_label(jump_label).map(|_| ())
                }
            }
        };
        self.pop.orgs[c] = Some(org);
        r
    }

    /// [`World::deliver`], ending the run on an error: the invalid
    /// `MESSAGE_RECV_BUFFER_BEHAVIOR` of `ReceiveMessage` is
    /// `Feedback().Error` and `Abort(INVALID_CONFIG)`; anything else is the
    /// C++ exiting where the port reports a fatal error.
    fn deliver_or_abort(&mut self, cell: i32, d: SelfDelivery) {
        if let Err(e) = self.deliver(cell, d) {
            if matches!(d, SelfDelivery::Message(_))
                && e.starts_with("MESSAGE_RECV_BUFFER_BEHAVIOR")
            {
                self.abort_run(&format!("error: {e}\n"), INVALID_CONFIG, &e);
            } else {
                self.set_fatal(e);
            }
        }
    }

    /// Would `cOrganism::ReceiveMessage` on the occupant of `cell` drop the
    /// message before `msg.SetReceiver(this)` (a full buffer with
    /// `MESSAGE_RECV_BUFFER_BEHAVIOR 1`)? The running organism's buffer is
    /// out of reach: its length was told at the start of the send
    /// ([`MessagingState::running_recv_len`]) and counts the messages this
    /// send has already queued for it. `None` when the sender's hardware did
    /// not tell (the delivery to it is assumed to be kept).
    fn receiver_drops_new(&mut self, cell: i32) -> bool {
        let bsize = self.cfg.message_recv_buffer_size;
        let behavior = self.cfg.message_recv_buffer_behavior;
        let full_and_drops = |len: usize| bsize != -1 && bsize <= len as i32 && behavior == 1;
        if self.is_running_cell(cell) {
            let Some(len) = self.messaging.running_recv_len else {
                return false;
            };
            if full_and_drops(len) {
                return true;
            }
            // Kept: it grows the buffer, unless a full buffer drops its
            // oldest message to make room (`BEHAVIOR 0`).
            let full = bsize != -1 && bsize <= len as i32;
            if !full {
                self.messaging.running_recv_len = Some(len + 1);
            }
            return false;
        }
        let Some(org) = usize::try_from(cell)
            .ok()
            .and_then(|c| self.pop.orgs.get(c))
            .and_then(Option::as_ref)
        else {
            return false;
        };
        full_and_drops(org.state.msg_received.len())
    }

    /// Messages travel between neural networking avatars
    /// (`NEURAL_NETWORKING` with `USE_AVATARS 2`).
    fn neural_avatars(&self) -> bool {
        self.cfg.neural_networking && self.cfg.use_avatars == 2
    }

    /// `GetOrganism()` of the interface of `sender_cell`: the organism being
    /// run (killed this step or not), else the cell's occupant.
    fn sender_uid(&self, sender_cell: i32) -> Option<OrgUid> {
        if let Some(r) = self
            .pop
            .running
            .as_ref()
            .filter(|r| r.cell_id == sender_cell)
        {
            return Some(r.uid);
        }
        let c = usize::try_from(sender_cell).ok()?;
        self.pop.orgs.get(c)?.as_ref().map(|o| o.state.uid)
    }

    /// `cPopulationInterface::SendMessage(msg, rcell)` of the organism in
    /// `sender_cell`.
    pub(crate) fn send_message_to_cell(
        &mut self,
        sender_cell: i32,
        msg: &mut OrgMessage,
        rcell: i32,
    ) -> bool {
        let drop_prob = *self
            .messaging
            .drop_prob
            .get_or_insert(self.cfg.net_drop_prob);
        let neural = self.neural_avatars();
        let self_comm = self.cfg.self_communication;
        let sender = self.sender_uid(sender_cell);
        let deme_id = self.deme_of(sender_cell);
        let mut dropped = false;
        if drop_prob > 0.0 && self.rng.p(drop_prob) {
            // message dropped
            let deme = &mut self.pop.demes[deme_id];
            deme.msg_dropped = deme.msg_dropped.wrapping_add(1);
            deme.msg_send_failed = deme.msg_send_failed.wrapping_add(1);
            dropped = true;
        }

        // Without neural networking avatars, fail if the cell we're facing
        // is not occupied. With them, the cell needs an input avatar, of
        // another organism unless SELF_COMMUNICATION is on.
        let lost = if neural {
            let inputs = self.avatars.cell(rcell).map_or(&[][..], |c| &c.pred[..]);
            inputs.is_empty() || (!self_comm && !inputs.iter().any(|&u| Some(u) != sender))
        } else {
            !self.is_occupied(rcell)
        };
        if lost {
            let deme = &mut self.pop.demes[deme_id];
            deme.msg_send_failed = deme.msg_send_failed.wrapping_add(1);
        }

        // record this message, regardless of whether it's actually received.
        if self.cfg.net_log_messages != 0 {
            self.stats.log_message(
                deme_id as i32,
                msg.sender_cell,
                msg.receiver_cell,
                msg.trans_cell,
                msg.data,
                msg.label,
                dropped,
                lost,
            );
        }

        if dropped || lost {
            return false;
        }

        if !neural {
            self.receive_counted(sender_cell, deme_id, msg, rcell);
            return true;
        }
        // If using neural networking avatars, message must be sent to all
        // orgs with input avatars in the cell. The C++ re-reads the cell's
        // list for each receiver.
        let mut i = 0;
        while let Some(recvr) = self
            .avatars
            .cell(rcell)
            .and_then(|c| c.pred.get(i).copied())
        {
            i += 1;
            if Some(recvr) == sender && !self_comm {
                continue;
            }
            let Some(recvr_cell) = self.cell_of_org(recvr) else {
                self.set_fatal(
                    "an input avatar of an organism that is no longer alive receives a message \
                     (C++: dangling cOrganism* in cPopulationCell::m_av_pred)",
                );
                return true;
            };
            // A running organism whose hardware did not tell its buffer
            // length cannot say whether a full buffer drops the message
            // before `SetReceiver` decides which receiver the predicates and
            // the log see.
            if self.is_running_cell(recvr_cell)
                && self.messaging.running_recv_len.is_none()
                && self.cfg.message_recv_buffer_size != -1
                && self.cfg.message_recv_buffer_behavior == 1
                && (!self.actions2.a3.msg_predicates.is_empty() || self.cfg.net_log_messages != 0)
            {
                self.set_fatal(
                    "unsupported: SELF_COMMUNICATION into a bounded MESSAGE_RECV_BUFFER_SIZE with \
                     MESSAGE_RECV_BUFFER_BEHAVIOR 1 from hardware that does not report its \
                     received-message buffer while message predicates or NET_LOG_MESSAGES read \
                     the receiver",
                );
                return true;
            }
            self.receive_counted(sender_cell, deme_id, msg, recvr_cell);
        }
        true
    }

    /// The delivery of `SendMessage(msg, rcell)` to the organism living in
    /// `recvr_cell`: `recvr->ReceiveMessage(msg)` stores a copy and points
    /// `msg` at its receiver (`SetReceiver`) unless the receiver drops it,
    /// then `cStats::SentMessage(msg)` evaluates the message predicates and
    /// the sender's deme counts the success.
    fn receive_counted(
        &mut self,
        sender_cell: i32,
        deme_id: usize,
        msg: &mut OrgMessage,
        recvr_cell: i32,
    ) {
        let delivery = SelfDelivery::Message(*msg);
        if !self.receiver_drops_new(recvr_cell) {
            msg.receiver_cell = recvr_cell;
        }
        self.deliver_or_abort(recvr_cell, delivery);
        self.a3_sent_message(msg, sender_cell);
        let deme = &mut self.pop.demes[deme_id];
        deme.msg_successfully_sent = deme.msg_successfully_sent.wrapping_add(1);
    }

    /// `cPopulationInterface::SendMessage(msg, cellid)`: with neural
    /// networking avatars the message records the cell it goes to.
    fn send_message_to_cell_id(
        &mut self,
        sender_cell: i32,
        msg: &mut OrgMessage,
        cellid: i32,
    ) -> bool {
        if cellid < 0 || cellid as usize >= self.pop.cells.len() {
            self.set_fatal(format!(
                "SendMessage to cell {cellid} outside the world (C++: out-of-bounds cell)"
            ));
            return false;
        }
        if self.neural_avatars() {
            msg.trans_cell = cellid;
        }
        self.send_message_to_cell(sender_cell, msg, cellid)
    }

    /// `cPopulationInterface::SendMessage(msg)`: to the faced cell, or
    /// with neural networking avatars through the sender's output avatars.
    /// QUIRK: `message_sent = (message_sent || SendMessage(...))` stops
    /// sending after the first avatar cell that takes the message.
    pub(crate) fn send_message(&mut self, sender_cell: i32, msg: &mut OrgMessage) -> bool {
        if self.neural_avatars() {
            let Some(uid) = self.sender_uid(sender_cell) else {
                return false;
            };
            let mut message_sent = false;
            let mut i = 0;
            while i < self.avatars.num_av(uid) {
                if let Some(av) = self.avatars.get(uid, i).copied() {
                    if av.output && !message_sent {
                        message_sent = self.send_message_to_cell_id(sender_cell, msg, av.cell_id);
                    }
                }
                i += 1;
            }
            return message_sent;
        }
        let Some(rcell) = self.pop.cells[sender_cell as usize].connections.first() else {
            self.set_fatal("SendMessage from a cell without neighbours (C++: NULL dereference)");
            return false;
        };
        self.send_message_to_cell(sender_cell, msg, rcell)
    }

    /// `cPopulationCell::GetNeighboringCells(cell_set, depth)`: the cells
    /// reachable in at most `depth` steps, found depth-first in connection
    /// list order. QUIRK: a cell is expanded only when first inserted, so
    /// one first reached by a long path is not expanded again when a
    /// shorter path reaches it, and the set can miss cells `depth` steps
    /// away. The cell itself is included when a path leads back to it.
    pub(crate) fn neighboring_cells(&self, cell: i32, depth: i32, set: &mut BTreeSet<i32>) {
        for n in self.pop.cells[cell as usize].connections.iter() {
            if set.insert(n) && depth > 1 {
                self.neighboring_cells(n, depth - 1, set);
            }
        }
    }

    /// `cPopulationInterface::BroadcastMessage(msg, depth)`: send towards
    /// every other cell within `depth`. The C++ set holds cell pointers
    /// into the cell array, so it is visited in cell id order.
    pub(crate) fn broadcast_message(
        &mut self,
        sender_cell: i32,
        msg: &mut OrgMessage,
        depth: i32,
    ) -> bool {
        let mut cell_set = BTreeSet::new();
        self.neighboring_cells(sender_cell, depth, &mut cell_set);
        cell_set.remove(&sender_cell);
        for rcell in cell_set {
            self.send_message_to_cell(sender_cell, msg, rcell);
        }
        true
    }

    /// `cPopulationInterface::BcastAlarm(jump_label, bcast_range)`
    pub(crate) fn bcast_alarm(
        &mut self,
        sender_cell: i32,
        jump_label: i32,
        bcast_range: i32,
    ) -> bool {
        let mut successfully_sent = false;
        // does an alarm affect the sender; 0=no  non-0=yes
        let alarm_self = self.cfg.alarm_self;

        if bcast_range > 1 {
            // multi-hop messaging
            let d = self.deme_of(sender_cell);
            for i in 0..self.pop.demes[d].size() {
                let possible_receiver_id = self.pop.demes[d].cell_id(i);
                if self.is_occupied(possible_receiver_id) && possible_receiver_id != sender_cell {
                    //check distance
                    let deme = &self.pop.demes[d];
                    let sender_pos = deme.cell_position(sender_cell);
                    let receiver_pos = deme.cell_position(possible_receiver_id);
                    let hop_distance = (sender_pos.0 - receiver_pos.0)
                        .abs()
                        .max((sender_pos.1 - receiver_pos.1).abs());
                    if hop_distance <= bcast_range {
                        // send alarm to organisms
                        self.deliver_or_abort(
                            possible_receiver_id,
                            SelfDelivery::Alarm(jump_label),
                        );
                        successfully_sent = true;
                    }
                }
            }
        } else {
            // single hop messaging
            let neighbors: Vec<i32> = self.pop.cells[sender_cell as usize]
                .connections
                .iter()
                .collect();
            for rcell in neighbors {
                // Fail if the cell we're facing is not occupied.
                if !self.is_occupied(rcell) {
                    continue;
                }
                self.deliver_or_abort(rcell, SelfDelivery::Alarm(jump_label));
                successfully_sent = true;
            }
        }

        if alarm_self {
            self.deliver_or_abort(sender_cell, SelfDelivery::Alarm(jump_label));
        }
        successfully_sent
    }

    /// `cPopulationInterface::SendFlash()`: a flash to every occupied
    /// neighbour. The C++ walks the list with `GetFirst` / `CircNext`, a
    /// full turn that leaves it as it was.
    pub(crate) fn send_flash(&mut self, sender_cell: i32) {
        let neighbors: Vec<i32> = self.pop.cells[sender_cell as usize]
            .connections
            .iter()
            .collect();
        for rcell in neighbors {
            if self.is_occupied(rcell) {
                self.deliver_or_abort(rcell, SelfDelivery::Flash);
            }
        }
    }

    /// `cStats::LogRetMessage(msg)`
    pub(crate) fn stats_log_ret_message(&mut self, msg: &OrgMessage) {
        self.stats.log_ret_message(
            msg.sender_cell,
            msg.receiver_cell,
            msg.trans_cell,
            msg.data,
            msg.label,
        );
    }

    /// `cStats::SentFlash(organism)` for the organism in `cell`: its deme
    /// and deme-relative cell.
    pub(crate) fn stats_sent_flash(&mut self, cell: i32) {
        let d = self.deme_of(cell);
        let rel = self.pop.demes[d].relative_cell_id(cell);
        self.stats.sent_flash(Some((d as i32, rel)));
    }

    /// `cStats::PrintMessageData(filename)`: the deme message counters.
    pub(crate) fn print_message_data(&mut self, filename: &str) -> Result<(), String> {
        let counts: Vec<(u32, u32, u32)> = self
            .pop
            .demes
            .iter()
            .map(|d| (d.msg_successfully_sent, d.msg_dropped, d.msg_send_failed))
            .collect();
        self.stats
            .print_message_data(&mut self.out, filename, &counts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_deliveries_are_taken_once_in_order() {
        let mut s = MessagingState::default();
        s.self_deliveries.push(SelfDelivery::Flash);
        s.self_deliveries.push(SelfDelivery::Alarm(1));
        assert_eq!(
            s.take_self_deliveries(),
            vec![SelfDelivery::Flash, SelfDelivery::Alarm(1)]
        );
        assert!(s.take_self_deliveries().is_empty());
    }
}
