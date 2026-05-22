# ADR-005: 安全能力收口到 Security Center

- 状态：Accepted
- 日期：2026-05-21

## 背景

当前只有局部 token 校验，缺少统一认证、授权、密钥和签名能力。

## 决策

建立 Security Center，集中处理 mTLS、RBAC、签名验证、密钥引用和凭证存储。

## 影响

Phase -1 禁用 raw shell command；后续命令执行、文件传输和应用控制必须经过安全决策点。
