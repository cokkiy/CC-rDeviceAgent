# CC-rDeviceAgent OTA 升级状态机详细设计

## 一、设计目标与原则

### 1.1 核心目标

| 目标               | 说明                                                           |
| ------------------ | -------------------------------------------------------------- |
| **零变砖**         | 任何阶段失败（含断电、断网、kernel panic）都能恢复到可工作状态 |
| **原子性**         | 升级要么完全成功，要么完全回滚，无中间状态遗留                 |
| **可观测**         | 每个状态可被远程查询、审计、调试                               |
| **可中断与可续做** | 升级可暂停/取消，断点续做                                      |
| **统一抽象**       | 系统升级、应用升级、配置升级共用同一状态机框架                 |
| **强安全**         | 签名验证、防回滚、防中间人攻击                                 |

### 1.2 设计原则

1. **A/B 双槽位**：系统升级采用 A/B 槽位（active/inactive），新版本写 inactive，bootloader 切换；应用升级类似但更轻量（旧版备份 + 新版安装）
2. **事务化**：每个状态转换是事务，崩溃后从持久化状态恢复
3. **健康闸门**：每个关键状态有"健康检查"作为闸门，不通过自动回滚
4. **幂等性**：相同升级任务重复触发不会产生副作用
5. **解耦**：状态机引擎与升级策略（系统/应用/配置）通过 trait 解耦

---

## 二、升级类型分类

为了让一个状态机能复用，先明确三类升级的差异：

| 维度             | 系统升级 (System OTA)                | 应用升级 (App Update)    | 配置升级 (Config Update) |
| ---------------- | ------------------------------------ | ------------------------ | ------------------------ |
| **目标**         | OS 镜像 / 内核 / rootfs / agent 自身 | 业务应用二进制/容器/WASM | 配置文件                 |
| **是否需要重启** | 是（切槽位）                         | 通常否（除非应用要求）   | 否（hot reload）         |
| **回滚机制**     | bootloader 切回旧槽                  | 备份目录还原             | 配置版本回滚             |
| **存储位置**     | 独立分区/卷                          | 应用目录 + 备份目录      | KV 存储 + 历史版本       |
| **升级时长**     | 分钟级                               | 秒级到分钟级             | 毫秒级到秒级             |
| **失败影响**     | 设备可能离线                         | 应用离线                 | 配置生效失败             |
| **依赖**         | bootloader (U-Boot/GRUB/RAUC/UEFI)   | 进程管理器               | 配置存储                 |

**结论**：三者共用**主状态机框架**，差异通过 `trait UpgradeStrategy` 注入。

---

## 三、主状态机设计

### 3.1 完整状态图

```
                            ┌─────────────┐
                            │    IDLE     │  (初始/无任务)
                            └──────┬──────┘
                                   │ ReceiveTask
                                   ▼
                            ┌─────────────┐
                       ┌────│  RECEIVED   │  (任务已接收，待校验)
                       │    └──────┬──────┘
                  Reject│           │ ValidateTask
                       │           ▼
                       │    ┌─────────────┐
                       ├────│  VALIDATED  │  (元数据校验通过)
                       │    └──────┬──────┘
                       │           │ StartDownload
                       │           ▼
                       │    ┌─────────────┐ ◄─── Pause/Resume
                       ├────│ DOWNLOADING │      (断点续传)
                       │    └──────┬──────┘
                       │           │ DownloadComplete
                       │           ▼
                       │    ┌─────────────┐
                       ├────│  VERIFYING  │  (签名/哈希校验)
                       │    └──────┬──────┘
                       │           │ VerifyPass
                       │           ▼
                       │    ┌─────────────┐
                       ├────│ PRE_CHECK   │  (前置健康检查)
                       │    └──────┬──────┘
                       │           │ PreCheckPass
                       │           ▼
                       │    ┌─────────────┐
                       ├────│ STAGING     │  (写入 inactive 槽/备份当前版本)
                       │    └──────┬──────┘
                       │           │ StagingDone
                       │           ▼
                       │    ┌─────────────┐
                       ├────│ READY_TO    │  (等待升级窗口/用户确认)
                       │    │ _ACTIVATE   │
                       │    └──────┬──────┘
                       │           │ Activate
                       │           ▼
                       │    ┌─────────────┐
                       ├────│ ACTIVATING  │  (切槽/启动新版/重启)
                       │    └──────┬──────┘
                       │           │ Booted
                       │           ▼
                       │    ┌─────────────┐
                       ├────│ POST_CHECK  │  (新版健康检查)
                       │    └──────┬──────┘
                       │           │ HealthOK + Confirm
                       │           ▼
                       │    ┌─────────────┐
                       │    │  COMMITTED  │  ─────► 终态：成功
                       │    └─────────────┘
                       │
                       │ ┌──────────────────────────────────┐
                       │ │  失败分支（任何状态触发 Fail）       │
                       │ └──────────────────────────────────┘
                       ▼
                ┌─────────────┐
                │ ROLLING_BACK│ (执行回滚)
                └──────┬──────┘
                       │ RollbackDone
                       ▼
                ┌─────────────┐
                │  ROLLED_BACK│  ─────► 终态：失败但已恢复
                └─────────────┘

                ┌─────────────┐
                │   FAILED    │  ─────► 终态：失败且无法恢复（需人工介入）
                └─────────────┘
```

### 3.2 状态定义详表

| 状态              | 含义                                       | 持久化 | 可中断 | 超时                | 退出条件                   |
| ----------------- | ------------------------------------------ | ------ | ------ | ------------------- | -------------------------- |
| `Idle`            | 无升级任务                                 | 否     | -      | -                   | 收到任务                   |
| `Received`        | 任务已接收，等待校验                       | 是     | 是     | 60s                 | 元数据合法                 |
| `Validated`       | 任务元数据已校验（版本、签名者、依赖）     | 是     | 是     | -                   | 用户/调度策略放行          |
| `Downloading`     | 正在下载升级包                             | 是     | **是** | 可配置（默认24h）   | 下载完成且块校验通过       |
| `Verifying`       | 整包签名+哈希校验                          | 是     | 否     | 5min                | 签名验证通过               |
| `PreCheck`        | 前置检查（磁盘空间、电量、依赖、运行环境） | 是     | 否     | 5min                | 全部检查通过               |
| `Staging`         | 写入 inactive 槽/备份当前版本              | 是     | 部分   | 30min               | 写入完成且校验通过         |
| `ReadyToActivate` | 已就绪，等待激活窗口                       | 是     | 是     | 可配置（默认7d）    | 收到激活命令 / 窗口到达    |
| `Activating`      | 执行切槽/重启/启动新版                     | 是     | **否** | 10min               | 新版本启动成功             |
| `PostCheck`       | 新版本运行健康检查                         | 是     | 否     | 可配置（默认10min） | 健康指标全绿 + 收到 Commit |
| `Committed`       | **终态-成功**                              | 是     | -      | -                   | -                          |
| `RollingBack`     | 正在回滚                                   | 是     | 否     | 10min               | 旧版本恢复并健康           |
| `RolledBack`      | **终态-已回滚**                            | 是     | -      | -                   | -                          |
| `Failed`          | **终态-失败且无法回滚**                    | 是     | -      | -                   | 需人工介入                 |

### 3.3 关键事件

| 事件                            | 触发源                | 携带数据                                                            |
| ------------------------------- | --------------------- | ------------------------------------------------------------------- |
| `ReceiveTask`                   | 后端推送 / 主动拉取   | UpgradeTask（含 id、type、target_version、manifest_url、signature） |
| `ValidateTask`                  | 内部                  | -                                                                   |
| `StartDownload`                 | 内部 / 调度器         | -                                                                   |
| `Pause` / `Resume`              | 控制命令              | -                                                                   |
| `Cancel`                        | 控制命令              | 仅在可中断状态生效                                                  |
| `DownloadComplete`              | 内部                  | -                                                                   |
| `VerifyPass` / `VerifyFail`     | 内部                  | 失败原因                                                            |
| `PreCheckPass` / `PreCheckFail` | 内部                  | 检查结果                                                            |
| `StagingDone` / `StagingFail`   | 内部                  | -                                                                   |
| `Activate`                      | 控制命令 / 调度器     | -                                                                   |
| `Booted`                        | 系统启动后 agent 上报 | 新版本号                                                            |
| `HealthOK` / `HealthFail`       | PostCheck 模块        | 健康详情                                                            |
| `Confirm`                       | 控制命令 / 自动       | -                                                                   |
| `Fail`                          | 任何阶段              | 错误码 + 详情                                                       |
| `RollbackDone` / `RollbackFail` | 内部                  | -                                                                   |

---

## 四、关键状态详细设计

### 4.1 Verifying：签名与完整性校验

校验顺序（任一失败 → `Failed`）：

```
1. 整包 SHA-256 == manifest.checksum
2. manifest.signature 用预置公钥（或 X.509 证书链）验证 Ed25519 签名
3. manifest.signer 在受信任签名者列表中
4. manifest.target_device_class 匹配本设备
5. manifest.min_agent_version <= 当前 agent 版本
6. manifest.target_version > 当前版本（防回滚，除非 force_downgrade=true 且签名授权）
7. manifest.expires_at > now （包未过期）
8. 与之前下载的同 task_id 升级包指纹一致（防中间人替换）
```

**Manifest 结构**：
```json
{
  "task_id": "uuid-v4",
  "upgrade_type": "system | application | config",
  "target": {
    "name": "rootfs | agent | app:com.example.x",
    "current_version_constraint": ">=1.2.0,<2.0.0",
    "target_version": "2.1.5"
  },
  "artifact": {
    "url": "https://...",
    "size": 12345678,
    "sha256": "abc...",
    "format": "tar.zst | raucb | bin | oci"
  },
  "dependencies": [...],
  "pre_check_script": "optional",
  "post_check_script": "optional",
  "rollback_policy": "auto | manual",
  "activation_policy": "immediate | scheduled | manual",
  "schedule_window": "0 2 * * *",
  "min_agent_version": "1.0.0",
  "min_battery_percent": 30,
  "min_disk_free_mb": 500,
  "signer": "ota-signer-prod",
  "signed_at": "2025-01-01T00:00:00Z",
  "expires_at": "2025-04-01T00:00:00Z",
  "signature": "base64-ed25519-sig"
}
```

### 4.2 PreCheck：前置闸门

前置检查必须**全部通过**，任一失败 → `Failed`（不进入 Staging 避免浪费磁盘）：

| 检查项                                 | 系统升级 | 应用升级 | 配置升级 |
| -------------------------------------- | -------- | -------- | -------- |
| 磁盘空间 ≥ 升级包大小 × 2.5            | ✅        | ✅        | ✅        |
| 电池电量 ≥ 阈值（默认 30%）或接通电源  | ✅        | ⚠️ 可配   | -        |
| CPU/IO 负载 ≤ 阈值                     | ✅        | ✅        | -        |
| 关键服务运行正常                       | ✅        | ✅        | -        |
| inactive 槽可写 / bootloader 健康      | ✅        | -        | -        |
| 依赖应用/库已就绪                      | -        | ✅        | -        |
| 当前没有活动业务会话（如远程桌面连接） | 可配     | 可配     | -        |
| 自定义脚本（manifest 指定）            | ✅        | ✅        | ✅        |

### 4.3 Staging：分类型实现

#### 4.3.1 系统升级 Staging（A/B 槽位方案）

```
1. 检测 inactive 槽位（通过 bootloader 接口或环境变量）
2. 卸载 inactive 槽（如已挂载）
3. 写入升级镜像到 inactive 槽（流式写入，避免内存爆）
4. 每写 1MB 计算累计 hash，与 manifest 比对
5. 写完后 sync + fsync，确保落盘
6. 挂载 inactive 槽，运行最小化健康检查（如能 ls /bin）
7. 卸载 inactive 槽
8. 更新 bootloader 元数据：标记 inactive 为 "trial"，重启次数 = 0
9. 持久化状态：staged_slot = inactive_slot_id
```

**关键点**：
- 此阶段**不切槽**，当前槽仍是活跃的，断电无害
- bootloader 元数据更新使用**双拷贝 + CRC**，自身也防止损坏
- 推荐使用 RAUC bundle 格式或自定义 zst 压缩格式

#### 4.3.2 应用升级 Staging

```
1. 创建临时目录 /var/lib/agent/staging/{task_id}/
2. 解压升级包到临时目录
3. 验证解压结果（文件清单 + 每个文件 hash）
4. 备份当前版本：/var/lib/agent/apps/{app_id}/ → /var/lib/agent/backups/{app_id}/{old_version}/
5. 备份保留策略：默认保留最近 2 个版本，超出删除最旧
6. 持久化：staged_path = staging dir, backup_path = backup dir
```

#### 4.3.3 配置升级 Staging

```
1. 解析新配置，进行 schema 校验
2. 与当前配置 diff，记录变更项
3. 备份当前配置版本到历史表
4. 将新配置写入"待激活"槽（不立即生效）
5. 如配置包含敏感字段（密钥），使用 KeyStore 加密
```

### 4.4 Activating：原子切换

#### 4.4.1 系统升级 Activating

```
1. 通知所有应用：即将重启（grace period 30s 可配）
2. 优雅停止应用（按依赖反向）
3. 持久化关键状态（防回滚计数器、当前 task_id 等）
4. 命令 bootloader：下次从 inactive 槽启动（trial mode）
5. sync + reboot
   ──────── 重启分界线 ────────
6. (新版本启动) agent 启动后读取持久化状态
7. 检测当前从 trial 槽启动 → 进入 PostCheck
```

**A/B 槽位 trial mode 关键机制**：
- bootloader 设置 `boot_count++`，达到阈值（默认 3）仍未 `mark good` 则自动回切
- agent 在 PostCheck 通过后必须调用 `bootloader_mark_good(slot)` 才算成功
- 这一机制由 RAUC/U-Boot/grub-btrfs 等提供

#### 4.4.2 应用升级 Activating

```
1. 优雅停止旧版本（SIGTERM → 等待 → SIGKILL）
2. 原子重命名：
   mv apps/{app_id}/ apps/{app_id}.old/
   mv staging/{task_id}/ apps/{app_id}/
3. 设置文件权限/SELinux 标签
4. 运行 pre-start 脚本（manifest 指定）
5. 启动新版本（通过应用进程管理器）
6. 等待应用就绪信号（通过 SDK 接口上报 ready）
```

#### 4.4.3 配置升级 Activating

```
1. 应用配置变更通知（向订阅了对应配置的应用发送 watch event）
2. 应用回应 ack（带 grace period）
3. 切换"待激活"槽为当前槽
4. 持久化新版本号
```

### 4.5 PostCheck：健康闸门

PostCheck **必须**通过才能进入 `Committed`。

| 检查项                          | 系统升级 | 应用升级 | 配置升级 |
| ------------------------------- | -------- | -------- | -------- |
| 进程/服务存活（持续 N 秒）      | ✅        | ✅        | -        |
| 关键端口可连通                  | ✅        | ✅        | -        |
| 内存/CPU/磁盘异常率 < 阈值      | ✅        | ✅        | -        |
| 上报心跳成功                    | ✅        | -        | -        |
| 与后端 mTLS 握手成功            | ✅        | -        | -        |
| 应用自报 ready（SDK API）       | -        | ✅        | -        |
| 自定义健康脚本（manifest 指定） | ✅        | ✅        | ✅        |
| 配置生效验证（如端口监听上）    | -        | -        | ✅        |

**双重确认机制**：
- **本地健康**：agent 自动判断
- **远程确认**：后端发 `Confirm` 命令（避免本地误判说健康但实际业务异常）
- 配置可选纯本地 / 纯远程 / 双重

### 4.6 RollingBack：回滚机制

#### 4.6.1 系统升级回滚

```
分两种情况：

情况 A：Activating 后启动失败（设备未起来）
  → bootloader 检测 boot_count 超阈值，自动切回 active 槽
  → 老版本 agent 启动，从持久化状态看到上次升级失败
  → 状态机进入 RolledBack

情况 B：Activating 后启动成功但 PostCheck 失败
  → agent 主动调用 bootloader_rollback(slot)
  → 调度重启
  → 重启后从 active 槽（旧版）启动
  → 状态机进入 RolledBack
```

#### 4.6.2 应用升级回滚

```
1. 停止当前（新版）应用
2. 删除 apps/{app_id}/
3. 重命名 apps/{app_id}.old/ → apps/{app_id}/
4. 启动旧版应用
5. 等待 ready，验证健康
6. 清理 staging 目录
```

#### 4.6.3 配置升级回滚

```
1. 从历史版本表恢复上一版配置
2. 通知应用配置变更（带 rollback 标记）
3. 验证生效
```

### 4.7 Failed：无法恢复

进入 `Failed` 的场景（即使尝试回滚也失败）：

- 系统升级：bootloader 损坏、两个槽都损坏
- 应用升级：备份目录损坏 + 新版本启动失败
- 配置升级：所有历史版本都校验失败

**处理策略**：
- 上报严重告警到后端
- 进入"维护模式"：仅保留心跳和远程命令通道，禁止再次升级
- 等待人工干预或紧急恢复包（emergency recovery package）

---

## 五、持久化与崩溃恢复

### 5.1 持久化存储

使用 **SQLite + WAL 模式**（同时支持事务和崩溃安全）：

```sql
CREATE TABLE upgrade_tasks (
  task_id           TEXT PRIMARY KEY,
  upgrade_type      TEXT NOT NULL,
  target_name       TEXT NOT NULL,
  target_version    TEXT NOT NULL,
  manifest_json     TEXT NOT NULL,
  manifest_sig      BLOB NOT NULL,
  current_state     TEXT NOT NULL,
  previous_state    TEXT,
  state_entered_at  INTEGER NOT NULL,    -- unix epoch ms
  retry_count       INTEGER DEFAULT 0,
  artifact_path     TEXT,
  artifact_offset   INTEGER DEFAULT 0,   -- 断点续传
  staged_path       TEXT,
  backup_path       TEXT,
  staged_slot       TEXT,
  error_code        TEXT,
  error_detail      TEXT,
  created_at        INTEGER NOT NULL,
  updated_at        INTEGER NOT NULL,
  pinned            INTEGER DEFAULT 0    -- 防止清理
);

CREATE TABLE upgrade_state_log (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  task_id         TEXT NOT NULL,
  from_state      TEXT,
  to_state        TEXT NOT NULL,
  event           TEXT NOT NULL,
  context_json    TEXT,
  prev_hash       TEXT,                  -- 哈希链防篡改
  hash            TEXT NOT NULL,
  ts              INTEGER NOT NULL,
  FOREIGN KEY (task_id) REFERENCES upgrade_tasks(task_id)
);

CREATE INDEX idx_state_log_task ON upgrade_state_log(task_id, id);
```

### 5.2 崩溃恢复算法

agent 启动时执行：

```rust
fn recover_on_startup() -> Result<()> {
    let tasks = db.query_active_tasks()?;  // current_state ∉ terminal states
    
    for task in tasks {
        match task.current_state {
            // 可安全续做的状态：从该状态继续
            Downloading => resume_download(task),
            Staging if task.upgrade_type != System => resume_staging(task),
            ReadyToActivate => check_activation_schedule(task),
            
            // 不应该在此状态崩溃，进入恢复流程
            Verifying | PreCheck => transition_to(task, Fail("crashed_during_check")),
            
            // 系统升级 Staging 中崩溃：检查 inactive 槽完整性
            Staging if task.upgrade_type == System => {
                if verify_inactive_slot(task)? {
                    transition_to(task, StagingDone)
                } else {
                    transition_to(task, Fail("staging_corrupted"))
                }
            }
            
            // Activating 中崩溃：检查当前从哪个槽启动
            Activating => {
                let booted_slot = bootloader.current_slot()?;
                if booted_slot == task.staged_slot {
                    transition_to(task, Booted { version: read_version() })
                } else {
                    // 还在老槽，说明重启未发生或失败
                    transition_to(task, Fail("activation_failed_to_boot"))
                }
            }
            
            // PostCheck 中崩溃：重新跑健康检查
            PostCheck => retry_post_check(task),
            
            // 回滚中崩溃：继续回滚
            RollingBack => continue_rollback(task),
        }
    }
}
```

### 5.3 状态转换的事务保证

```rust
fn transition(task_id: &str, event: Event) -> Result<()> {
    let tx = db.begin_transaction()?;
    
    let task = tx.lock_task(task_id)?;  // SELECT ... FOR UPDATE
    let from = task.current_state;
    let to = STATE_TABLE.resolve(from, &event)?;  // 校验合法转换
    
    // 1. 执行状态进入动作（如有副作用，必须幂等）
    run_entry_action(&to, &task)?;
    
    // 2. 持久化新状态
    tx.update_task_state(task_id, to)?;
    
    // 3. 记录日志（哈希链）
    let prev = tx.last_log_hash(task_id)?;
    let log_hash = hash(prev, from, to, event, ts);
    tx.append_log(task_id, from, to, event, log_hash)?;
    
    // 4. 提交
    tx.commit()?;
    
    // 5. 上报状态到后端（异步，不阻塞）
    spawn(report_state_async(task_id, to));
    
    Ok(())
}
```

---

## 六、A/B 槽位与 bootloader 集成

### 6.1 抽象接口

```rust
trait Bootloader: Send + Sync {
    fn current_slot(&self) -> Result<Slot>;
    fn inactive_slot(&self) -> Result<Slot>;
    
    /// 标记某槽为下次启动目标，进入 trial 模式
    fn set_trial(&self, slot: Slot) -> Result<()>;
    
    /// 在 PostCheck 通过后调用，将 trial 标记为 good
    fn mark_good(&self, slot: Slot) -> Result<()>;
    
    /// 主动回滚到另一个槽
    fn rollback(&self, slot: Slot) -> Result<()>;
    
    /// 查询 trial 重启计数
    fn boot_count(&self) -> Result<u32>;
    
    /// 槽位元数据
    fn slot_info(&self, slot: Slot) -> Result<SlotInfo>;
}

#[derive(Clone, Copy)]
enum Slot { A, B }

struct SlotInfo {
    slot: Slot,
    is_active: bool,
    is_trial: bool,
    version: Option<String>,
    health: SlotHealth,
}
```

### 6.2 具体实现

| 实现                   | 适用平台             | 备注                        |
| ---------------------- | -------------------- | --------------------------- |
| `RaucBootloader`       | 嵌入式 Linux         | 推荐方案，社区成熟          |
| `UBootEnvBootloader`   | 嵌入式 Linux（自研） | U-Boot 环境变量驱动         |
| `GrubBootloader`       | x86 Linux            | grub-mkconfig + grub-reboot |
| `EfiBootloader`        | UEFI 设备            | efibootmgr 操作             |
| `WindowsBcdBootloader` | Windows              | BCD 编辑（双系统槽）        |
| `OverlayfsBootloader`  | 无硬件支持的设备     | 软件实现，性能略差          |
| `MockBootloader`       | 测试                 | 内存模拟                    |

### 6.3 设备无 A/B 硬件支持时的降级方案

并非所有设备都支持 A/B 分区（如低端工业网关）。降级方案：

**方案 1：基于 overlayfs 的伪 A/B**
```
/
├── lower/        # 只读基础 rootfs
├── upper-a/      # 槽 A 的覆盖层
├── upper-b/      # 槽 B 的覆盖层
└── work/
启动时根据 bootloader 标志决定挂载哪个 upper
```

**方案 2：仅升级 agent 自身（不动 OS）**
- 系统级升级降级为 "agent 升级"
- 升级前完整备份 agent 二进制 + 配置 + 数据
- 失败回滚 = 替换回备份

**方案 3：完全依赖应用层**
- 不支持系统升级
- 仅支持应用 + 配置升级

---

## 七、防回滚与版本管理

### 7.1 单调版本号

设备维护 `min_allowed_version`，存储位置（优先级从高到低）：

1. **eFuse / OTP**：硬件熔丝，不可逆（最强保护，需硬件支持）
2. **TPM NV index**：受 TPM 保护
3. **bootloader env**：U-Boot 环境变量
4. **加密文件 + 设备密钥**：软件方案

每次升级成功后：
```
if new_version 安全等级 > 当前 min_allowed_version 对应的安全等级:
    min_allowed_version = new_version
```

### 7.2 版本号规范

```
<major>.<minor>.<patch>-<security_level>+<build>
例：2.1.5-s3+20250115
            └── security level：每发现一个 CVE 修复 +1
```

降级策略：
- `target_version >= min_allowed_version`：允许
- `target_version.security_level >= min_allowed_version.security_level`：允许（即使版本号低）
- 否则：拒绝（除非 manifest 含 `force_downgrade=true` 且签名为紧急恢复密钥）

---

## 八、状态机与升级策略解耦

### 8.1 核心 trait

```rust
trait UpgradeStrategy: Send + Sync {
    fn upgrade_type(&self) -> UpgradeType;
    
    /// 前置检查
    fn pre_check(&self, task: &UpgradeTask, ctx: &Context) -> Result<()>;
    
    /// Staging 阶段
    fn stage(&self, task: &UpgradeTask, ctx: &mut Context) -> Result<StageResult>;
    
    /// 激活
    fn activate(&self, task: &UpgradeTask, ctx: &mut Context) -> Result<ActivateResult>;
    
    /// 健康检查
    fn post_check(&self, task: &UpgradeTask, ctx: &Context) -> Result<HealthReport>;
    
    /// 提交（mark good）
    fn commit(&self, task: &UpgradeTask, ctx: &mut Context) -> Result<()>;
    
    /// 回滚
    fn rollback(&self, task: &UpgradeTask, ctx: &mut Context) -> Result<()>;
    
    /// 清理（删除 staging 文件、旧备份等）
    fn cleanup(&self, task: &UpgradeTask, ctx: &mut Context) -> Result<()>;
}

enum UpgradeType {
    System,
    Agent,           // agent 自身（特殊，需自我替换）
    Application,
    Config,
}

enum ActivateResult {
    /// 已激活，可直接进入 PostCheck
    InPlace,
    /// 需要重启才能激活，agent 会保存状态并重启
    RebootRequired,
    /// 需要应用层重启
    AppRestartRequired { app_id: String },
}
```

### 8.2 具体策略实现

```rust
struct SystemUpgradeStrategy {
    bootloader: Arc<dyn Bootloader>,
    slot_writer: Arc<dyn SlotWriter>,
}

struct AgentUpgradeStrategy {
    /// agent 自身升级的特殊处理：
    /// 1. 下载新 agent 二进制到 /opt/agent/new
    /// 2. 触发：把当前进程替换为更新器（updater），
    ///    更新器替换二进制后启动新 agent
    /// 3. 新 agent 启动后做 PostCheck
    updater_path: PathBuf,
}

struct AppUpgradeStrategy {
    app_manager: Arc<dyn AppManager>,
    backup_dir: PathBuf,
    max_backup_versions: usize,
}

struct ConfigUpgradeStrategy {
    config_store: Arc<dyn ConfigStore>,
    notifier: Arc<dyn ConfigNotifier>,
}
```

### 8.3 Agent 自我升级的特殊处理

agent 升级自己有"鸡生蛋"问题，方案：

```
1. 下载新 agent 二进制到 /opt/agent/new/agent
2. 验证签名，写持久化状态：state=Staging, staged_path=new/agent
3. Activating 阶段：
   a. fork 出独立的 updater 进程（来自资源文件或单独的小工具）
   b. updater 等待父 agent 退出（30s）
   c. 父 agent 完成清理后退出
   d. updater 执行：
      mv /opt/agent/agent /opt/agent/old/agent
      mv /opt/agent/new/agent /opt/agent/agent
      systemctl restart agent (或 exec)
   e. 新 agent 启动，读持久化状态 → PostCheck
4. 失败回滚：updater 还原 old 到当前位置，重启
```

**关键点**：updater 是一个**极简的独立小程序**（< 200KB），单独签名，几乎不会变化。

---

## 九、安全要点

### 9.1 签名与信任链

```
                      ┌──────────────────┐
                      │  Root CA (离线)   │
                      └────────┬─────────┘
                               │ sign
                ┌──────────────┼──────────────┐
                ▼              ▼              ▼
        ┌──────────────┐ ┌──────────┐ ┌──────────────┐
        │ OTA Signer A │ │ Signer B │ │ Recovery Key │
        │  (prod)      │ │ (staging)│ │ (emergency)  │
        └──────┬───────┘ └────┬─────┘ └──────┬───────┘
               │ sign         │              │
               ▼              ▼              ▼
           manifest        manifest      manifest
        (target_version)               (force_downgrade)
```

设备预置：
- Root CA 公钥（出厂烧录到只读区）
- 受信任 Signer 列表（可通过签名命令更新）
- Recovery Key（仅用于紧急恢复，需双因素授权）

### 9.2 攻击场景与缓解

| 攻击                             | 缓解措施                                 |
| -------------------------------- | ---------------------------------------- |
| 中间人替换升级包                 | mTLS 下载 + 整包签名                     |
| 重放旧的合法升级包（含旧 CVE）   | min_allowed_version 防回滚               |
| 篡改下载到本地的包               | 落盘后再次校验 + 写入只读临时区          |
| 篡改持久化状态使设备进入错误状态 | SQLite 文件权限 600 + 哈希链审计日志     |
| 攻破后端推送恶意升级             | 签名密钥与传输证书分离，签名密钥严格离线 |
| 攻击 bootloader 元数据           | 双拷贝 + CRC + 关键字段写入 OTP          |
| 在 trial 期间发起新升级          | 状态机禁止：trial 未 commit 时拒绝新任务 |
| 资源耗尽攻击（不断推送升级包）   | 速率限制 + 磁盘配额 + 任务队列上限       |

### 9.3 审计日志哈希链

```
log[n].hash = SHA-256(
    log[n-1].hash || 
    log[n].task_id || 
    log[n].from_state || 
    log[n].to_state || 
    log[n].event || 
    log[n].context || 
    log[n].ts
)
```

后端定期采集日志，发现哈希断链即为篡改证据。

---

## 十、可观测性

### 10.1 状态上报

每次状态变更立即上报（best-effort，失败入持久化队列）：

```json
{
  "device_id": "dev-001",
  "task_id": "uuid",
  "from_state": "Downloading",
  "to_state": "Verifying",
  "event": "DownloadComplete",
  "ts": 1736900000000,
  "context": {
    "bytes_downloaded": 12345678,
    "duration_ms": 56789
  },
  "agent_version": "2.1.5"
}
```

### 10.2 进度上报

`Downloading`/`Staging`/`PostCheck` 阶段定期上报进度（5s 一次）：

```json
{
  "task_id": "uuid",
  "state": "Downloading",
  "progress_percent": 45,
  "rate_bytes_per_sec": 1048576,
  "eta_seconds": 67
}
```

### 10.3 关键指标（Prometheus）

```
upgrade_tasks_total{type, state, result}
upgrade_duration_seconds{type, state}    # histogram
upgrade_download_bytes_total{task_id}
upgrade_rollback_total{type, reason}
upgrade_active_tasks
upgrade_state_transitions_total{from, to, event}
```

### 10.4 trace

整个升级过程是一个 trace，每个状态是一个 span：

```
trace_id: upgrade-task-uuid
  ├── span: Received       (50ms)
  ├── span: Validated      (200ms)
  ├── span: Downloading    (180s)
  │   ├── event: paused
  │   └── event: resumed
  ├── span: Verifying      (5s)
  ├── span: PreCheck       (2s)
  ├── span: Staging        (60s)
  ├── span: Activating     (45s, reboot)
  ├── span: PostCheck      (30s)
  └── span: Committed
```

---

## 十一、Protobuf 接口定义

```protobuf
syntax = "proto3";
package cc_ragent.upgrade.v1;

service UpgradeService {
  // 推送升级任务
  rpc PushTask(PushTaskRequest) returns (PushTaskResponse);
  
  // 控制任务（暂停/恢复/取消/激活/提交）
  rpc ControlTask(ControlTaskRequest) returns (ControlTaskResponse);
  
  // 查询任务状态（单次）
  rpc GetTask(GetTaskRequest) returns (UpgradeTask);
  
  // 列出任务
  rpc ListTasks(ListTasksRequest) returns (ListTasksResponse);
  
  // 订阅状态变更（server stream）
  rpc WatchTask(WatchTaskRequest) returns (stream TaskEvent);
  
  // 上报状态（设备 → 后端，或本地组件 → agent 核心）
  rpc ReportState(ReportStateRequest) returns (ReportStateResponse);
}

message UpgradeTask {
  string task_id = 1;
  UpgradeType upgrade_type = 2;
  string target_name = 3;
  string target_version = 4;
  bytes manifest = 5;            // 序列化的 manifest（JSON 或 protobuf）
  bytes signature = 6;
  State current_state = 7;
  int64 state_entered_at = 8;
  Progress progress = 9;
  ErrorInfo error = 10;
  int64 created_at = 11;
  int64 updated_at = 12;
}

enum UpgradeType {
  UPGRADE_TYPE_UNSPECIFIED = 0;
  UPGRADE_TYPE_SYSTEM = 1;
  UPGRADE_TYPE_AGENT = 2;
  UPGRADE_TYPE_APPLICATION = 3;
  UPGRADE_TYPE_CONFIG = 4;
}

enum State {
  STATE_UNSPECIFIED = 0;
  STATE_IDLE = 1;
  STATE_RECEIVED = 2;
  STATE_VALIDATED = 3;
  STATE_DOWNLOADING = 4;
  STATE_VERIFYING = 5;
  STATE_PRE_CHECK = 6;
  STATE_STAGING = 7;
  STATE_READY_TO_ACTIVATE = 8;
  STATE_ACTIVATING = 9;
  STATE_POST_CHECK = 10;
  STATE_COMMITTED = 11;
  STATE_ROLLING_BACK = 12;
  STATE_ROLLED_BACK = 13;
  STATE_FAILED = 14;
}

message Progress {
  int32 percent = 1;
  int64 bytes_done = 2;
  int64 bytes_total = 3;
  int64 rate_bps = 4;
  int64 eta_seconds = 5;
}

message ErrorInfo {
  string code = 1;
  string message = 2;
  string detail = 3;
}

message ControlTaskRequest {
  string task_id = 1;
  Action action = 2;
  
  enum Action {
    ACTION_UNSPECIFIED = 0;
    ACTION_PAUSE = 1;
    ACTION_RESUME = 2;
    ACTION_CANCEL = 3;
    ACTION_ACTIVATE = 4;       // ReadyToActivate → Activating
    ACTION_COMMIT = 5;         // PostCheck → Committed
    ACTION_ROLLBACK = 6;       // 主动触发回滚
  }
}

message TaskEvent {
  string task_id = 1;
  State from_state = 2;
  State to_state = 3;
  string event = 4;
  int64 ts = 5;
  bytes context = 6;
}
```

---

## 十二、状态转换表（编程参考）

| From State      | Event              | To State            | 守卫条件          |
| --------------- | ------------------ | ------------------- | ----------------- |
| Idle            | ReceiveTask        | Received            | manifest 可解析   |
| Received        | ValidateTask       | Validated           | 元数据合法        |
| Received        | Fail               | Failed              | -                 |
| Validated       | StartDownload      | Downloading         | 调度策略允许      |
| Validated       | Cancel             | Failed("cancelled") | -                 |
| Downloading     | DownloadComplete   | Verifying           | 块校验全通过      |
| Downloading     | Pause              | Downloading         | 标记 paused=true  |
| Downloading     | Resume             | Downloading         | 标记 paused=false |
| Downloading     | Cancel             | Failed("cancelled") | -                 |
| Downloading     | Fail               | Failed              | 重试上限达到      |
| Verifying       | VerifyPass         | PreCheck            | 签名+哈希通过     |
| Verifying       | VerifyFail         | Failed              | -                 |
| PreCheck        | PreCheckPass       | Staging             | -                 |
| PreCheck        | PreCheckFail       | Failed              | -                 |
| Staging         | StagingDone        | ReadyToActivate     | -                 |
| Staging         | StagingFail        | Failed              | -                 |
| ReadyToActivate | Activate           | Activating          | 激活窗口/手动     |
| ReadyToActivate | Cancel             | Failed("cancelled") | -                 |
| Activating      | Booted             | PostCheck           | 新版启动并上报    |
| Activating      | Fail               | RollingBack         | -                 |
| PostCheck       | HealthOK + Confirm | Committed           | 双重确认通过      |
| PostCheck       | HealthFail         | RollingBack         | -                 |
| PostCheck       | Timeout            | RollingBack         | -                 |
| RollingBack     | RollbackDone       | RolledBack          | 旧版健康          |
| RollingBack     | RollbackFail       | Failed              | 旧版也起不来      |

---

## 十三、测试策略

### 13.1 单元测试
- 每个状态转换的合法性
- 状态机持久化与恢复
- 签名校验各种异常情况
- 哈希链完整性

### 13.2 集成测试
- 完整升级流程（mock bootloader）
- 各阶段失败注入与回滚
- 并发任务的拒绝/排队
- 网络中断与续传

### 13.3 故障注入测试（关键！）
使用 fault injection 框架，在每个状态强制崩溃/断电：

| 注入点                      | 期望结果                            |
| --------------------------- | ----------------------------------- |
| Downloading 中 kill -9      | 重启后从断点续做                    |
| Staging 中拔电              | 重启后检测 inactive 槽损坏 → Failed |
| Activating 中拔电（重启前） | 重启后从旧槽起来 → Failed           |
| Activating 中拔电（重启中） | bootloader trial 计数器机制保护     |
| PostCheck 中拔电            | 重启后重新跑 PostCheck              |
| RollingBack 中拔电          | 重启后继续回滚                      |

### 13.4 长期稳定性测试
- 1000 次连续升级（成功 + 失败混合）
- 7 天不间断升级压力
- 真实硬件回归（树莓派、x86 边缘盒、Windows）

---

## 十四、与现有代码的集成路径

基于 CC-rDeviceAgent 现状的实施步骤：

| 步骤 | 工作                                                          | 工期   |
| ---- | ------------------------------------------------------------- | ------ |
| 1    | 引入 `tracing` 替代现有日志；引入 SQLite 持久化               | 1 周   |
| 2    | 实现状态机引擎核心（不带任何 strategy）                       | 2 周   |
| 3    | 实现 `AppUpgradeStrategy`（最简单，最有价值）                 | 2 周   |
| 4    | 增强 `FileTransfer`：分块 SHA-256 + 断点续传 + 签名验证       | 1 周   |
| 5    | 实现 `ConfigUpgradeStrategy` + 配置版本管理                   | 1.5 周 |
| 6    | 实现 `AgentUpgradeStrategy` + updater 小工具                  | 2 周   |
| 7    | 实现 `Bootloader` trait + `MockBootloader` + `RaucBootloader` | 2 周   |
| 8    | 实现 `SystemUpgradeStrategy`                                  | 2 周   |
| 9    | 故障注入测试套件                                              | 2 周   |
| 10   | gRPC 接口定义 + 后端联调                                      | 1 周   |

**总计**：约 16-18 周（含必要的并行）。

---

## 十五、关键设计权衡总结

| 决策           | 选择                                   | 理由                                    |
| -------------- | -------------------------------------- | --------------------------------------- |
| 状态持久化     | SQLite                                 | 比文件存储事务性强，比独立 KV 库依赖少  |
| A/B 切换       | bootloader 驱动                        | 软件方案不可靠，bootloader 是唯一可靠点 |
| trial 验证机制 | bootloader boot_count + 显式 mark_good | 双保险：即使 agent 起不来也能自动回滚   |
| 升级包格式     | tar.zst + 独立 manifest                | 灵活，压缩率高，签名独立易管理          |
| 签名算法       | Ed25519                                | 速度快，密钥小，无随机数依赖            |
| Strategy 抽象  | trait + 注册表                         | 解耦，便于扩展新升级类型                |
| Agent 自升级   | updater 独立小程序                     | 避免自我替换的复杂性                    |
| 状态机引擎     | 自研基于事件的简单实现                 | rust-fsm 等库功能过重，自研可控         |

---

## 后续可以深入的方向

如果需要继续展开，告诉我以下哪个方向：

1. **Bootloader 抽象的具体实现**（RAUC/U-Boot/GRUB/Windows BCD）
2. **Agent 自升级的 updater 程序详细设计**
3. **签名系统的 PKI 拓扑与密钥轮换**
4. **后端管理面的设计**（任务编排、灰度策略、批次管理）
5. **故障注入测试框架的实现**
6. **状态机引擎的 Rust 代码骨架**