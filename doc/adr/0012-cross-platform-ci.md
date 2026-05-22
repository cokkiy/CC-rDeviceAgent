# ADR-012: 三平台 CI 作为 Phase -1 出口门禁

- 状态：Accepted
- 日期：2026-05-21

## 背景

目标架构要求跨平台一致，但当前没有 CI，平台差异只能靠人工发现。

## 决策

建立 Linux x64、Windows x64、macOS arm64 CI 矩阵，执行 fmt、check、test，Linux 额外执行 clippy deny warnings。

## 影响

后续 PAL 和核心服务变更必须保持三平台编译基线。
