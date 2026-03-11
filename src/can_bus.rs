use crate::plugins::CanPlugin;
use crate::types::{SensorPayload, ControlCommand};
use crate::config::{InterfaceCfg, DeviceCfg};
use socketcan::{CanFrame, CanFdSocket, EmbeddedFrame, Frame, StandardId, Socket};
use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::{Arc, RwLock, Mutex};
use std::thread;
use std::time::Instant;
use log::info;
use anyhow::Result;

pub struct CanBus {
    plugins: Arc<RwLock<Vec<Box<dyn CanPlugin>>>>,
    sockets: Arc<Mutex<HashMap<String, CanFdSocket>>>,
    tx_senders: Arc<Mutex<HashMap<String, mpsc::Sender<(u16, [u8; 8])>>>>,
}

impl CanBus {
    pub fn new(ifaces: &[InterfaceCfg], devs: &[DeviceCfg]) -> Result<Self> {
        let mut sockets = HashMap::new();
        for cfg in ifaces {
            if cfg.fd_rate > 0 {
                info!("Opening {} @ {}bps (FD: {}bps)", cfg.name, cfg.bitrate, cfg.fd_rate);
                let _ = std::process::Command::new("ip")
                    .args(["link", "set", &cfg.name, "type", "can", "bitrate", &cfg.bitrate.to_string(), "dbitrate", &cfg.fd_rate.to_string(), "fd", "on"])
                    .status();
            } else {
                info!("Opening {} @ {}bps", cfg.name, cfg.bitrate);
                let _ = std::process::Command::new("ip")
                    .args(["link", "set", &cfg.name, "type", "can", "bitrate", &cfg.bitrate.to_string()])
                    .status();
            }
            let _ = std::process::Command::new("ip")
                .args(["link", "set", &cfg.name, "up"])
                .status();
            let sock = CanFdSocket::open(&cfg.name)?;
            sockets.insert(cfg.name.clone(), sock);
        }

        let mut plugins = Vec::new();
        for d in devs.iter().filter(|x| x.enabled) {
            let p: Box<dyn CanPlugin> = match d.protocol.as_str() {
                "dm_mit" => crate::plugins::dm_mit::create(
                    d.name.clone(), d.interface.clone(), d.motor_id, d.can_rx_id,
                    d.kp_default, d.kd_default, d.ff_default
                ),
                "dji_gm6020" => crate::plugins::dji_gm6020::create(
                    d.name.clone(), d.interface.clone(), d.motor_id, d.can_rx_id
                ),
                "dm_imu_l1" => crate::plugins::dm_imu_l1::create(
                    d.name.clone(), d.interface.clone(), d.can_rx_id
                ),
                _ => continue,
            };
            plugins.push(p);
        }

        Ok(Self {
            plugins: Arc::new(RwLock::new(plugins)),
            sockets: Arc::new(Mutex::new(sockets)),
            tx_senders: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn start_rx(&self, tx_chan: std::sync::mpsc::Sender<SensorPayload>) {
        let socks = std::mem::take(&mut *self.sockets.lock().unwrap());
        let plugins_arc = Arc::clone(&self.plugins);
        let tx_senders = Arc::clone(&self.tx_senders);

        for (iface_name, socket) in socks {
            let (tx, rx) = mpsc::channel();
            tx_senders.lock().unwrap().insert(iface_name.clone(), tx);
            let p_arc = Arc::clone(&plugins_arc);
            let tx_clone = tx_chan.clone();

            thread::spawn(move || {
                info!("📡 RX Thread: {}", iface_name);
                loop {
                    // Drain pending TX before blocking read
                    while let Ok((id, data)) = rx.try_recv() {
                        if let Some(fid) = StandardId::new(id) {
                            if let Some(frame) = CanFrame::new(fid, &data) {
                                let _ = socket.write_frame(&frame);
                            }
                        }
                    }
                    if let Ok(frame) = socket.read_frame() {
                        let id = frame.raw_id() as u16;
                        let data = frame.data();
                        let now = Instant::now();
                        
                        let mut pls = p_arc.write().unwrap();
                        for p in pls.iter_mut() {
                            if p.interface() == iface_name && p.listen_can_id() == id {
                                if let Some(payload) = p.handle_rx(data, now) {
                                    if tx_clone.send(payload).is_err() { break; }
                                }
                            }
                        }
                    }
                }
            });
        }
    }

    pub fn send_command(&self, cmd: ControlCommand) {
        let mut pls = self.plugins.write().unwrap();
        let mut buffers: HashMap<String, HashMap<u16, [u8; 8]>> = HashMap::new();

        for p in pls.iter_mut() {
            if let Some(frag) = p.handle_cmd(&cmd) {
                let entry = buffers.entry(frag.interface).or_insert_with(HashMap::new);
                if let Some(data) = frag.direct_data {
                    entry.insert(frag.target_can_id, data);
                } else {
                    let buf = entry.entry(frag.target_can_id).or_insert([0u8; 8]);
                    buf[frag.byte_offset] = ((frag.value >> 8) & 0xFF) as u8;
                    buf[frag.byte_offset + 1] = (frag.value & 0xFF) as u8;
                }
            }
        }

        let senders = self.tx_senders.lock().unwrap();
        for (iface, cans) in buffers {
            if let Some(tx) = senders.get(&iface) {
                for (id, data) in cans {
                    let _ = tx.send((id, data));
                }
            }
        }
    }

    pub fn update_params(&self, devs: &[DeviceCfg]) {
        let mut pls = self.plugins.write().unwrap();
        for d in devs.iter().filter(|x| x.enabled) {
            for p in pls.iter_mut() {
                if p.name() == d.name && d.protocol == "dm_mit" {
                    p.update_params(d.kp_default, d.kd_default, d.ff_default);
                }
            }
        }
    }
}