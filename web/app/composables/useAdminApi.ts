import { useState } from '#app'
import type {
  AdminMeResponse,
  AdminSessionResponse,
  AdminUser,
  ApiErrorBody,
  ApiPage,
  AssignPolicyRequest,
  DeviceActionResponse,
  DeviceDetail,
  DeviceStatus,
  DeviceSummary,
  PolicyDetail,
  PolicyDraftRequest,
  PolicyStatus,
  PolicySummary,
  RotateSigningKeyResponse,
  SigningKey,
} from '~/types/admin'

const PROTOCOL_VERSION = '1'
const CSRF_COOKIE = 'anchor_admin_csrf'

export class AdminApiError extends Error {
  readonly status: number
  readonly code: string

  constructor(status: number, code: string, message: string) {
    super(message)
    this.name = 'AdminApiError'
    this.status = status
    this.code = code
  }
}

interface AdminState {
  authenticated: boolean
  admin: AdminUser | null
  signingKey: SigningKey | null
  csrfToken: string
}

const initialState = (): AdminState => ({
  authenticated: false,
  admin: null,
  signingKey: null,
  csrfToken: '',
})

export function useAdminApi() {
  const state = useState<AdminState>('anchor-admin-state', initialState)
  const pending = useState<number>('anchor-admin-pending', () => 0)
  const restored = useState<boolean>('anchor-admin-restored', () => false)
  const restoreError = useState<string>('anchor-admin-restore-error', () => '')

  const authenticated = computed(() => state.value.authenticated)
  const admin = computed(() => state.value.admin)
  const signingKey = computed(() => state.value.signingKey)
  const csrfToken = computed(() => state.value.csrfToken)

  function protocolHeaders(
    extra: Record<string, string> = {},
  ): Record<string, string> {
    return {
      'X-Anchor-Protocol': PROTOCOL_VERSION,
      ...extra,
    }
  }

  function setSession(payload: AdminMeResponse) {
    state.value = {
      authenticated: true,
      admin: payload.admin,
      signingKey: payload.signing_key,
      csrfToken: cookieValue(CSRF_COOKIE),
    }
    restoreError.value = ''
  }

  function clearSession() {
    state.value = initialState()
  }

  async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
    const response = await fetch(path, {
      ...init,
      credentials: 'same-origin',
      headers: protocolHeaders({
        ...(init.body ? { 'Content-Type': 'application/json' } : {}),
        ...(init.headers as Record<string, string> | undefined),
      }),
    })
    if (!response.ok) {
      throw await parseApiError(response)
    }
    if (response.status === 204) {
      return undefined as T
    }
    return (await response.json()) as T
  }

  async function adminFetch<T>(
    path: string,
    init: RequestInit = {},
  ): Promise<T> {
    if (!state.value.authenticated) {
      throw new AdminApiError(401, 'unauthorized', '管理会话尚未建立')
    }
    const method = (init.method ?? 'GET').toUpperCase()
    if (!isSafeMethod(method) && !state.value.csrfToken) {
      throw new AdminApiError(
        403,
        'csrf_invalid',
        '缺少 CSRF Cookie，请重新登录',
      )
    }

    pending.value += 1
    try {
      const response = await fetch(path, {
        ...init,
        credentials: 'same-origin',
        headers: protocolHeaders({
          ...(init.body ? { 'Content-Type': 'application/json' } : {}),
          ...(!isSafeMethod(method)
            ? { 'X-CSRF-Token': state.value.csrfToken }
            : {}),
          ...(init.headers as Record<string, string> | undefined),
        }),
      })
      if (!response.ok) {
        const error = await parseApiError(response)
        if (response.status === 401) {
          clearSession()
        }
        throw error
      }
      if (response.status === 204) {
        return undefined as T
      }
      return (await response.json()) as T
    } finally {
      pending.value -= 1
    }
  }

  async function restoreSession(): Promise<boolean> {
    if (restored.value) {
      return state.value.authenticated
    }
    restored.value = true
    if (!import.meta.client) {
      return false
    }

    try {
      const response = await fetch('/api/admin/me', {
        credentials: 'same-origin',
        headers: protocolHeaders(),
      })
      if (response.status === 401) {
        clearSession()
        return false
      }
      if (!response.ok) {
        throw await parseApiError(response)
      }
      const payload = (await response.json()) as AdminMeResponse
      setSession(payload)
      return true
    } catch (error) {
      clearSession()
      restoreError.value = errorMessage(error, '无法连接管理服务端')
      return false
    }
  }

  async function login(
    username: string,
    password: string,
  ): Promise<AdminSessionResponse> {
    const payload = await request<AdminSessionResponse>('/api/admin/login', {
      method: 'POST',
      body: JSON.stringify({ username, password }),
    })
    setSession(payload)
    restored.value = true
    return payload
  }

  async function setup(payload: {
    setup_token: string
    username: string
    password: string
    signing_key_hex?: string
  }): Promise<AdminSessionResponse> {
    const response = await request<AdminSessionResponse>('/api/admin/setup', {
      method: 'POST',
      body: JSON.stringify({
        ...payload,
        signing_key_hex: payload.signing_key_hex?.trim() || null,
      }),
    })
    setSession(response)
    restored.value = true
    return response
  }

  async function logout(): Promise<void> {
    try {
      if (state.value.authenticated) {
        await adminFetch<void>('/api/admin/logout', { method: 'POST' })
      }
    } finally {
      clearSession()
    }
  }

  async function rotateSigningKey(): Promise<RotateSigningKeyResponse> {
    const response = await adminFetch<RotateSigningKeyResponse>(
      '/api/admin/signing-keys/rotate',
      { method: 'POST' },
    )
    state.value = {
      ...state.value,
      signingKey: response.signing_key,
    }
    return response
  }

  async function loadDevices(
    status: DeviceStatus | null,
    cursor: string | null = null,
  ): Promise<ApiPage<DeviceSummary>> {
    const query = new URLSearchParams({ limit: '50' })
    if (status) {
      query.set('status', status)
    }
    if (cursor) {
      query.set('cursor', cursor)
    }
    return await adminFetch<ApiPage<DeviceSummary>>(
      `/api/admin/devices?${query.toString()}`,
    )
  }

  async function loadDevice(deviceId: string): Promise<DeviceDetail> {
    return await adminFetch<DeviceDetail>(
      `/api/admin/devices/${encodeURIComponent(deviceId)}`,
    )
  }

  async function deviceAction(
    deviceId: string,
    action: 'approve' | 'reject' | 'disable' | 'reset-token',
  ): Promise<DeviceActionResponse> {
    return await adminFetch<DeviceActionResponse>(
      `/api/admin/devices/${encodeURIComponent(deviceId)}/${action}`,
      { method: 'POST' },
    )
  }

  async function loadPolicies(
    status: PolicyStatus | null,
    cursor: string | null = null,
  ): Promise<ApiPage<PolicySummary>> {
    const query = new URLSearchParams({ limit: '50' })
    if (status) {
      query.set('status', status)
    }
    if (cursor) {
      query.set('cursor', cursor)
    }
    return await adminFetch<ApiPage<PolicySummary>>(
      `/api/admin/policies?${query.toString()}`,
    )
  }

  async function loadPolicy(policyId: string): Promise<PolicyDetail> {
    return await adminFetch<PolicyDetail>(
      `/api/admin/policies/${encodeURIComponent(policyId)}`,
    )
  }

  async function createPolicy(
    request: PolicyDraftRequest,
  ): Promise<PolicyDetail> {
    return await adminFetch<PolicyDetail>('/api/admin/policies', {
      method: 'POST',
      body: JSON.stringify(request),
    })
  }

  async function signPolicy(policyId: string): Promise<PolicyDetail> {
    return await adminFetch<PolicyDetail>(
      `/api/admin/policies/${encodeURIComponent(policyId)}/sign`,
      { method: 'POST' },
    )
  }

  async function assignPolicy(
    policyId: string,
    request: AssignPolicyRequest,
  ): Promise<void> {
    await adminFetch<void>(
      `/api/admin/policies/${encodeURIComponent(policyId)}/assign`,
      {
        method: 'POST',
        body: JSON.stringify(request),
      },
    )
  }

  async function exportPolicy(policyId: string): Promise<void> {
    if (!import.meta.client) {
      return
    }
    pending.value += 1
    try {
      const response = await fetch(
        `/api/admin/policies/${encodeURIComponent(policyId)}/export`,
        {
          credentials: 'same-origin',
          headers: protocolHeaders(),
        },
      )
      if (!response.ok) {
        throw await parseApiError(response)
      }
      const blob = await response.blob()
      const disposition = response.headers.get('content-disposition') ?? ''
      const filename =
        /filename="([^"]+)"/.exec(disposition)?.[1] ??
        `anchor-guard-policy-${policyId}.json`
      const url = URL.createObjectURL(blob)
      const anchor = document.createElement('a')
      anchor.href = url
      anchor.download = filename
      anchor.click()
      URL.revokeObjectURL(url)
    } finally {
      pending.value -= 1
    }
  }

  async function loadActiveDevices(): Promise<DeviceSummary[]> {
    const page = await loadDevices('active')
    return page.items
  }

  return {
    state,
    pending,
    restored,
    restoreError,
    authenticated,
    admin,
    signingKey,
    csrfToken,
    request,
    adminFetch,
    restoreSession,
    login,
    setup,
    logout,
    rotateSigningKey,
    loadDevices,
    loadDevice,
    deviceAction,
    loadPolicies,
    loadPolicy,
    createPolicy,
    signPolicy,
    assignPolicy,
    exportPolicy,
    loadActiveDevices,
    clearSession,
  }
}

export function errorMessage(error: unknown, fallback: string): string {
  if (error instanceof AdminApiError) {
    return error.code ? `${error.message}（${error.code}）` : error.message
  }
  return error instanceof Error && error.message ? error.message : fallback
}

async function parseApiError(response: Response): Promise<AdminApiError> {
  let body: ApiErrorBody | null = null
  try {
    body = (await response.json()) as ApiErrorBody
  } catch {
    // 非 JSON 错误响应使用状态文本。
  }
  return new AdminApiError(
    response.status,
    body?.error?.code ?? 'http_error',
    body?.error?.message ??
      (response.statusText || `请求失败（${response.status}）`),
  )
}

function isSafeMethod(method: string): boolean {
  return method === 'GET' || method === 'HEAD' || method === 'OPTIONS'
}

function cookieValue(name: string): string {
  if (!import.meta.client) {
    return ''
  }
  const prefix = `${name}=`
  const entry = document.cookie
    .split(';')
    .map((value) => value.trim())
    .find((value) => value.startsWith(prefix))
  return entry ? decodeURIComponent(entry.slice(prefix.length)) : ''
}
