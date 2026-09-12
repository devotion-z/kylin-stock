# KylinStock 数据库设计 V1

## 1. 目标

数据库采用 SQLite，围绕“物资 + 库存余额 + 出入库流水 + 去向 + 备份记录”建模。界面不要求商品编号，但数据库内部使用主键 ID。

## 2. Core Tables

### materials

- id INTEGER PRIMARY KEY
- name TEXT NOT NULL
- unit_id INTEGER
- category TEXT
- default_location_id INTEGER
- remark TEXT
- status INTEGER NOT NULL DEFAULT 1
- created_at TEXT NOT NULL
- updated_at TEXT NOT NULL

Material name should be unique among active records unless later requirements explicitly allow duplicate names/specifications.

### units

- id INTEGER PRIMARY KEY
- name TEXT NOT NULL UNIQUE
- status INTEGER NOT NULL DEFAULT 1

### locations

- id INTEGER PRIMARY KEY
- name TEXT NOT NULL UNIQUE
- remark TEXT
- status INTEGER NOT NULL DEFAULT 1

### inventory_balances

- id INTEGER PRIMARY KEY
- material_id INTEGER NOT NULL
- location_id INTEGER NOT NULL
- quantity NUMERIC NOT NULL DEFAULT 0
- updated_at TEXT NOT NULL
- UNIQUE(material_id, location_id)

### stock_transactions

Unified immutable business ledger.

- id INTEGER PRIMARY KEY
- transaction_no TEXT NOT NULL UNIQUE
- type TEXT NOT NULL (`IN`, `OUT`, `ADJUST`)
- material_id INTEGER NOT NULL
- location_id INTEGER NOT NULL
- quantity NUMERIC NOT NULL
- occurred_at TEXT NOT NULL
- related_unit TEXT
- destination TEXT
- handler TEXT
- receiver TEXT
- remark TEXT
- created_at TEXT NOT NULL

For OUT records, `destination` stores the outbound destination. For IN records, `related_unit` may represent source/supplier when used.

### backup_records

- id INTEGER PRIMARY KEY
- file_name TEXT NOT NULL
- file_path TEXT NOT NULL
- backup_type TEXT NOT NULL (`MANUAL`, `ANNUAL`)
- backup_year INTEGER
- file_size INTEGER
- checksum TEXT
- created_at TEXT NOT NULL
- remark TEXT

### app_settings

- key TEXT PRIMARY KEY
- value TEXT
- updated_at TEXT NOT NULL

### business_options

保存来源/领用单位和出库去向的本机可搜索选项；历史流水仍保存文本快照，不因选项库后续调整而改变。

- id INTEGER PRIMARY KEY
- kind TEXT NOT NULL (`RELATED_UNIT`, `DESTINATION`)
- name TEXT NOT NULL
- status INTEGER NOT NULL DEFAULT 1
- UNIQUE(kind, name)

### attachments

物资备注与出入库流水的单据图片附件。图片内容直接存入数据库，保证备份、恢复和迁移时不会因为原文件路径变化而丢失。

- id INTEGER PRIMARY KEY
- entity_type TEXT NOT NULL (`MATERIAL`, `TRANSACTION`)
- entity_id INTEGER NOT NULL
- file_name TEXT NOT NULL
- mime_type TEXT NOT NULL
- file_size INTEGER NOT NULL
- data BLOB NOT NULL
- created_at TEXT NOT NULL

应用层校验所属业务记录存在，并限制为常见图片格式、单张 15 MB、每条业务记录最多 10 张。

## 3. Inventory Transaction Rules

### Stock In

Within one SQLite transaction:

1. validate material/location/quantity;
2. insert `stock_transactions(IN)`;
3. insert or increment `inventory_balances`;
4. commit.

### Stock Out

Within one SQLite transaction:

1. validate quantity > 0;
2. read current balance;
3. reject if balance < requested quantity;
4. insert `stock_transactions(OUT)` including destination;
5. decrement `inventory_balances`;
6. commit.

Any failure rolls back the entire operation.

## 4. Query Indexes

Create indexes for common filters:

- materials(name)
- stock_transactions(material_id)
- stock_transactions(occurred_at)
- stock_transactions(type)
- stock_transactions(related_unit)
- stock_transactions(destination)
- inventory_balances(material_id, location_id)
- attachments(entity_type, entity_id)

## 5. Export Semantics

Exports are projections of query results, not separate business data. The export service receives the same normalized filter object used by the on-screen query to guarantee “what is queried is what is exported”.

## 6. Deletion / Audit Semantics

Historical stock transactions are not physically deleted through ordinary application UI. Materials/units/locations referenced by history should be disabled rather than deleted.

## 7. Numeric Precision

Quantity is modeled as NUMERIC rather than assuming integer-only stock because units may include kg, m, etc. UI and Rust both reject values with more than two decimal places instead of rounding them.

## 8. Migration

Schema changes must use versioned migrations. Application startup checks schema version before normal business access. Production data must never be reset as part of an application upgrade.
