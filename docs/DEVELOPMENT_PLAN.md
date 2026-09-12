# KylinStock 开发计划

## Phase 0 - Requirements Baseline

Status: **completed / continuously maintained**.

- REQUIREMENTS.md is the V1 scope baseline.
- New customer feedback increments the document version before implementation.

## Phase 1 - Foundation

Status: **implemented; CI compile validation passed**.

Completed:

- Vue 3 + TypeScript + Vite project skeleton.
- Tauri 1.8 desktop shell for the Kylin V10 WebKitGTK 4.0 baseline.
- Project directory conventions.
- Element Plus application shell/sidebar/topbar.
- SQLite initialization layer and initial schema.
- Rust 原生 SQLite 命令及 Tauri dialog/filesystem allowlist configuration.
- Customer-required centered table styling baseline.
- Main navigation entries for all V1 modules.
- GitHub Actions build gate.
- `npm run build` passes in Linux CI.
- Rust `cargo check` passes in Linux CI.

Remaining target-runtime validation:

- Validate `tauri dev` / packaged desktop startup on Linux.
- Validate SQLite initialization/read/write on the real Kylin machine.
- Add structured logging/error boundary if needed during target-machine testing.

## Phase 2 - Master Data

Status: **implemented and merged to main**.

Completed:

- Materials create/edit/search.
- Units quick creation.
- Locations quick creation.
- Disable/re-enable semantics instead of deleting historical master data.
- Customer-required centered table convention.
- Material query result XLSX export added during Phase 4.

## Phase 3 - Inventory Core

Status: **implemented and merged; transaction implementation hardened during Phase 6**.

Completed:

- Stock-in form.
- Stock-out form and mandatory destination.
- Insufficient-stock guard / no normal negative inventory.
- Per-material/per-location inventory balances.
- Current inventory view.
- Inventory distribution view.
- Transaction ledger.

Hardening:

The initial frontend implementation issued `BEGIN/COMMIT` through multiple SQL plugin calls. The Kylin-compatible build removes that plugin and routes all SQLite access through native Rust commands. Each stock-in/out operation opens one dedicated SQLite connection and executes `BEGIN IMMEDIATE -> ledger mutation -> balance mutation -> COMMIT` on that same connection. Failures roll back before returning to the UI.

Core flow:

`material -> stock in -> inventory -> stock out -> destination -> ledger`

## Phase 4 - Query & Export

Status: **implemented and merged to main; target-machine validation pending**.

Completed:

- Material-name filter.
- Date-range filter.
- Related-unit filter.
- Transaction-type filter.
- Destination filter.
- Combined ledger filters.
- Inventory filters for material name, measurement unit and location.
- Native system save dialog.
- XLSX workbook generation.
- Export current material query results.
- Export current transaction query results.
- Export current inventory/distribution query results.
- “What is queried is what is exported” semantics: export uses the exact rows currently shown in the table.

Validation remaining:

- `.xlsx` opening and Chinese-content validation in target office software.
- Real Kylin V10 / ARM64 save-dialog and file-write test.

## Phase 5 - Backup & Restore

Status: **implemented, CI-green and merged to main**.

Completed:

- Manual backup with native save dialog.
- Annual-labelled full database snapshot.
- Backup history / metadata.
- Native Rust database file backup command.
- SQLite connection pool is closed before file-level backup or restore.
- Native backup uses temporary-file + sync + rename semantics.
- Restore-file SQLite header validation.
- Restore confirmation in UI.
- Automatic pre-restore safety backup.
- Database replacement through temporary and old-file swap.
- Rollback to previous database if restored target cannot be safely synced.
- Post-restore `PRAGMA integrity_check`.
- Automatic attempt to recover the pre-restore safety copy if integrity check fails.
- Database connection reopened after backup/restore lifecycle.
- Linux CI passed TypeScript/Vite build and Rust `cargo check` before merge.

Implementation note:

Native Rust database and backup modules both resolve `kylin-stock.db` under Tauri's `app_config_dir`.

Target-machine validation remaining:

- Manual backup-create test.
- Annual-backup test.
- Modify-data -> restore -> data-recovered test.
- Corrupt/non-SQLite file rejection test.
- USB/removable-media backup target test on Kylin.

## Phase 6 - UX & Data-Safety Hardening

Status: **in progress on `feat/ux-hardening`**.

Implemented / in branch:

- Dashboard backed by real SQLite statistics.
- Real recent-transaction table on dashboard.
- Real stock-overview table on dashboard.
- Local-day “today stock-in / stock-out” statistics.
- Human-readable local date/time formatting in ledger, inventory, backup history and XLSX exports.
- Stock operation form defaults to local desktop time instead of UTC text slicing.
- Stronger stock form validation and no-master-data warning.
- Lazy-loaded business routes to reduce initial application payload.
- Native Rust single-connection inventory transactions to guarantee ledger/balance atomicity.
- Native stock-out performs both pre-check and guarded balance update to prevent negative inventory.

Still planned:

- CI validation of the Phase 6 changes.
- Review empty/loading/error states across all pages.
- Long-text handling review.
- Chinese UI consistency review.
- Table alignment requirement verification.

## Phase 7 - Kylin ARM64 Compatibility

Target machine: Kylin Linux Desktop V10 JICAI, Phytium D2000, ARM64, UKUI.

Validate:

- runtime/WebKitGTK;
- native dependencies;
- SQLite;
- Chinese IME/fonts;
- XLSX export/file picker;
- backup/restore paths;
- removable-media write;
- desktop launcher;
- package installation;
- reboot persistence.

## Phase 8 - Acceptance

Execute all acceptance cases in REQUIREMENTS.md. Record failures and fixes. Produce installation/use notes and final release artifact.

## Scope Guard

Do not introduce procurement, sales, finance, SaaS, complex RBAC, barcode hardware, cloud dependency or multi-machine architecture unless REQUIREMENTS.md is explicitly revised.
