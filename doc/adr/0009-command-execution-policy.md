# ADR-009: 禁止 raw shell command

- 状态：Accepted
- 日期：2026-05-21

## 背景

直接执行 `sh -c` 会造成命令注入和不可审计的远程执行能力。

## 决策

Phase -1 默认禁用 raw shell command；Phase 1 以命令白名单、参数 schema、RBAC 和审计恢复受控执行。

## 影响

旧客户端调用命令执行接口会收到明确失败结果，不再执行系统 shell。
