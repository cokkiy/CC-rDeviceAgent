# ADR-010: 可观测性优先使用 tracing 并扩展 OTLP

- 状态：Accepted
- 日期：2026-05-21

## 背景

当前已有 `tracing` 日志，但缺 metrics 和 trace 输出。

## 决策

保留 `tracing` 作为基础，Phase 0 接入 OpenTelemetry logs/metrics/traces 和可选 OTLP exporter。

## 影响

新服务和 PAL adapter 需要携带 span 上下文并记录关键失败路径。
