import request, { type ApiResponse } from './request'

export interface SftpUser {
  id: number
  username: string
  root_dir: string
  upload_speed: number
  download_speed: number
  enabled: boolean
  permission: string // "rw" | "r"
  ip_whitelist: string[]
  created_at: string | null
  updated_at: string | null
}

export interface SftpUserListQuery {
  keyword?: string
  page?: number
  page_size?: number
}

export interface SftpUserListData {
  total: number
  page: number
  page_size: number
  list: SftpUser[]
}

export interface SftpUserPayload {
  username: string
  password?: string
  root_dir: string
  upload_speed: number
  download_speed: number
  enabled: boolean
  permission?: string // "rw" | "r"，可选，不传默认 "rw"
  ip_whitelist: string[]
}

export function listUsers(params: SftpUserListQuery) {
  return request.get('/sftp/user/list', { params }) as unknown as Promise<
    ApiResponse<SftpUserListData>
  >
}

export function addUser(data: SftpUserPayload) {
  return request.post('/sftp/user/add', data) as unknown as Promise<ApiResponse<unknown>>
}

export function updateUser(data: SftpUserPayload) {
  return request.put('/sftp/user/update', data) as unknown as Promise<ApiResponse<unknown>>
}

export function deleteUser(username: string) {
  return request.delete('/sftp/user/del', {
    params: { username },
  }) as unknown as Promise<ApiResponse<unknown>>
}
