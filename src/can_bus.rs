use crate::plugins::CanPlugin;
use crate::types::{SensorPayload, ControlCommand};
use crate::config::{InterfaceCfg, DeviceCfg};
use socketcan::{CanFrame, CanFdSocket, EmbeddedFrame, Frame, StandardId, Socket};
use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::{Arc, RwLock, Mutex};
use std::thread;
use std::time::Instant;
use log::{debug, info, warn};
use anyhow::Result;

fn fmt_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        use std::fmt::Write as _;
        let _ = write!(&mut s, "{:02X}", b);
    }
    s
}

pub struct CanBus {
    plugins: Arc<RwLock<Vec<Box<dyn CanPlugin>>>>,
    sockets: Arc<Mutex<HashMap<String, CanFdSocket>>>,
    tx_senders: Arc<Mutex<HashMap<String, mpsc::Sender<(u16, [u8; 8])>>>>,
    last_tx: Arc<Mutex<HashMap<String, (Vec<(u16, [u8; 8])>, std::time::Instant)>>>,
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
                "dji_c620" => crate::plugins::dji_c620::create(
                    d.name.clone(), d.interface.clone(), d.motor_id, d.can_rx_id
                ),
                _ => continue,
            };
            plugins.push(p);
        }

        Ok(Self {
            plugins: Arc::new(RwLock::new(plugins)),
            sockets: Arc::new(Mutex::new(sockets)),
            tx_senders: Arc::new(Mutex::new(HashMap::new())),
            last_tx: Arc::new(Mutex::new(HashMap::new())),
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
                let mut rx_frames: u64 = 0;
                let mut last_stats = std::time::Instant::now();
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
                        rx_frames = rx_frames.saturating_add(1);

                        debug!(
                            "CAN RX {} id=0x{:03X} dlc={} data={}",
                            iface_name,
                            id,
                            data.len(),
                            fmt_hex(data)
                        );

                        let stats_now = std::time::Instant::now();
                        if stats_now.duration_since(last_stats) >= std::time::Duration::from_secs(1) {
                            info!("CAN RX stats {}: {} frames/s", iface_name, rx_frames);
                            rx_frames = 0;
                            last_stats = stats_now;
                        }
                        
                        let mut pls = p_arc.write().unwrap();
                        for p in pls.iter_mut() {
                            if p.interface() == iface_name && p.listen_can_id() == id {
                                if let Some(payload) = p.handle_rx(data, now) {
                                    if tx_clone.send(payload).is_err() { break; }
                                }
                            }
                        }
                    } else {
                        // 如果 read_frame() 返回错误/暂时不可读，这里不 panic；留给下一轮重试
                        // 只在 debug 下提示一次性问题即可，避免刷屏
                        // （socketcan 的 read_frame 在阻塞模式下通常不会走到这里）
                        warn!("CAN read_frame failed on {}", iface_name);
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
        let now = std::time::Instant::now();
        for (iface, cans) in buffers {
            if let Some(tx) = senders.get(&iface) {
                let frames: Vec<(u16, [u8; 8])> = cans.into_iter().collect();
                for &(id, ref data) in &frames {
                    let _ = tx.send((id, data.clone()));
                }
                self.last_tx.lock().unwrap().insert(iface, (frames, now));
            }
        }
    }

    /// 按各接口配置的 control_freq_hz 周期重发上一帧，避免通讯超时
    pub fn tick_control(&self, ifaces: &[crate::config::InterfaceCfg]) {
        let now = std::time::Instant::now();
        let senders = self.tx_senders.lock().unwrap();
        let mut last = self.last_tx.lock().unwrap();
        for cfg in ifaces {
            if cfg.control_freq_hz == 0 { continue; }
            let interval = std::time::Duration::from_secs_f64(1.0 / cfg.control_freq_hz as f64);
            let frames = match last.get(&cfg.name) {
                Some((f, t)) if now.duration_since(*t) >= interval && !f.is_empty() => f.clone(),
                _ => continue,
            };
            if let Some(tx) = senders.get(&cfg.name) {
                for (id, data) in &frames {
                    let _ = tx.send((*id, *data));
                }
                last.insert(cfg.name.clone(), (frames, now));
            }
        }
    }

    /// DM-MIT 电机上电自检后需发送使能才能控制，可于启动后延迟调用
    pub fn enable_all_mit(&self, devs: &[DeviceCfg]) {
        for d in devs.iter().filter(|x| x.enabled && x.protocol == "dm_mit") {
            self.send_command(ControlCommand::MitEnable {
                name: d.name.clone(),
            });
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