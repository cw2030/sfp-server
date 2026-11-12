import request, { type ApiResponse } from './request'

export interface GlobalConfig {
  global_ip_whitelist: string[]
  max_global_conn: number
  max_user_conn: number
  idle_timeout_sec: number
}

export function getGlobalConfig() {
  return request.get('/config/global') as unknown as Promise<ApiResponse<GlobalConfig>>
}

export function updateGlobalConfig(data: GlobalConfig) {
  return request.put('/config/global', data) as unknown as Promise<ApiResponse<unknown>>
}
