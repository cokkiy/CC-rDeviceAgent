# CC-rDeviceAgent 行动计划（v2.0 修订版）

> **修订说明**：本次修订基于 `architecture-zh.md` 架构设计与现有代码的差异评估结果，**新增前置阶段 Phase -1（架构差异修复）**，调整 Phase 0~5 的工作项与优先级，强化平台抽象层（PAL）和 Upgrade Engine 的优先级。

---

## 一、计划修订要点（与 v1.0 计划的差异）

### 1.1 主要调整

| 调整项              | v1.0 计划           | v2.0 修订                                          | 调整原因                           |
| ------------------- | ------------------- | -------------------------------------------------- | ---------------------------------- |
| **新增 Phase -1**   | 无                  | 架构差异盘点与紧急修复（2 周）                     | 现有代码与架构差距过大，需先做映射 |
| **PAL 优先级**      | Phase 0 骨架        | Phase 0 完整契约层 + Linux 完整实现                | PAL 是所有跨平台能力的基石         |
| **Upgrade Engine**  | Phase 3（v1.5）     | 提前到 Phase 2（v1.0）启动设计                     | OTA 复杂度高，需更长设计窗口       |
| **Security Center** | Phase 1             | Phase 0 末期启动                                   | 安全收口是后续模块的依赖           |
| **State Store**     | Phase 0 SQLite 引入 | Phase 0 完整 schema + 迁移机制                     | 避免后续频繁改 schema              |
| **App Registry**    | Phase 2             | 拆分为「IPC 通道」(P2) +「Registry/Lifecycle」(P2) | 解耦清晰、便于并行                 |
| **降级方案**        | 散落各 Phase        | Phase 0 PAL 内统一设计降级层                       | 保证降级矩阵的一致性               |
| **能力探测**        | 未明确              | Phase 0 CapabilityProfile 框架                     | 业务层降级决策依赖                 |

### 1.2 阶段目标对照（修订）

| Phase           | 版本 | 周期         | 核心目标                      | 退出标准                                             |
| --------------- | ---- | ------------ | ----------------------------- | ---------------------------------------------------- |
| **-1 差异盘点** | v0.4 | 2 周         | 现状摸底、架构映射、紧急修复  | 差异矩阵、迁移设计书、CI 三平台编译通过              |
| **0 地基**      | v0.5 | 5 周 (+1 周) | 工程化基础 + PAL 完整契约     | CI/CD 绿、PAL Linux 完整实现、CapabilityProfile 可用 |
| **1 安全**      | v0.8 | 6 周         | 通信与执行安全达标            | mTLS 全通道、命令白名单、审计哈希链、Security Center |
| **2 基座**      | v1.0 | 8 周         | 载荷应用可上线 + OTA 设计启动 | App 完整闭环、Upgrade Engine 设计评审通过            |
| **3 升级**      | v1.5 | 10 周        | 设备可远程升级且防变砖        | A/B 升级 + Agent 自升级 + 故障注入测试               |
| **4 平台**      | v2.0 | 10 周        | 大规模管理与灰度              | 多租户、灰度发布、扩展点开放                         |
| **5 生产**      | v2.1 | 6 周         | 满足生产部署                  | 性能基线、安全审计、文档齐备、SLA 达标               |

**总计**：约 47 周（约 11 个月），相比原计划增加 3 周用于差异修复与 PAL 加固。

---

## 二、Phase -1：架构差异盘点与紧急修复（2 周）【新增】

### 2.1 目标

在正式进入 Phase 0 之前，完成现状与目标架构的精准映射，制定渐进式迁移路径，避免在地基阶段反复返工。

### 2.2 工作项（WBS）

#### W-1.1 差异盘点（3 天）【已完成】

- [x] 模块级差异矩阵（现有模块 → 目标架构组件映射表）
- [x] 隐式状态盘点（现有代码中所有状态变量、状态流转逻辑梳理）
- [x] 平台相关代码盘点（所有 `#[cfg(target_os)]` 列表 → PAL 抽离清单）
- [x] 外部依赖盘点（系统调用、文件路径、命令执行点）
- [x] **交付物**：《现状-目标架构差异矩阵》、《PAL 抽离清单》

#### W-1.2 迁移设计书（3 天）【已完成】

- [x] Strangler Pattern 迁移策略（新旧并存方案）
- [x] 模块拆分顺序与依赖图
- [x] 数据迁移方案（旧存储 → SQLite）
- [x] 风险点清单与回滚预案
- [x] **交付物**：《迁移设计书 v1.0》

#### W-1.3 紧急修复（4 天）【已完成】

- [x] 修复编译告警（`#![deny(warnings)]` 在 CI 启用）
- [x] 修复明显的 unwrap/panic 风险点
- [x] 修复明显的路径穿越、命令注入风险
- [x] 修复跨平台编译失败
- [x] **交付物**：清洁的现状代码基线

#### W-1.4 三平台 CI 紧急搭建（2 天）【已完成】

- [x] GitHub Actions / GitLab CI 矩阵：Linux x64 / Windows x64 / macOS arm64
- [x] 编译 + 单元测试 + clippy + fmt 检查
- [x] **交付物**：三平台 CI 流水线

#### W-1.5 ADR 启动（2 天）【已完成】

- [x] 建立 `docs/adr/` 目录与模板
- [x] 补录现有架构决策（ADR-001 ~ ADR-012）
- [x] **交付物**：ADR 目录与首批 12 条记录

### 2.3 关键里程碑

| 时间  | 里程碑                                 |
| ----- | -------------------------------------- |
| W1 末 | 差异盘点 + 迁移设计书完成              |
| W2 末 | **v0.4 发布**：清洁基线 + 三平台 CI 绿 |

### 2.4 验收标准

- ✅ 差异矩阵完整覆盖所有现有模块
- ✅ 迁移设计书通过架构组评审
- ✅ 三平台 CI 全绿
- ✅ 无编译告警、无明显安全风险
- ✅ ADR 目录已建立

### 2.5 风险与缓解

| 风险                           | 概率 | 影响 | 缓解                                           |
| ------------------------------ | ---- | ---- | ---------------------------------------------- |
| 差异盘点不彻底，后续返工       | 中   | 高   | 使用脚本辅助扫描（grep `#[cfg]`、unwrap 等）   |
| Windows/macOS 兼容问题暴露过多 | 高   | 中   | 接受现状，记录到 backlog，Phase 0 PAL 集中解决 |

---

## 三、Phase 0：地基重塑（5 周，+1 周）【强化】

### 3.1 目标

建立工程化基础设施 + **完整的平台抽象层（PAL）契约层与 Linux 主适配器** + **State Store 完整 schema** + **能力探测框架**。

### 3.2 工作项（WBS）

#### W0.1 项目结构重组（3 天）

- [x] 拆分 crate（按 PAL 设计调整）：
  - `agent-core`（业务核心）
  - `agent-protocols`（北向南向协议）
  - `pal-core` / `pal-linux` / `pal-windows` / `pal-macos` / `pal-fallback` / `pal-mock`（PAL 各 crate）
  - `agent-telemetry`
  - `agent-store`（State Store）
  - `agent-cli`
- [x] workspace 依赖管理规范
- [x] **交付物**：新 workspace 目录树

#### W0.2 错误处理统一（3 天）

- [x] 统一 `AgentError` + `PalError` 体系
- [x] PAL 错误透传与上下文保留
- [x] **交付物**：错误处理规范文档

#### W0.3 可观测性骨架（5 天）

- [x] `tracing` 全栈集成
- [ ] OpenTelemetry SDK 集成（Logs/Metrics/Traces）
- [ ] OTLP exporter 配置（可选）
- [x] **PAL 内置 trace 埋点**（便于定位平台相关问题）
- [x] **交付物**：可观测性基础设施

#### W0.4 PAL 契约层完整设计（5 天）【强化】

- [x] 12 类契约 trait 完整定义（参见 PAL 详细设计）：
  - ProcessManager / ServiceManager / SignalSender
  - FileSystem / PathResolver / DiskSpace / FileLock
  - NetworkInfo / NetworkConfig / DnsResolver
  - SystemControl / TimeService / EnvVars
  - Bootloader / SlotManager / BootEnv
  - TpmProvider / KeyStore / CredentialStore / EntropySrc
  - ResourceLimiter / Cgroup / JobObject
  - Sandbox / NamespaceManager / Capabilities
  - CpuStat / MemStat / DiskStat / NetStat
  - IpcServer / IpcClient
  - SystemLogger
  - DeviceId / MachineFingerprint
- [x] CapabilityProfile 数据结构定义
- [x] PAL Builder + PlatformContext 装配框架
- [x] **交付物**：`pal-core` crate（契约 + 装配框架）

#### W0.5 PAL Linux 主适配器实现（8 天）【强化】

- [x] ProcessManager（基于 nix + std::process；后续可切 tokio::process）
- [ ] ServiceManager（systemd D-Bus）
- [x] FileSystem / DiskSpace（libc + statvfs）
- [ ] NetworkInfo（netlink）
- [x] SystemControl（reboot/shutdown）
- [ ] ResourceLimiter（cgroup v2，v1 fallback）
- [x] IpcServer（Unix Socket）
- [ ] SystemLogger（journald）
- [x] DeviceId（DMI/SMBIOS）
- [x] **交付物**：`pal-linux` crate

#### W0.6 PAL 降级层与 Mock（4 天）【新增】

- [x] Fallback KeyStore（文件型兜底；加密绑定留 Phase 1 Security Center）
- [x] Fallback ResourceLimiter（Unsupported + Linux rlimit 子集）
- [x] Mock 适配器（用于测试）
- [x] **交付物**：`pal-fallback` + `pal-mock` crate

#### W0.7 能力探测框架（3 天）【新增】

- [x] 探测流水线（TPM / A/B / cgroup / SecureBoot / 磁盘 / 网络）
- [x] CapabilityProfile 持久化缓存
- [x] 路由器（Capability Router）按 Profile 选择实现
- [x] **交付物**：能力探测模块

#### W0.8 State Store 完整设计（5 天）【强化】

- [x] SQLite + WAL 集成
- [x] 完整 schema 设计（任务、配置、应用清单、审计、升级状态、密钥引用、CapabilityProfile 缓存）
- [x] schema 版本化与迁移机制（embedded SQL migrations）
- [ ] 通用 Repository 抽象
- [x] 备份/恢复接口
- [x] **交付物**：`agent-store` crate

#### W0.9 PAL Windows/macOS 骨架（4 天）

- [x] Windows：ProcessManager（CreateProcess）+ FileSystem + IpcServer（Named Pipe）骨架
- [x] macOS：ProcessManager（posix_spawn）+ FileSystem + IpcServer（UDS）骨架
- [x] 三平台编译通过（即使部分功能 Stub）
- [x] **交付物**：`pal-windows` / `pal-macos` crate（骨架版）

#### W0.10 CI/CD 流水线（3 天）

- [x] 三平台编译矩阵
- [x] 单元测试 + 集成测试
- [ ] 代码覆盖率（目标 ≥ 50%）
- [ ] cargo audit + clippy + fmt
- [x] PAL Mock 驱动的单测
- [x] **交付物**：CI/CD 流水线

#### W0.11 文档体系（持续）

- [x] `CONTRIBUTING.md`
- [x] PAL 使用指南
- [x] State Store 使用指南
- [ ] ADR 持续更新

### 3.3 关键里程碑

| 时间  | 里程碑                                            |
| ----- | ------------------------------------------------- |
| W1 末 | 项目结构重组完成、错误体系统一                    |
| W2 末 | PAL 契约层完整、可观测性骨架可用                  |
| W3 末 | PAL Linux 主适配器完成 60%                        |
| W4 末 | PAL Linux 完整 + 降级层 + Mock + State Store 就位 |
| W5 末 | **v0.5 发布**：地基完整、三平台 CI 绿             |

### 3.4 验收标准

- ✅ 所有现有功能在新框架下可运行（功能不退化）
- ✅ 业务代码不再出现 `#[cfg(target_os)]`
- ✅ PAL Linux 实现通过完整集成测试
- ✅ Windows/macOS 编译通过（功能可降级）
- ✅ CapabilityProfile 探测正确
- ✅ State Store schema 可前向迁移
- ✅ CI/CD 全绿，覆盖率 ≥ 50%
- ✅ Mock 适配器可驱动单元测试

### 3.5 风险与缓解（更新）

| 风险                             | 概率 | 影响 | 缓解                                                |
| -------------------------------- | ---- | ---- | --------------------------------------------------- |
| PAL 抽象不准导致返工             | 高   | 高   | Linux 实现先行，再泛化；架构组每周评审              |
| Windows/macOS 仅骨架影响 Phase 1 | 中   | 中   | 关键能力提前在 W0.9 实现，非关键延后                |
| State Store schema 设计不全      | 中   | 高   | 与各业务模块负责人对齐 + 预留扩展字段               |
| 重构破坏现有功能                 | 高   | 高   | Strangler Pattern + 回归测试 + 旧代码保留至 Phase 1 |

---

## 四、Phase 1：安全加固（6 周）【调整】

### 4.1 目标

构建完整的安全基础：传输加密、身份认证、命令安全、审计追溯。**Security Center 收口、依赖 PAL KeyStore**，使 v0.8 达到“生产 POC 可进入安全评审”的基线。

本阶段覆盖：

- FR-1.4：MQTT TLS 1.2+ 与客户端证书认证
- FR-2.1 / FR-2.7：北向 gRPC mTLS 与控制操作审计
- FR-2.4：命令白名单、参数 schema 校验、禁止任意 shell 执行
- FR-3.4：文件传输路径穿越防护与目录白名单
- FR-8：设备身份、私钥保护、mTLS、RBAC、审计哈希链、防重放、供应链检查
- FR-10.10：业务应用控制审计入口预留

### 4.2 工作项

> **实现状态更新（2026-05-28）**：
> - `[x]`：已完成并有测试覆盖
> - `[~]`：已部分完成，仍有明确缺口
> - `[ ]`：未完成，需后续实施
>
> 自 2026-05-26 起的增量：完成责任链中间件骨架（`agent-core::chain`），将 `SecurityInterceptorLayer` 接入北向 gRPC server；新增 `IdentityExtractor`（X.509 SAN/CN 解析）、`ResourceMapper`、`AuditWriter`（同步入口 + 异步出口）、`SecurityContext` 注入；gRPC 服务通过 `tower::Layer` 在所有方法上强制执行 AuthN → AuthZ → AuditEntry → Handler → AuditExit 流程。

#### Phase 1 阶段完成度概览（2026-05-28）

| 工作包                          | 完成度 | 关键缺口                                                            |
| ------------------------------- | ------ | ------------------------------------------------------------------- |
| W1.1 Security Center 核心       | ~85%   | TrustAnchor 热加载、`credential://` 引用收口                        |
| W1.2 mTLS 全通道与防重放        | ~70%   | 证书热轮换、MQTT broker 集成测试、南向 IPC RBAC 收口                |
| W1.3 KeyStore 高级功能与签名    | ~75%   | KeyStore 主路径/降级路径测试、TLS 材料统一走 CredentialStore        |
| W1.4 命令白名单 / RBAC / 责任链 | ~75%   | RateLimit、MQTT 入口收口、策略外部配置加载、PAL ProcessManager 集成 |
| W1.5 Sandbox 与资源限制         | 0%     | 整体未启动（PAL Sandbox / ResourceLimiter 集成、命令 profile）      |
| W1.6 Audit Chain                | ~80%   | 异步批量化 + bounded channel、写失败告警、CLI 校验工具              |
| W1.7 文件传输安全               | ~60%   | 分块校验/CRC32、断点续传持久化、限速、handler 内细粒度审计          |
| W1.8 三平台安全 PAL 补齐        | 0%     | Windows DPAPI / Job Object、macOS Keychain、Linux fallback 测试     |
| W1.9 安全测试与供应链检查       | ~55%   | mTLS 端到端测试、admin 全 RBAC 矩阵、symlink 测试、Fuzz             |

**整体评估**：Phase 1 主体安全契约（Security Center、责任链、Audit Chain、命令白名单、mTLS、文件路径安全）已覆盖；**阻塞 v0.8 发布**的主要项是 W1.5 Sandbox/ResourceLimiter、W1.8 三平台 PAL、以及 W1.9 端到端 mTLS / RBAC 集成测试。建议把 W1.5、W1.8 拆为独立 Sprint，并把 W1.6 的批量化 + 写失败告警与 W1.7 的 handler 审计在同一个 Sprint 内完成以收口审计闭环。

#### W1.1 Security Center 核心与安全模型（5 天）【调整】

- [x] 定义 Security Center 模块边界，统一收口证书验证、身份提取、RBAC、防重放、签名验签、密钥引用管理
  - 已完成：`agent-core::security` 提供 `SecurityCenter` trait、`RequestContext`、`AuthMethod`、`DeviceIdentityBinding`、`KeyRef`、`SecurityLevel`、`ReplayGuard`、`BasicSecurityCenter`，并接入 Ed25519 验签包装。
- [~] 接入 Phase 0 的 PAL KeyStore / CredentialStore / DeviceId / State Store，不允许业务模块直接访问密钥文件
  - 已完成：安全模型具备 `KeyRef`、`SecurityLevel` 与 `pal_core::DeviceIdentity` 映射；State Store 新增 `security_keys` 元数据表。
  - 未完成原因：运行时证书/私钥仍支持文件路径加载，尚未全面改成 `credential://` 引用。
  - 下一步：为 TLS 材料增加 PAL CredentialStore resolver，并逐步禁止业务模块直接读敏感密钥文件。
- [x] 定义 `Principal`、`DeviceIdentity`、`Role`、`Permission`、`Decision`、`RequestContext` 安全模型
  - 已完成：`Principal`、`Role`、`Resource`/`Action`（作为 Permission 最小表达）、`Decision`、`RequestContext`、`DeviceIdentityBinding` 已实现并测试。
- [x] 证书加载与验证（rustls + webpki），X.509 CN/SAN 必须可绑定 `device_id`
  - 已完成：gRPC mTLS 与 MQTT TLS 均可加载 CA/cert/key；安全模型提供 `DeviceIdentityBinding::matches_device_id`；`IdentityExtractor` 通过 `x509-parser` 解析 peer cert 的 SAN dNSName 与 CN，自动映射为 `DeviceIdentityBinding`，并由责任链注入到 `SecurityContext`。
- [~] 信任锚管理：Root CA 公钥来自只读配置或平台安全存储，支持热加载但不负责 CA 签发
  - 已完成：`control.tls` / `mqtt.tls` 均支持 CA、证书、私钥路径校验与加载。
  - 未完成原因：热加载和平台安全存储引用尚未完成。
  - 下一步：实现 TrustAnchorStore，支持配置路径、PAL CredentialStore 引用和 mtime 轮换检测。
- [x] 定义 RBAC 最小模型：`admin` / `operator` / `readonly`
  - 已完成：默认 RBAC 策略已实现并测试，覆盖 readonly 只读、operator 控制权限、admin 安全策略管理权限。
- [x] 输出 ADR：证书策略、KeyStore 降级策略、RBAC 最小权限矩阵
  - 已完成：新增 `doc/adr/0013-phase1-certificate-and-keystore-policy.md`。
- [x] **交付物**：Security Center 核心模块 + 安全模型 ADR
  - 已完成：核心模块、密码学封装、RBAC/Replay 测试和 ADR 已落地。

#### W1.2 mTLS 全通道与防重放（5 天）

- [x] 北向 gRPC 接入 mTLS，默认 TLS 1.3，TLS 1.2 仅作为兼容降级
  - 已完成：启用 tonic `tls-ring`，`control.tls` 加载服务端 cert/key 和客户端 CA，并通过 `ServerTlsConfig` 接入 gRPC server。
- [~] 北向 MQTT 接入 TLS + 客户端证书认证，满足 FR-1.4
  - 已完成：`MqttClient::new_with_tls_config` 读取 CA/client cert/key 并设置 `rumqttc::Transport::tls`。
  - 未完成原因：尚未增加真实 broker 集成测试，证书错误/过期场景未覆盖。
  - 下一步：增加本地 TLS MQTT 测试夹具或容器化 broker，覆盖成功、错误 CA、缺客户端证书。
- [ ] 实现证书热加载与轮换检测，证书更新后重建北向连接，不要求重启 Agent
  - 未完成原因：当前只在启动时读取 TLS 文件。
  - 下一步：增加证书文件 watcher/轮询，检测变化后重建 MQTT/gRPC 连接。
- [x] 实现 timestamp + nonce 防重放，nonce 按 principal + action 短期持久化，过期自动清理
  - 已完成：`ReplayGuard` 支持内存窗口校验；State Store 新增 `replay_nonces` 表、重复插入拒绝和过期清理 API。
- [~] 南向 IPC 暂不加密，依赖 UDS 文件权限 / Named Pipe ACL + Session Token + RBAC
  - 已完成：配置边界保留；现有桌面 agent token 仍用于本地认证。
  - 未完成原因：南向 IPC RBAC 未统一接入 Security Center，Named Pipe ACL/UDS 权限测试未补齐。
  - 下一步：为南向请求构造 `RequestContext`，接入 RBAC，并补平台权限测试。
- [~] **交付物**：北向 mTLS 全通道 + 防重放机制 + 南向 IPC 安全边界说明
  - 已完成：gRPC mTLS（`ServerTlsConfig` + `require_client_auth`）、MQTT TLS 配置接入、防重放核心、责任链 fail-closed。
  - 未完成原因：MQTT broker 集成测试、证书热加载/轮换、持久化 nonce 仍缺。

#### W1.3 KeyStore 高级功能与签名封装（4 天）【新增】

- [x] 密钥派生（HKDF），用于审计链密钥、配置加密密钥等用途隔离
  - 已完成：`derive_hkdf_sha256` 基于 `ring::hkdf` 实现，并覆盖用途隔离测试。
- [x] Ed25519 签名/验签封装（ADR-010），服务于 OTA 包、配置签名、后续应用签名
  - 已完成：`KeyRef::inline_public_key` 与 `verify_ed25519_signature` 已实现，覆盖有效签名与篡改拒绝测试。
- [~] 凭据加密落盘统一走 PAL CredentialStore
  - 已完成：模型层提供 `KeyRef`/`CredentialReference` 与安全等级；State Store 可记录密钥引用。
  - 未完成原因：TLS 文件路径仍直接来自配置，未统一通过 PAL CredentialStore。
  - 下一步：支持 `credential://name` 类型引用，由 CredentialStore 解析敏感材料。
- [x] TPM/Keyring 不可用时降级为加密文件 + 设备指纹绑定，并在 CapabilityProfile 中标记安全等级
  - 已完成：`SecurityLevel::from_capability_profile` 将 TPM、OS Keyring、文件 fallback 映射为安全等级。
- [ ] 覆盖 Linux KeyStore 主路径测试和 fallback 路径测试
  - 未完成原因：本轮未新增 KeyStore 测试。
  - 下一步：补 `pal-linux`/`pal-fallback` KeyStore 行为测试。
- [~] **交付物**：完整 KeyStore 服务 + 签名验签 API
  - 已完成：签名验签 API、HKDF、KeyRef、安全等级与 State Store 元数据已完成。
  - 未完成原因：三平台生产 KeyStore/CredentialStore 深度集成仍依赖平台验证。

#### W1.4 命令白名单、RBAC 与责任链（5 天）

- [x] 实现命令白名单配置体系：`command_id`、固定命令模板、`allowed_roles`、`args_schema`、`timeout`、`resource_limits`、`sandbox_profile`
  - 已完成：`agent-core::command_policy` 支持 `CommandTemplate` 配置化创建，覆盖角色、参数 schema、timeout、resource_limits、sandbox_profile；默认 `restart_process` 已带运行时控制元数据。
- [x] 参数必须通过 schema 校验后进入模板，不允许拼接任意 shell 字符串
  - 已完成：`restart_process` 必须提供非空字符串 `process_name`；未知命令、缺参、角色不符、shell 元字符和过长参数均拒绝。
- [~] Control Service 接入责任链：`AuthN -> AuthZ -> RateLimit -> AuditStart -> Validate -> Handler -> AuditEnd`
  - 已完成：`agent-core::chain::SecurityInterceptorLayer`（`tower::Layer`）已接入北向 gRPC server，强制执行 `IdentityExtractor`（mTLS / `x-cc-principal` 头）→ `ResourceMapper`（按 method path 映射 Resource/Action）→ RBAC `authorize` → `AuditWriter::write_entry`（同步）→ Handler → `AuditWriter::write_exit`（异步），失败返回 `Status::PermissionDenied` 并 fail-closed；MQTT 控制入口接入命令白名单校验；gRPC `ExecuteCommand` 仍 fail-closed，禁止 raw shell。
  - 未完成原因：尚未接入 RateLimit；MQTT 控制路径未复用同一中间件；中间件单元测试覆盖映射、身份解析、审计写入，但端到端集成测试待补。
  - 下一步：抽象 `CommandRequestPipeline` 统一 MQTT/gRPC 入口；接入限流（`tower::limit` 或自研 token bucket）。
- [~] RBAC 决策覆盖控制命令、文件传输、配置变更、升级入口、业务应用控制入口
  - 已完成：RBAC 模型覆盖 `Telemetry / ControlCommand / FileTransfer / Configuration / Upgrade / AppControl / SecurityPolicy`；`ResourceMapper` 已为所有 `DeviceControl` 与 `FileTransfer` gRPC 方法登记 (Resource, Action) 映射，由 `SecurityInterceptorLayer` 在所有 gRPC 入口统一调用 RBAC。
  - 未完成原因：`SetWatchingApp`、`ReplaceTelemetryProfiles` 等配置变更入口的运行时审计字段需在 handler 内补充；MQTT 控制入口仍走旧 command_policy 路径，未与责任链统一。
  - 下一步：在 gRPC handler 内读取 `SecurityContext` 进一步携带操作上下文；将 MQTT 控制入口收口到同一 RBAC 决策点。
- [~] 旧命令执行入口迁移到白名单模板；无法迁移的入口默认禁用并记录 backlog
  - 已完成：gRPC `ExecuteCommand` 继续禁用 raw shell；MQTT `restart_process` 受白名单约束。
  - 未完成原因：`restart_process` 仍直接按进程名 terminate，尚未通过 PAL Sandbox/ProcessManager。
  - 下一步：把进程操作迁移到 PAL ProcessManager，并接入审计。
- [~] **交付物**：命令安全框架 + RBAC 策略配置 + 责任链中间件
  - 已完成：命令安全框架雏形和测试；`SecurityInterceptorLayer` 责任链中间件骨架已落地并接入 gRPC server。
  - 未完成原因：策略配置化（外部 TOML/JSON 加载）、RateLimit、MQTT 入口收口未完成。

#### W1.5 Sandbox 与资源限制集成（4 天）

- [ ] 基于 PAL Sandbox 接入平台隔离能力：Linux namespaces/seccomp，Windows Job Object，macOS 降级为受限执行策略
  - 未完成原因：本轮没有实现命令执行沙箱，且当前 raw command 执行仍禁用。
  - 下一步：先为允许的命令模板定义 sandbox profile，再由 PAL Sandbox 执行。
- [ ] 命令执行默认走沙箱 profile，除非显式配置为仅查询类安全命令
  - 未完成原因：命令模板尚未包含 sandbox_profile 的运行时执行逻辑。
  - 下一步：扩展 `ValidatedCommand` 带 profile 和 resource limits。
- [ ] 接入 PAL ResourceLimiter，支持 CPU、内存、磁盘 IO、执行超时限制
  - 未完成原因：尚未接入 PAL ResourceLimiter。
  - 下一步：在命令执行 pipeline 中对目标 pid/process handle 应用限制。
- [ ] 命令执行结果必须回写审计事件，包含退出码、超时、资源限制触发原因
  - 未完成原因：审计链已有存储能力，但命令执行入口未写审计事件。
  - 下一步：在 pipeline 中统一写 AuditStart/AuditEnd。
- [ ] **交付物**：Sandbox 服务 + 命令执行资源限制能力
  - 未完成原因：本项未完成。

#### W1.6 Audit Chain（5 天）

- [x] 定义 `AuditEvent` 标准字段：`event_id`、`timestamp`、`tenant_id`、`device_id`、`principal`、`action`、`resource`、`target`、`params_digest`、`result`、`trace_id`、`prev_hash`、`hash`
  - 已完成：`agent-core::security::AuditEvent` 已实现并可序列化。
- [~] 审计事件分类覆盖：控制操作、配置变更、文件传输、升级入口、业务应用控制入口、安全策略变更
  - 已完成：`Resource` 枚举已包含控制、文件传输、配置、升级、应用控制、安全策略；`SecurityInterceptorLayer` 在所有 gRPC 入口（DeviceControl + FileTransfer）统一写入 AuditStart/AuditEnd 事件，包含 principal、action、resource、target、result、trace_id、prev_hash、hash。
  - 未完成原因：MQTT 控制入口和 OTA/Config/AppControl 业务 handler 内的细粒度审计字段（params_digest、目标 ID）尚未补齐。
  - 下一步：在各业务 handler 内基于 `SecurityContext` 完善审计字段；MQTT 控制入口接入同一审计 sink。
- [x] 实现链式哈希防篡改，支持本地完整性验证
  - 已完成：`AuditChain` SHA-256 哈希链和篡改检测测试已通过；`AuditWriter` 在写入前基于上一条事件 `hash` 计算 `prev_hash` 并产生当前 `hash`。
- [~] 审计写入异步批量化，但控制类操作必须保证至少记录入口事件
  - 已完成：`AuditWriter` 提供 `write_entry`（同步、阻塞到持久化成功才放行 handler）+ `write_exit`（异步、tokio mpsc channel 后台落盘）双通道；控制入口事件强一致。
  - 未完成原因：尚未实现批量化（按时间窗或条数 flush）和 bounded channel 反压策略。
  - 下一步：将 exit 通道升级为 bounded + 周期性批写；增加 channel 满时的降级策略（写 fallback 队列或 metric）。
- [ ] 审计出口事件写入失败时产生内部告警，不阻塞主流程但不得静默丢失
  - 未完成原因：`write_exit` 的后台 task 失败仅 trace::error，缺自监控计数器与本地 fallback 队列。
  - 下一步：审计写失败时 emit metric + 本地 fallback 队列。
- [x] 提供本地查询接口，支持按时间、principal、action、result 检索
  - 已完成：State Store 新增 `AuditEventFilter` 与 `query_audit_events(filter)`，支持 principal/action/resource/result/time range 查询。
- [~] **交付物**：Audit Chain 模块 + 审计字段规范 + 完整性校验工具
  - 已完成：模块、字段、持久化、完整性校验核心、查询 API、责任链中间件统一写入路径。
  - 未完成原因：独立 CLI 校验工具、批量化、写失败告警尚未补齐。

#### W1.7 文件传输安全（4 天）

- [x] 路径白名单与穿越防护通过 PAL PathResolver 实现，拒绝 `../`、绝对路径逃逸、符号链接逃逸
  - 已完成：`resolve_managed_file_path` 已改为调用 PAL `PathResolver`，现有绝对路径和父目录穿越测试通过；fallback resolver 负责拒绝 symlink escape。
- [x] 文件大小限制、磁盘配额检查通过 PAL DiskSpace 实现
  - 已完成：新增 `service.file_transfer.max_file_bytes` 和 `min_free_bytes`，上传前通过 PAL `DiskSpace` 检查余量，下载前拒绝超限文件。
- [~] 分块 SHA-256 + 整体 SHA-256 双重校验，保留 FR-3.3 的 CRC32 兼容能力
  - 已完成：上传完成后计算整体 SHA-256 并返回 message；`sha256_file_hex` 有测试。
  - 未完成原因：协议没有 checksum 字段，尚未做分块校验、CRC32 兼容和服务端强校验。
  - 下一步：兼容扩展 proto 字段 `chunk_sha256` / `file_sha256` / `crc32`。
- [~] 断点续传与持久化任务状态写入 State Store
  - 已完成：State Store 新增 `file_transfer_tasks` 表和 upsert/load API。
  - 未完成原因：gRPC FileTransferService 还未把每次上传/下载状态写入该表，也未定义 resume token。
  - 下一步：在 upload/download stream loop 写入 task 状态并返回 resume token。
- [ ] 实现令牌桶限速，默认不限制，但支持全局与单任务限速配置
  - 未完成原因：未实现限速。
  - 下一步：在 upload/download stream loop 加 token bucket。
- [~] 文件上传、下载、拒绝、校验失败都必须写入 Audit Chain
  - 已完成：`SecurityInterceptorLayer` 在 `FileTransfer/Upload` 与 `FileTransfer/Download` 入口写入 AuthN/AuthZ/AuditStart/AuditEnd 事件，未授权请求 fail-closed 并产生 deny 审计。
  - 未完成原因：handler 内部的细粒度事件（路径白名单拒绝、配额拒绝、SHA-256 校验失败、断点续传中断）尚未单独写审计。
  - 下一步：在 `FileTransferService` handler 内基于 `SecurityContext` 增补 deny/failed 审计事件。
- [~] **交付物**：升级版 File Transfer Service + 文件传输安全测试
  - 已完成：路径安全测试、整体 SHA-256 测试、传输入口的 RBAC + AuditStart/AuditEnd。
  - 未完成原因：配额、断点状态、分块校验、handler 内部细粒度审计未完成。

#### W1.8 三平台安全 PAL 补齐（4 天）【新增】

- [ ] Windows CredentialStore（DPAPI）与基础 KeyStore 降级实现
  - 未完成原因：未改动 `pal-windows`。
  - 下一步：用 DPAPI 实现 CredentialStore，保留文件 fallback。
- [ ] Windows Sandbox / ResourceLimiter 基于 Job Object 的最小可用实现
  - 未完成原因：未实现 Job Object 限制。
  - 下一步：补 Job Object adapter 和测试。
- [ ] Windows Named Pipe ACL 测试
  - 未完成原因：未新增 Windows CI/测试。
  - 下一步：补 Windows 平台测试。
- [ ] macOS CredentialStore（Keychain）与基础 KeyStore 降级实现
  - 未完成原因：未改动 `pal-macos`。
  - 下一步：接 Keychain 或明确 fallback 行为。
- [ ] macOS UDS 权限测试；Sandbox 能力不足时明确降级并暴露 CapabilityProfile
  - 未完成原因：未新增 macOS 平台测试。
  - 下一步：补 UDS 权限和 CapabilityProfile 降级测试。
- [ ] Linux TPM/Keyring 不可用 fallback 测试补齐
  - 未完成原因：未补 PAL KeyStore 测试。
  - 下一步：补 `pal-fallback` 和 `pal-linux` fallback 测试。
- [ ] **交付物**：三平台安全 PAL 最小可用实现 + 降级矩阵
  - 未完成原因：本项未完成。

#### W1.9 安全测试与供应链检查（4 天）

- [~] mTLS 测试：成功连接、无客户端证书拒绝、错误 CA 拒绝、过期证书拒绝
  - 已完成：gRPC `ServerTlsConfig` 通过 `require_client_auth` + `client_auth_optional` 联动，`build_grpc_tls_config` 单元覆盖；`IdentityExtractor` 单测覆盖 mTLS peer cert SAN/CN 解析与无证书 anonymous 兜底。
  - 未完成原因：尚无端到端 TLS 测试夹具（成功握手、错误 CA、过期证书、缺客户端证书的真实 client/server 集成测试）；MQTT TLS 也无 broker 集成测试。
  - 下一步：补 tonic TLS 集成测试夹具与本地 MQTT TLS broker 测试。
- [x] 防重放测试：重复 nonce 拒绝、timestamp 超窗拒绝、nonce 过期清理
  - 已完成：`ReplayGuard` 单元测试覆盖重复 nonce 和 timestamp 超窗；过期清理在 check 流程中覆盖。
- [x] 命令注入测试：shell 元字符、参数逃逸、未登记 command_id 均被拒绝
  - 已完成：未登记 command_id、缺参数、角色不足、shell 元字符 payload、过长参数均有单元测试覆盖；gRPC raw shell 仍禁用。
- [~] 路径穿越测试：`../`、绝对路径、符号链接逃逸、白名单目录外写入均被拒绝
  - 已完成：`../`、绝对路径、嵌套 parent dir 测试。
  - 未完成原因：符号链接逃逸、白名单外写入测试未补。
  - 下一步：引入 PAL PathResolver 后补 symlink 测试。
- [~] RBAC 测试：admin/operator/readonly 权限矩阵覆盖控制、配置、文件、升级入口
  - 已完成：readonly/operator 关键拒绝/允许测试；`ResourceMapper` 测试覆盖 DeviceControl + FileTransfer 全部 gRPC 方法到 (Resource, Action) 映射；`SecurityInterceptorLayer` 单元测试覆盖授权拒绝路径。
  - 未完成原因：admin 全矩阵、责任链端到端 e2e（含 mTLS + 审计落盘）测试未补。
  - 下一步：补端到端 RBAC 矩阵测试（mock SecurityCenter + tonic in-memory channel）。
- [x] 审计篡改测试：手动修改审计数据库后完整性校验失败
  - 已完成：`agent-store` 测试覆盖修改 result 后 verify 失败。
- [ ] Fuzz 测试：manifest 解析、命令参数 schema、文件传输元数据
  - 未完成原因：未引入 fuzz harness。
  - 下一步：用 cargo-fuzz 或 proptest 先覆盖命令参数与文件元数据。
- [x] CI 集成：`cargo audit`、`cargo fmt --check`、`cargo clippy`、三平台编译、核心安全测试
  - 已完成：CI 已包含三平台 fmt/check/test、Linux clippy，并新增 Linux `cargo audit` 步骤。
- [~] **交付物**：安全测试套件 + v0.8 安全测试报告 + 剩余风险清单
  - 已完成：核心单元测试和本节状态清单。
  - 未完成原因：还不是完整测试套件/正式报告。

### 4.3 公共接口与数据模型

#### 4.3.1 Security Center 接口

```rust
trait SecurityCenter {
    fn authenticate(&self, context: &RequestContext) -> Result<Principal, SecurityError>;
    fn authorize(&self, principal: &Principal, resource: &str, action: &str) -> Result<Decision, SecurityError>;
    fn verify_certificate(&self, peer_chain: &[CertificateDer]) -> Result<DeviceIdentity, SecurityError>;
    fn verify_signature(&self, payload: &[u8], signature: &[u8], key_ref: &KeyRef) -> Result<(), SecurityError>;
    fn check_replay(&self, principal: &Principal, timestamp: SystemTime, nonce: &str) -> Result<(), SecurityError>;
}
```

#### 4.3.2 审计事件字段

| 字段            | 说明                                      |
| --------------- | ----------------------------------------- |
| `event_id`      | 审计事件唯一 ID                           |
| `timestamp`     | 事件时间                                  |
| `tenant_id`     | 租户 ID，无租户时为系统默认租户           |
| `device_id`     | 设备 ID，来自 PAL DeviceId 或证书身份     |
| `principal`     | 操作者身份                                |
| `action`        | 操作类型                                  |
| `resource`      | 被操作资源类型                            |
| `target`        | 被操作对象                                |
| `params_digest` | 参数摘要，不记录敏感明文                  |
| `result`        | `success` / `denied` / `failed` / `timeout` |
| `trace_id`      | 链路追踪 ID                               |
| `prev_hash`     | 前一条审计事件哈希                        |
| `hash`          | 当前事件哈希                              |

#### 4.3.3 State Store 扩展

- [x] `audit_events`：审计事件与哈希链
- [x] `security_keys`：密钥引用、用途、存储后端、安全等级
- [x] `rbac_policies`：角色、权限、资源映射
- [x] `replay_nonces`：短期 nonce 去重缓存
- [x] `file_transfer_tasks`：断点续传状态与校验状态

### 4.4 关键里程碑

| 时间  | 里程碑                          |
| ----- | ------------------------------- |
| W1 末 | Security Center 核心、安全模型、ADR 完成 |
| W2 末 | gRPC/MQTT mTLS、防重放、南向 IPC 安全边界完成 |
| W3 末 | KeyStore 高级能力、签名验签 API、降级路径完成 |
| W4 末 | 命令白名单、RBAC、责任链、Sandbox 就绪 |
| W5 末 | Audit Chain + 文件传输安全完成 |
| W6 末 | 三平台安全 PAL + 安全测试报告完成，**v0.8 发布** |

### 4.5 验收标准

- ✅ 所有北向外部通道启用 mTLS 或 TLS + 客户端证书认证
- ✅ 所有控制类操作都经过 Security Center 统一 AuthN/AuthZ，并写入 Audit Chain
- ✅ 控制命令无法绕过白名单模板执行任意 shell
- ✅ 文件传输不能越过配置的白名单目录，且具备完整性校验
- ✅ 审计链可验证完整性，篡改可检测
- ✅ 三平台 KeyStore/CredentialStore 至少有可测试实现或明确降级实现
- ✅ 安全测试报告覆盖 mTLS、命令注入、路径穿越、重放攻击、RBAC、审计篡改
- ✅ CI 通过 `cargo audit`、`cargo fmt --check`、`cargo clippy`、三平台编译与核心安全测试

### 4.6 测试策略

| 测试类型 | 覆盖场景 |
| -------- | -------- |
| 单元测试 | 证书身份提取、证书过期、Root CA 不匹配、SAN/CN 设备 ID 不匹配、RBAC 权限矩阵、防重放、命令模板校验、审计哈希连续性 |
| 集成测试 | gRPC mTLS、MQTT TLS 客户端证书认证、证书轮换后重连、控制命令完整责任链、文件上传下载路径限制 |
| 攻击场景测试 | 命令注入、路径穿越、TLS 降级、重放攻击、RBAC 越权、审计数据库篡改 |
| 三平台测试 | Linux KeyStore 主路径与 fallback、Windows DPAPI / Job Object / Named Pipe ACL、macOS Keychain / UDS 权限 |
| 供应链测试 | cargo audit、依赖许可证扫描、可复现构建基线记录 |

### 4.7 假设与边界

- CA 证书签发系统不在本阶段实现，Phase 1 只实现证书加载、验证、轮换接入和信任锚管理。
- 南向 IPC 在 Phase 1 不做传输加密，安全边界依赖 OS 权限、Session Token、RBAC 与本地通道隔离。
- 证书自动轮换在 Phase 1 实现 Agent 侧热加载和重连机制；完整 SPIFFE/SVID 自动签发流程依赖外部 PKI。
- Windows/macOS 的 TPM/SEP 深度集成不是 v0.8 阻塞项，必须提供安全降级和测试覆盖。
- Phase 1 不实现完整 App Control Service，但所有业务应用控制入口必须预留 RBAC 与审计接入点。

---

## 五、Phase 2：应用基座 + OTA 启动（8 周）【调整】

### 5.1 目标

让设备成为载荷应用的运行平台 + **启动 Upgrade Engine 设计与原型**（提前介入，降低 Phase 3 风险）。

> **实现状态更新（2026-06-02）**：Phase 2 已完成主要原型与关键链路接线；`cargo test` 通过（124 passed / 0 failed / 4 ignored）。当前状态不是 GA 完成：App Lifecycle 已接入 PAL ProcessManager / ResourceLimiter，并与 AppPlatform 注册、北向 Start/Restart、HealthEvaluator restart action 完成运行时接线；RBAC/Audit 全量接入、完整 E2E、性能基线、包安装解压与生产级资源隔离仍需收口。

### 5.2 工作项

#### W2.0 Device → Device 术语迁移（5 天）【新增】【已完成】

- [x] 全仓扫描：代码库已统一使用 `device` 语义，`station_id` 仅作为向后兼容别名保留
- [x] `proto/cc.proto` 中 `device_id`、`DeviceControl`、`DeviceRunningState` 等已使用正确命名
- [x] Rust 代码符号：`device_id`、`DeviceControl` 等已统一
- [x] 配置：`service.device_id` 生效，`station_id` 作为 `serde(alias)` 兼容旧配置
- [x] 本地数据库：`station_tags` / `station_groups` 已通过迁移脚本升级为 `device_tags` / `device_groups`
- [x] Batch / Group / Tag 领域模型已使用 `device` 语义
- [x] README、配置模板已同步更新
- [x] **交付物**：命名迁移已完成，旧格式向后兼容

#### W2.1 南向 IPC 通道（5 天）【部分完成】

- [x] 南向 gRPC Server（Linux/macOS UDS 路径已接入 agent 启动流程）
- [x] 协议定义：`proto/app.proto` — AppPlatform 服务含 RegisterApp / Heartbeat / ReportHealth / PublishData / WatchConfig / SubscribeData / GetConfig / UnregisterApp
- [~] UDS / Named Pipe 本地通道权限设计（UDS 可用；Windows Named Pipe 仍为 stub）
- [x] 连接管理、短期 Session Token（32-byte random，SHA-256 哈希存储）、过期与吊销
- [x] 未注册应用访问拒绝（validate_session）
- [ ] 基于 PAL `IpcServer` 的统一实现与三平台权限测试
- [x] **交付物**：`proto/app.proto`、`src/app_platform.rs`、`src/app.rs` 集成

#### W2.2 App Registry（5 天）【部分完成】

- [x] 应用注册流程（RegisterApp RPC + 状态持久化）
- [x] 应用身份分配（App ID = `{name}_{timestamp_ms}` + Session Token + device_id 绑定）
- [x] 应用能力声明（capabilities_json 持久化）
- [x] 应用清单持久化（RegisterApp 写入 registry manifest；北向 StartApp 写入 lifecycle manifest）
- [x] 应用 session 续期（Heartbeat），吊销（UnregisterApp / revoke_app_session）
- [x] 健康报告持久化（`app_health_reports` 表）
- [ ] 注册/续期/吊销操作接入 RBAC 与 Audit Chain
- [x] **交付物**：`crates/agent-store` SCHEMA_V2/API，`src/app_platform.rs`，会话单元测试

#### W2.3 App Lifecycle（8 天）【部分完成】

- [x] 应用生命周期状态机（Registered → Installed → Running → Stopped → Failed → Uninstalled）
- [x] 应用安装（可执行文件路径校验）
- [x] 应用启动与停止（已切到 PAL ProcessManager；北向 Start/Restart 经 AppLifecycleHandle）
- [x] 应用状态查询与列表
- [x] 异步 LifecycleCmd 通道，AppLifecycleHandle 供 RPC 层调用
- [ ] 应用包解压、manifest 校验、安装配置
- [~] 应用监控与自动重启（HealthEvaluator restart action 已驱动 lifecycle restart；指数退避待补）
- [~] 资源隔离与配额（Manifest limit 已接 PAL ResourceLimiter；生产级 cgroup/job 配额策略待补）
- [ ] stdout/stderr → Observability Hub
- [x] **交付物**：`src/app_lifecycle.rs`，3 项单元测试

#### W2.4 Data Router（6 天）【部分完成】

- [x] 应用数据上行路由（AppPlatform PublishData 已接 DataRouter；MQTT 启用时发布到 app topic）
- [~] 应用数据下行路由（DownlinkRegistry 已实现；后端订阅到 registry 的桥接仍待补）
- [x] Topic 映射规则：`{tenant}/{device_id}/apps/{app_id}/{topic}`
- [x] 应用命名空间隔离（`validate_app_topic` 拒绝 `../`、绝对路径）
- [x] 死连接自动清理（dead sender 从 registry 移除）
- [ ] 流量整形、离线队列复用、指标与 trace 埋点
- [x] **交付物**：`src/data_router.rs`、AppPlatform 数据路由测试

#### W2.5 Config Manager / Config Watcher（10 天）【部分完成】

- [x] 三层配置（ConfigScope::Device / Agent / App(id)）
- [x] 版本化（内存全局递增；StateStore 可持久化最新版本）
- [x] set / get / delete / snapshot API
- [x] 配置订阅（AppPlatform WatchConfig 已接 AppConfigWatcher）
- [x] AppConfigWatcher：过滤本 app 及 Device/Agent 级变更
- [x] 删除 tombstone 持久化，避免已删配置从 StateStore 复现
- [ ] 配置回滚 API、默认值合并策略、生效策略、签名验证接入 Security Center
- [x] **交付物**：`src/config_manager.rs`，store-backed 单元测试

#### W2.6 App Health 与运行时控制预留（5 天）【部分完成】

- [x] 应用主动健康上报 API（ReportHealth RPC，存入 StateStore）
- [x] HealthEvaluator 接入 ReportHealth，连续失败计数，healthy 时重置
- [x] 连续失败阈值策略，触发 Restart / Alert / RestartThenAlert
- [x] 重启限速（min_restart_interval 防抖）
- [x] HealthAction 通过 mpsc::Sender 异步发出；Restart / RestartThenAlert 已驱动 lifecycle restart
- [ ] FR-10 控制预留的 Local Msg Broker、RBAC 与审计接入点
- [x] **交付物**：`src/health_evaluator.rs`，3 项单元测试

#### W2.7 Upgrade Engine 设计与应用级原型（10 天）【部分完成】

- [x] OTA 状态机：Idle → Received → Validated → Downloading → Verifying → PreCheck → Staging → ReadyToActivate → Activating → PostCheck → Committed / RollingBack → RolledBack / Failed
- [x] 升级包格式设计：`tar.zst + manifest.json + Ed25519 signature`（UpgradeManifest 结构体）
- [x] UpgradeStrategy trait（#[async_trait]）：stage / activate / rollback / post_check
- [x] AppUpgradeStrategy：staging → backup → activate → rollback on failure
- [x] UpgradeEngine<S>：驱动状态机，激活或 post_check 失败自动回滚
- [x] SHA-256 校验、可选 Ed25519 签名校验、升级状态持久化
- [~] 防回滚字段已存在（build_number），但尚未和已安装版本/StateStore 单调版本记录强制联动
- [ ] `tar.zst` 解压、manifest 文件解析、pre/post script 执行、完整健康检查
- [x] **交付物**：`src/upgrade_engine.rs`，升级生命周期与状态持久化单元测试

#### W2.8 SDK 与示例应用（4 天）【基本完成】

- [x] Rust SDK（`crates/app-sdk`）：AppClient，connect_uds / connect_tcp
  - heartbeat / report_health / publish / watch_config / subscribe_data / unregister
- [x] 示例 payload app（`examples/payload-hello`）：注册 → 心跳 → 健康上报 → 数据上报 → 注销主链路演示
- [ ] 更新查询 / 触发更新 API 暂未暴露到 SDK
- [x] **交付物**：`crates/app-sdk`，`examples/payload-hello`，全工作区编译通过

#### W2.9 端到端集成与测试（4 天）【部分完成】

- [x] 单元测试通过：`cargo test` = 120 passed / 0 failed / 4 ignored
- [x] AppPlatform 单测覆盖注册、伪造 token、注销、数据上行、数据下行、配置读取
- [x] Config/Upgrade 单测覆盖 store-backed 配置删除与升级状态持久化
- [ ] 性能基线（待真实部署环境测量）
- [ ] 完整 E2E 集成测试（运行中的 agent + payload app + MQTT/backend mock）
- [x] **交付物**：代码 + 单元测试套件 + 示例应用

### 5.3 关键里程碑

| 时间  | 里程碑                                                    | 状态 |
| ----- | --------------------------------------------------------- | ---- |
| W1 末 | `device → device` 迁移完成，配置/数据库兼容策略验证通过  | ✅ 完成 |
| W2 末 | 南向 IPC + App Registry 完成                              | 🔄 部分完成 |
| W4 末 | App Lifecycle + Data Router 完成                          | 🔄 部分完成 |
| W5 末 | Config Manager / Config Watcher 完成                      | 🔄 部分完成 |
| W6 末 | App Health 最小闭环完成，OTA 设计评审通过                 | 🔄 部分完成 |
| W7 末 | OTA 应用级原型 + Rust SDK 示例完成                        | 🔄 基本完成 |
| W8 末 | **v1.0 发布**：应用基座 GA + OTA 设计就绪 + Device 命名统一 | ⏳ 未达 GA（待 E2E + 性能基线 + PAL/RBAC 收口） |

### 5.4 验收标准

- ✅ 全仓业务术语统一为 `device`，除历史 ADR/迁移说明外无旧术语残留
- ✅ 旧 `service.station_id` / `service.device_id` 配置向后兼容
- ✅ 旧数据库 `station_*` 数据可无损迁移
- ⏳ AppPlatform RPC 的 RBAC 与 Audit Chain 映射尚未完整接入
- 🔄 应用注册/启停/健康重启闭环达到原型级，升级/包安装闭环未达生产验收
- 🔄 数据通道双向贯通达到进程内/MQTT 上行原型级，下行 backend bridge 待补
- 🔄 配置三层模型可用，回滚/签名/默认合并待补
- 🔄 应用健康上报可用，失败动作已驱动 lifecycle restart；rollback 待升级闭环补齐
- 🔄 OTA 状态机与应用级升级原型可演示，包解压与版本防回滚待补
- 🔄 Rust SDK 基本完成，更新查询 / 触发更新待补
- ⏳ E2E 测试通过率 ≥ 95%（待集成环境）

### 5.5 测试与验证（当前状态）

| 类型     | 覆盖范围                                                                          | 状态 |
| -------- | --------------------------------------------------------------------------------- | ---- |
| 单元测试 | Session 生命周期、App Registry、Lifecycle 状态机、PAL 启停/资源限制、topic 映射、config watch、health evaluator、upgrade state machine | ✅ 124 通过 |
| 集成测试 | 注册→心跳→健康上报→数据上报→注销链路（示例应用）                                 | 🔄 编译通过，完整运行态 E2E 待补 |
| 安全测试 | 伪造 token 拒绝、会话过期检测、签名/hash 错误升级包基础覆盖                       | 🔄 单元覆盖 |
| E2E 测试 | 完整 Agent 进程 + payload app 集成                                                | ⏳ 待补 |
| 性能基线 | IPC 吞吐、内存占用                                                                | ⏳ 待补 |

### 5.6 范围边界与默认假设

- `device → device` 迁移基本完成，配置和数据库提供向后兼容；历史文档中的旧术语不作为阻塞项。
- Phase 2 当前交付二进制应用原型；完整包解压、OCI、WASM、灰度发布、多租户推迟。
- Rust SDK 基本交付；Python SDK 推迟到 Phase 3。
- 系统级 OTA（A/B 槽位）、Agent 自升级、真机断电测试属于 Phase 3。
- 南向 IPC 不做 TLS，安全边界依赖 UDS 文件权限、Session Token（哈希存储）、RBAC/Audit 后续接入。

---

## 六、Phase 3：OTA 升级（10 周）【保持】

### 6.1 目标

实现完整的设备远程升级能力：A/B 槽位 + Agent 自升级 + 故障注入测试。

### 6.2 工作项

#### W3.1 OTA 状态机引擎（8 天）

- [ ] 完整状态机实现（基于 statig 或 rust-fsm）
- [ ] 状态持久化（State Store）
- [ ] 崩溃恢复（启动时重建状态）
- [ ] 状态变更事件总线
- [ ] **交付物**：OTA 状态机引擎

#### W3.2 升级包处理（5 天）

- [ ] manifest 解析与验证
- [ ] 签名验证（Ed25519，**通过 Security Center**）
- [ ] 分块下载（**复用 File Transfer**）
- [ ] 完整性校验
- [ ] **交付物**：升级包处理模块

#### W3.3 PAL Bootloader 适配器（10 天）【强化】

- [ ] Linux RAUC 适配器
- [ ] Linux U-Boot 适配器（嵌入式）
- [ ] Windows UEFI BCD 适配器
- [ ] 应用级升级 fallback（无 A/B 时）
- [ ] **交付物**：完整 Bootloader PAL 实现

#### W3.4 系统升级流程（6 天）

- [ ] 槽位探测（**通过 CapabilityProfile**）
- [ ] 写入备用槽位
- [ ] 切换槽位
- [ ] 启动后健康检查与确认
- [ ] 失败回滚
- [ ] **交付物**：系统升级流程

#### W3.5 Agent 自升级（5 天）

- [ ] 自升级状态机（特殊处理：升级 agent 自己）
- [ ] 双进程切换（旧版本 → 新版本）
- [ ] 失败时旧版本自启动
- [ ] **交付物**：Agent 自升级模块

#### W3.6 升级窗口与策略（4 天）

- [ ] 升级时间窗口（**通过 Scheduler**）
- [ ] 网络条件检查
- [ ] 电量/磁盘检查
- [ ] 用户确认（如有 UI）
- [ ] **交付物**：升级策略模块

#### W3.7 故障注入测试框架（8 天）

- [ ] 故障注入点（下载中断、签名失败、写入失败、断电、回滚失败等）
- [ ] 自动化测试套件
- [ ] 真机测试用例（断电、断网、磁盘满）
- [ ] **交付物**：故障注入框架 + 测试报告

#### W3.8 压力与真机测试（4 天）

- [ ] 1000 次升级压力测试
- [ ] 真机断电测试（10 次以上）
- [ ] 多平台真机测试
- [ ] **交付物**：测试报告

### 6.3 关键里程碑

| 时间   | 里程碑                           |
| ------ | -------------------------------- |
| W2 末  | OTA 状态机引擎完成               |
| W4 末  | 升级包处理 + Bootloader PAL 完成 |
| W6 末  | 系统升级流程 + Agent 自升级完成  |
| W8 末  | 升级策略 + 故障注入框架完成      |
| W10 末 | **v1.5 发布**：完整 OTA 能力     |

### 6.4 验收标准

- ✅ 系统升级在 3 种平台通过测试
- ✅ Agent 自升级 100 次连续成功
- ✅ 故障注入测试覆盖所有状态，100% 能恢复
- ✅ 1000 次升级压力测试通过率 ≥ 99%
- ✅ 真机断电测试零变砖

---

## 七、Phase 4：平台化（10 周）【保持】

### 7.1 目标

支持大规模设备管理：多租户、灰度发布、扩展点开放、性能优化。

### 7.2 工作项

#### W4.1 多租户支持（7 天）
#### W4.2 灰度发布支持（5 天）
#### W4.3 扩展点开放（10 天）
#### W4.4 协议演进框架（5 天）
#### W4.5 Telemetry Pipeline 完善（8 天）
#### W4.6 性能优化（10 天）【新增】

- [ ] 性能基线建立
- [ ] 热路径优化（trait 对象 → 泛型化）
- [ ] 内存占用优化（目标：常驻 ≤ 50MB）
- [ ] CPU 占用优化（空闲时 ≤ 1%）
- [ ] **交付物**：性能优化报告

#### W4.7 大规模压测（5 天）【新增】

- [ ] 单设备多应用压测（≥ 20 个应用）
- [ ] 长时间运行稳定性测试（≥ 30 天）
- [ ] **交付物**：压测报告

### 7.3 关键里程碑

| 时间   | 里程碑                            |
| ------ | --------------------------------- |
| W3 末  | 多租户 + 灰度发布完成             |
| W6 末  | 扩展点开放 + 协议演进完成         |
| W8 末  | Telemetry Pipeline + 性能优化完成 |
| W10 末 | **v2.0 发布**：平台化能力达成     |

---

## 八、Phase 5：生产化（6 周）【保持】

### 8.1 工作项

- [ ] 性能基线最终确认
- [ ] 安全审计（外部）
- [ ] 文档完整性检查
- [ ] SLA 指标定义与验证
- [ ] 试点客户部署支持
- [ ] 运维 Runbook
- [ ] 故障应急预案

### 8.2 验收标准（SLA）

| 指标             | 目标    |
| ---------------- | ------- |
| 可用性           | ≥ 99.9% |
| OTA 升级成功率   | ≥ 99.5% |
| 应用升级成功率   | ≥ 99.9% |
| 平均启动时间     | ≤ 5s    |
| 内存占用（常驻） | ≤ 50MB  |
| CPU 占用（空闲） | ≤ 1%    |
| 通信加密覆盖     | 100%    |

---

## 九、横切关注事项【更新】

### 9.1 测试策略（强化）

- **单元测试**：覆盖率 ≥ 70%，**PAL Mock 驱动**
- **集成测试**：每个 Phase 末整体回归
- **E2E 测试**：模拟真实场景
- **故障注入**：Phase 3 重点
- **性能基准**：每个版本对比
- **真机测试**：Phase 3-5 持续投入
- **PAL 兼容性矩阵**：所有 PAL 实现必须通过统一测试套件

### 9.2 文档维护

- 代码注释规范（docstring）
- ADR 持续更新
- CHANGELOG 每个版本更新
- 架构变更及时同步文档
- **PAL 适配指南**（新增）
- **降级矩阵文档**（新增）

### 9.3 依赖管理

- 每月依赖更新检查
- 安全漏洞扫描（cargo audit）
- 重大依赖升级评估

### 9.4 社区与反馈

- 内部用户反馈收集
- Issue 跟踪与响应
- 每月内部分享会

---

## 十、团队配置建议【更新】

### 10.1 核心团队角色

| 角色                  | 人数 | 主要职责                                 |
| --------------------- | ---- | ---------------------------------------- |
| Tech Lead / Architect | 1    | 架构设计、技术决策、Review               |
| Rust 后端工程师       | 3-4  | 核心模块开发                             |
| **平台/PAL 工程师**   | 2-3  | **PAL 实现、bootloader 集成（人数 +1）** |
| 安全工程师            | 1    | 安全设计、审计、渗透测试                 |
| 测试工程师            | 1-2  | 自动化测试、故障注入、E2E                |
| DevOps / SRE          | 1    | CI/CD、部署、监控                        |
| 技术写作              | 0.5  | 文档体系                                 |

### 10.2 各阶段团队投入

| Phase    | 人月估算              | 关键角色加强           |
| -------- | --------------------- | ---------------------- |
| Phase -1 | 4 人月                | Tech Lead + 全员盘点   |
| Phase 0  | 8 人月（+2）          | **PAL 工程师全力投入** |
| Phase 1  | 12 人月               | 安全工程师加入         |
| Phase 2  | 18 人月（+2）         | **OTA 设计提前介入**   |
| Phase 3  | 25 人月               | 平台工程师全力投入     |
| Phase 4  | 20 人月               | 性能与扩展性聚焦       |
| Phase 5  | 15 人月               | 测试与文档为主         |
| **合计** | **约 102 人月**（+8） |                        |

---

## 十一、关键 KPI 与基线【保持】

| 指标                  | Phase 1 | Phase 3 | Phase 5 |
| --------------------- | ------- | ------- | ------- |
| 支持平台数            | 3       | 4       | 5+      |
| OTA 升级成功率        | -       | ≥ 95%   | ≥ 99.5% |
| 应用升级成功率        | ≥ 95%   | ≥ 99%   | ≥ 99.9% |
| 通信加密覆盖          | 100%    | 100%    | 100%    |
| 单元测试覆盖率        | ≥ 60%   | ≥ 70%   | ≥ 75%   |
| **PAL Mock 测试覆盖** | ≥ 80%   | ≥ 90%   | ≥ 95%   |

---

## 十二、风险登记册【更新】

| ID  | 风险                                   | 概率 | 影响 | 缓解                                      | Owner      |
| --- | -------------------------------------- | ---- | ---- | ----------------------------------------- | ---------- |
| R0  | **现状代码与目标架构差距大，迁移返工** | 高   | 高   | **Phase -1 充分盘点 + Strangler Pattern** | Tech Lead  |
| R1  | OTA 设计复杂度超预期                   | 高   | 高   | **Phase 2 提前启动设计 + 原型验证**       | Tech Lead  |
| R2  | A/B 升级在低端嵌入式不可行             | 中   | 高   | 准备 fallback（应用级升级）               | 平台工程师 |
| R3  | 多平台维护成本高                       | 高   | 中   | **强化 PAL + CI 矩阵 + Mock 测试**        | DevOps     |
| R4  | 团队 Rust 经验不足                     | 中   | 高   | 资深 Reviewer + 内训                      | Tech Lead  |
| R5  | 安全审计严重问题需返工                 | 中   | 高   | Phase 1 即引入安全工程师                  | 安全工程师 |
| R6  | 性能不达标                             | 中   | 中   | 早期建立基准 + Phase 4 优化               | Tech Lead  |
| R7  | 依赖库（如 RAUC）兼容性                | 中   | 中   | **PAL 抽象层隔离**                        | 平台工程师 |
| R8  | 进度滞后                               | 中   | 中   | 优先级管理 + 关键路径保护                 | PM         |
| R9  | 后端协作进度不匹配                     | 中   | 高   | 早期对齐接口 + Mock 后端                  | Tech Lead  |
| R10 | 真机测试设备不足                       | 高   | 中   | 早期采购 + CI 集成真机                    | 测试工程师 |
| R11 | **PAL 抽象不准导致返工**               | 中   | 高   | **Linux 实现先行 + 架构组每周评审**       | 平台工程师 |
| R12 | **State Store schema 设计不全**        | 中   | 高   | **与各业务模块对齐 + 预留扩展**           | Tech Lead  |

---

## 十三、决策节点（Decision Gates）【更新】

| Gate         | 检查项                                              | 决策                    |
| ------------ | --------------------------------------------------- | ----------------------- |
| **G-1 → G0** | **v0.4 退出标准、差异盘点完整、迁移设计书评审通过** | **通过 / 调整迁移策略** |
| G0 → G1      | v0.5 退出标准、PAL Linux 完整、CI 绿                | 通过 / 延期 / 范围调整  |
| G1 → G2      | v0.8 退出标准、安全测试通过                         | 通过 / 延期 / 重新设计  |
| G2 → G3      | v1.0 退出标准、E2E 通过、OTA 设计评审               | 通过 / 延期 / 范围调整  |
| G3 → G4      | v1.5 退出标准、OTA 真机 1000 次通过                 | 通过 / 延期 / 增强测试  |
| G4 → G5      | v2.0 退出标准、性能达标                             | 通过 / 延期 / 持续优化  |
| G5 → GA      | v2.1 退出标准、试点稳定、审计通过                   | GA 发布 / 延期          |

---

## 十四、优先级与最小可行版本【更新】

### 14.1 P0（必须做）

- **Phase -1：差异盘点（地基的地基）**
- Phase 0：全部（地基不能省，**特别是 PAL 完整契约**）
- Phase 1：mTLS、KeyStore、命令白名单、审计链
- Phase 2：南向 IPC、App Lifecycle、Config Manager
- Phase 3：A/B 升级或应用级升级（至少其一）+ Agent 自升级

### 14.2 P1（强烈建议）

- Phase 2：Data Router、Upgrade Engine 设计前移
- Phase 3：完整故障注入测试
- Phase 4：扩展点开放
- Phase 5：完整文档与 SLA 验证

### 14.3 P2（资源允许时）

- Phase 4：多租户、灰度发布
- Phase 5：外部安全审计

---

## 十五、近期 Sprint 行动项（Sprint 1-2，覆盖 Phase -1）

| 任务                          | Owner            | 交付物       |
| ----------------------------- | ---------------- | ------------ |
| 现状-目标差异矩阵             | Tech Lead + 全员 | 差异矩阵文档 |
| PAL 抽离清单（扫描 `#[cfg]`） | 工程师 A         | 清单文档     |
| 隐式状态盘点                  | 工程师 B         | 状态清单     |
| 迁移设计书                    | Tech Lead        | 设计书 v1.0  |
| 紧急修复（unwrap、路径穿越）  | 工程师 C         | PR           |
| 三平台 CI 紧急搭建            | DevOps           | CI 流水线    |
| ADR 目录建立 + 首批补录       | Tech Lead + 全员 | ADR-001~012  |

---

## 十六、Sprint 节奏建议【保持】

- **Sprint 长度**：2 周
- **每个 Sprint**：
  - Day 1：Sprint Planning
  - Daily：15 分钟 stand-up
  - Day 9：Sprint Review + Demo
  - Day 10：Retrospective + 下一 Sprint 准备
- **每月**：架构对齐会、跨团队同步会
- **每季度**：阶段性 Demo Day、KPI 复盘

---

## 十七、修订版 90 天目标

### Sprint 1（W1-W2）：完成 Phase -1
- 差异盘点 + 迁移设计书
- 三平台 CI 绿
- v0.4 发布

### Sprint 2-4（W3-W8）：完成 Phase 0
- PAL 完整契约层
- PAL Linux 完整实现
- State Store + 能力探测
- v0.5 发布

### Sprint 5-7（W9-W14）：完成 Phase 1 主体
- mTLS、KeyStore、命令白名单、审计链
- Windows/macOS 安全 PAL 补齐
- v0.8 发布

**90 天交付物**：
- ✅ v0.4：清洁基线 + 三平台 CI
- ✅ v0.5：完整地基 + PAL Linux 主线
- ✅ v0.8：安全基线达成
- ✅ 12 条 ADR
- ✅ 差异矩阵 + 迁移设计书 + PAL 设计书 + OTA 设计书启动
- ✅ 单元测试覆盖率 ≥ 60%

---

## 十八、与 v1.0 计划的核心变化总结

| 变化点                    | 说明                                                           |
| ------------------------- | -------------------------------------------------------------- |
| ✅ **新增 Phase -1**       | 2 周差异盘点期，避免地基阶段返工                               |
| ✅ **Phase 0 +1 周**       | PAL 契约层 + Linux 实现 + 能力探测 + State Store schema 完整化 |
| ✅ **PAL 优先级提升**      | 从骨架升级到完整契约 + 主适配器，业务代码完全平台无关          |
| ✅ **Upgrade Engine 提前** | Phase 2 启动设计与原型，降低 Phase 3 风险                      |
| ✅ **降级层显式设计**      | Phase 0 PAL 内统一降级矩阵                                     |
| ✅ **能力探测框架**        | CapabilityProfile 驱动业务降级决策                             |
| ✅ **State Store 强化**    | schema 版本化与迁移机制前置                                    |
| ✅ **风险登记册扩充**      | 新增 R0/R11/R12 三个高优先级风险                               |
| ✅ **团队投入 +8 人月**    | 主要用于 PAL 强化与 OTA 提前                                   |
| ✅ **新增决策节点 G-1**    | Phase -1 出口卡点，确保差异盘点充分                            |

---

**计划维护说明**：本计划为 v2.0 修订版，将随项目进展持续迭代。每个 Phase 末进行复盘，必要时在下一 Phase 启动前更新。
