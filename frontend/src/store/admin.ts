import { defineStore } from 'pinia'
import { ref } from 'vue'

export const useAdminStore = defineStore('admin', () => {
  const token = ref<string>(localStorage.getItem('sftp_admin_token') || '')
  const adminName = ref<string>(localStorage.getItem('sftp_admin_name') || '')

  function setLogin(newToken: string, name: string) {
    token.value = newToken
    adminName.value = name
    localStorage.setItem('sftp_admin_token', newToken)
    localStorage.setItem('sftp_admin_name', name)
  }

  function logout() {
    token.value = ''
    adminName.value = ''
    localStorage.removeItem('sftp_admin_token')
    localStorage.removeItem('sftp_admin_name')
  }

  return { token, adminName, setLogin, logout }
})
