# Phase -1 迁移设计书 v1.0

## 迁移策略

采用 Strangler Pattern：保留当前单 crate 和 proto 合同，在 Phase 0 逐步把平台能力、状态存储、协议入口从 legacy 模块旁路抽离。每次抽离都先建立 trait/adapter，再迁移调用方，最后删除 legacy 入口。

## 模块拆分顺序

1. 建立 `pal-core` 契约和 legacy adapter，先覆盖文件路径、进程、系统控制、网络/磁盘统计。
2. 建立 `agent-store`，将现有 SQLite stores 统一到 repository 模式，TOML 配置保持兼容入口。
3. 拆出协议层：gRPC、MQTT 只负责接入，内部统一投递 command/event。
4. 拆出核心服务：Control、File Transfer、Telemetry、Config。
5. 在 Phase 2/3 引入 App Registry、Upgrade Engine、Security Center、Audit Chain。

## 数据迁移

| 当前数据 | Phase 0 方案 | 回滚策略 |
| --- | --- | --- |
| `CC-rDeviceAgent.toml` | 保持读取兼容，新增 schema version 后再同步到 SQLite | 保留 TOML 原文件，SQLite 迁移失败时继续使用 TOML |
| scripts/batches/groups/tags SQLite | 复用现表，新增 migrations 目录管理版本 | migration 事务失败则回滚 |
| telemetry profiles | 先保留 TOML，后续进入 `config_profiles` 表 | 保留 watch channel 行为 |
| upload/download progress | Phase 1/2 引入 transfer session 表 | 无持久状态时按旧行为重新传输 |

## 风险与回滚

| 风险 | 缓解 |
| --- | --- |
| 拆分过早导致大面积返工 | Phase -1 只做清洁基线，Phase 0 先 trait 后迁移 |
| Windows/macOS 编译暴露平台 API 差异 | CI 先保证编译，功能 stub 留到 PAL 实现 |
| 文件安全加固影响旧客户端绝对路径传输 | Phase -1 明确改为 managed root；需要旧行为必须经后续 RBAC/白名单恢复 |
| 禁用 raw shell command 影响远程运维 | Phase 1 通过命令白名单和审计链恢复受控执行 |

## Phase -1 出口标准

`cargo fmt --check`、`cargo check --all-targets`、`cargo test --all-targets` 通过；Linux clippy 以 `-D warnings` 通过；三平台 CI 已建立；差异矩阵、PAL 清单、状态盘点、迁移设计和 ADR 目录已提交。
