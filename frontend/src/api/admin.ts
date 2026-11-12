import request, { type ApiResponse } from './request'

export interface LoginParams {
  admin_name: string
  password: string
}

export interface LoginResult {
  token: string
  admin_name: string
  expires_in: number
}

export function login(data: LoginParams) {
  return request.post('/admin/login', data) as unknown as Promise<ApiResponse<LoginResult>>
}
