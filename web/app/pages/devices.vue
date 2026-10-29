<script setup lang="ts">
import { Alert, Button, Select, useConfirm, useNotification } from '@stellar/ui'
import AdminShell from '~/components/admin/AdminShell.vue'
import DeviceDetailDrawer from '~/components/devices/DeviceDetailDrawer.vue'
import DeviceTable from '~/components/devices/DeviceTable.vue'
import { errorMessage, useAdminApi } from '~/composables/useAdminApi'
import type { DeviceStatus, DeviceSummary } from '~/types/admin'

const api = useAdminApi()
const notifications = useNotification()
const { confirm } = useConfirm()

const devices = ref<DeviceSummary[]>([])
const statusFilter = ref<DeviceStatus | ''>('')
const nextCursor = ref<string | null>(null)
const loading = ref(false)
const loadingMore = ref(false)
const errorText = ref('')
const selectedDeviceId = ref<string | null>(null)
const detailVisible = ref(false)

const statusOptions = [
  { value: '', label: '全部状态' },
  { value: 'pending', label: '待审批' },
  { value: 'active', label: '已批准' },
  { value: 'rejected', label: '已拒绝' },
  { value: 'disabled', label: '已禁用' },
]
const busy = computed(
  () => loading.value || loadingMore.value || api.pending.value > 0,
)

watch(statusFilter, () => {
  void load(true)
})

onMounted(() => {
  void load(true)
})

async function load(reset: boolean) {
  if (reset) {
    loading.value = true
    nextCursor.value = null
  } else {
    loadingMore.value = true
  }
  errorText.value = ''
  try {
    const page = await api.loadDevices(
      statusFilter.value || null,
      reset ? null : nextCursor.value,
    )
    devices.value = reset ? page.items : [...devices.value, ...page.items]
    nextCursor.value = page.next_cursor
  } catch (error) {
    errorText.value = errorMessage(error, '无法读取设备列表')
  } finally {
    loading.value = false
    loadingMore.value = false
  }
}

function openDetail(device: DeviceSummary) {
  selectedDeviceId.value = device.id
  detailVisible.value = true
}

async function quickAction(
  device: DeviceSummary,
  action: 'approve' | 'reject' | 'disable',
) {
  const labels = {
    approve: {
      title: '批准设备',
      message: `允许 ${device.hostname} 注册并同步策略？`,
    },
    reject: {
      title: '拒绝设备',
      message: `拒绝 ${device.hostname} 的注册请求？`,
    },
    disable: {
      title: '禁用设备',
      message: `禁用 ${device.hostname} 后，现有令牌立即失效。`,
    },
  }[action]
  const confirmed = await confirm({
    title: labels.title,
    message: labels.message,
    confirmText: action === 'approve' ? '批准' : '确认',
    danger: action !== 'approve',
  })
  if (!confirmed) {
    return
  }
  try {
    await api.deviceAction(device.id, action)
    notifications.success(
      action === 'approve'
        ? '设备已批准'
        : action === 'reject'
          ? '设备已拒绝'
          : '设备已禁用',
    )
    await load(true)
  } catch (error) {
    notifications.error(errorMessage(error, '设备操作失败'))
  }
}
</script>

<template>
  <AdminShell title="设备" description="审批设备并查看完整硬件清单">
    <template #actions>
      <Select v-model="statusFilter" :options="statusOptions" size="small" />
      <Button icon="ph:arrows-clockwise" :disabled="busy" @click="load(true)">
        刷新
      </Button>
    </template>

    <div class="flex min-h-0 flex-1 flex-col gap-[var(--spacing-md)]">
      <Alert v-if="errorText" semantic="error">{{ errorText }}</Alert>
      <div
        class="flex items-center justify-between text-[var(--text-sm)] text-[var(--st-text-muted)]"
      >
        <span>已加载 {{ devices.length }} 台设备</span>
        <span v-if="nextCursor">还有更多设备</span>
      </div>
      <div class="min-h-0 flex-1 overflow-auto">
        <DeviceTable
          :devices="devices"
          :loading="loading"
          :busy="busy"
          @select="openDetail"
          @action="quickAction"
        />
      </div>
      <div v-if="nextCursor" class="flex justify-center">
        <Button :disabled="busy" @click="load(false)">
          {{ loadingMore ? '加载中…' : '加载更多' }}
        </Button>
      </div>
    </div>

    <DeviceDetailDrawer
      v-model:visible="detailVisible"
      :device-id="selectedDeviceId"
      @changed="load(true)"
    />
  </AdminShell>
</template>
