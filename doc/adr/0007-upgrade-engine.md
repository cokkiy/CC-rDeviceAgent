# ADR-007: Upgrade Engine 使用显式状态机

- 状态：Accepted
- 日期：2026-05-21

## 背景

目标架构要求 OTA、A/B 切换、签名验证和回滚，当前完全缺失。

## 决策

Upgrade Engine 采用显式状态机并持久化状态，依赖 File Transfer、Security Center、Bootloader PAL 和 State Store。

## 影响

Phase 2 启动详细设计，Phase 3 实现防变砖升级链路。
