use super::CanPlugin;
use crate::config::DeviceCfg;
use crate::types::{SensorPayload, ControlCommand, MitMotorData, TxFragment};
use std::time::Instant;
use log::{info, warn};

// DM 达妙 V4 协议：使能/失能/清除错误（帧 ID 同控制帧，数据段固定）
const CMD_ENABLE: [u8; 8] = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFC];
const CMD_DISABLE: [u8; 8] = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFD];
const CMD_CLEAR_ERROR: [u8; 8] = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFB];

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
    last_err: u8,
    last_temp_mos: f32,
    last_temp_rotor: f32,
}

impl MitPlugin {
    pub fn new(name: String, iface: String, motor_id: u16, listen_id: u16, kp: f32, kd: f32, ff: f32) -> Self {
        if kp > 0.0 && kd == 0.0 {
            warn!("⚠️ [{}] KP>0 but KD=0! Oscillation risk.", name);
        }
        info!("🦾 MIT '{}' init: ID={}, KP={:.1}, KD={:.1}", name, motor_id, kp, kd);
        Self { name, interface: iface, motor_id, listen_id, kp, kd, ff,
            last_pos: 0.0, last_vel: 0.0, last_torque: 0.0, last_err: 0, last_temp_mos: 0.0, last_temp_rotor: 0.0 }
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

    /// DM V4 反馈帧：D[0]=MST_ID, D[1]=ID|ERR<<4, D[2..3]=POS, D[4..6]=VEL+T, D[7]=T_MOS, D[8]=T_Rotor (H3510)
    /// 注：标准 DM-MIT 反馈帧为 8 字节，D[6]=T_MOS, D[7]=T_Rotor
    fn unpack(&mut self, data: &[u8]) {
        if data.len() < 8 { return; }
        let err = (data[1] >> 4) & 0xF;
        if err != self.last_err && err > 1 {
            let err_str = match err {
                0x8 => "Overvoltage",
                0x9 => "Undervoltage", 
                0xA => "Overcurrent",
                0xB => "MOS Overtemp",
                0xC => "Coil Overtemp",
                0xD => "Comm Lost",
                0xE => "Overload",
                _ => "Unknown Error",
            };
            warn!("⚠️ [{}] Motor ERROR: {} (0x{:X})", self.name, err_str, err);
        }
        self.last_err = err;
        let p_raw = ((data[2] as u16) << 8) | (data[3] as u16);
        let vel_12 = ((data[4] as u16) << 4) | ((data[5] >> 4) as u16);  // VEL 12bit
        let t_12 = (((data[5] & 0xF) as u16) << 8) | (data[6] as u16);   // T 12bit
        // H3510: D[6]=T_MOS, D[7]=T_Rotor (说明书明确分开)
        self.last_temp_mos = data[6] as f32;   // T_MOS 温度
        self.last_temp_rotor = data[7] as f32; // T_Rotor 温度
        self.last_pos = (p_raw as f32) / 65535.0 * (P_MAX - P_MIN) + P_MIN;
        self.last_vel = (vel_12 as f32) / 4095.0 * (V_MAX - V_MIN) + V_MIN;
        self.last_torque = (t_12 as f32) / 4095.0 * (T_MAX - T_MIN) + T_MIN;
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
            temp: self.last_temp_mos,         // T_MOS 为主要温度
            temp_rotor: self.last_temp_rotor, // T_Rotor 为线圈温度
            err_code: self.last_err,
        }))
    }

    fn handle_cmd(&mut self, cmd: &ControlCommand) -> Option<TxFragment> {
        let name_match = |n: &String| *n == self.name;
        match cmd {
            ControlCommand::MitEnable { name } if name_match(name) => {
                info!("🟢 [{}] Enable", self.name);
                Some(TxFragment {
                    interface: self.interface.clone(),
                    target_can_id: self.motor_id,
                    byte_offset: 0, value: 0,
                    direct_data: Some(CMD_ENABLE),
                })
            }
            ControlCommand::MitDisable { name } if name_match(name) => {
                info!("🔴 [{}] Disable", self.name);
                Some(TxFragment {
                    interface: self.interface.clone(),
                    target_can_id: self.motor_id,
                    byte_offset: 0, value: 0,
                    direct_data: Some(CMD_DISABLE),
                })
            }
            ControlCommand::MitClearError { name } if name_match(name) => {
                info!("🔄 [{}] Clear Error", self.name);
                Some(TxFragment {
                    interface: self.interface.clone(),
                    target_can_id: self.motor_id,
                    byte_offset: 0, value: 0,
                    direct_data: Some(CMD_CLEAR_ERROR),
                })
            }
            ControlCommand::MitControl { name, pos, vel, params } if name_match(name) => {
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
                    target_can_id: self.motor_id,
                    byte_offset: 0, value: 0,
                    direct_data: Some(data),
                })
            }
            _ => None,
        }
    }

    fn update_params(&mut self, dev: &DeviceCfg) {
        self.kp = dev.kp_default;
        self.kd = dev.kd_default;
        self.ff = dev.ff_default;
        info!("⚙️ [{}] Params Updated: KP={:.1}, KD={:.1}", self.name, self.kp, self.kd);
    }
}

pub fn create(name: String, iface: String, mid: u16, lid: u16, kp: f32, kd: f32, ff: f32) -> Box<dyn CanPlugin> {
    Box::new(MitPlugin::new(name, iface, mid, lid, kp, kd, ff))
}