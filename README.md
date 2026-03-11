# can_bridge

can_bridge 基于 [rs_ctrl_os](https://crates.io/crates/rs_ctrl_os) 构建，复用其分布式节点框架：

- **节点发现**：`start_discovery` + `ServiceRegistry`（UDP 多播心跳）
- **消息通信**：`PubSubManager`（ZeroMQ pub/sub）
- **配置结构**：`static_config` 采用 `rs_ctrl_os::StaticBase`（my_id、host、port、publishers、subscribers 等）
- **初始化**：`init_logging`、`TimeSynchronizer` 等

配置中的 `[static_config]` 与 rs_ctrl_os 规范完全一致，需正确填写 `host`（本机 IP）、`subscribers`（从哪个节点订阅控制指令）等，才能与其他节点互通。

---

CAN 总线网关：将 Linux SocketCAN 与 ZeroMQ 打通，实现「CAN ↔ 分布式消息」的双向桥接。支持 DM-MIT、DJI GM6020、DM-IMU 等协议。

## 功能概览

| 方向 | 说明 |
|------|------|
| **CAN → ZMQ** | 传感器数据（MIT/DJI/IMU）解析后发布到 `sensor_mit` / `sensor_dji` / `sensor_imu` |
| **ZMQ → CAN** | 订阅 `ctrl_dji` / `ctrl_mit` 等控制话题，下发电流/位置指令到 CAN 设备 |

**支持的协议：**

- `dm_mit`：DM-MIT 协议（MIT Cheetah 系列电机）
- `dji_gm6020`：大疆 GM6020 云台电机（电流环）
- `dm_imu_l1`：DM-IMU 系列 IMU（占位实现）

---

## 环境要求

- **Linux**（SocketCAN 需要内核支持）
- **ZeroMQ** 库（如 `libzmq3-dev` / `zeromq-devel`）
- **Rust** 1.70+
- 本机需存在虚拟/物理 CAN 接口（如 `can0`、`vcan0`）

### 虚拟 CAN 测试（无硬件时）

```bash
sudo modprobe vcan
sudo ip link add dev vcan0 type vcan
sudo ip link set up vcan0
```

将配置中的 `can0` 改为 `vcan0` 即可本地测试。

---

## 安装与运行

### 1. 克隆并构建

```bash
git clone <your-repo>/can_bridge.git
cd can_bridge
cargo build --release
```

### 2. 配置

编辑 `configs/config.toml`：

- **static_config**：节点 ID、本机 IP、端口、发布/订阅拓扑（与 rs_ctrl_os 一致）
- **dynamic**：CAN 接口列表、设备列表、控制开关

示例：

```toml
[static_config]
my_id = "gateway_node_01"
host = "192.168.1.100"   # 本机 IP
port = 5555
is_master = false

[static_config.publishers]
sensor_mit = "self"
sensor_dji = "self"
sensor_imu = "self"

[static_config.subscribers]
ctrl_mit = "ctrl_node"
ctrl_dji = "ctrl_node"

[dynamic]
interfaces = [
    { name = "can0", bitrate = 5000000, fd_rate = 2000000, control_freq_hz = 1000 },
    { name = "can1", bitrate = 1000000, fd_rate = 0, control_freq_hz = 500 },
]
devices = [
    { name = "mit_joint_1", protocol = "dm_mit", interface = "can0",
      can_rx_id = 0x001, motor_id = 1, enabled = true,
      kp_default = 50.0, kd_default = 1.5, ff_default = 0.0 },
    { name = "dji_joint_1", protocol = "dji_gm6020", interface = "can1",
      can_rx_id = 0x205, motor_id = 1, enabled = true },
]
control_enable = true
mit_auto_enable = true   # 启动 0.5s 后自动使能 MIT 电机
```

### 3. 运行

配置 CAN 接口通常需要 root 权限：

```bash
sudo ./target/release/can_bridge
```

或先手动配置接口后以普通用户运行（`control_enable` 可按需关闭以仅做监听）。

---

## 配置说明

### 接口 (interfaces)

| 字段 | 说明 |
|------|------|
| `name` | 接口名，如 `can0`、`vcan0` |
| `bitrate` | 仲裁段波特率（bps） |
| `fd_rate` | CAN FD 数据段波特率；`0` 表示经典 CAN |
| `control_freq_hz` | 控制帧发送频率 Hz，按此周期重发上一帧避免通讯超时；`0` 表示不周期发送（默认 1000） |

启动时会执行 `ip link set` 配置接口并 `ip link set up`，失败时仅记录日志，仍会尝试打开 socket。DM-MIT 等协议需周期性指令，建议 `control_freq_hz = 1000`；IMU 等只收不发可设为 `0`。

### 设备 (devices)

| 字段 | 说明 |
|------|------|
| `name` | 设备逻辑名，用于控制命令匹配 |
| `protocol` | `dm_mit` / `dji_gm6020` / `dm_imu_l1` |
| `interface` | 所属 CAN 接口 |
| `can_rx_id` | 监听的 CAN ID（16 进制） |
| `motor_id` | MIT 电机 ID 或 DJI 电机编号（IMU 可省略） |
| `enabled` | 是否启用 |
| `kp_default`, `kd_default`, `ff_default` | MIT 默认控制参数 |

---

## 消息格式

### 传感器数据 (发布)

- **sensor_mit**：`MitMotorData`（position, velocity, torque, temp, err_code）
  - `err_code`：DM 反馈 ERR，0=失能，1=使能，8=超压，9=欠压，0xA=过流，0xB=MOS过温，0xC=线圈过温，0xD=通讯丢失，0xE=过载
- **sensor_dji**：`DjiMotorData`（angle_rad, current_a, torque_nm, temp_c）
- **sensor_imu**：`ImuData`（ax, ay, az, wx, wy, wz）

均以 JSON 形式发布到 `publish_topic(topic_key, "data", &json_string)`。

### 控制命令 (订阅)

通过 `ctrl_dji` / `ctrl_mit` 的 `"cmd"` 子话题接收，payload 为 JSON：

**DJI 电流环：**

```json
{ "DjiCurrent": { "name": "dji_joint_1", "target_a": 0.5 } }
```

**MIT 阻抗控制**（`params` 可选，省略时使用 config 的 `kp_default`/`kd_default`/`ff_default`）：

```json
{
  "MitControl": {
    "name": "mit_joint_1",
    "pos": 0.0,
    "vel": 0.0,
    "params": { "kp": 50.0, "kd": 1.5, "ff": 0.0 }
  }
}
```

**DM-MIT 使能 / 失能 / 清除错误**（达妙 V4 协议，上电自检后需使能才能控制）：

```json
{ "MitEnable": { "name": "mit_joint_1" } }
{ "MitDisable": { "name": "mit_joint_1" } }
{ "MitClearError": { "name": "mit_joint_1" } }
```

**如何发送使能指令**：上位机需订阅 `ctrl_mit` 对应的 ZMQ 端点（由 `static_config.subscribers` 中的 `ctrl_mit` 配置），向子话题 `"cmd"` 发布上述 JSON。例如通过 rs_ctrl_os 的 `publish_topic("ctrl_mit", "cmd", &json_string)`，或直接连接 ZMQ 发布端发送 `["ctrl_mit", "cmd", json_string]` 形式的多帧消息。can_bridge 收到后会解析并转发到 CAN 总线。

启动时若 `mit_auto_enable = true`（默认），can_bridge 将在约 0.5s 后自动向所有 MIT 设备发送使能；设为 `false` 可禁用自动使能，改由上位机通过上述方式手动发送。

---

## 热更新

修改 `configs/config.toml` 中的 `[dynamic]` 并保存后，约 2 秒内会重载设备参数（如 MIT 的 KP/KD/FF），无需重启进程。

---

## 如何添加新协议插件

本节以添加一个虚构的 `my_motor` 协议为例，说明从零接入新设备驱动的完整流程。

### 1. 定义数据类型 (`src/types.rs`)

在 `SensorPayload` 中增加新变体，用于 CAN → ZMQ 的传感器数据：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyMotorData {
    pub name: String,
    pub timestamp_ms: i64,
    pub angle_rad: f32,
    pub current_a: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SensorPayload {
    Mit(MitMotorData),
    Dji(DjiMotorData),
    Imu(ImuData),
    MyMotor(MyMotorData),  // 新增
}
```

若协议支持控制，在 `ControlCommand` 中增加：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControlCommand {
    DjiCurrent { name: String, target_a: f32 },
    MitControl { ... },
    ImuConfig { ... },
    MyMotorSet { name: String, target_a: f32 },  // 新增
}
```

### 2. 创建插件 (`src/plugins/my_motor.rs`)

实现 `CanPlugin` trait，核心接口：

| 方法 | 作用 |
|------|------|
| `name()` / `interface()` / `listen_can_id()` | 标识设备、所属接口、监听的 CAN ID |
| `handle_rx(data, ts)` | 解析 CAN 帧 → `Some(SensorPayload::MyMotor(...))` |
| `handle_cmd(cmd)` | 解析控制命令 → `Some(TxFragment)` |

```rust
use super::CanPlugin;
use crate::types::{SensorPayload, ControlCommand, MyMotorData, TxFragment};
use std::time::Instant;

pub struct MyMotorPlugin {
    name: String,
    interface: String,
    listen_id: u16,
    tx_id: u16,
}

impl MyMotorPlugin {
    pub fn new(name: String, iface: String, listen_id: u16, motor_id: u16) -> Self {
        Self { name, interface: iface, listen_id, tx_id: 0x200 + motor_id }
    }
}

impl CanPlugin for MyMotorPlugin {
    fn name(&self) -> &str { &self.name }
    fn interface(&self) -> &str { &self.interface }
    fn listen_can_id(&self) -> u16 { self.listen_id }

    fn handle_rx(&mut self, data: &[u8], ts: Instant) -> Option<SensorPayload> {
        if data.len() < 6 { return None; }
        let angle_raw = ((data[0] as u16) << 8) | (data[1] as u16);
        let cur_raw = ((data[2] as u16) << 8) | (data[3] as u16);
        let angle = (angle_raw as f32) * 0.001;  // 按协议换算
        let current = (cur_raw as i16) as f32 * 0.001;
        Some(SensorPayload::MyMotor(MyMotorData {
            name: self.name.clone(),
            timestamp_ms: ts.elapsed().as_millis() as i64,
            angle_rad: angle, current_a: current,
        }))
    }

    fn handle_cmd(&mut self, cmd: &ControlCommand) -> Option<TxFragment> {
        if let ControlCommand::MyMotorSet { name, target_a } = cmd {
            if *name != self.name { return None; }
            let raw = (target_a.clamp(-5.0, 5.0) * 1000.0) as u16;
            Some(TxFragment {
                interface: self.interface.clone(),
                target_can_id: self.tx_id,
                byte_offset: 0, value: raw, direct_data: None,
            })
        } else { None }
    }
}

pub fn create(name: String, iface: String, motor_id: u16, listen_id: u16) -> Box<dyn CanPlugin> {
    Box::new(MyMotorPlugin::new(name, iface, listen_id, motor_id))
}
```

`TxFragment` 两种用法：

- **`direct_data: Some([u8;8])`**：整帧 CAN 数据，用于 MIT 等自定义帧格式
- **`byte_offset + value`**：16bit 写入指定偏移，用于 DJI 等多电机共 ID 场景

### 3. 注册模块 (`src/plugins/mod.rs`)

```rust
pub mod my_motor;  // 新增
```

### 4. 在 `can_bus.rs` 中创建插件

在 `CanBus::new()` 的 `match d.protocol.as_str()` 中增加分支：

```rust
"my_motor" => crate::plugins::my_motor::create(
    d.name.clone(), d.interface.clone(), d.motor_id, d.can_rx_id
),
```

### 5. 在 `main.rs` 中发布与订阅

发布传感器数据（在 `match payload` 中）：

```rust
SensorPayload::MyMotor(m) => ("sensor_mymotor", serde_json::to_string(&m).unwrap()),
```

订阅控制命令（复制 `ctrl_dji` 块并改名）：

```rust
if let Ok(Some((topic, raw))) = bus.try_recv_raw("ctrl_mymotor") {
    if topic == "cmd" {
        if let Ok(cmd) = serde_json::from_slice::<ControlCommand>(&raw) {
            if initial_cfg.dynamic.control_enable {
                can_bus.send_command(cmd);
            }
        }
    }
}
```

### 6. 配置文件

在 `config.toml` 的 `static_config.publishers` 和 `static_config.subscribers` 中增加 `sensor_mymotor`、`ctrl_mymotor`；在 `[dynamic]` 的 `devices` 中增加设备项，`protocol = "my_motor"`。

---

## 项目结构

```
can_bridge/
├── src/
│   ├── main.rs       # 入口：CAN↔ZMQ 桥接主循环
│   ├── config.rs     # 配置加载
│   ├── types.rs      # 传感器/控制命令类型
│   ├── can_bus.rs    # CAN 总线管理、接口配置、RX/TX
│   └── plugins/
│       ├── mod.rs    # CanPlugin trait
│       ├── dm_mit.rs # DM-MIT 电机协议
│       ├── dji_gm6020.rs
│       └── dm_imu_l1.rs
├── configs/
│   └── config.toml
└── Cargo.toml
```

---

## 依赖

- **rs_ctrl_os**：节点发现、ZeroMQ pub/sub、StaticBase 配置（核心框架）
- **socketcan**：Linux SocketCAN（含 CAN FD）
- **serde** / **serde_json**：序列化
- **anyhow** / **log**：错误与日志

---

## 许可证

MIT License
