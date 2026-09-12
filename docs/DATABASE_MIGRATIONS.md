# KylinStock 数据库迁移规范

KylinStock 是单机桌面库存系统，SQLite 文件本身就是客户业务资产。版本升级必须优先保证旧数据可继续使用，禁止通过“删库重建”解决结构变化。

## 当前机制

- 数据库结构版本使用 SQLite `PRAGMA user_version`。
- migration 实现在 `src-tauri/src/migration.rs`。
- migration 由 Rust 在一条独占 `SqliteConnection` 上执行。
- 每个待执行版本都使用：

```text
BEGIN IMMEDIATE
  -> 执行该版本全部 DDL / 数据修复
  -> PRAGMA user_version = N
COMMIT
```

- 任一步骤失败都会 `ROLLBACK`，不得把半升级状态暴露给业务层。
- 前端只有 migration 成功后才开放 Rust 原生数据库访问。
- 如果数据库 `user_version` 高于当前程序支持版本，程序必须拒绝继续打开，避免旧程序误写新版本数据库。

## 新增 migration 的硬规则

1. **已经发布过的 migration 永远不修改。**
   - 不修改旧 SQL。
   - 不重新解释旧版本含义。
   - 所有变化只能追加新版本。

2. **每次结构变化都必须提升版本号。**
   - `LATEST_SCHEMA_VERSION` 加 1。
   - 在 `MIGRATIONS` 尾部追加对应版本。
   - 版本必须严格递增且不可跳过依赖逻辑。

3. **不得删除客户历史数据来完成升级。**
   - 优先 `ALTER TABLE ADD COLUMN`、新表、新索引或可验证的数据回填。
   - 如果 SQLite 限制导致必须重建表，应在同一事务内采用 `new_table -> copy -> verify -> rename`，并补回归测试。

4. **migration 必须可从真实旧版本升级。**
   - 不只测试全新空库。
   - CI 至少覆盖 `legacy version -> latest version`。
   - 必须验证关键业务记录在升级后仍存在且值不变。

5. **migration 不负责偷偷修正无法确定含义的业务数据。**
   - 结构迁移和业务纠错分开。
   - 对无法自动判断的数据，宁可中止升级并给出明确错误，也不要猜测后改写。

6. **禁止在前端通过多次数据库命令模拟 migration transaction。**
   - 多次命令使用不同的短连接，不能保证落在同一个 SQLite connection。
   - 需要事务原子性的结构升级必须留在 Rust 单连接执行器中。

## 发布前数据库回归要求

每个涉及 schema 的 PR 至少验证：

- 全新数据库可直接升级到 `LATEST_SCHEMA_VERSION`。
- 必需表和索引存在。
- 旧版 `user_version` 数据库升级成功。
- 升级前业务数据升级后仍存在。
- migration 重复执行不产生副作用。
- 高于当前支持版本的数据库被明确拒绝。
- 现有入库、出库、库存不足回滚测试继续通过。

GitHub Actions 当前通过 `.github/workflows/inventory-tests.yml` 执行 Rust `--lib` 回归测试，因此 migration 与库存事务测试共用同一数据库核心门禁。

## 备份与 migration 的关系

恢复历史 `.db` 后，应用重新打开数据库时会先执行 migration，再开放原生数据库访问。因此：

```text
选择历史备份
  -> 关闭当前数据库
  -> 替换数据库文件
  -> Rust migration 升级结构
  -> 打开 SQL pool
  -> integrity_check
```

这允许后续版本继续恢复旧备份，同时保持最新程序所需的 schema。

## 下一版本示例

假设 v2 需要给 `materials` 增加 `specification`：

```rust
Migration {
    version: 2,
    statements: &[
        "ALTER TABLE materials ADD COLUMN specification TEXT",
    ],
}
```

同时必须新增测试：构造 v1 数据库、写入至少一条现有物资记录、执行 migration、验证 `user_version = 2`、旧记录完整保留且新字段可用。
