# KylinStock 技术架构设计

> Version: 1.0
> Target: Kylin Linux Desktop V10 JICAI / Phytium D2000 / ARM64

## 1. Architecture Decision

KylinStock is a local-first single-machine desktop inventory application. The first release deliberately avoids a web server, Java runtime, external database service and mandatory network dependency.

Recommended stack:

- Vue 3 + TypeScript
- Vite
- Element Plus
- Tauri 1.8（银河麒麟 V10 / WebKitGTK 4.0 兼容基线）
- Rust
- SQLite
- Rust 原生 SQLite integration（短连接查询与专用事务命令）
- XLSX-compatible export library selected after ARM64/Kylin compatibility validation

Runtime topology:

`UKUI Desktop -> KylinStock(Tauri) -> Vue UI -> Tauri Commands/SQL -> SQLite`

## 2. Design Principles

1. Offline-first: all core operations work without Internet.
2. Local persistence: business data is stored on the designated machine.
3. Traceability: stock changes must have business records.
4. Transaction safety: stock and transaction records change atomically.
5. Simple deployment: desktop icon, double-click launch.
6. ARM64 first: dependencies must be validated for Phytium/Kylin.
7. No premature ERP complexity.

## 3. Logical Layers

### Presentation
Vue pages, Element Plus components, forms, tables, filters and dialogs.

### Application
Use cases such as create material, stock-in, stock-out, query ledger, export and backup.

### Domain
Inventory rules: positive quantity, no negative stock, destination tracking, immutable history semantics.

### Infrastructure
SQLite persistence, filesystem backup, export generation, Tauri desktop integration.

## 4. Main Modules

- Dashboard
- Materials
- Stock In
- Stock Out
- Current Inventory
- Inventory Distribution
- Transaction Ledger
- Query & Export
- Backup & Restore
- Settings

## 5. Data Safety

Stock-in and stock-out must execute inside database transactions. A stock-out transaction must verify available stock before mutation. The ledger record and inventory balance must commit or roll back together.

Business records should not be physically deleted in normal UI flows. Materials referenced by historical transactions use disable/archive semantics.

## 6. Backup Strategy

SQLite database backup is treated as a first-class feature. Support:

- manual snapshot backup;
- annual labelled backup;
- backup metadata;
- restore confirmation;
- backup integrity checks where practical;
- copying backup files to removable media.

## 7. Deployment

Primary target is an ARM64 package for Kylin V10. Final packaging format will be selected after validation on the customer's real machine. Candidate formats include `.deb` and other Tauri-supported Linux bundles.

The delivered UX is:

`Desktop icon -> double click -> KylinStock window -> local database`

No boot auto-start is required in V1.

## 8. Compatibility Gate

Before final delivery validate on the real target machine:

- Tauri/WebKitGTK startup;
- ARM64 native libraries;
- SQLite read/write;
- Chinese input/display;
- file chooser and export;
- backup/restore filesystem permissions;
- desktop icon/menu integration;
- install/uninstall;
- application restart and OS reboot persistence.
