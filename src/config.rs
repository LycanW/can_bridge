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
}

#[derive(Debug, Clone, Deserialize)]
pub struct InterfaceCfg {
    pub name: String,
    pub bitrate: u32,
    pub fd_rate: u32,
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