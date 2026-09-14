<script setup lang="ts">
import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import {
  Box,
  DataAnalysis,
  Document,
  Download,
  Goods,
  HomeFilled,
  Position,
  UploadFilled,
} from '@element-plus/icons-vue'
import appIcon from './assets/app-icon.png'

const route = useRoute()
const router = useRouter()

const activePath = computed(() => route.path)

const menu = [
  { path: '/', label: '库存总览', icon: HomeFilled },
  { path: '/materials', label: '物资管理', icon: Goods },
  { path: '/stock-in', label: '入库登记', icon: Download },
  { path: '/stock-out', label: '出库登记', icon: UploadFilled },
  { path: '/inventory', label: '当前库存', icon: Box },
  { path: '/distribution', label: '物资分布', icon: Position },
  { path: '/ledger', label: '出入库明细', icon: Document },
  { path: '/backup', label: '备份与恢复', icon: DataAnalysis },
]
</script>

<template>
  <el-container class="app-shell">
    <el-aside width="220px" class="sidebar">
      <div class="brand">
        <img class="brand-mark" :src="appIcon" alt="物资管理系统图标" />
        <div>
          <strong>物资管理系统</strong>
        </div>
      </div>

      <el-menu
        :default-active="activePath"
        class="nav-menu"
        @select="(path: string) => router.push(path)"
      >
        <el-menu-item v-for="item in menu" :key="item.path" :index="item.path">
          <el-icon><component :is="item.icon" /></el-icon>
          <span>{{ item.label }}</span>
        </el-menu-item>
      </el-menu>

      <div class="sidebar-footer">银河麒麟 V10 · 本地单机版</div>
    </el-aside>

    <el-container>
      <el-header class="topbar">
        <div>
          <h1>{{ route.meta.title || '物资管理系统' }}</h1>
          <p>{{ route.meta.subtitle || '本地物资出入库与库存追溯' }}</p>
        </div>
        <el-tag type="success" effect="plain">离线可用</el-tag>
      </el-header>

      <el-main class="content">
        <router-view v-slot="{ Component }">
          <keep-alive>
            <component :is="Component" />
          </keep-alive>
        </router-view>
      </el-main>
    </el-container>
  </el-container>
</template>
