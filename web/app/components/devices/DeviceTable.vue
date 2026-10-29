<script setup lang="ts">
import { Button, Table } from '@stellar/ui'
import StatusBadge from '~/components/devices/StatusBadge.vue'
import type { DeviceSummary } from '~/types/admin'
import { formatDateTime } from '~/utils/format'

const props = defineProps<{
  devices: DeviceSummary[]
  loading?: boolean
  busy?: boolean
}>()

const emit = defineEmits<{
  select: [device: DeviceSummary]
  action: [device: DeviceSummary, action: 'approve' | 'reject' | 'disable']
}>()

const columns = [
  { key: 'hostname', title: '设备', minWidth: 220, ellipsis: true },
  { key: 'status', title: '状态', width: 96 },
  { key: 'system', title: '系统', minWidth: 180, ellipsis: true },
  { key: 'last_seen', title: '最后在线', width: 168 },
  { key: 'client_version', title: '客户端', width: 96 },
  { key: 'created_at', title: '首次上报', width: 168 },
  {
    key: 'actions',
    title: '操作',
    width: 132,
    align: 'right' as const,
    fixed: 'right' as const,
  },
]

const rows = computed(
  () => props.devices as unknown as Record<string, unknown>[],
)

function deviceRow(row: Record<string, unknown>): DeviceSummary {
  return row as unknown as DeviceSummary
}

function systemLabel(device: DeviceSummary): string {
  const summary = device.inventory_summary
  if (!summary) {
    return '未上报'
  }
  return `${summary.os_name} ${summary.os_version}`.trim()
}

function open(device: DeviceSummary) {
  emit('select', device)
}
</script>

<template>
  <Table
    :columns="columns"
    :data="rows"
    :loading="loading"
    empty-text="没有符合条件的设备"
    layout="fixed"
    fixed-header
    row-key="id"
    @row-click="open(deviceRow($event))"
  >
    <template #cell-hostname="{ row }">
      <div class="grid min-w-0 gap-[var(--spacing-2xs)]">
        <strong class="truncate">{{ deviceRow(row).hostname }}</strong>
        <span
          class="truncate font-mono text-[var(--text-xs)] text-[var(--st-text-muted)]"
        >
          {{ deviceRow(row).installation_id }}
        </span>
      </div>
    </template>
    <template #cell-status="{ row }">
      <StatusBadge :status="deviceRow(row).status" />
    </template>
    <template #cell-system="{ row }">
      {{ systemLabel(deviceRow(row)) }}
    </template>
    <template #cell-last_seen="{ row }">
      {{ formatDateTime(deviceRow(row).last_seen_at) }}
    </template>
    <template #cell-client_version="{ row }">
      {{ deviceRow(row).client_version ?? '-' }}
    </template>
    <template #cell-created_at="{ row }">
      {{ formatDateTime(deviceRow(row).created_at) }}
    </template>
    <template #cell-actions="{ row }">
      <div class="flex justify-end gap-[var(--spacing-xs)]">
        <template v-if="deviceRow(row).status === 'pending'">
          <Button
            size="small"
            square
            variant="solid"
            semantic="success"
            icon="ph:check"
            title="批准设备"
            aria-label="批准设备"
            :disabled="busy"
            @click.stop="emit('action', deviceRow(row), 'approve')"
          />
          <Button
            size="small"
            square
            semantic="error"
            icon="ph:x"
            title="拒绝设备"
            aria-label="拒绝设备"
            :disabled="busy"
            @click.stop="emit('action', deviceRow(row), 'reject')"
          />
        </template>
        <Button
          v-else-if="deviceRow(row).status === 'active'"
          size="small"
          square
          semantic="warning"
          icon="ph:prohibit"
          title="禁用设备"
          aria-label="禁用设备"
          :disabled="busy"
          @click.stop="emit('action', deviceRow(row), 'disable')"
        />
        <Button
          size="small"
          square
          icon="ph:eye"
          title="查看详情"
          aria-label="查看详情"
          @click.stop="open(deviceRow(row))"
        />
      </div>
    </template>
  </Table>
</template>
