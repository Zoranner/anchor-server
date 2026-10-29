<script setup lang="ts">
import { Badge } from '@stellar/ui'
import type { DeviceStatus } from '~/types/admin'

const props = defineProps<{
  status: DeviceStatus
}>()

const meta = computed(() => {
  switch (props.status) {
    case 'active':
      return { label: '已批准', semantic: 'success' as const }
    case 'rejected':
      return { label: '已拒绝', semantic: 'error' as const }
    case 'disabled':
      return { label: '已禁用', semantic: 'warning' as const }
    default:
      return { label: '待审批', semantic: 'info' as const }
  }
})
</script>

<template>
  <Badge :semantic="meta.semantic">{{ meta.label }}</Badge>
</template>
