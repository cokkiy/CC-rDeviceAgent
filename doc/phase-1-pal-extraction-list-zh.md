# Phase -1 PAL 抽离清单

## 平台条件编译入口

| 位置 | 当前平台逻辑 | 目标 PAL 契约 |
| --- | --- | --- |
| `src/platform.rs` | Unix daemon、Windows no-op、关机/重启命令 | `ProcessManager`、`SystemControl`、`ServiceManager` |
| `src/main.rs` | Unix signal、Windows service dispatcher | `ServiceManager`、`SignalSource` |
| `src/agent.rs` | Linux Wayland `grim` fallback | `SensorReader` / screen capture adapter |
| `src/network_counters.rs` | Linux `/proc/net/*`、Windows IP helper、其他平台 stub | `NetStat`、`NetworkInfo` |
| `src/state.rs` | Windows disk root 枚举、默认浏览根目录 | `FileSystem`、`DiskStat`、`PathResolver` |
| `src/app.rs` | Unix chmod executable mode | `FileSystem`、`PermissionManager` |
| `Cargo.toml` | Unix `nix`、Windows `windows-service/windows` 依赖 | PAL implementation crates |

## 系统调用与外部命令

| 位置 | 调用 | 风险 | 处理策略 |
| --- | --- | --- | --- |
| `src/app.rs` | app path `Command::new` | 可执行路径由请求输入控制 | Phase 1 接入命令白名单；Phase -1 保留启动功能但记录风险 |
| `src/app.rs` | raw shell command | 命令注入 | Phase -1 默认禁用，Phase 1 通过 RBAC + whitelist 恢复受控能力 |
| `src/agent.rs` | `grim -` | 外部依赖不可用 | Phase 0 放入 CapabilityProfile 探测和 fallback |
| `src/platform.rs` | `shutdown/reboot` | 平台差异与权限失败 | Phase 0 抽为 `SystemControl` |

## 文件系统边界

| 位置 | 当前行为 | 抽离目标 |
| --- | --- | --- |
| `src/app.rs` upload/download/rename/delete | 客户端传入路径直接操作 | `FileSystem` + `PathResolver` + managed root |
| `src/state.rs::file_info` | 任意路径浏览 | `PathResolver` + 权限策略 |
| `src/config.rs` | TOML 配置文件读写 | `ConfigRepository`，Phase 0 兼容 TOML |
| `src/*_store.rs` | SQLite 文件路径由调用方给定 | `StateStore` repository |

## Phase 0 抽离顺序

1. `PathResolver` / `FileSystem`：优先承接 Phase -1 的 managed root 策略。
2. `SystemControl` / `ProcessManager`：替换 `platform.rs` 和 app 启停路径。
3. `NetStat` / `DiskStat`：迁移 telemetry 采集平台差异。
4. `ServiceManager` / `SignalSource`：统一 Linux daemon、Windows service 和进程生命周期。
5. `CapabilityProfile`：记录 grim、cgroup、service manager、network counter 等能力。
