# ADR-003: 建立平台抽象层 PAL

- 状态：Accepted
- 日期：2026-05-21

## 背景

当前 `#[cfg]`、系统命令、平台文件和网络 API 分散在多个模块。

## 决策

建立 PAL trait，将 Process、FileSystem、Network、Service、SystemControl、Sensor 等平台能力收口。

## 影响

业务代码只依赖 trait；Linux 先完整实现，Windows/macOS 先保证编译和关键 stub。
