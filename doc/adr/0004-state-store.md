# ADR-004: 使用 SQLite 作为统一 State Store

- 状态：Accepted
- 日期：2026-05-21

## 背景

项目已有多个 SQLite store，同时配置和运行状态仍散落在 TOML 与内存中。

## 决策

Phase 0 建立统一 State Store、schema version 和 embedded migrations。

## 影响

现有 SQLite 模块作为迁移样板；TOML 配置保持兼容直到 Config Manager 接管。
