use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MitMotorData {
    pub name: String,
    pub timestamp_ms: i64,
    pub position: f32,
    pub velocity: f32,
    pub torque: f32,
    pub temp: f32,
    /// DM 反馈帧 ERR：0=失能, 1=使能, 8=超压, 9=欠压, 0xA=过流, 0xB=MOS过温, 0xC=线圈过温, 0xD=通讯丢失, 0xE=过载
    #[serde(default)]
    pub err_code: u8,
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
        params: Option<MitControlParams>,  // 省略时使用 config 的 kp_default/kd_default/ff_default
    },
    /// DM-MIT 使能：上电自检后需发送才能控制
    MitEnable { name: String },
    /// DM-MIT 失能
    MitDisable { name: String },
    /// DM-MIT 清除错误（如过热等故障后）
    MitClearError { name: String },
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