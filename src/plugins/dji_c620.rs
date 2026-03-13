use super::CanPlugin;
use crate::config::DeviceCfg;
use crate::types::{SensorPayload, ControlCommand, DjiMotorData, TxFragment};
use std::time::Instant;

pub struct DjiC620Plugin {
    name: String, 
    interface: String,
    motor_id: u8,
    listen_id: u16,
    tx_id: u16,
    torque_constant: std::sync::RwLock<f32>,
}

impl DjiC620Plugin {
    pub fn new(name: String, iface: String, mid: u8, lid: u16, torque_constant: f32) -> Self {
        let tid = if mid <= 4 { 0x200 } else { 0x1FF };
        Self {
            name, interface: iface, motor_id: mid, listen_id: lid, tx_id: tid,
            torque_constant: std::sync::RwLock::new(torque_constant),
        }
    }
    fn offset(&self) -> usize { match self.motor_id { 1|5=>0, 2|6=>2, 3|7=>4, 4=>6, _=>0 } }
}

impl CanPlugin for DjiC620Plugin {
    fn name(&self) -> &str { &self.name }
    fn interface(&self) -> &str { &self.interface }
    fn listen_can_id(&self) -> u16 { self.listen_id }

    fn handle_rx(&mut self, data: &[u8], ts: Instant) -> Option<SensorPayload> {
        if data.len() < 8 { return None; }
        let angle_raw = ((data[0] as u16) << 8) | (data[1] as u16);
        let cur_raw = ((data[4] as u16) << 8) | (data[5] as u16);
        let cur = (cur_raw as i16) as f32 * (3.0 / 16384.0);
        let angle = (angle_raw & 0x3FFF) as f32 * (2.0 * std::f32::consts::PI / 8192.0);
        let kt = *self.torque_constant.read().unwrap();
        
        Some(SensorPayload::Dji(DjiMotorData {
            name: self.name.clone(), timestamp_ms: ts.elapsed().as_millis() as i64,
            angle_rad: angle, speed_rpm: 0.0, current_a: cur, torque_nm: cur * kt, temp_c: data[6] as f32,
        }))
    }

    fn update_params(&mut self, dev: &DeviceCfg) {
        if let Some(kt) = dev.torque_constant {
            *self.torque_constant.write().unwrap() = kt;
        }
    }

    fn handle_cmd(&mut self, cmd: &ControlCommand) -> Option<TxFragment> {
        if let ControlCommand::DjiCurrent { name, target_a } = cmd {
            if *name != self.name { return None; }
            let raw = (target_a.clamp(-20.0, 20.0) * (16384.0 / 20.0)) as u16;
            Some(TxFragment {
                interface: self.interface.clone(), target_can_id: self.tx_id,
                byte_offset: self.offset(), value: raw, direct_data: None,
            })
        } else { None }
    }
}

pub fn create(
    name: String,
    iface: String,
    mid: u16,
    lid: u16,
    torque_constant: Option<f32>,
) -> Result<Box<dyn CanPlugin>, String> {
    let kt = torque_constant
        .ok_or_else(|| format!("dji_c620 设备 '{}' 必须在 config 中设置 torque_constant", name))?;
    Ok(Box::new(DjiC620Plugin::new(name, iface, mid as u8, lid, kt)))
}