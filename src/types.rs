use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MitMotorData {
    pub name: String,
    pub timestamp_ms: i64,
    pub position: f32,
    pub velocity: f32,
    pub torque: f32,
    pub temp: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DjiMotorData {
    pub name: String,
    pub timestamp_ms: i64,
    pub angle_rad: f32,
    pub speed_rpm: f32,
    pub current_a: f32,
    pub torque_nm: f32,
    pub temp_c: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImuData {
    pub name: String,
    pub timestamp_ms: i64,
    pub ax: f32, pub ay: f32, pub az: f32,
    pub wx: f32, pub wy: f32, pub wz: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SensorPayload {
    Mit(MitMotorData),
    Dji(DjiMotorData),
    Imu(ImuData),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MitControlParams {
    pub kp: f32,
    pub kd: f32,
    #[serde(default)]
    pub ff: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControlCommand {
    DjiCurrent { name: String, target_a: f32 },
    MitControl { 
        name: String, 
        pos: f32, 
        vel: f32, 
        params: Option<MitControlParams> 
    },
    ImuConfig { rate_hz: u8 },
}

// CAN 发送片段
#[derive(Debug, Clone)]
pub struct TxFragment {
    pub interface: String,
    pub target_can_id: u16,
    pub byte_offset: usize,
    pub value: u16,
    pub direct_data: Option<[u8; 8]>, // 用于 MIT 整帧发送
}