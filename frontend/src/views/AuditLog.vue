<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue'
import { listAuditLogs, type AuditLog } from '@/api/audit'
import { formatBytes, formatSpeed } from '@/utils/format'

const opTypes = [
  { label: '登录', value: 'login' },
  { label: '上传', value: 'upload' },
  { label: '下载', value: 'download' },
  { label: '删除', value: 'rm' },
  { label: '重命名', value: 'rename' },
  { label: '创建目录', value: 'mkdir' },
  { label: '断开连接', value: 'disconnect' },
]

const loading = ref(false)
const list = ref<AuditLog[]>([])
const total = ref(0)
const page = ref(1)
const pageSize = ref(20)
const filters = reactive({
  username: '',
  op_type: '',
  range: [] as string[],
})

async function loadData() {
  loading.value = true
  try {
    const res = await listAuditLogs({
      username: filters.username.trim() || undefined,
      op_type: filters.op_type || undefined,
      start_time: filters.range?.[0] || undefined,
      end_time: filters.range?.[1] || undefined,
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

function onReset() {
  filters.username = ''
  filters.op_type = ''
  filters.range = []
  page.value = 1
  loadData()
}

onMounted(loadData)
</script>

<template>
  <div class="page">
    <el-card shadow="never">
      <div class="toolbar">
        <el-input
          v-model="filters.username"
          placeholder="用户名"
          clearable
          style="width: 160px"
          @keyup.enter="onSearch"
        />
        <el-select v-model="filters.op_type" placeholder="操作类型" clearable style="width: 150px">
          <el-option v-for="op in opTypes" :key="op.value" :label="op.label" :value="op.value" />
        </el-select>
        <el-date-picker
          v-model="filters.range"
          type="datetimerange"
          range-separator="至"
          start-placeholder="开始时间"
          end-placeholder="结束时间"
          value-format="YYYY-MM-DD HH:mm:ss"
          style="width: 360px"
        />
        <el-button type="primary" @click="onSearch">查询</el-button>
        <el-button @click="onReset">重置</el-button>
      </div>

      <el-table v-loading="loading" :data="list" border stripe>
        <el-table-column prop="created_at" label="时间" width="170" />
        <el-table-column prop="username" label="用户" width="110" />
        <el-table-column prop="client_ip" label="客户端IP" width="130" />
        <el-table-column prop="op_type" label="操作" width="100">
          <template #default="{ row }">
            <el-tag size="small">{{ row.op_type }}</el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="file_path" label="文件/说明" min-width="220" show-overflow-tooltip />
        <el-table-column label="文件大小" width="100">
          <template #default="{ row }">{{ formatBytes(row.file_size) }}</template>
        </el-table-column>
        <el-table-column label="传输量" width="100">
          <template #default="{ row }">{{ formatBytes(row.transferred_bytes) }}</template>
        </el-table-column>
        <el-table-column label="平均速度" width="120">
          <template #default="{ row }">{{ formatSpeed(row.speed_bps) }}</template>
        </el-table-column>
        <el-table-column label="结果" width="90">
          <template #default="{ row }">
            <el-tag :type="row.result === 'success' ? 'success' : 'danger'" size="small">
              {{ row.result === 'success' ? '成功' : '失败' }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="msg" label="备注" min-width="160" show-overflow-tooltip />
        <el-table-column prop="session_id" label="会话ID" width="120" show-overflow-tooltip />
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
  </div>
</template>

<style scoped>
.toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  margin-bottom: 16px;
}
.pager {
  display: flex;
  justify-content: flex-end;
  margin-top: 16px;
}
</style>
