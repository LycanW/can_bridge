use serde::Deserialize;

/// can_bridge 专属的 [dynamic] 配置，由 rs_ctrl_os::ConfigManager 解析并热重载。
#[derive(Debug, Clone, Deserialize)]
pub struct DynamicConfig {
    pub interfaces: Vec<InterfaceCfg>,
    pub devices: Vec<DeviceCfg>,
    pub control_enable: bool,
    /// DM-MIT 启动后是否自动使能（上电自检约 0.5s 后发送使能命令）
    #[serde(default = "default_true")]
    pub mit_auto_enable: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct InterfaceCfg {
    pub name: String,
    pub bitrate: u32,
    pub fd_rate: u32,
    /// 控制帧发送频率 Hz，避免通讯丢失（DM-MIT 等需周期性指令）
    #[serde(default = "default_control_freq")]
    pub control_freq_hz: u32,
}

fn default_control_freq() -> u32 {
    1000
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCfg {
    pub name: String,
    pub protocol: String,
    pub interface: String,
    #[serde(default)]
    pub motor_id: u16,      // 用于 MIT ID 计算
    pub can_rx_id: u16,     // 监听 ID
    pub enabled: bool,
    // MIT 动态参数
    #[serde(default)]
    pub kp_default: f32,
    #[serde(default)]
    pub kd_default: f32,
    #[serde(default)]
    pub ff_default: f32,
}