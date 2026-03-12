use serde::Deserialize;
use std::fs;
use anyhow::Result;

#[derive(Debug, Clone, Deserialize)]
pub struct GatewayConfig {
    pub static_config: rs_ctrl_os::StaticBase,
    pub dynamic: DynamicConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DynamicConfig {
    pub interfaces: Vec<InterfaceCfg>,
    pub devices: Vec<DeviceCfg>,
    pub control_enable: bool,
    /// 传感器数据发布频率（Hz）
    /// - >0: 固定频率发布（主循环 sleep 到该频率）
    /// - =0: 动态频率（收到多少数据就发布多快；空闲时会短暂 sleep 防止 CPU 空转）
    #[serde(default = "default_publish_hz")]
    pub publish_hz: u32,
    /// DM-MIT 启动后是否自动使能（上电自检约 0.5s 后发送使能命令）
    #[serde(default = "default_true")]
    pub mit_auto_enable: bool,
}

fn default_true() -> bool {
    true
}

fn default_publish_hz() -> u32 {
    1000
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

pub fn load_config(path: &str) -> Result<GatewayConfig> {
    let content = fs::read_to_string(path)?;
    // 注意：toml crate 可以直接反序列化为结构体，如果结构体字段匹配
    // 这里假设 TOML 根节点直接对应 GatewayConfig 字段，或者需要自定义解析
    // 为了简化，假设 TOML 结构完全匹配 GatewayConfig (即没有根节点名，直接是字段)
    // 但我们的 TOML 有 [static_config] 和 [dynamic]，所以需要自定义中间结构或手动映射
    
    // 使用 toml::from_str 解析为 Value 然后提取，或者定义一个包含所有字段的扁平结构
    // 这里采用最稳妥的方式：定义一个包含所有部分的 Wrapper
    let val: toml::Value = toml::from_str(&content)?;
    
    let static_val = val.get("static_config").ok_or_else(|| anyhow::anyhow!("Missing static_config"))?;
    let dyn_val = val.get("dynamic").ok_or_else(|| anyhow::anyhow!("Missing dynamic"))?;
    
    Ok(GatewayConfig {
        static_config: toml::from_str(&toml::to_string(static_val)?)?,
        dynamic: toml::from_str(&toml::to_string(dyn_val)?)?,
    })
}