# KylinStock

KylinStock 是一套面向银河麒麟桌面环境的轻量级单机物资出入库管理软件。

目标场景是：在一台专用银河麒麟电脑上，通过桌面图标双击启动，完成物资建档、入库、出库、库存、去向追溯、组合查询、Excel 导出以及数据备份/恢复。

## Target

当前客户目标环境：

- 银河麒麟桌面操作系统 V10 JICAI
- UKUI
- Phytium D2000 / ARM64 (AArch64)
- 16 GB RAM
- 单机、本地、离线优先

## Stack

- Vue 3
- TypeScript
- Vite
- Element Plus
- Tauri 1.8（兼容银河麒麟 V10 的 WebKitGTK 4.0）
- Rust
- SQLite
- Rust 原生 SQLite 命令 / Tauri Dialog / Filesystem APIs
- SheetJS XLSX

## Core Features

- 物资品名管理
- 计量单位和存放位置
- 内置常用计量单位（发、支、具、件、套、枚、米、公斤、箱），并支持现场新增
- 入库登记
- 出库登记
- 出库去向记录
- 库存不足阻止出库
- 当前库存
- 库存物资分布
- 出入库流水
- 物资备注、入库备注和出库备注可附加单据图片，并可在流水中回看
- 名称 / 时间 / 单位 / 类型 / 去向组合查询
- 当前查询结果 XLSX 导出
- 物资明细导出
- 出入库明细导出
- 库存物资分布导出
- 即时数据备份
- 年度归档备份
- 恢复前自动安全副本
- 数据恢复后 SQLite 完整性检查

## Data Safety

库存写操作由 Rust 原生命令使用单独 SQLite connection 执行：

```text
BEGIN IMMEDIATE
  -> transaction ledger mutation
  -> inventory balance mutation
COMMIT
```

中间任一步骤失败都会回滚。

单据图片以 BLOB 形式随业务记录保存在 SQLite 中，不依赖原图片路径，因此会随即时/年度备份一起备份和恢复。恢复业务数据时会先生成恢复前安全副本，再执行数据库替换和完整性检查。

## Development Status

- Phase 0：需求基线 ✅
- Phase 1：工程骨架 + Linux CI ✅
- Phase 2：物资基础资料 ✅
- Phase 3：出入库核心闭环 ✅
- Phase 4：组合查询 + XLSX 导出 ✅
- Phase 5：备份与恢复 ✅
- Phase 6：UX / 数据一致性加固 ✅（Linux CI 已通过）
- Phase 7：银河麒麟 ARM64 真机兼容与打包 ⏳
- Phase 8：客户验收 ⏳

## Documentation

- [`docs/REQUIREMENTS.md`](docs/REQUIREMENTS.md) — 客户需求基线
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — 技术架构
- [`docs/DATABASE_DESIGN.md`](docs/DATABASE_DESIGN.md) — 数据库设计
- [`docs/DEVELOPMENT_PLAN.md`](docs/DEVELOPMENT_PLAN.md) — 阶段开发计划
- [`docs/KYLIN_DEPLOYMENT.md`](docs/KYLIN_DEPLOYMENT.md) — 银河麒麟 ARM64 部署与验收

## Local Development

```bash
npm install
npm run tauri:dev
```

Frontend build check:

```bash
npm run build
```

Rust compile check:

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

## Kylin Environment Doctor

在目标麒麟机器进行兼容性验收前：

```bash
chmod +x scripts/kylin-doctor.sh
./scripts/kylin-doctor.sh | tee kylin-stock-doctor.txt
```

## CI

GitHub Actions currently validates:

1. x64 Linux TypeScript/Vite build + Rust `cargo check`;
2. native Linux ARM64 Tauri `.deb` packaging smoke test.

ARM64 CI 产物只用于编译和打包烟测。最终客户正式安装包必须通过银河麒麟 V10 + 飞腾 D2000 实机验证后才能发布。

## Scope

V1 是轻量物资管理系统，不包含采购 ERP、财务、销售、在线支付、SaaS、多级复杂审批、扫码硬件或第三方 ERP/WMS 集成。新增范围必须先更新需求文档。
