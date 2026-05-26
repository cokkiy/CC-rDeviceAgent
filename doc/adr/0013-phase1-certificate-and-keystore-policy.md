# ADR-013: Phase 1 证书、KeyStore 与 RBAC 基线

- 状态：Accepted
- 日期：2026-05-26

## 背景

Phase 1 要求北向通道启用 TLS/mTLS、密钥不由业务代码直接读取，并为控制命令、文件传输、配置、升级和应用控制建立最小权限边界。

## 决策

1. 北向 gRPC 使用 `control.tls` 加载服务端证书、私钥和客户端 CA；开启 `require_client_auth` 时客户端证书为强制项。
2. MQTT 使用 `mqtt.tls` 加载 CA、客户端证书和私钥；真实 broker 互操作验证依赖测试环境。
3. Security Center 定义 `KeyRef`、`SecurityLevel`、HKDF 和 Ed25519 验签 API；硬件 TPM/OS Keyring 不可用时降级为文件型 KeyStore，并通过 CapabilityProfile 显式标记。
4. RBAC 最小矩阵为 `admin`、`operator`、`readonly`；默认拒绝未授权资源/动作。
5. 外部 CA 签发、SPIFFE/SVID、TPM/TEE/SEP 生产级验证不在仓库内伪实现，作为平台集成项推进。

## 影响

业务入口必须通过 Security Center 或命令策略构造安全决策。KeyStore/CredentialStore 的平台深度集成可以分平台演进，但不能绕过统一 KeyRef 模型。
