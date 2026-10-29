<script setup lang="ts">
import { Button, Card, Input, Select } from '@stellar/ui'
import type { PolicyForm } from '~/utils/policy'
import { emptyNetworkAllowForm } from '~/utils/policy'

const model = defineModel<PolicyForm['network']>({ required: true })

const actionOptions = [
  { value: 'deny', label: '拒绝' },
  { value: 'allow', label: '允许' },
]
</script>

<template>
  <Card title="网络" :hoverable="false">
    <div class="grid gap-[var(--spacing-lg)]">
      <p class="m-0 text-[var(--text-sm)] text-[var(--st-text-muted)]">
        拒绝时关闭未列入允许清单的网卡；允许时只保留清单中的网卡。
      </p>
      <Select v-model="model.policy" label="策略" :options="actionOptions" />

      <div class="grid gap-[var(--spacing-sm)]">
        <div class="flex items-center justify-between gap-[var(--spacing-sm)]">
          <strong class="text-[var(--text-sm)]">允许的网卡</strong>
          <Button
            size="small"
            icon="ph:plus"
            @click="model.allowed.push(emptyNetworkAllowForm())"
          >
            添加网卡
          </Button>
        </div>
        <div
          v-for="(rule, index) in model.allowed"
          :key="index"
          class="flex items-end gap-[var(--spacing-sm)]"
        >
          <div class="min-w-0 flex-1">
            <Input
              v-model="rule.adapter_id"
              label="网卡实例 ID"
              placeholder="例如 PCI\\VEN_..."
            />
          </div>
          <Button
            square
            semantic="error"
            icon="ph:trash"
            title="移除网卡"
            aria-label="移除网卡"
            @click="model.allowed.splice(index, 1)"
          />
        </div>
        <p
          v-if="model.allowed.length === 0"
          class="m-0 text-[var(--text-sm)] text-[var(--st-text-muted)]"
        >
          当前没有允许的网卡。
        </p>
      </div>
    </div>
  </Card>
</template>
