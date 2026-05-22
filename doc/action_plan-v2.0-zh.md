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

- [ ] 拆分 crate（按 PAL 设计调整）：
  - `agent-core`（业务核心）
  - `agent-protocols`（北向南向协议）
  - `pal-core` / `pal-linux` / `pal-windows` / `pal-macos` / `pal-fallback` / `pal-mock`（PAL 各 crate）
  - `agent-telemetry`
  - `agent-store`（State Store）
  - `agent-cli`
- [ ] workspace 依赖管理规范
- [ ] **交付物**：新 workspace 目录树

#### W0.2 错误处理统一（3 天）

- [ ] 统一 `AgentError` + `PalError` 体系
- [ ] PAL 错误透传与上下文保留
- [ ] **交付物**：错误处理规范文档

#### W0.3 可观测性骨架（5 天）

- [ ] `tracing` 全栈集成
- [ ] OpenTelemetry SDK 集成（Logs/Metrics/Traces）
- [ ] OTLP exporter 配置（可选）
- [ ] **PAL 内置 trace 埋点**（便于定位平台相关问题）
- [ ] **交付物**：可观测性基础设施

#### W0.4 PAL 契约层完整设计（5 天）【强化】

- [ ] 12 类契约 trait 完整定义（参见 PAL 详细设计）：
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
- [ ] CapabilityProfile 数据结构定义
- [ ] PAL Builder + PlatformContext 装配框架
- [ ] **交付物**：`pal-core` crate（契约 + 装配框架）

#### W0.5 PAL Linux 主适配器实现（8 天）【强化】

- [ ] ProcessManager（基于 nix + tokio::process）
- [ ] ServiceManager（systemd D-Bus）
- [ ] FileSystem / DiskSpace（libc + statvfs）
- [ ] NetworkInfo（netlink）
- [ ] SystemControl（reboot/shutdown）
- [ ] ResourceLimiter（cgroup v2，v1 fallback）
- [ ] IpcServer（Unix Socket）
- [ ] SystemLogger（journald）
- [ ] DeviceId（DMI/SMBIOS）
- [ ] **交付物**：`pal-linux` crate

#### W0.6 PAL 降级层与 Mock（4 天）【新增】

- [ ] Fallback KeyStore（加密文件 + 设备指纹）
- [ ] Fallback ResourceLimiter（rlimit）
- [ ] Mock 适配器（用于测试）
- [ ] **交付物**：`pal-fallback` + `pal-mock` crate

#### W0.7 能力探测框架（3 天）【新增】

- [ ] 探测流水线（TPM / A/B / cgroup / SecureBoot / 磁盘 / 网络）
- [ ] CapabilityProfile 持久化缓存
- [ ] 路由器（Capability Router）按 Profile 选择实现
- [ ] **交付物**：能力探测模块

#### W0.8 State Store 完整设计（5 天）【强化】

- [ ] SQLite + WAL 集成
- [ ] 完整 schema 设计（任务、配置、应用清单、审计、升级状态、密钥引用、CapabilityProfile 缓存）
- [ ] schema 版本化与迁移机制（embedded SQL migrations）
- [ ] 通用 Repository 抽象
- [ ] 备份/恢复接口
- [ ] **交付物**：`agent-store` crate

#### W0.9 PAL Windows/macOS 骨架（4 天）

- [ ] Windows：ProcessManager（CreateProcess）+ FileSystem + IpcServer（Named Pipe）骨架
- [ ] macOS：ProcessManager（posix_spawn）+ FileSystem + IpcServer（UDS）骨架
- [ ] 三平台编译通过（即使部分功能 Stub）
- [ ] **交付物**：`pal-windows` / `pal-macos` crate（骨架版）

#### W0.10 CI/CD 流水线（3 天）

- [ ] 三平台编译矩阵
- [ ] 单元测试 + 集成测试
- [ ] 代码覆盖率（目标 ≥ 50%）
- [ ] cargo audit + clippy + fmt
- [ ] PAL Mock 驱动的单测
- [ ] **交付物**：CI/CD 流水线

#### W0.11 文档体系（持续）

- [ ] `CONTRIBUTING.md`
- [ ] PAL 使用指南
- [ ] State Store 使用指南
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

构建完整的安全基础：传输加密、身份认证、命令安全、审计追溯。**Security Center 收口、依赖 PAL KeyStore**。

### 4.2 工作项

#### W1.1 Security Center 核心（5 天）【调整】

- [ ] 设计 Security Center 模块结构
- [ ] **依赖 PAL KeyStore**（TPM/Keyring/File 三级降级）
- [ ] 证书加载与验证（rustls + webpki）
- [ ] 信任锚管理（与 PAL DeviceId 联动）
- [ ] **交付物**：Security Center 核心模块

#### W1.2 mTLS 全通道（5 天）

- [ ] 北向 gRPC mTLS
- [ ] 北向 MQTT mTLS
- [ ] 南向 IPC（暂不加密，依赖 OS 权限）
- [ ] 证书轮换机制
- [ ] **交付物**：全通道 mTLS

#### W1.3 KeyStore 高级功能（4 天）【新增】

- [ ] 密钥派生（HKDF）
- [ ] Ed25519 签名/验签封装（ADR-010）
- [ ] 凭据加密落盘（基于 PAL CredentialStore）
- [ ] **交付物**：完整 KeyStore 服务

#### W1.4 命令白名单与 RBAC（5 天）

- [ ] 命令白名单配置体系
- [ ] RBAC 模型（角色、权限、命令映射）
- [ ] 责任链中间件（AuthN → AuthZ → RateLimit → Audit → Handler）
- [ ] **交付物**：命令安全框架

#### W1.5 Sandbox 集成（4 天）

- [ ] **基于 PAL Sandbox**（namespaces / Job Object / sandbox-exec）
- [ ] 命令执行沙箱化
- [ ] 资源限制（CPU、内存、磁盘 IO）
- [ ] **交付物**：Sandbox 服务

#### W1.6 Audit Chain（5 天）

- [ ] 审计事件分类与字段规范
- [ ] 链式哈希（防篡改）
- [ ] 异步批量上报（不阻塞主流程）
- [ ] 本地查询接口
- [ ] **交付物**：Audit Chain 模块

#### W1.7 文件传输安全（4 天）

- [ ] 路径白名单与穿越防护（**通过 PAL PathResolver**）
- [ ] 文件大小限制、磁盘配额检查（**通过 PAL DiskSpace**）
- [ ] 分块 SHA-256 + 整体 SHA-256 双重校验
- [ ] 断点续传与持久化任务状态（**State Store**）
- [ ] 限速（令牌桶）
- [ ] **交付物**：升级版 File Transfer Service

#### W1.8 安全测试（4 天）

- [ ] 渗透测试用例（命令注入、路径穿越、TLS 降级）
- [ ] Fuzz 测试（manifest 解析、命令参数）
- [ ] 攻击场景集成测试
- [ ] **交付物**：安全测试套件、测试报告

#### W1.9 PAL Windows/macOS 安全能力补齐（4 天）【新增】

- [ ] Windows KeyStore（CNG / TBS）
- [ ] Windows CredentialStore（DPAPI）
- [ ] Windows Sandbox（Job Object）
- [ ] macOS KeyStore（Keychain / SEP）
- [ ] macOS CredentialStore（Keychain）
- [ ] **交付物**：Windows/macOS 安全 PAL 实现

### 4.3 关键里程碑

| 时间  | 里程碑                          |
| ----- | ------------------------------- |
| W2 末 | Security Center + KeyStore 完成 |
| W3 末 | mTLS 全通道打通                 |
| W4 末 | 命令白名单 + Sandbox 就绪       |
| W5 末 | 审计链 + 文件传输安全完成       |
| W6 末 | **v0.8 发布**：安全基线达成     |

### 4.4 验收标准

- ✅ 所有外部通道使用 mTLS
- ✅ 所有控制命令经过 RBAC 与审计
- ✅ 审计哈希链可验证完整性
- ✅ 三平台 KeyStore（含降级）通过测试
- ✅ 安全测试套件全部通过

---

## 五、Phase 2：应用基座 + OTA 启动（8 周）【调整】

### 5.1 目标

让设备成为载荷应用的运行平台 + **启动 Upgrade Engine 设计与原型**（提前介入，降低 Phase 3 风险）。

### 5.2 工作项

#### W2.1 南向 IPC 通道（5 天）

- [ ] 南向 gRPC Server（**基于 PAL IpcServer**）
- [ ] 协议定义（应用注册、数据传输、配置订阅、升级查询）
- [ ] 连接管理与 Session
- [ ] **交付物**：南向 IPC 框架 + protobuf 定义

#### W2.2 App Registry（5 天）

- [ ] 应用注册流程（启动握手）
- [ ] 应用身份分配（App ID + Session Token）
- [ ] 应用能力声明与发现
- [ ] 应用清单持久化（State Store）
- [ ] **交付物**：App Registry 模块

#### W2.3 App Lifecycle（8 天）

- [ ] 应用生命周期状态机（Registered → Installed → Running → Stopped → Uninstalled）
- [ ] 应用安装（解压 + 校验 + 配置）
- [ ] 应用启动与停止（**通过 PAL ProcessManager**）
- [ ] 应用监控与自动重启
- [ ] 资源隔离与配额（**通过 PAL ResourceLimiter**）
- [ ] 应用日志收集（stdout/stderr → Observability Hub）
- [ ] **交付物**：App Lifecycle 模块

#### W2.4 Data Router（6 天）

- [ ] 应用数据上行路由（应用 → 后端）
- [ ] 应用数据下行路由（后端 → 应用）
- [ ] Topic 映射规则（应用命名空间隔离）
- [ ] 流量整形（限速、配额）
- [ ] 离线队列复用
- [ ] **交付物**：Data Router 模块

#### W2.5 Config Manager（10 天）

- [ ] 三层配置（重连生效 / 重启生效 / 下次升级生效）
- [ ] 版本化与回滚
- [ ] 配置订阅与下发（应用级）
- [ ] 默认值合并策略
- [ ] **交付物**：Config Manager 模块

#### W2.6 Upgrade Engine 设计与原型（10 天）【新增/前移】

- [ ] OTA 状态机详细设计（Idle → Downloading → Verifying → Staging → Switching → Validating → Committed/RolledBack）
- [ ] 升级包格式设计（manifest + payload + 签名）
- [ ] **PAL Bootloader trait 完整设计**（RAUC / UEFI BCD / 应用级 fallback）
- [ ] 状态持久化设计（State Store schema）
- [ ] 应用级升级原型（小步快跑，先验证状态机）
- [ ] **设计评审**（架构组 + 安全组 + 平台组）
- [ ] **交付物**：《OTA 设计书》、应用级升级原型

#### W2.7 端到端集成与测试（4 天）

- [ ] 应用从注册到升级完整 E2E 测试
- [ ] 性能基线（应用并发数、IPC 吞吐）
- [ ] **交付物**：E2E 测试报告

### 5.3 关键里程碑

| 时间  | 里程碑                                    |
| ----- | ----------------------------------------- |
| W2 末 | 南向 IPC + App Registry 完成              |
| W4 末 | App Lifecycle + Data Router 完成          |
| W6 末 | Config Manager 完成、OTA 设计评审通过     |
| W7 末 | OTA 应用级原型完成                        |
| W8 末 | **v1.0 发布**：应用基座 GA + OTA 设计就绪 |

### 5.4 验收标准

- ✅ 应用注册/启停/升级完整闭环
- ✅ 数据通道双向贯通
- ✅ 配置三层模型可用
- ✅ OTA 设计书通过评审
- ✅ 应用级升级原型可演示
- ✅ E2E 测试通过率 ≥ 95%

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