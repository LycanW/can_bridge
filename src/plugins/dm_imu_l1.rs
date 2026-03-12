use super::CanPlugin;
use crate::types::{SensorPayload, ControlCommand, ImuData, TxFragment};
use std::time::Instant;

// DM-IMU-L1 CAN 数据帧（说明书 V1.2）：
// - DLC=8，标准帧
// - DATA[0]=数据类型：0x01 加速度，0x02 角速度，0x03 欧拉角，0x04 四元数
// - 映射值均为小端序；除四元数为 14bit 映射值外，其它为 16bit 映射值
// - 加速度范围（映射到 16bit）：[-235.2, 235.2]（单位 m/s^2）
// - 角速度范围（映射到 16bit）：[-34.88, 34.88]（单位 rad/s）

const ACCEL_MIN: f32 = -235.2;
const ACCEL_MAX: f32 = 235.2;
const GYRO_MIN: f32 = -34.88;
const GYRO_MAX: f32 = 34.88;

fn uint_to_float(x_int: u16, x_min: f32, x_max: f32, bits: u8) -> f32 {
    let span = x_max - x_min;
    if span <= 0.0 || bits == 0 {
        return x_min;
    }
    let max_int = ((1u32 << bits) - 1) as f32;
    (x_int as f32) * span / max_int + x_min
}

pub struct ImuPlugin {
    name: String,
    interface: String,
    listen_id: u16,
    last_accel: [f32; 3],
    last_gyro: [f32; 3],
    have_accel: bool,
    have_gyro: bool,
}

impl ImuPlugin {
    pub fn new(n: String, i: String, l: u16) -> Self {
        Self {
            name: n,
            interface: i,
            listen_id: l,
            last_accel: [0.0; 3],
            last_gyro: [0.0; 3],
            have_accel: false,
            have_gyro: false,
        }
    }

    fn update_accel(&mut self, data: &[u8]) {
        if data.len() < 8 {
            return;
        }
        // DATA[1]=温度（文档中未给出映射到物理量的公式，这里暂不使用）
        let ax_u = u16::from_le_bytes([data[2], data[3]]);
        let ay_u = u16::from_le_bytes([data[4], data[5]]);
        let az_u = u16::from_le_bytes([data[6], data[7]]);
        self.last_accel[0] = uint_to_float(ax_u, ACCEL_MIN, ACCEL_MAX, 16);
        self.last_accel[1] = uint_to_float(ay_u, ACCEL_MIN, ACCEL_MAX, 16);
        self.last_accel[2] = uint_to_float(az_u, ACCEL_MIN, ACCEL_MAX, 16);
        self.have_accel = true;
    }

    fn update_gyro(&mut self, data: &[u8]) {
        if data.len() < 8 {
            return;
        }
        // DATA[1]=0x00
        let gx_u = u16::from_le_bytes([data[2], data[3]]);
        let gy_u = u16::from_le_bytes([data[4], data[5]]);
        let gz_u = u16::from_le_bytes([data[6], data[7]]);
        self.last_gyro[0] = uint_to_float(gx_u, GYRO_MIN, GYRO_MAX, 16);
        self.last_gyro[1] = uint_to_float(gy_u, GYRO_MIN, GYRO_MAX, 16);
        self.last_gyro[2] = uint_to_float(gz_u, GYRO_MIN, GYRO_MAX, 16);
        self.have_gyro = true;
    }
}

impl CanPlugin for ImuPlugin {
    fn name(&self) -> &str { &self.name }
    fn interface(&self) -> &str { &self.interface }
    fn listen_can_id(&self) -> u16 { self.listen_id }

    fn handle_rx(&mut self, data: &[u8], ts: Instant) -> Option<SensorPayload> {
        if data.len() < 8 {
            return None;
        }

        match data[0] {
            0x01 => self.update_accel(data),
            0x02 => self.update_gyro(data),
            _ => {}
        }

        // 只要收到 accel 或 gyro 任意一类，就输出一次；如果另一类还没收到则保持上一次值（初始为 0）
        if !(self.have_accel || self.have_gyro) {
            return None;
        }

        Some(SensorPayload::Imu(ImuData {
            name: self.name.clone(),
            timestamp_ms: ts.elapsed().as_millis() as i64,
            ax: self.last_accel[0],
            ay: self.last_accel[1],
            az: self.last_accel[2],
            wx: self.last_gyro[0],
            wy: self.last_gyro[1],
            wz: self.last_gyro[2],
        }))
    }
    fn handle_cmd(&mut self, _cmd: &ControlCommand) -> Option<TxFragment> { None }
}

pub fn create(n: String, i: String, l: u16) -> Box<dyn CanPlugin> { Box::new(ImuPlugin::new(n, i, l)) }