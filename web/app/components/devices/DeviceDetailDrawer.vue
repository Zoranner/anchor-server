<script setup lang="ts">
import {
  Alert,
  Button,
  Drawer,
  Tag,
  useConfirm,
  useNotification,
} from '@stellar/ui'
import InventoryPanel from '~/components/devices/InventoryPanel.vue'
import StatusBadge from '~/components/devices/StatusBadge.vue'
import { errorMessage, useAdminApi } from '~/composables/useAdminApi'
import type { DeviceDetail } from '~/types/admin'
import { formatDateTime, shortId } from '~/utils/format'

const props = defineProps<{
  visible: boolean
  deviceId: string | null
}>()

const emit = defineEmits<{
  'update:visible': [visible: boolean]
  changed: []
}>()

const api = useAdminApi()
const notifications = useNotification()
const { confirm } = useConfirm()

const detail = ref<DeviceDetail | null>(null)
const loading = ref(false)
const actionBusy = ref(false)
const errorText = ref('')

const visible = computed({
  get: () => props.visible,
  set: (value: boolean) => emit('update:visible', value),
})
const drawerTitle = computed(() =>
  detail.value ? `设备 ${detail.value.hostname}` : '设备详情',
)
const busy = computed(() => actionBusy.value || api.pending.value > 0)

watch(
  () => [props.visible, props.deviceId] as const,
  ([isVisible, deviceId]) => {
    if (isVisible && deviceId) {
      void load(deviceId)
    }
  },
  { immediate: true },
)

async function load(deviceId = props.deviceId) {
  if (!deviceId) {
    return
  }
  loading.value = true
  errorText.value = ''
  try {
    detail.value = await api.loadDevice(deviceId)
  } catch (error) {
    detail.value = null
    errorText.value = errorMessage(error, '无法读取设备详情')
  } finally {
    loading.value = false
  }
}

async function runAction(
  action: 'approve' | 'reject' | 'disable' | 'reset-token',
) {
  if (!detail.value || busy.value) {
    return
  }
  const messages = {
    approve: {
      title: '批准设备',
      message: `允许 ${detail.value.hostname} 注册并同步策略？`,
      success: '设备已批准',
    },
    reject: {
      title: '拒绝设备',
      message: `拒绝 ${detail.value.hostname} 的注册请求？`,
      success: '设备已拒绝',
    },
    disable: {
      title: '禁用设备',
      message: `禁用 ${detail.value.hostname} 后，现有令牌立即失效。`,
      success: '设备已禁用',
    },
    'reset-token': {
      title: '重置设备令牌',
      message: `重置后 ${detail.value.hostname} 会回到待审批状态，需要重新注册。`,
      success: '设备令牌已重置',
    },
  }[action]

  const confirmed = await confirm({
    title: messages.title,
    message: messages.message,
    confirmText: action === 'approve' ? '批准' : '确认',
    danger:
      action === 'reject' || action === 'disable' || action === 'reset-token',
  })
  if (!confirmed) {
    return
  }

  actionBusy.value = true
  errorText.value = ''
  try {
    await api.deviceAction(detail.value.id, action)
    notifications.success(messages.success)
    await load(detail.value.id)
    emit('changed')
  } catch (error) {
    errorText.value = errorMessage(error, '设备操作失败')
  } finally {
    actionBusy.value = false
  }
}
</script>

<template>
  <Drawer
    v-model:visible="visible"
    :title="drawerTitle"
    size="large"
    :show-footer="false"
  >
    <div class="grid gap-[var(--spacing-lg)] pb-[var(--spacing-xl)]">
      <Alert v-if="errorText" semantic="error">{{ errorText }}</Alert>
      <div
        v-if="loading"
        class="py-[var(--spacing-2xl)] text-center text-[var(--st-text-muted)]"
      >
        正在读取设备详情…
      </div>

      <template v-else-if="detail">
        <div class="flex flex-wrap items-center gap-[var(--spacing-sm)]">
          <StatusBadge :status="detail.status" />
          <Tag size="small" icon="ph:identification-card">
            {{ shortId(detail.installation_id, 18) }}
          </Tag>
          <div class="ml-auto flex flex-wrap gap-[var(--spacing-xs)]">
            <template v-if="detail.status === 'pending'">
              <Button
                size="small"
                variant="solid"
                semantic="success"
                icon="ph:check"
                :disabled="busy"
                @click="runAction('approve')"
              >
                批准
              </Button>
              <Button
                size="small"
                semantic="error"
                icon="ph:x"
                :disabled="busy"
                @click="runAction('reject')"
              >
                拒绝
              </Button>
            </template>
            <Button
              v-if="detail.status === 'active'"
              size="small"
              semantic="warning"
              icon="ph:prohibit"
              :disabled="busy"
              @click="runAction('disable')"
            >
              禁用
            </Button>
            <Button
              v-if="detail.status !== 'pending'"
              size="small"
              icon="ph:arrows-clockwise"
              :disabled="busy"
              @click="runAction('reset-token')"
            >
              重置令牌
            </Button>
          </div>
        </div>

        <div
          class="grid gap-[var(--spacing-sm)] rounded-[var(--radius-md)] border border-[var(--st-border-divider)] p-[var(--spacing-md)] text-[var(--text-sm)] md:grid-cols-2"
        >
          <div class="flex justify-between gap-[var(--spacing-md)]">
            <span class="text-[var(--st-text-muted)]">主机名</span>
            <span class="break-all text-right font-medium">{{
              detail.hostname
            }}</span>
          </div>
          <div class="flex justify-between gap-[var(--spacing-md)]">
            <span class="text-[var(--st-text-muted)]">客户端版本</span>
            <span>{{ detail.client_version ?? '-' }}</span>
          </div>
          <div class="flex justify-between gap-[var(--spacing-md)]">
            <span class="text-[var(--st-text-muted)]">最后在线</span>
            <span>{{ formatDateTime(detail.last_seen_at) }}</span>
          </div>
          <div class="flex justify-between gap-[var(--spacing-md)]">
            <span class="text-[var(--st-text-muted)]">首次上报</span>
            <span>{{ formatDateTime(detail.created_at) }}</span>
          </div>
          <div class="flex justify-between gap-[var(--spacing-md)]">
            <span class="text-[var(--st-text-muted)]">批准时间</span>
            <span>{{ formatDateTime(detail.approved_at) }}</span>
          </div>
          <div class="flex justify-between gap-[var(--spacing-md)]">
            <span class="text-[var(--st-text-muted)]">禁用时间</span>
            <span>{{ formatDateTime(detail.disabled_at) }}</span>
          </div>
          <div class="md:col-span-2">
            <span class="text-[var(--st-text-muted)]">当前分配策略</span>
            <span v-if="detail.effective_policy" class="ml-[var(--spacing-sm)]">
              版本 {{ detail.effective_policy.version }}，到期
              {{ formatDateTime(detail.effective_policy.expires_at) }}
            </span>
            <span v-else class="ml-[var(--spacing-sm)]">没有可用策略</span>
          </div>
          <div class="md:col-span-2">
            <span class="text-[var(--st-text-muted)]">客户端最后拉取</span>
            <span
              v-if="detail.last_policy_fetched_at"
              class="ml-[var(--spacing-sm)]"
            >
              版本 {{ detail.last_policy_version ?? '-' }}，
              {{ formatDateTime(detail.last_policy_fetched_at) }}
            </span>
            <span v-else class="ml-[var(--spacing-sm)]">尚未拉取</span>
          </div>
        </div>

        <InventoryPanel
          :inventory="detail.inventory"
          :captured-at="detail.inventory_captured_at"
          :received-at="detail.inventory_received_at"
        />
      </template>
    </div>
  </Drawer>
</template>
