<script setup lang="ts">
import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ElMessageBox } from 'element-plus'
import { useAdminStore } from '@/store/admin'

const route = useRoute()
const router = useRouter()
const store = useAdminStore()

const activeMenu = computed(() => route.path)
const pageTitle = computed(() => (route.meta.title as string) || 'SFTP 管理后台')

async function onLogout() {
  try {
    await ElMessageBox.confirm('确定退出登录吗？', '提示', {
      confirmButtonText: '退出',
      cancelButtonText: '取消',
      type: 'warning',
    })
  } catch {
    return
  }
  store.logout()
  router.replace('/login')
}
</script>

<template>
  <el-container class="layout">
    <el-aside width="220px" class="aside">
      <div class="brand">SFTP 管理后台</div>
      <el-menu :default-active="activeMenu" router background-color="#111827" text-color="#cbd5e1" active-text-color="#38bdf8">
        <el-menu-item index="/sftp-users">SFTP用户管理</el-menu-item>
        <el-menu-item index="/global-config">全局配置</el-menu-item>
        <el-menu-item index="/audit-logs">审计日志</el-menu-item>
      </el-menu>
    </el-aside>
    <el-container>
      <el-header class="header">
        <span class="title">{{ pageTitle }}</span>
        <div class="right">
          <span class="admin">{{ store.adminName || 'admin' }}</span>
          <el-button link type="primary" @click="onLogout">退出登录</el-button>
        </div>
      </el-header>
      <el-main class="main">
        <router-view />
      </el-main>
    </el-container>
  </el-container>
</template>

<style scoped>
.layout {
  min-height: 100vh;
}
.aside {
  background: #111827;
  color: #e2e8f0;
}
.brand {
  height: 60px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 16px;
  font-weight: 600;
  color: #f8fafc;
  border-bottom: 1px solid #1f2937;
}
.aside :deep(.el-menu) {
  border-right: none;
}
.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  background: #fff;
  border-bottom: 1px solid #e5e7eb;
}
.title {
  font-size: 16px;
  font-weight: 600;
  color: #0f172a;
}
.right {
  display: flex;
  align-items: center;
  gap: 12px;
}
.admin {
  font-size: 13px;
  color: #475569;
}
.main {
  background: #f1f5f9;
}
</style>
