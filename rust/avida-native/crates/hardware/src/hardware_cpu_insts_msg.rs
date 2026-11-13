//! Messaging, alarm, flash and neighbourhood handlers of `cHardwareCPU`
//! (avida-core/source/cpu/cHardwareCPU.cc at 47f13dadb: `Inst_SendMessage*`
//! through `Inst_Alarm_Label`, `Inst_Flash` through `Inst_ResetFlashInfo`,
//! `Inst_GetNeighborhood`, `Inst_IfNeighborhoodChanged` and
//! `Inst_Broadcast*`), with the `cOrganism` messaging support they use
//! (avida-core/source/main/cOrganism.cc: `SendMessage`, `BroadcastMessage`,
//! `MessageSent`, `ReceiveMessage`, `RetrieveMessage`, `BcastAlarmMSG`,
//! `SendFlash`, `GetNeighborhood`, `LoadNeighborhood`,
//! `HasNeighborhoodChanged`).
//!
//! The receiving side (`cOrganism::ReceiveMessage`, `ReceiveFlash`,
//! `moveIPtoAlarmLabel`) is called by the population on another organism
//! and is public on [`Organism`]; what the population delivers to the
//! sender itself comes back through `take_self_deliveries` and is applied
//! by [`deliver_to_self`].

use crate::hardware::Hardware;
use crate::hardware_cpu::{HardwareCpu, REG_BX};
use crate::hardware_cpu_insts::{bit_count, InstFn, CONSENSUS24, MASK24, R};
use crate::nhardware::{FAULT_LOC_THREAD_KILL, FAULT_TYPE_WARNING, HEAD_IP, MSG_INTERRUPT};
use crate::org_interface::{Faced, OrgInterface, OrgMessage, SelfDelivery};
use crate::organism::{OrgState, Organism};
use std::collections::BTreeSet;

/// The handlers of this module, by `cHardwareCPU` method name.
pub(crate) fn handler(method: &str) -> Option<InstFn> {
    let f: InstFn = match method {
        "Inst_SendMessage" => |h, o, w| send_message(h, o, w, 0),
        "Inst_SendMessageInterruptType0" => |h, o, w| send_message(h, o, w, 0),
        "Inst_SendMessageInterruptType1" => |h, o, w| send_message(h, o, w, 1),
        "Inst_SendMessageInterruptType2" => |h, o, w| send_message(h, o, w, 2),
        "Inst_SendMessageInterruptType3" => |h, o, w| send_message(h, o, w, 3),
        "Inst_SendMessageInterruptType4" => |h, o, w| send_message(h, o, w, 4),
        "Inst_SendMessageInterruptType5" => |h, o, w| send_message(h, o, w, 5),
        "Inst_START_Handler" => inst_start_handler,
        "Inst_End_Handler" => inst_end_handler,
        "Inst_RetrieveMessage" => inst_retrieve_message,
        "Inst_Alarm_MSG_multihop" => |h, o, w| {
            let reg_used = h.find_modified_register(REG_BX);
            let label = h.reg(reg_used).wrapping_rem(2).wrapping_abs();
            let hops = w.config().bcast_hops;
            bcast_alarm_msg(h, o, w, label, hops)
        },
        "Inst_Alarm_MSG_Bit_Cons24_multihop" => |h, o, w| {
            let reg_used = h.find_modified_register(REG_BX);
            let label = cons24(h.reg(reg_used));
            let hops = w.config().bcast_hops;
            bcast_alarm_msg(h, o, w, label, hops)
        },
        "Inst_Alarm_MSG_local" => |h, o, w| {
            let reg_used = h.find_modified_register(REG_BX);
            let label = h.reg(reg_used).wrapping_rem(2).wrapping_abs();
            bcast_alarm_msg(h, o, w, label, 1)
        },
        "Inst_Alarm_MSG_Bit_Cons24_local" => |h, o, w| {
            let reg_used = h.find_modified_register(REG_BX);
            let label = cons24(h.reg(reg_used));
            bcast_alarm_msg(h, o, w, label, 1)
        },
        "Inst_Alarm_Label" => |_, _, _| Ok(true),
        "Inst_Flash" => inst_flash,
        "Inst_IfRecvdFlash" => |h, _, _| {
            if h.flash_info.0 == 0 {
                h.ip_advance();
            }
            Ok(true)
        },
        "Inst_FlashInfo" => |h, _, _| {
            let bx = h.find_modified_register(REG_BX);
            let cx = HardwareCpu::find_next_register(bx);
            if h.flash_info.0 > 0 {
                h.set_reg(bx, h.flash_info.0 as i32);
                h.set_reg(cx, h.cycle_counter.wrapping_sub(h.flash_info.1) as i32);
            } else {
                h.set_reg(bx, 0);
                h.set_reg(cx, 0);
            }
            Ok(true)
        },
        "Inst_FlashInfoB" => |h, _, _| {
            let bx = h.find_modified_register(REG_BX);
            let v = if h.flash_info.0 > 0 {
                h.flash_info.0 as i32
            } else {
                0
            };
            h.set_reg(bx, v);
            Ok(true)
        },
        "Inst_ResetFlashInfo" => |h, _, _| {
            h.flash_info = (0, 0);
            Ok(true)
        },
        "Inst_GetNeighborhood" => |_, o, w| {
            o.neighborhood = Some(get_neighborhood(o, w));
            Ok(true)
        },
        "Inst_IfNeighborhoodChanged" => |h, o, w| {
            if !has_neighborhood_changed(o, w) {
                h.ip_advance();
            }
            Ok(true)
        },
        "Inst_Broadcast1" => |h, o, w| broadcast_x(h, o, w, 1),
        "Inst_Broadcast2" => |h, o, w| broadcast_x(h, o, w, 2),
        "Inst_Broadcast4" => |h, o, w| broadcast_x(h, o, w, 4),
        "Inst_Broadcast8" => |h, o, w| broadcast_x(h, o, w, 8),
        _ => return None,
    };
    Some(f)
}

/// `(BitCount(v & MASK24) >= CONSENSUS24) ? 1 : 0`
fn cons24(v: i32) -> i32 {
    i32::from(bit_count((v as u32) & MASK24) >= CONSENSUS24)
}

/// `cOrgMessage(sender, messageType)`: data and label 0, the sender's cell
/// and id, receiver cell 0 (the C++ initialiser; `SetReceiver` changes it).
pub(crate) fn new_message(
    org: &OrgState,
    world: &dyn OrgInterface,
    message_type: i32,
) -> OrgMessage {
    OrgMessage {
        data: 0,
        label: 0,
        sender_cell: world.get_cell_id(),
        receiver_cell: 0,
        sender_org_id: org.id,
        message_type,
        trans_cell: -1,
    }
}

/// `cOrganism::MessageSent(ctx, msg)`: keep the last
/// `MESSAGE_SEND_BUFFER_SIZE` sent messages (-1: all), receiver cleared.
pub(crate) fn message_sent(org: &mut OrgState, world: &dyn OrgInterface, msg: &OrgMessage) {
    let bsize = world.config().message_send_buffer_size;
    if bsize > 0 || bsize == -1 {
        let mut stored = *msg;
        stored.receiver_cell = -1;
        org.msg_sent.push_back(stored);
        while bsize != -1 && org.msg_sent.len() as i32 > bsize {
            org.msg_sent.pop_front();
        }
    }
}

/// `cOrganism::BroadcastMessage(ctx, msg, depth)`. With neural networking
/// avatars the sender's own input avatar can sit in a neighbouring cell,
/// so the broadcast can deliver to the sender itself.
pub(crate) fn org_broadcast_message(
    hw: &mut dyn Hardware,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    msg: &mut OrgMessage,
    depth: i32,
) -> Result<bool, String> {
    world.set_sender_recv_len(org.msg_received.len());
    let sent = world.broadcast_message(msg, depth);
    deliver_to_self(hw, org, world)?;
    if sent {
        message_sent(org, world, msg);
        return Ok(true);
    }
    Ok(false)
}

/// `cOrganism::SendMessage(ctx, msg)`: test outputs first under
/// `CHECK_TASK_ON_SEND`, then hand the message to the faced organism.
fn org_send_message(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    msg: &mut OrgMessage,
) -> Result<bool, String> {
    if world.config().check_task_on_send {
        hw.org_do_output(org, world, Some(msg.data as i32), false)?;
    }
    world.set_sender_recv_len(org.msg_received.len());
    let sent = world.send_message(msg);
    deliver_to_self(hw, org, world)?;
    if sent {
        message_sent(org, world, msg);
        return Ok(true);
    }
    Ok(false)
}

/// Apply what the last messaging call delivered to the running organism
/// itself ([`SelfDelivery`]), in order. Called right after that call.
pub(crate) fn deliver_to_self(
    hw: &mut dyn Hardware,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> Result<(), String> {
    for d in world.take_self_deliveries() {
        match d {
            SelfDelivery::Message(msg) => receive_message_parts(hw, org, world, msg)?,
            SelfDelivery::Flash => hw.receive_flash()?,
            SelfDelivery::Alarm(jump_label) => {
                hw.jump_to_alarm_label(org, jump_label)?;
            }
        }
    }
    Ok(())
}

/// `cOrganism::ReceiveMessage(msg)` on an organism's parts; see
/// [`Organism::receive_message`].
pub(crate) fn receive_message_parts(
    hardware: &mut dyn Hardware,
    state: &mut OrgState,
    world: &mut dyn OrgInterface,
    mut msg: OrgMessage,
) -> Result<(), String> {
    let bsize = world.config().message_recv_buffer_size;
    if bsize != -1 && bsize <= state.msg_received.len() as i32 {
        match world.config().message_recv_buffer_behavior {
            0 => {
                state.msg_received.pop_front();
            }
            1 => return Ok(()),
            _ => return Err("MESSAGE_RECV_BUFFER_BEHAVIOR is set to an invalid value.".to_string()),
        }
    }
    msg.receiver_cell = world.get_cell_id();
    state.msg_received.push_back(msg);
    if world.config().active_messages_enabled > 0 {
        hardware.interrupt_thread(state, world, MSG_INTERRUPT)?;
    }
    Ok(())
}

/// `cOrganism::RetrieveMessage()`: pop the oldest received message.
pub(crate) fn org_retrieve_message(org: &mut OrgState) -> Option<OrgMessage> {
    org.msg_received.pop_front()
}

/// `SendMessage(ctx, messageType)`: label from ?BX?, data from the next
/// register.
fn send_message(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    message_type: i32,
) -> R {
    let label_reg = hw.find_modified_register(REG_BX);
    let data_reg = HardwareCpu::find_next_register(label_reg);
    let mut msg = new_message(org, world, message_type);
    msg.label = hw.reg(label_reg) as u32;
    msg.data = hw.reg(data_reg) as u32;
    org_send_message(hw, org, world, &mut msg)
}

/// `BroadcastX(ctx, depth)`
fn broadcast_x(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    depth: i32,
) -> R {
    let label_reg = hw.find_modified_register(REG_BX);
    let data_reg = HardwareCpu::find_next_register(label_reg);
    let mut msg = new_message(org, world, 0);
    msg.label = hw.reg(label_reg) as u32;
    msg.data = hw.reg(data_reg) as u32;
    org_broadcast_message(hw, org, world, &mut msg, depth)
}

/// `Inst_START_Handler`: jump past the next `end-handler` (the IP stays
/// put when there is none).
fn inst_start_handler(hw: &mut HardwareCpu, _: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    hw.advance_ip = false;
    let label_inst = hw.base.inst_set.get_inst("end-handler")?;
    let size = hw.memory.size();
    let mut search_head = hw.ip_head();
    let start_pos = search_head.position();
    search_head.advance(size, 1);
    while start_pos != search_head.position() {
        if search_head.get_inst(&hw.memory) == label_inst {
            search_head.advance(size, 1);
            hw.head_set_pos(HEAD_IP, search_head.position());
            return Ok(true);
        }
        search_head.advance(size, 1);
    }
    Ok(false)
}

/// `Inst_End_Handler`: end a thread started by a message interrupt.
fn inst_end_handler(hw: &mut HardwareCpu, org: &mut OrgState, _: &mut dyn OrgInterface) -> R {
    let ct = hw.ct();
    if hw.threads[ct].message_trigger_type == -1 {
        return Ok(true);
    }
    if !hw.kill_thread() {
        org.fault(FAULT_LOC_THREAD_KILL, FAULT_TYPE_WARNING, "");
    }
    Ok(true)
}

/// `Inst_RetrieveMessage`: ?BX? = label and the next register = data of
/// the oldest received message.
pub(crate) fn inst_retrieve_message(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
) -> R {
    let Some(msg) = org_retrieve_message(org) else {
        return Ok(false);
    };
    let label_reg = hw.find_modified_register(REG_BX);
    let data_reg = HardwareCpu::find_next_register(label_reg);
    hw.set_reg(label_reg, msg.label as i32);
    hw.set_reg(data_reg, msg.data as i32);
    if world.config().net_log_retmessages != 0 {
        world.stats_log_ret_message(&msg);
    }
    Ok(true)
}

/// `cOrganism::BcastAlarmMSG(ctx, jump_label, bcast_range)`: test outputs
/// when the alarm went out.
fn bcast_alarm_msg(
    hw: &mut HardwareCpu,
    org: &mut OrgState,
    world: &mut dyn OrgInterface,
    jump_label: i32,
    bcast_range: i32,
) -> R {
    let sent = world.bcast_alarm(jump_label, bcast_range);
    deliver_to_self(hw, org, world)?;
    if sent {
        hw.org_do_output(org, world, None, false)?;
        return Ok(true);
    }
    Ok(false)
}

/// `Inst_Flash`: `cOrganism::SendFlash`, lost with probability
/// `SYNC_FLASH_LOSSRATE`.
fn inst_flash(hw: &mut HardwareCpu, org: &mut OrgState, world: &mut dyn OrgInterface) -> R {
    let lossrate = world.config().sync_flash_lossrate;
    if lossrate > 0.0 && world.rng().p(lossrate) {
        return Ok(true);
    }
    world.send_flash();
    deliver_to_self(hw, org, world)?;
    world.stats_sent_flash(org);
    hw.org_do_output(org, world, None, false)?;
    Ok(true)
}

/// `cOrganism::GetNeighborhood(ctx)`: the ids around this organism, read
/// by turning a full circle (its own id where it faces its own cell).
fn get_neighborhood(org: &OrgState, world: &mut dyn OrgInterface) -> BTreeSet<i32> {
    let mut neighbors = BTreeSet::new();
    let mut i = 0;
    while i < world.get_num_neighbors() {
        if world.is_neighbor_cell_occupied() {
            let mut id = None;
            match world.with_neighbor(&mut |o| id = Some(o.state.id)) {
                Faced::SelfOrg => id = Some(org.id),
                Faced::Other | Faced::Empty => {}
            }
            if let Some(id) = id {
                neighbors.insert(id);
            }
        }
        i += 1;
        world.rotate(1);
    }
    neighbors
}

/// `cOrganism::HasNeighborhoodChanged(ctx)`: false until a neighbourhood
/// was loaded.
fn has_neighborhood_changed(org: &mut OrgState, world: &mut dyn OrgInterface) -> bool {
    if org.neighborhood.is_none() {
        return false;
    }
    let current = get_neighborhood(org, world);
    org.neighborhood.as_ref().is_some_and(|old| *old != current)
}

impl HardwareCpu {
    /// The `MSG_INTERRUPT` tail of `InterruptThread`: on the new thread,
    /// back the IP up one, run `Inst_RetrieveMessage`, step it forward.
    pub(crate) fn msg_interrupt_retrieve(
        &mut self,
        org: &mut OrgState,
        world: &mut dyn OrgInterface,
        new_thread: usize,
    ) -> Result<(), String> {
        let old_thread = self.cur_thread;
        self.cur_thread = new_thread;
        let size = self.memory.size();
        self.head_ref(HEAD_IP).retreat(size, 1);
        let r = inst_retrieve_message(self, org, world);
        self.head_ref(HEAD_IP).advance(size, 1);
        self.cur_thread = old_thread;
        r.map(|_| ())
    }

    /// `Jump_To_Alarm_Label(jump_label)` without the sleeping check (see
    /// [`Organism::move_ip_to_alarm_label`]).
    pub(crate) fn jump_to_alarm_label_impl(&mut self, jump_label: i32) -> Result<bool, String> {
        let channel = match jump_label {
            1 => "high",
            0 => "low",
            // assert(false); a release build looks up "alarm-label-".
            _ => "",
        };
        let label_inst = self
            .base
            .inst_set
            .get_inst(&format!("alarm-label-{channel}"))?;
        let size = self.memory.size();
        let mut search_head = self.ip_head();
        let start_pos = search_head.position();
        search_head.advance(size, 1);
        while start_pos != search_head.position() {
            if search_head.get_inst(&self.memory) == label_inst {
                self.head_set_pos(HEAD_IP, search_head.position());
                self.advance_ip = false;
                return Ok(true);
            }
            search_head.advance(size, 1);
        }
        Ok(false)
    }
}

impl Organism {
    /// `cOrganism::ReceiveMessage(msg)`: store a message delivered by the
    /// population (`MESSAGE_RECV_BUFFER_SIZE` / `_BEHAVIOR`) and raise the
    /// message interrupt when `ACTIVE_MESSAGES_ENABLED`. `world` is this
    /// organism's interface. An invalid `MESSAGE_RECV_BUFFER_BEHAVIOR`
    /// aborts the C++ run (`INVALID_CONFIG`) and is the `Err`.
    pub fn receive_message(
        &mut self,
        world: &mut dyn OrgInterface,
        msg: OrgMessage,
    ) -> Result<(), String> {
        receive_message_parts(self.hardware.as_mut(), &mut self.state, world, msg)
    }

    /// `cOrganism::ReceiveFlash()`
    pub fn receive_flash(&mut self) -> Result<(), String> {
        self.hardware.receive_flash()
    }

    /// `cOrganism::moveIPtoAlarmLabel(jump_label)`:
    /// `Jump_To_Alarm_Label`, which does nothing while the organism sleeps.
    pub fn move_ip_to_alarm_label(&mut self, jump_label: i32) -> Result<bool, String> {
        self.hardware.jump_to_alarm_label(&self.state, jump_label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cons24_needs_twelve_low_bits() {
        assert_eq!(cons24(0), 0);
        assert_eq!(cons24(0x0fff), 1);
        assert_eq!(cons24(0x07ff), 0);
        assert_eq!(cons24(-1), 1);
        assert_eq!(cons24(0x7f00_07ff), 0);
    }
}
