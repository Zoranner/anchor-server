import type {
  ActionPolicy,
  PolicyDraftRequest,
  PolicyPayload,
  ScanRuleKind,
} from '../types/admin'

export interface UsbAllowForm {
  instance_id: string
  vendor_id: string
  product_id: string
  serial: string
}

export interface NetworkAllowForm {
  adapter_id: string
}

export interface BurningAllowForm {
  drive_id: string
}

export interface ScanRuleForm {
  rule_id: string
  kind: ScanRuleKind
  pattern: string
}

export interface PolicyForm {
  expiresAt: string
  usb: {
    read: ActionPolicy
    write: ActionPolicy
    allowed: UsbAllowForm[]
  }
  network: {
    policy: ActionPolicy
    allowed: NetworkAllowForm[]
  }
  burning: {
    policy: ActionPolicy
    allowed: BurningAllowForm[]
  }
  scan: {
    controlledDirs: string
    fileTypes: string
    rules: ScanRuleForm[]
  }
}

export interface PolicyFormResult {
  request: PolicyDraftRequest | null
  errors: string[]
}

export function emptyUsbAllowForm(): UsbAllowForm {
  return {
    instance_id: '',
    vendor_id: '',
    product_id: '',
    serial: '',
  }
}

export function emptyNetworkAllowForm(): NetworkAllowForm {
  return { adapter_id: '' }
}

export function emptyBurningAllowForm(): BurningAllowForm {
  return { drive_id: '' }
}

export function emptyScanRuleForm(index = 0): ScanRuleForm {
  return {
    rule_id: `r-${index + 1}`,
    kind: 'keyword',
    pattern: '',
  }
}

export function defaultExpiryInput(now = new Date()): string {
  const value = new Date(now)
  value.setDate(value.getDate() + 7)
  value.setSeconds(0, 0)
  return toLocalDateTimeInput(value)
}

export function emptyPolicyForm(now = new Date()): PolicyForm {
  return {
    expiresAt: defaultExpiryInput(now),
    usb: {
      read: 'deny',
      write: 'deny',
      allowed: [],
    },
    network: {
      policy: 'deny',
      allowed: [],
    },
    burning: {
      policy: 'deny',
      allowed: [],
    },
    scan: {
      controlledDirs: '',
      fileTypes: '',
      rules: [],
    },
  }
}

export function policyFormFromPayload(payload: PolicyPayload): PolicyForm {
  return {
    expiresAt: toLocalDateTimeInput(payload.expires_at),
    usb: {
      read: payload.usb.read,
      write: payload.usb.write,
      allowed: payload.usb.allowed.map((rule) => ({
        instance_id: rule.instance_id ?? '',
        vendor_id: rule.vendor_id ?? '',
        product_id: rule.product_id ?? '',
        serial: rule.serial ?? '',
      })),
    },
    network: {
      policy: payload.network.policy,
      allowed: payload.network.allowed.map((rule) => ({
        adapter_id: rule.adapter_id,
      })),
    },
    burning: {
      policy: payload.burning.policy,
      allowed: payload.burning.allowed.map((rule) => ({
        drive_id: rule.drive_id,
      })),
    },
    scan: {
      controlledDirs: payload.scan.controlled_dirs.join('\n'),
      fileTypes: payload.scan.file_types.join(', '),
      rules: payload.scan.rules.map((rule) => ({
        rule_id: rule.rule_id,
        kind: rule.kind,
        pattern: rule.pattern,
      })),
    },
  }
}

export function buildPolicyDraft(form: PolicyForm): PolicyFormResult {
  const errors: string[] = []
  const expiresAt = parseExpiry(form.expiresAt, errors)
  const usbAllowed = form.usb.allowed
    .map((rule) => {
      const instanceId = optionalValue(rule.instance_id)
      const vendorId = optionalValue(rule.vendor_id)
      const productId = optionalValue(rule.product_id)
      const serial = optionalValue(rule.serial)
      if (!instanceId && !vendorId && !productId && !serial) {
        return null
      }
      return {
        instance_id: instanceId,
        vendor_id: vendorId,
        product_id: productId,
        serial,
      }
    })
    .filter((rule): rule is NonNullable<typeof rule> => rule !== null)

  if (
    (form.usb.read === 'allow' || form.usb.write === 'allow') &&
    usbAllowed.length === 0
  ) {
    errors.push('USB 允许读取或写入时，至少需要一条允许规则。')
  }

  const networkAllowed = form.network.allowed
    .map((rule) => ({ adapter_id: rule.adapter_id.trim() }))
    .filter((rule) => rule.adapter_id.length > 0)
  if (form.network.policy === 'allow' && networkAllowed.length === 0) {
    errors.push('网络策略为允许时，至少需要一条允许规则。')
  }

  const burningAllowed = form.burning.allowed
    .map((rule) => ({ drive_id: rule.drive_id.trim() }))
    .filter((rule) => rule.drive_id.length > 0)
  if (form.burning.policy === 'allow' && burningAllowed.length === 0) {
    errors.push('刻录策略为允许时，至少需要一条允许规则。')
  }

  const controlledDirs = form.scan.controlledDirs
    .split(/\r?\n/)
    .map((value) => value.trim())
    .filter((value) => value.length > 0)
  const fileTypes = [
    ...new Set(
      form.scan.fileTypes
        .split(/[\s,]+/)
        .map((value) => value.trim().toLowerCase().replace(/^\./, ''))
        .filter((value) => value.length > 0),
    ),
  ]
  const scanRules = form.scan.rules.map((rule, index) => {
    const ruleId = rule.rule_id.trim() || `r-${index + 1}`
    const pattern = rule.pattern.trim()
    if (!pattern) {
      errors.push(`扫描规则 ${ruleId} 的内容不能为空。`)
    }
    return {
      rule_id: ruleId,
      kind: rule.kind,
      pattern,
    }
  })

  return {
    request: errors.length
      ? null
      : {
          expires_at: expiresAt,
          usb: {
            read: form.usb.read,
            write: form.usb.write,
            allowed: usbAllowed,
          },
          network: {
            policy: form.network.policy,
            allowed: networkAllowed,
          },
          burning: {
            policy: form.burning.policy,
            allowed: burningAllowed,
          },
          scan: {
            controlled_dirs: controlledDirs,
            file_types: fileTypes,
            rules: scanRules,
          },
        },
    errors,
  }
}

export function toLocalDateTimeInput(value: string | Date): string {
  const date = value instanceof Date ? value : new Date(value)
  if (Number.isNaN(date.getTime())) {
    return ''
  }
  const pad = (part: number) => String(part).padStart(2, '0')
  return [
    date.getFullYear(),
    '-',
    pad(date.getMonth() + 1),
    '-',
    pad(date.getDate()),
    'T',
    pad(date.getHours()),
    ':',
    pad(date.getMinutes()),
  ].join('')
}

function parseExpiry(value: string, errors: string[]): string {
  if (!value) {
    errors.push('请选择策略到期时间。')
    return ''
  }
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) {
    errors.push('策略到期时间格式无效。')
    return ''
  }
  if (date.getTime() <= Date.now()) {
    errors.push('策略到期时间必须晚于当前时间。')
    return ''
  }
  return date.toISOString()
}

function optionalValue(value: string): string | null {
  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : null
}
