# PAL 使用指南

Phase 0 将平台能力收口到 `pal-core` trait，并通过 `PlatformContext` 注入业务层。

## 基本原则

- 业务代码只依赖 `pal-core` 契约。
- Linux 主实现位于 `pal-linux`。
- Windows/macOS Phase 0 只保证骨架编译，未完成能力返回 `PalErrorKind::Unsupported`。
- 测试使用 `pal-mock`，不依赖真实系统能力。
- 兜底能力位于 `pal-fallback`，包括托管路径解析和文件型 KeyStore。

## 装配

根 crate 当前通过 `src/platform.rs` 暴露兼容入口：

- `platform::context()` 获取全局 `PlatformContext`
- `platform::reboot()` / `platform::shutdown()` 转发到 `SystemControl`
- `network_counters::collect()` 转发到 `NetStat`

后续业务模块应直接接收 `PlatformContext` 或具体 trait，而不是继续新增全局函数。
