use crate::config::DeviceCfg;
use crate::types::{SensorPayload, ControlCommand, TxFragment};
use std::time::Instant;

pub mod dm_mit;
pub mod dji_gm6020;
pub mod dm_imu_l1;
pub mod dji_c620;

pub trait CanPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn interface(&self) -> &str;
    fn listen_can_id(&self) -> u16;
    
    fn handle_rx(&mut self, data: &[u8], ts: Instant) -> Option<SensorPayload>;
    fn handle_cmd(&mut self, cmd: &ControlCommand) -> Option<TxFragment>;
    
    /// 动态参数更新（热重载 [dynamic] devices 时调用）
    fn update_params(&mut self, _dev: &DeviceCfg) {
        // 默认空实现
    }
}