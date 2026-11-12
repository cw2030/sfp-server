import { createRouter, createWebHistory } from 'vue-router'
import { useAdminStore } from '@/store/admin'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/login',
      name: 'login',
      component: () => import('@/views/Login.vue'),
      meta: { public: true },
    },
    {
      path: '/',
      component: () => import('@/views/Layout.vue'),
      redirect: '/sftp-users',
      children: [
        {
          path: 'sftp-users',
          name: 'sftp-users',
          component: () => import('@/views/SftpUserList.vue'),
          meta: { title: 'SFTP用户管理' },
        },
        {
          path: 'global-config',
          name: 'global-config',
          component: () => import('@/views/GlobalConfig.vue'),
          meta: { title: '全局配置' },
        },
        {
          path: 'audit-logs',
          name: 'audit-logs',
          component: () => import('@/views/AuditLog.vue'),
          meta: { title: '审计日志' },
        },
      ],
    },
  ],
})

router.beforeEach((to) => {
  const store = useAdminStore()
  if (!to.meta.public && !store.token) {
    return { path: '/login', query: { redirect: to.fullPath } }
  }
  if (to.path === '/login' && store.token) {
    return { path: '/sftp-users' }
  }
  return true
})

export default router
