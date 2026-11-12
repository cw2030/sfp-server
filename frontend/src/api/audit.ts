import request, { type ApiResponse } from './request'

export interface AuditLog {
  id: number
  session_id: string
  username: string | null
  client_ip: string
  op_type: string
  file_path: string | null
  file_size: number
  transferred_bytes: number
  speed_bps: number
  result: string
  msg: string | null
  created_at: string | null
}

export interface AuditLogQuery {
  username?: string
  op_type?: string
  start_time?: string
  end_time?: string
  page?: number
  page_size?: number
}

export interface AuditLogListData {
  total: number
  page: number
  page_size: number
  list: AuditLog[]
}

export function listAuditLogs(params: AuditLogQuery) {
  return request.get('/audit/log', { params }) as unknown as Promise<
    ApiResponse<AuditLogListData>
  >
}
