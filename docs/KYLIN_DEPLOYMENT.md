# KylinStock 银河麒麟 V10 / 飞腾 ARM64 部署与验证指南

> 目标设备：银河麒麟桌面操作系统 V10 JICAI / UKUI / Phytium D2000 / ARM64 / 16GB RAM

## 1. 交付目标

最终用户体验：

`麒麟桌面 -> 双击“物资管理系统” -> KylinStock 独立桌面窗口 -> 本地 SQLite 数据库`

第一阶段不要求开机自启动，不依赖互联网，不要求用户手动启动数据库或后台服务。

## 2. 交付原则

1. 最终生产包必须经过目标银河麒麟设备实机验证；
2. GitHub Actions 在原生 ARM64 runner 的 Ubuntu 20.04 容器中构建/安装，以 WebKitGTK 4.0 和较旧 GLIBC 作为兼容基线，但仍不能替代银河麒麟兼容性验收；
3. 不允许为了安装软件破坏客户机器现有系统库；
4. 如银河麒麟系统 WebKitGTK/GTK/GLIBC 与 CI 构建环境存在 ABI 差异，应调整构建基线或在兼容环境重新构建，不直接强行升级客户核心系统库；
5. 首次安装和最终验收都应保留可追溯证据，而不是只口头确认“能打开”。

## 3. 第一次到目标机后的环境诊断

在仓库根目录执行：

```bash
chmod +x scripts/kylin-doctor.sh
./scripts/kylin-doctor.sh | tee kylin-stock-doctor.txt
```

重点记录：

- `uname -m` / CPU 架构；
- 银河麒麟发行版信息；
- GLIBC 版本；
- WebKitGTK 4.0 情况（目标机已确认提供 `libwebkit2gtk-4.0-37`）；
- GTK3；
- OpenSSL；
- 桌面会话与语言环境；
- 如需目标机本机构建，则额外确认 Node.js、Rust、gcc、make、dpkg-deb。

诊断输出保存在项目验收记录中。

## 4. CI ARM64 构建/安装烟测

GitHub Actions 使用原生 `ubuntu-22.04-arm` runner，并在 Ubuntu 20.04 ARM64 容器内构建。正式门禁应覆盖：

1. 确认 runner `uname -m = aarch64`；
2. 安装 Tauri Linux 构建依赖；
3. 安装前端依赖；
4. 原生执行 Tauri ARM64 `.deb` 构建；
5. 验证 Debian `Package` / `Version` / `Architecture` 元数据；
6. 真实执行 `dpkg -i`；
7. 用 `dpkg-query` 确认包已安装且架构为 `arm64`；
8. 检查安装包依赖明确为 `libwebkit2gtk-4.0-37`，并检查已安装可执行文件及 `ldd` 缺失动态库；
9. 检查 Linux desktop entry，并确认用户看到的名称为“物资管理系统”；
10. 在 Xvfb 虚拟显示器中真实启动应用并确认不会立即退出；
11. 上传短期保存的 smoke artifact。

Linux 系统包身份使用 ASCII 名称 `kylin-stock`；用户界面/桌面入口仍使用中文“物资管理系统”。

该 CI 产物的意义是：验证 KylinStock 的 Rust/Tauri/Node 依赖能够在 Linux ARM64 上原生构建，并验证生成 Debian 包至少能在 CI 基线系统完成真实安装。

**该 CI 产物仍不是默认的客户正式交付包。** 银河麒麟 V10 目标机可能具有不同的 GLIBC、WebKitGTK、GTK 和系统库版本，最终包必须通过目标机实测。

## 5. 真机验收证据采集

正式候选包安装后执行：

```bash
chmod +x scripts/kylin-acceptance-evidence.sh
./scripts/kylin-acceptance-evidence.sh | tee kylin-acceptance-evidence.txt
```

该脚本只读，不安装软件、不修改数据库、不主动启动应用。它用于采集：

- 系统版本、架构、内核、CPU；
- 桌面和语言环境；
- GLIBC / WebKitGTK / GTK 等运行时线索；
- 已安装 KylinStock 包；
- desktop entry；
- 可执行文件和动态链接状态；
- SQLite 数据文件位置、大小和 SHA256；
- 当目标机装有 `sqlite3` CLI 时，额外记录 `PRAGMA user_version` / `quick_check` 和基础行数。

**脚本输出不能替代业务验收。** 完整的 AC-01 ~ AC-23 操作步骤和预期结果见：

`docs/ACCEPTANCE_CHECKLIST.md`

## 6. 真机业务验收

正式候选包至少覆盖以下类别：

- 安装、桌面入口、中文显示与中文输入；
- 新增/编辑物资和同名物资保护；
- 常用计量单位可直接选择，自定义单位/位置可输入后回车创建；
- 入库、出库、去向记录、库存不足回滚；
- 物资、入库、出库备注可添加单据图片，流水中可回看，备份恢复后图片仍存在；
- 数量最多两位小数且不四舍五入、业务日期仅年月日、单位/去向选项可新增搜索；
- 应用只能运行一个实例，窗口与应用菜单显示正式仓库图标；
- 当前库存和库存物资分布；
- 名称、时间、单位、类型、去向以及多条件组合查询；
- “查询什么，就导出什么”；
- 出入库明细、当前库存/分布分别导出；
- `.xlsx` 在客户实际麒麟办公软件中打开；
- 所有业务表格表头和单元格居中；
- 即时备份、年度备份、数据恢复；
- 退出、断网和电脑重启后的数据持久化。

现场结果必须记录在 `docs/ACCEPTANCE_CHECKLIST.md` 的副本中，并归档关键截图、导出样例、备份样例和 evidence 输出。

## 7. 数据目录

Rust 原生数据库命令将 `kylin-stock.db` 放在应用 `app_config_dir` 下。库存事务、普通查询、数据库 migration 与备份/恢复模块必须定位同一业务数据库。

现场不得根据猜测手工移动或修改数据库。需要迁移数据时优先使用系统提供的“备份与恢复”功能。

## 8. 数据安全验收

### 入库事务

`BEGIN IMMEDIATE -> 写入入库流水 -> 增加库存 -> COMMIT`

### 出库事务

`BEGIN IMMEDIATE -> 检查库存 -> 写入出库流水 -> 条件扣减库存 -> COMMIT`

任何中间错误必须回滚。

### 数据库升级

`读取 user_version -> BEGIN IMMEDIATE -> migration -> 更新 user_version -> COMMIT`

升级失败不得把半迁移数据库暴露给业务页面；高于当前程序支持版本的数据库应拒绝由旧程序继续写入。

### 恢复流程

`关闭数据库连接 -> 恢复前安全副本 -> 临时恢复文件（含单据图片） -> 文件替换 -> migration/reopen -> integrity_check`

恢复失败时应优先回退到恢复前安全副本。

## 9. 发布候选包与归档

正式交付时建议归档一个固定目录：

```text
KylinStock-Release-0.1.0/
├── kylin-stock_0.1.0_arm64.deb
├── SHA256SUMS.txt
├── kylin-stock-doctor.txt
├── kylin-acceptance-evidence.txt
├── ACCEPTANCE_CHECKLIST-filled.md
├── screenshots/
├── export-samples/
└── backup-sample/
```

CI 生成的测试构建和客户正式候选包应明确区分，避免把未完成目标机验证的 smoke artifact 误当成最终交付件。

## 10. 最终发布门槛

只有同时满足以下条件，才标记为客户正式 Release：

- x64 Linux TypeScript/Vite + Rust build-check 通过；
- 核心数据库回归测试通过；
- ARM64 Linux native `.deb` build + `dpkg -i` install smoke 通过；
- 银河麒麟 V10 + 飞腾 D2000 实机安装通过；
- `AC-01 ~ AC-23` 全部通过，或未通过项已有明确问题单与客户确认；
- 查询/导出、备份/恢复和重启持久化通过；
- 正式安装包 SHA256、验收证据和最终数据库备份完成归档。
