# Phase -1 现状-目标架构差异矩阵

## 目标

本矩阵对照 `architecture-zh.md` 的“双面三层”目标架构和当前代码实现，作为 Phase 0 拆分 PAL、State Store、Security Center 与协议层的输入。

## 模块映射

| 当前模块 | 当前职责 | 目标架构归属 | 差距 | Phase -1 结论 |
| --- | --- | --- | --- | --- |
| `src/app.rs` | 北向 gRPC、文件传输、应用启停、命令执行 | Protocol Layer + Control Service + File Transfer Service | 高 | 必须先收口文件路径和命令执行风险，Phase 0 拆出协议与核心服务 |
| `src/agent.rs` | 桌面截图 Agent、Linux grim fallback | 南向应用 / Sensor Reader | 中 | 平台截图能力需进入 PAL 或采集器插件，保留 loopback token 保护 |
| `src/platform.rs` | daemon、进程控制、系统关机/重启 | PAL Process/SystemControl | 高 | Phase 0 迁入 PAL trait，当前仅作为 legacy adapter |
| `src/state.rs` | 运行状态聚合、配置更新、文件浏览、MQTT 命令处理 | State Store + Config Manager + Telemetry Pipeline | 高 | 拆分状态聚合、持久化、配置写入和文件浏览边界 |
| `src/telemetry.rs` | telemetry schema、调度、数据结构 | Telemetry Pipeline | 中 | 已有 profile 雏形，缺 Collector/Processor/Exporter 分层 |
| `src/*_monitor.rs` | CPU/内存/磁盘/网络/进程采集 | Sensor Reader + Telemetry Collector | 中 | 采集器接口可保留，平台数据源迁入 PAL |
| `src/network_counters.rs` | Linux `/proc` 与 Windows IP helper 计数 | PAL NetworkInfo / NetStat | 高 | `#[cfg]` 集中迁入 PAL，业务只依赖统一快照 |
| `src/mqtt.rs` | MQTT telemetry/status/command | Protocol Layer MQTT Client | 中 | 与 gRPC 控制逻辑解耦，命令统一进入内部 command bus |
| `src/config.rs` | TOML 配置加载和保存 | Config Manager | 中 | Phase 0 保持兼容，新增 schema 版本和 SQLite 迁移设计 |
| `src/script_*`, `src/batch*`, `src/groups*`, `src/tags*` | 脚本、批量任务、分组、标签存储 | App Platform / State Store | 中 | 已用 SQLite 的模块作为 State Store 迁移样板 |
| `proto/*.proto` | 北向控制、文件传输、桌面 Agent proto | Protocol Contracts | 中 | Phase -1 不改 wire schema，Phase 0 开始拆分协议契约 |
| `packaging/*` | Linux/Windows 安装与服务配置 | Deployment / ServiceManager | 中 | 服务安装逻辑后续接入 PAL ServiceManager |

## 缺失组件

| 目标组件 | 当前状态 | 优先级 | 后续阶段 |
| --- | --- | --- | --- |
| PAL trait 契约 | 缺失，仅有零散 `#[cfg]` 和 `platform.rs` | P0 | Phase 0 |
| Security Center | 缺失，只有桌面 Agent token | P0 | Phase 0/1 |
| Audit Chain | 缺失 | P0 | Phase 1 |
| Upgrade Engine | 缺失 | P0 | Phase 2 设计，Phase 3 实现 |
| Config Manager | TOML 直写，未版本化 | P1 | Phase 0/2 |
| App Registry/Lifecycle | 仅有直接 start/close app | P1 | Phase 2 |
| Local Message Broker | 缺失 | P2 | Phase 2 |
| CapabilityProfile | 缺失 | P0 | Phase 0 |

## Phase -1 修复边界

Phase -1 只建立清洁基线，不进行 crate 拆分、不引入完整 PAL、不变更 proto wire schema。当前阶段的代码修复限定为：编译警告清零、明显 panic/unwrap 风险收敛、文件路径和命令执行紧急加固、三平台 CI 建立。
