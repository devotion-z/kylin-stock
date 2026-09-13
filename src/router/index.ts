import { createRouter, createWebHashHistory } from 'vue-router'

const DashboardView = () => import('../views/DashboardView.vue')
const MaterialsView = () => import('../views/MaterialsView.vue')
const StockOperationView = () => import('../views/StockOperationView.vue')
const InventoryView = () => import('../views/InventoryView.vue')
const LedgerView = () => import('../views/LedgerView.vue')
const BackupView = () => import('../views/BackupView.vue')

const routes = [
  { path: '/', component: DashboardView, meta: { title: '库存总览', subtitle: '查看当前库存与近期业务情况' } },
  { path: '/materials', component: MaterialsView, meta: { title: '物资管理', subtitle: '维护物资名称、单位和存放位置' } },
  { path: '/stock-in', component: StockOperationView, meta: { title: '入库登记', subtitle: '登记物资入库并自动增加库存' } },
  { path: '/stock-out', component: StockOperationView, meta: { title: '出库登记', subtitle: '登记物资出库、领用单位与去向' } },
  { path: '/inventory', component: InventoryView, meta: { title: '当前库存', subtitle: '查询当前物资库存' } },
  { path: '/distribution', component: InventoryView, meta: { title: '库存物资分布', subtitle: '查看各存放位置的库存分布' } },
  { path: '/ledger', component: LedgerView, meta: { title: '出入库明细', subtitle: '按名称、时间、单位和去向查询流水' } },
  { path: '/backup', component: BackupView, meta: { title: '备份与恢复', subtitle: '创建即时备份、年度归档或恢复历史数据' } },
]

export default createRouter({ history: createWebHashHistory(), routes })
