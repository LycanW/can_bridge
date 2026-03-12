mod config;
mod types;
mod plugins;
mod can_bus;

use config::load_config;
use can_bus::CanBus;
use types::{SensorPayload, ControlCommand};
use rs_ctrl_os::{init_logging, start_discovery, PubSubManager, TimeSynchronizer};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use log::info;
use anyhow::Result;

fn main() -> Result<()> {
    init_logging();
    info!("🚀 Gateway Node Starting...");

    // 1. Load Config
    let cfg_path = "configs/config.toml";
    let initial_cfg = load_config(cfg_path)?;
    
    // 2. Init rs_ctrl_os
    let static_cfg = initial_cfg.static_config.clone();
    let time_sync = Arc::new(TimeSynchronizer::new());
    let registry = start_discovery(
        &static_cfg.my_id, &static_cfg.host, static_cfg.port, static_cfg.is_master,
        Some(time_sync)
    )?;
    let mut bus = PubSubManager::new(&static_cfg, registry)?;

    // 3. Init CAN Bus
    let can_bus = Arc::new(CanBus::new(
        &initial_cfg.dynamic.interfaces,
        &initial_cfg.dynamic.devices
    )?);

    // 4. Start RX Thread (CAN -> ZMQ)
    let (tx_chan, rx_chan) = std::sync::mpsc::channel::<SensorPayload>();
    can_bus.start_rx(tx_chan);

    // 4.1 DM-MIT 使能：上电自检后需发送使能命令才能控制（延迟 0.5s 待自检完成）
    if initial_cfg.dynamic.mit_auto_enable {
        thread::sleep(Duration::from_millis(500));
        can_bus.enable_all_mit(&initial_cfg.dynamic.devices);
    }

    // 5. Config Watcher (Hot Reload)
    let can_bus_cfg = Arc::clone(&can_bus);
    thread::spawn(move || {
        loop {
            // 模拟等待配置变化 (实际需调用 cfg_manager.wait_for_change())
            thread::sleep(Duration::from_secs(2)); 
            if let Ok(new_cfg) = load_config(cfg_path) {
                can_bus_cfg.update_params(&new_cfg.dynamic.devices);
            }
        }
    });

    // 6. Main Loop (ZMQ pub + ZMQ -> CAN)
    // 控制调度频率：用于 tick_control()（周期重发控制帧）
    let control_hz = initial_cfg
        .dynamic
        .interfaces
        .iter()
        .map(|i| i.control_freq_hz)
        .filter(|&hz| hz > 0)
        .max()
        .unwrap_or(1000)
        .max(1);

    // 主循环频率：只需保证 >= control_hz，以便 tick_control() 正常工作。
    // 具体的 pub/sub 限频由 PubSubManager 内部根据 publish_hz / subscribe_hz 处理。
    let loop_hz = control_hz;
    let loop_interval = Duration::from_secs_f64(1.0 / loop_hz as f64);
    info!("Entering Main Loop @ {}Hz (control_hz={})", loop_hz, control_hz);
    loop {
        // 发布传感器数据 (CAN -> ZMQ)
        while let Ok(payload) = rx_chan.try_recv() {
            let (topic, data) = match payload {
                SensorPayload::Mit(m) => ("sensor_mit", serde_json::to_string(&m).unwrap()),
                SensorPayload::Dji(d) => ("sensor_dji", serde_json::to_string(&d).unwrap()),
                SensorPayload::Imu(i) => ("sensor_imu", serde_json::to_string(&i).unwrap()),
            };
            let _ = bus.publish_topic(topic, "data", &data);
        }
        // 尝试接收 DJI 指令
        if let Ok(Some((topic, raw))) = bus.try_recv_raw("ctrl_dji") {
            if topic == "cmd" {
                if let Ok(cmd) = serde_json::from_slice::<ControlCommand>(&raw) {
                    if initial_cfg.dynamic.control_enable {
                        can_bus.send_command(cmd);
                    }
                }
            }
        }
        // 尝试接收 MIT 指令
        if let Ok(Some((topic, raw))) = bus.try_recv_raw("ctrl_mit") {
            if topic == "cmd" {
                if let Ok(cmd) = serde_json::from_slice::<ControlCommand>(&raw) {
                    if initial_cfg.dynamic.control_enable {
                        can_bus.send_command(cmd);
                    }
                }
            }
        }
        can_bus.tick_control(&initial_cfg.dynamic.interfaces);
        thread::sleep(loop_interval);
    }
}