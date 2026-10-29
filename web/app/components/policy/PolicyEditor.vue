<script setup lang="ts">
import { Alert, Card, Input, TabPanel, Tabs } from '@stellar/ui'
import BurningRules from '~/components/policy/BurningRules.vue'
import NetworkRules from '~/components/policy/NetworkRules.vue'
import ScanRules from '~/components/policy/ScanRules.vue'
import UsbRules from '~/components/policy/UsbRules.vue'
import type { PolicyForm } from '~/utils/policy'
import { buildPolicyDraft } from '~/utils/policy'

const model = defineModel<PolicyForm>({ required: true })

defineProps<{
  errors: string[]
}>()

const activeTab = ref('usb')
const preview = computed(() => buildPolicyDraft(model.value))
const previewJson = computed(() =>
  preview.value.request ? JSON.stringify(preview.value.request, null, 2) : '',
)
</script>

<template>
  <div class="grid gap-[var(--spacing-lg)]">
    <Alert v-if="errors.length" semantic="error" title="请先修正以下问题">
      <ul class="m-0 pl-[var(--spacing-lg)]">
        <li v-for="message in errors" :key="message">{{ message }}</li>
      </ul>
    </Alert>

    <Card title="有效期" :hoverable="false">
      <div class="grid gap-[var(--spacing-md)] md:grid-cols-2">
        <Input
          v-model="model.expiresAt"
          type="datetime-local"
          label="到期时间"
          help-text="客户端从拉取时刻起按此时间判断策略是否有效"
        />
        <div class="flex items-end">
          <p class="m-0 text-[var(--text-sm)] text-[var(--st-text-muted)]">
            已签名策略的版本由服务端分配，客户端不能提交版本号。
          </p>
        </div>
      </div>
    </Card>

    <Tabs v-model="activeTab">
      <TabPanel name="usb" label="USB" icon="ph:usb">
        <UsbRules v-model="model.usb" />
      </TabPanel>
      <TabPanel name="network" label="网络" icon="ph:wifi-high">
        <NetworkRules v-model="model.network" />
      </TabPanel>
      <TabPanel name="burning" label="刻录" icon="ph:disc">
        <BurningRules v-model="model.burning" />
      </TabPanel>
      <TabPanel name="scan" label="扫描" icon="ph:magnifying-glass">
        <ScanRules v-model="model.scan" />
      </TabPanel>
    </Tabs>

    <Card title="策略预览" :hoverable="false">
      <p
        class="m-0 mb-[var(--spacing-sm)] text-[var(--text-sm)] text-[var(--st-text-muted)]"
      >
        这是将提交给服务端的内容；保存草稿后不可修改，签名时会重新校验。
      </p>
      <pre
        v-if="previewJson"
        class="m-0 max-h-[24rem] overflow-auto rounded-[var(--radius-md)] bg-[var(--st-bg-elevated)] p-[var(--spacing-md)] font-mono text-[var(--text-xs)] leading-5"
        >{{ previewJson }}</pre>
      <p v-else class="m-0 text-[var(--text-sm)] text-[var(--st-text-muted)]">
        修正上方问题后显示预览。
      </p>
    </Card>
  </div>
</template>
