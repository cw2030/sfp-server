<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import {
  addUser,
  deleteUser,
  listUsers,
  updateUser,
  type SftpUser,
  type SftpUserPayload,
} from '@/api/sftpUser'
import { formatBytes, formatSpeed } from '@/utils/format'

const loading = ref(false)
const list = ref<SftpUser[]>([])
const total = ref(0)
const page = ref(1)
const pageSize = ref(20)
const keyword = ref('')

const dialogVisible = ref(false)
const saving = ref(false)
const editing = ref(false)

const emptyForm = () => ({
  username: '',
  password: '',
  root_dir: '/data/sftp',
  upload_speed: 0,
  download_speed: 0,
  permission: 'rw' as 'rw' | 'r',
  enabled: true,
  whitelist: '',
})

const form = reactive(emptyForm())

async function loadData() {
  loading.value = true
  try {
    const res = await listUsers({
      keyword: keyword.value.trim() || undefined,
      page: page.value,
      page_size: pageSize.value,
    })
    list.value = res.data.list
    total.value = res.data.total
  } catch {
    // 错误提示由拦截器统一处理
  } finally {
    loading.value = false
  }
}

function onSearch() {
  page.value = 1
  loadData()
}

function openCreate() {
  Object.assign(form, emptyForm())
  form.root_dir = '/data/sftp/'
  editing.value = false
  dialogVisible.value = true
}

function openEdit(row: SftpUser) {
  Object.assign(form, {
    username: row.username,
    password: '',
    root_dir: row.root_dir,
    upload_speed: row.upload_speed,
    download_speed: row.download_speed,
    permission: (row.permission === 'r' ? 'r' : 'rw') as 'rw' | 'r',
    enabled: row.enabled,
    whitelist: row.ip_whitelist.join('\n'),
  })
  editing.value = true
  dialogVisible.value = true
}

async function onSubmit() {
  if (!form.username.trim()) {
    ElMessage.warning('请输入账号')
    return
  }
  if (!form.root_dir.trim()) {
    ElMessage.warning('请输入根目录绝对路径')
    return
  }
  if (!editing.value && !form.password) {
    ElMessage.warning('请输入密码')
    return
  }

  const payload: SftpUserPayload = {
    username: form.username.trim(),
    root_dir: form.root_dir.trim(),
    upload_speed: Number(form.upload_speed) || 0,
    download_speed: Number(form.download_speed) || 0,
    enabled: form.enabled,
    permission: form.permission,
    ip_whitelist: form.whitelist
      .split(/\r?\n/)
      .map((s) => s.trim())
      .filter(Boolean),
  }
  if (form.password) payload.password = form.password

  saving.value = true
  try {
    if (editing.value) {
      await updateUser(payload)
      ElMessage.success('保存成功')
    } else {
      await addUser(payload)
      ElMessage.success('新增成功')
    }
    dialogVisible.value = false
    loadData()
  } catch {
    // 错误提示由拦截器统一处理
  } finally {
    saving.value = false
  }
}

async function onDelete(row: SftpUser) {
  try {
    await ElMessageBox.confirm(`确定删除用户 ${row.username} 吗？`, '删除确认', {
      type: 'warning',
      confirmButtonText: '删除',
      cancelButtonText: '取消',
    })
  } catch {
    return
  }
  try {
    await deleteUser(row.username)
    ElMessage.success('删除成功')
    if (list.value.length === 1 && page.value > 1) page.value--
    loadData()
  } catch {
    // 错误提示由拦截器统一处理
  }
}

onMounted(loadData)
</script>

<template>
  <div class="page">
    <el-card shadow="never">
      <div class="toolbar">
        <el-input
          v-model="keyword"
          placeholder="按账号搜索"
          clearable
          style="width: 220px"
          @keyup.enter="onSearch"
          @clear="onSearch"
        />
        <el-button @click="onSearch">搜索</el-button>
        <div class="spacer" />
        <el-button type="primary" @click="openCreate">新增用户</el-button>
      </div>

      <el-table v-loading="loading" :data="list" border stripe>
        <el-table-column prop="username" label="账号" min-width="120" />
        <el-table-column prop="root_dir" label="根目录" min-width="200" show-overflow-tooltip />
        <el-table-column label="上传限速" width="120">
          <template #default="{ row }">{{ formatSpeed(row.upload_speed) }}</template>
        </el-table-column>
        <el-table-column label="下载限速" width="120">
          <template #default="{ row }">{{ formatSpeed(row.download_speed) }}</template>
        </el-table-column>
        <el-table-column label="IP白名单" min-width="200">
          <template #default="{ row }">
            <template v-if="row.ip_whitelist.length">
              <el-tag v-for="cidr in row.ip_whitelist" :key="cidr" class="cidr-tag" size="small">
                {{ cidr }}
              </el-tag>
            </template>
            <span v-else class="muted">不限制</span>
          </template>
        </el-table-column>
        <el-table-column label="权限" width="100">
          <template #default="{ row }">
            <el-tag :type="row.permission === 'r' ? 'info' : 'success'" size="small">
              {{ row.permission === 'r' ? '只读' : '读写' }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column label="状态" width="90">
          <template #default="{ row }">
            <el-tag :type="row.enabled ? 'success' : 'info'" size="small">
              {{ row.enabled ? '启用' : '禁用' }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="created_at" label="创建时间" width="170" />
        <el-table-column label="操作" width="140" fixed="right">
          <template #default="{ row }">
            <el-button link type="primary" @click="openEdit(row)">编辑</el-button>
            <el-button link type="danger" @click="onDelete(row)">删除</el-button>
          </template>
        </el-table-column>
      </el-table>

      <div class="pager">
        <el-pagination
          v-model:current-page="page"
          v-model:page-size="pageSize"
          :total="total"
          :page-sizes="[10, 20, 50, 100]"
          layout="total, sizes, prev, pager, next, jumper"
          @current-change="loadData"
          @size-change="onSearch"
        />
      </div>
    </el-card>

    <el-dialog v-model="dialogVisible" :title="editing ? '编辑用户' : '新增用户'" width="580px">
      <el-form label-width="110px">
        <el-form-item label="账号" required>
          <el-input v-model="form.username" :disabled="editing" placeholder="仅字母、数字、_ - ." />
        </el-form-item>
        <el-form-item label="密码" required>
          <el-input
            v-model="form.password"
            type="password"
            show-password
            :placeholder="editing ? '留空表示不修改密码' : '请输入登录密码'"
          />
        </el-form-item>
        <el-form-item label="根目录" required>
          <el-input v-model="form.root_dir" placeholder="/data/sftp/test01" />
        </el-form-item>
        <el-form-item label="上传限速">
          <el-input-number v-model="form.upload_speed" :min="0" :step="1048576" controls-position="right" />
          <span class="hint">byte/s，0 表示不限速（当前 {{ formatBytes(form.upload_speed) }}/s）</span>
        </el-form-item>
        <el-form-item label="下载限速">
          <el-input-number v-model="form.download_speed" :min="0" :step="1048576" controls-position="right" />
          <span class="hint">byte/s，0 表示不限速（当前 {{ formatBytes(form.download_speed) }}/s）</span>
        </el-form-item>
        <el-form-item label="IP白名单">
          <el-input
            v-model="form.whitelist"
            type="textarea"
            :rows="3"
            placeholder="每行一个 IP 或 CIDR，例如：&#10;192.168.1.0/24&#10;10.0.0.8"
          />
          <span class="hint">留空表示不限制来源 IP</span>
        </el-form-item>
        <el-form-item label="读写权限">
          <el-radio-group v-model="form.permission">
            <el-radio value="rw">读写</el-radio>
            <el-radio value="r">只读</el-radio>
          </el-radio-group>
          <span class="hint">只读用户禁止上传、删除、重命名、mkdir、rmdir 等所有写操作</span>
        </el-form-item>
        <el-form-item label="启用">
          <el-switch v-model="form.enabled" />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="dialogVisible = false">取消</el-button>
        <el-button type="primary" :loading="saving" @click="onSubmit">保存</el-button>
      </template>
    </el-dialog>
  </div>
</template>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 16px;
}
.spacer {
  flex: 1;
}
.pager {
  display: flex;
  justify-content: flex-end;
  margin-top: 16px;
}
.cidr-tag {
  margin-right: 6px;
  margin-bottom: 4px;
}
.muted {
  color: #94a3b8;
  font-size: 12px;
}
.hint {
  margin-left: 10px;
  font-size: 12px;
  color: #94a3b8;
}
</style>
