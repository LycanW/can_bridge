use super::CanPlugin;
use crate::types::{SensorPayload, ControlCommand, MitMotorData, TxFragment};
use std::time::Instant;
use log::{info, warn};

const P_MIN: f32 = -12.5;
const P_MAX: f32 = 12.5;
const V_MIN: f32 = -30.0;
const V_MAX: f32 = 30.0;
const KP_MIN: f32 = 0.0;
const KP_MAX: f32 = 500.0;
const KD_MIN: f32 = 0.0;
const KD_MAX: f32 = 5.0;
const T_MIN: f32 = -10.0;
const T_MAX: f32 = 10.0;

pub struct MitPlugin {
    name: String,
    interface: String,
    motor_id: u16,
    listen_id: u16,
    kp: f32,
    kd: f32,
    ff: f32,
    last_pos: f32,
    last_vel: f32,
    last_torque: f32,
}

impl MitPlugin {
    pub fn new(name: String, iface: String, motor_id: u16, listen_id: u16, kp: f32, kd: f32, ff: f32) -> Self {
        if kp > 0.0 && kd == 0.0 {
            warn!("⚠️ [{}] KP>0 but KD=0! Oscillation risk.", name);
        }
        info!("🦾 MIT '{}' init: ID={}, KP={:.1}, KD={:.1}", name, motor_id, kp, kd);
        Self { name, interface: iface, motor_id, listen_id, kp, kd, ff, last_pos: 0.0, last_vel: 0.0, last_torque: 0.0 }
    }

    fn float_to_uint(val: f32, min: f32, max: f32, bits: u8) -> u16 {
        let v = val.clamp(min, max);
        let span = max - min;
        if span <= 0.0 { return 0; }
        ((v - min) * ((1u16 << bits) - 1) as f32 / span) as u16
    }

    fn pack(&self, pos: f32, vel: f32, kp: f32, kd: f32, torq: f32) -> [u8; 8] {
        let p = Self::float_to_uint(pos, P_MIN, P_MAX, 16);
        let v = Self::float_to_uint(vel, V_MIN, V_MAX, 12);
        let kps = Self::float_to_uint(kp, KP_MIN, KP_MAX, 12);
        let kds = Self::float_to_uint(kd, KD_MIN, KD_MAX, 12);
        let t = Self::float_to_uint(torq, T_MIN, T_MAX, 12);

        let mut d = [0u8; 8];
        d[0] = (p >> 8) as u8;
        d[1] = p as u8;
        d[2] = (v >> 4) as u8;
        d[3] = (((v & 0xF) << 4) | (kps >> 8)) as u8;
        d[4] = kps as u8;
        d[5] = (kds >> 4) as u8;
        d[6] = (((kds & 0xF) << 4) | (t >> 8)) as u8;
        d[7] = t as u8;
        d
    }

    // 注意：反馈格式需根据 DM 具体手册调整，此处假设为标准 16bit 线性映射
    fn unpack(&mut self, data: &[u8]) {
        if data.len() < 6 { return; }
        // 假设前 6 字节为 Pos, Vel, Torque (各 16bit signed)
        let p_raw = ((data[0] as i16) << 8) | (data[1] as i16);
        let v_raw = ((data[2] as i16) << 8) | (data[3] as i16);
        let t_raw = ((data[4] as i16) << 8) | (data[5] as i16);
        
        self.last_pos = (p_raw as f32) * (P_MAX - P_MIN) / 65535.0 + P_MIN;
        self.last_vel = (v_raw as f32) * (V_MAX - V_MIN) / 65535.0 + V_MIN;
        self.last_torque = (t_raw as f32) * (T_MAX - T_MIN) / 65535.0 + T_MIN;
    }
}

impl CanPlugin for MitPlugin {
    fn name(&self) -> &str { &self.name }
    fn interface(&self) -> &str { &self.interface }
    fn listen_can_id(&self) -> u16 { self.listen_id }

    fn handle_rx(&mut self, data: &[u8], ts: Instant) -> Option<SensorPayload> {
        self.unpack(data);
        Some(SensorPayload::Mit(MitMotorData {
            name: self.name.clone(),
            timestamp_ms: ts.elapsed().as_millis() as i64,
            position: self.last_pos,
            velocity: self.last_vel,
            torque: self.last_torque,
            temp: 0.0,
        }))
    }

    fn handle_cmd(&mut self, cmd: &ControlCommand) -> Option<TxFragment> {
        if let ControlCommand::MitControl { name, pos, vel, params } = cmd {
            if *name != self.name { return None; }
            
            let (kp, kd, ff) = match params {
                Some(p) => (p.kp, p.kd, p.ff),
                None => (self.kp, self.kd, self.ff),
            };

            if kp > 0.0 && kd == 0.0 {
                warn!("⚠️ [{}] Cmd Rejected: KP>0, KD=0", self.name);
                return None;
            }

            let data = self.pack(*pos, *vel, kp, kd, ff);
            Some(TxFragment {
                interface: self.interface.clone(),
                target_can_id: self.motor_id, // DM MIT: ID = motor_id
                byte_offset: 0,
                value: 0,
                direct_data: Some(data),
            })
        } else { None }
    }

    fn update_params(&mut self, kp: f32, kd: f32, ff: f32) {
        self.kp = kp;
        self.kd = kd;
        self.ff = ff;
        info!("⚙️ [{}] Params Updated: KP={:.1}, KD={:.1}", self.name, kp, kd);
    }
}

pub fn create(name: String, iface: String, mid: u16, lid: u16, kp: f32, kd: f32, ff: f32) -> Box<dyn CanPlugin> {
    Box::new(MitPlugin::new(name, iface, mid, lid, kp, kd, ff))
}