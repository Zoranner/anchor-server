import { describe, expect, test } from 'bun:test'
import { formatBytes } from '../app/utils/format'
import {
  buildPolicyDraft,
  emptyPolicyForm,
  policyFormFromPayload,
} from '../app/utils/policy'
import type { PolicyPayload } from '../app/types/admin'

describe('policy form', () => {
  test('builds a deny-all draft with a future expiry', () => {
    const result = buildPolicyDraft(emptyPolicyForm())
    expect(result.errors).toEqual([])
    expect(result.request).not.toBeNull()
    expect(result.request?.usb).toEqual({
      read: 'deny',
      write: 'deny',
      allowed: [],
    })
    expect(result.request?.network.policy).toBe('deny')
    expect(result.request?.burning.policy).toBe('deny')
    expect(Date.parse(result.request?.expires_at ?? '')).toBeGreaterThan(
      Date.now(),
    )
  })

  test('rejects allow policies without matching allow rules', () => {
    const form = emptyPolicyForm()
    form.usb.read = 'allow'
    form.network.policy = 'allow'
    form.burning.policy = 'allow'

    const result = buildPolicyDraft(form)

    expect(result.request).toBeNull()
    expect(result.errors.some((message) => message.includes('USB'))).toBe(true)
    expect(result.errors.some((message) => message.includes('网络'))).toBe(true)
    expect(result.errors.some((message) => message.includes('刻录'))).toBe(true)
  })

  test('normalizes scan values and round-trips a signed payload', () => {
    const payload: PolicyPayload = {
      version: 3,
      expires_at: new Date(Date.now() + 86_400_000).toISOString(),
      usb: {
        read: 'allow',
        write: 'deny',
        allowed: [
          {
            instance_id: 'USBSTOR\\Disk',
            vendor_id: null,
            product_id: null,
            serial: null,
          },
        ],
      },
      network: { policy: 'deny', allowed: [] },
      burning: { policy: 'deny', allowed: [] },
      scan: {
        controlled_dirs: ['D:\\data'],
        file_types: ['docx'],
        rules: [{ rule_id: 'r-1', kind: 'keyword', pattern: '机密' }],
      },
    }

    const form = policyFormFromPayload(payload)
    form.scan.fileTypes = '.DOCX, xlsx'
    form.scan.controlledDirs = 'D:\\data\n\nE:\\out'
    const result = buildPolicyDraft(form)

    expect(result.errors).toEqual([])
    expect(result.request?.scan.file_types).toEqual(['docx', 'xlsx'])
    expect(result.request?.scan.controlled_dirs).toEqual([
      'D:\\data',
      'E:\\out',
    ])
    expect(result.request?.usb.allowed[0]?.instance_id).toBe('USBSTOR\\Disk')
  })

  test('rejects an empty or past expiry', () => {
    const form = emptyPolicyForm()
    form.expiresAt = ''
    expect(
      buildPolicyDraft(form).errors.some((message) => message.includes('到期')),
    ).toBe(true)

    form.expiresAt = '2020-01-01T00:00'
    expect(
      buildPolicyDraft(form).errors.some((message) => message.includes('晚于')),
    ).toBe(true)
  })
})

describe('formatting', () => {
  test('formats byte values', () => {
    expect(formatBytes(0)).toBe('0 B')
    expect(formatBytes(1024)).toBe('1.0 KB')
    expect(formatBytes(1024 * 1024 * 1024)).toBe('1.0 GB')
  })
})
