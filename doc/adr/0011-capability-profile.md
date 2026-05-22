# ADR-011: 使用 CapabilityProfile 驱动降级

- 状态：Accepted
- 日期：2026-05-21

## 背景

TPM、A/B、cgroup、systemd、屏幕采集等能力在不同平台和部署形态差异明显。

## 决策

Phase 0 建立 CapabilityProfile 探测和缓存，业务根据能力选择实现或降级路径。

## 影响

平台能力缺失不应导致启动失败，除非该能力被配置为强制要求。
