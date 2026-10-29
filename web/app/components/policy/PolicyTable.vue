<script setup lang="ts">
import { Badge, Button, Table } from '@stellar/ui'
import type { PolicySummary } from '~/types/admin'
import { formatDateTime, shortId } from '~/utils/format'

const props = defineProps<{
  policies: PolicySummary[]
  loading?: boolean
  busy?: boolean
}>()

const emit = defineEmits<{
  select: [policy: PolicySummary]
}>()

const columns = [
  { key: 'version', title: '版本', width: 96 },
  { key: 'status', title: '状态', width: 96 },
  { key: 'expires_at', title: '到期时间', width: 168 },
  { key: 'created_at', title: '创建时间', width: 168 },
  { key: 'signed_at', title: '签名时间', width: 168 },
  { key: 'server_key_id', title: '签名密钥', minWidth: 140, ellipsis: true },
  {
    key: 'actions',
    title: '操作',
    width: 72,
    align: 'right' as const,
    fixed: 'right' as const,
  },
]

const rows = computed(
  () => props.policies as unknown as Record<string, unknown>[],
)

function policyRow(row: Record<string, unknown>): PolicySummary {
  return row as unknown as PolicySummary
}

function statusMeta(status: PolicySummary['status']) {
  return status === 'signed'
    ? { label: '已签名', semantic: 'success' as const }
    : { label: '草稿', semantic: 'info' as const }
}
</script>

<template>
  <Table
    :columns="columns"
    :data="rows"
    :loading="loading"
    empty-text="还没有策略"
    layout="fixed"
    fixed-header
    row-key="id"
    @row-click="emit('select', policyRow($event))"
  >
    <template #cell-version="{ row }">
      <span class="font-mono">{{ policyRow(row).version ?? '草稿' }}</span>
    </template>
    <template #cell-status="{ row }">
      <Badge :semantic="statusMeta(policyRow(row).status).semantic">
        {{ statusMeta(policyRow(row).status).label }}
      </Badge>
    </template>
    <template #cell-expires_at="{ row }">
      {{ formatDateTime(policyRow(row).expires_at) }}
    </template>
    <template #cell-created_at="{ row }">
      {{ formatDateTime(policyRow(row).created_at) }}
    </template>
    <template #cell-signed_at="{ row }">
      {{ formatDateTime(policyRow(row).signed_at) }}
    </template>
    <template #cell-server_key_id="{ row }">
      <span class="font-mono text-[var(--text-xs)]">
        {{ shortId(policyRow(row).server_key_id) }}
      </span>
    </template>
    <template #cell-actions="{ row }">
      <div class="flex justify-end">
        <Button
          size="small"
          square
          icon="ph:eye"
          title="查看策略"
          aria-label="查看策略"
          :disabled="busy"
          @click.stop="emit('select', policyRow(row))"
        />
      </div>
    </template>
  </Table>
</template>
