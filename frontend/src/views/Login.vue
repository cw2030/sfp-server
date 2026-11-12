<script setup lang="ts">
import { reactive, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'
import { login } from '@/api/admin'
import { useAdminStore } from '@/store/admin'

const router = useRouter()
const route = useRoute()
const store = useAdminStore()

const form = reactive({ admin_name: '', password: '' })
const loading = ref(false)

async function onSubmit() {
  if (!form.admin_name.trim() || !form.password) {
    ElMessage.warning('请输入管理员账号和密码')
    return
  }
  loading.value = true
  try {
    const res = await login({ admin_name: form.admin_name.trim(), password: form.password })
    store.setLogin(res.data.token, res.data.admin_name)
    ElMessage.success('登录成功')
    const redirect = typeof route.query.redirect === 'string' ? route.query.redirect : '/sftp-users'
    router.replace(redirect)
  } catch {
    // 错误提示已在拦截器统一处理
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <div class="login-page">
    <div class="login-card">
      <h1>SFTP 管理后台</h1>
      <p class="sub">账号与 SFTP 用户完全隔离，请使用管理员账号登录</p>
      <el-form label-position="top" @submit.prevent="onSubmit">
        <el-form-item label="管理员账号">
          <el-input v-model="form.admin_name" placeholder="admin" size="large" autocomplete="username" />
        </el-form-item>
        <el-form-item label="密码">
          <el-input
            v-model="form.password"
            type="password"
            show-password
            placeholder="请输入密码"
            size="large"
            autocomplete="current-password"
            @keyup.enter="onSubmit"
          />
        </el-form-item>
        <el-button type="primary" size="large" class="submit" :loading="loading" @click="onSubmit">
          登录
        </el-button>
      </el-form>
    </div>
  </div>
</template>

<style scoped>
.login-page {
  min-height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  background: linear-gradient(135deg, #1f2937 0%, #334155 45%, #0f766e 100%);
}
.login-card {
  width: 380px;
  padding: 36px 32px 28px;
  border-radius: 14px;
  background: #fff;
  box-shadow: 0 20px 60px rgba(15, 23, 42, 0.35);
}
h1 {
  margin: 0 0 8px;
  font-size: 22px;
  color: #0f172a;
}
.sub {
  margin: 0 0 24px;
  font-size: 12px;
  color: #94a3b8;
  line-height: 1.6;
}
.submit {
  width: 100%;
  margin-top: 8px;
}
</style>
