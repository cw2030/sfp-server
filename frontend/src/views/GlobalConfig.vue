<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { getGlobalConfig, updateGlobalConfig } from '@/api/config'

const loading = ref(false)
const saving = ref(false)

const form = reactive({
  whitelist: '',
  max_global_conn: 0,
  max_user_conn: 0,
  idle_timeout_sec: 600,
})

async function loadData() {
  loading.value = true
  try {
    const res = await getGlobalConfig()
    form.whitelist = res.data.global_ip_whitelist.join('\n')
    form.max_global_conn = res.data.max_global_conn
    form.max_user_conn = res.data.max_user_conn
    form.idle_timeout_sec = res.data.idle_timeout_sec
  } catch {
    // 错误提示由拦截器统一处理
  } finally {
    loading.value = false
  }
}

async function onSave() {
  saving.value = true
  try {
    await updateGlobalConfig({
      global_ip_whitelist: form.whitelist
        .split(/\r?\n/)
        .map((s) => s.trim())
        .filter(Boolean),
      max_global_conn: Number(form.max_global_conn) || 0,
      max_user_conn: Number(form.max_user_conn) || 0,
      idle_timeout_sec: Number(form.idle_timeout_sec) || 600,
    })
    ElMessage.success('全局配置已保存，热生效')
    loadData()
  } catch {
    // 错误提示由拦截器统一处理
  } finally {
    saving.value = false
  }
}

onMounted(loadData)
</script>

<template>
  <div class="page">
    <el-card v-loading="loading" shadow="never" header="全局配置">
      <el-form label-width="140px" class="form">
        <el-form-item label="全局IP白名单">
          <el-input
            v-model="form.whitelist"
            type="textarea"
            :rows="5"
            placeholder="每行一个 IP 或 CIDR，例如：&#10;192.168.0.0/16&#10;127.0.0.1/32"
          />
          <div class="hint">
            在 SSH 握手前校验，命中任意一条才允许连接；留空表示不限制所有来源 IP
          </div>
        </el-form-item>
        <el-form-item label="全局最大连接数">
          <el-input-number v-model="form.max_global_conn" :min="0" controls-position="right" />
          <span class="hint">0 表示不限制</span>
        </el-form-item>
        <el-form-item label="单用户最大会话">
          <el-input-number v-model="form.max_user_conn" :min="0" controls-position="right" />
          <span class="hint">0 表示不限制</span>
        </el-form-item>
        <el-form-item label="空闲超时（秒）">
          <el-input-number v-model="form.idle_timeout_sec" :min="30" controls-position="right" />
          <span class="hint">无任何数据交互超过该时长后自动断开，最小 30 秒</span>
        </el-form-item>
        <el-form-item>
          <el-button type="primary" :loading="saving" @click="onSave">保存配置</el-button>
        </el-form-item>
      </el-form>
    </el-card>
  </div>
</template>

<style scoped>
.form {
  max-width: 720px;
}
.hint {
  font-size: 12px;
  color: #94a3b8;
  line-height: 1.6;
  margin-top: 4px;
}
</style>
