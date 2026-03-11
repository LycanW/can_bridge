use super::CanPlugin;
use crate::types::{SensorPayload, ControlCommand, ImuData, TxFragment};
use std::time::Instant;

pub struct ImuPlugin { name: String, interface: String, listen_id: u16 }

impl ImuPlugin {
    pub fn new(n: String, i: String, l: u16) -> Self { Self { name: n, interface: i, listen_id: l } }
}

impl CanPlugin for ImuPlugin {
    fn name(&self) -> &str { &self.name }
    fn interface(&self) -> &str { &self.interface }
    fn listen_can_id(&self) -> u16 { self.listen_id }
    fn handle_rx(&mut self, _data: &[u8], ts: Instant) -> Option<SensorPayload> {
        Some(SensorPayload::Imu(ImuData {
            name: self.name.clone(), timestamp_ms: ts.elapsed().as_millis() as i64,
            ax: 0.0, ay: 0.0, az: 9.8, wx: 0.0, wy: 0.0, wz: 0.0,
        }))
    }
    fn handle_cmd(&mut self, _cmd: &ControlCommand) -> Option<TxFragment> { None }
}

pub fn create(n: String, i: String, l: u16) -> Box<dyn CanPlugin> { Box::new(ImuPlugin::new(n, i, l)) }