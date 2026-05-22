# ADR-002: 协议层只负责接入适配

- 状态：Accepted
- 日期：2026-05-21

## 背景

当前 gRPC、文件传输、应用控制逻辑集中在 `src/app.rs`，协议和业务耦合。

## 决策

协议层负责认证上下文、反序列化、流管理和错误映射；业务语义进入核心服务层。

## 影响

Phase 0 起逐步拆分 gRPC/MQTT handler，新增核心 service trait 和内部 command/event 模型。
