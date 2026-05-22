# State Store 使用指南

Phase 0 使用 `agent-store` 作为统一 SQLite State Store。

## Schema

当前 v1 schema 包含：

- `tasks`
- `config_versions`
- `app_manifests`
- `audit_events`
- `upgrade_state`
- `key_refs`
- `capability_profile_cache`

## 使用方式

- `StateStore::open(path)` 打开持久化数据库，并自动开启 WAL 与迁移。
- `StateStore::open_in_memory()` 用于单测。
- `save_capability_profile()` / `load_capability_profile()` 用于缓存 PAL 探测结果。
- `backup_to(path)` 使用 SQLite `VACUUM INTO` 生成备份。

后续新增状态必须通过 embedded migration 前向演进，不直接在业务模块中散落建表逻辑。
