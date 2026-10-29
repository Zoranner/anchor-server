<script setup lang="ts">
import { Alert, Button, Card, useConfirm, useNotification } from '@stellar/ui'
import AdminShell from '~/components/admin/AdminShell.vue'
import { errorMessage, useAdminApi } from '~/composables/useAdminApi'
import { formatDateTime } from '~/utils/format'

const api = useAdminApi()
const notifications = useNotification()
const { confirm } = useConfirm()

const key = computed(() => api.signingKey.value)
const busy = computed(() => api.pending.value > 0)

async function rotate() {
  if (busy.value) {
    return
  }
  const confirmed = await confirm({
    title: '轮换签名密钥',
    message:
      '将生成新的活动签名密钥，旧密钥转为退休并保留用于验证已签发策略。继续？',
    confirmText: '轮换',
  })
  if (!confirmed) {
    return
  }
  try {
    const response = await api.rotateSigningKey()
    notifications.success(
      `签名密钥已轮换，旧密钥 ${response.retired_key_id.slice(0, 12)}… 已退休`,
    )
  } catch (error) {
    notifications.error(errorMessage(error, '轮换签名密钥失败'))
  }
}

async function copyPublicKey() {
  if (!key.value || !import.meta.client || !navigator.clipboard) {
    notifications.error('无法访问剪贴板，请手动复制公钥')
    return
  }
  try {
    await navigator.clipboard.writeText(key.value.public_key_hex)
    notifications.success('公钥已复制')
  } catch {
    notifications.error('复制失败，请手动复制公钥')
  }
}
</script>

<template>
  <AdminShell title="签名密钥" description="查看活动签名密钥并完成轮换">
    <div class="grid gap-[var(--spacing-lg)]">
      <Alert v-if="!key" semantic="warning">
        当前没有活动签名密钥。请确认初始化已完成，或检查服务端数据目录和主密钥文件。
      </Alert>

      <Card v-else title="当前活动密钥" :hoverable="false">
        <dl
          class="grid grid-cols-[minmax(6rem,9rem)_minmax(0,1fr)] gap-x-[var(--spacing-md)] gap-y-[var(--spacing-sm)] text-[var(--text-sm)]"
        >
          <dt class="m-0 text-[var(--st-text-muted)]">密钥 ID</dt>
          <dd class="m-0 break-all font-mono">{{ key.key_id }}</dd>
          <dt class="m-0 text-[var(--st-text-muted)]">公钥</dt>
          <dd class="m-0 break-all font-mono">{{ key.public_key_hex }}</dd>
          <dt class="m-0 text-[var(--st-text-muted)]">创建时间</dt>
          <dd class="m-0">{{ formatDateTime(key.created_at) }}</dd>
          <dt class="m-0 text-[var(--st-text-muted)]">激活时间</dt>
          <dd class="m-0">{{ formatDateTime(key.activated_at) }}</dd>
        </dl>
        <div
          class="mt-[var(--spacing-lg)] flex flex-wrap gap-[var(--spacing-sm)]"
        >
          <Button icon="ph:copy" @click="copyPublicKey">复制公钥</Button>
          <Button
            semantic="warning"
            icon="ph:arrows-clockwise"
            :disabled="busy"
            @click="rotate"
          >
            轮换密钥
          </Button>
        </div>
      </Card>

      <Card title="轮换边界" :hoverable="false">
        <div
          class="grid gap-[var(--spacing-sm)] text-[var(--text-sm)] text-[var(--st-text-muted)]"
        >
          <p class="m-0">
            轮换后新策略使用新密钥，旧公钥保留在密钥环中用于验证已经签发的策略。
          </p>
          <p class="m-0">
            客户端下次同步会获取新的活动公钥；已导出的策略包仍由其签名对应的公钥验证。
          </p>
        </div>
      </Card>
    </div>
  </AdminShell>
</template>
