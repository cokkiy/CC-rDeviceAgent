# Phase -1 隐式状态盘点

## 状态分类

| 状态类型 | 当前位置 | 当前存储 | 目标归属 |
| --- | --- | --- | --- |
| 服务配置 | `src/config.rs`、`CC-rDeviceAgent.toml` | TOML 文件 | Config Manager + State Store |
| watched processes | `AppState.watched_processes` | 内存 + TOML 回写 | Config Manager |
| telemetry profiles | `AppState.telemetry_profiles` | 内存 + TOML 回写 + watch channel | Config Manager / Telemetry Pipeline |
| network sampler | `AppState.network_sampler` | 内存 | Telemetry Pipeline |
| MQTT client/session | `AppState.mqtt_client` | 内存 | Protocol Layer |
| script store | `src/script_store.rs` | SQLite | State Store repository |
| batch/group/tag stores | `src/*_store.rs` | SQLite / 内存 cache | State Store repository |
| plugin status/stats | `src/*_monitor.rs` | 内存 RwLock | Telemetry Collector runtime state |
| desktop capture cache | `src/agent.rs` | 内存 Mutex | Sensor Reader cache |
| file transfer progress | `src/app.rs` | stream 局部变量 | File Transfer Service + resumable state |

## 关键状态流转

| 流程 | 当前流转 | 差距 |
| --- | --- | --- |
| 文件上传 | stream chunk -> open file -> seek/write -> chmod | 无 chunk hash、无断点状态、无统一路径策略 |
| telemetry profile 更新 | gRPC replace -> validate -> write config -> watch notify | 缺 schema version migration 和 audit |
| app 启停 | gRPC request -> process lookup/spawn/terminate | 缺 App Registry、RBAC、audit、resource limit |
| MQTT command | MQTT receive -> `AppState.handle_mqtt_command` -> ack | 缺内部 command bus 和统一鉴权 |
| desktop capture | local token -> screenshot -> stream chunks | 缺能力探测和平台抽象 |

## Phase -1 处理

Phase -1 不迁移状态存储，只将高风险状态边界显式化：文件传输限定到 managed root，raw shell command 默认禁用，编译警告清零，CI 固化当前可运行基线。
