<script setup lang="ts">
import { Button, Card, Input, Select } from '@stellar/ui'
import type { PolicyForm } from '~/utils/policy'
import { emptyBurningAllowForm } from '~/utils/policy'

const model = defineModel<PolicyForm['burning']>({ required: true })

const actionOptions = [
  { value: 'deny', label: '拒绝' },
  { value: 'allow', label: '允许' },
]
</script>

<template>
  <Card title="刻录" :hoverable="false">
    <div class="grid gap-[var(--spacing-lg)]">
      <p class="m-0 text-[var(--text-sm)] text-[var(--st-text-muted)]">
        刻录策略作用于光驱设备；允许时需要列出允许使用的光驱。
      </p>
      <Select v-model="model.policy" label="策略" :options="actionOptions" />

      <div class="grid gap-[var(--spacing-sm)]">
        <div class="flex items-center justify-between gap-[var(--spacing-sm)]">
          <strong class="text-[var(--text-sm)]">允许的光驱</strong>
          <Button
            size="small"
            icon="ph:plus"
            @click="model.allowed.push(emptyBurningAllowForm())"
          >
            添加光驱
          </Button>
        </div>
        <div
          v-for="(rule, index) in model.allowed"
          :key="index"
          class="flex items-end gap-[var(--spacing-sm)]"
        >
          <div class="min-w-0 flex-1">
            <Input
              v-model="rule.drive_id"
              label="光驱实例 ID"
              placeholder="例如 SCSI\\CDROM..."
            />
          </div>
          <Button
            square
            semantic="error"
            icon="ph:trash"
            title="移除光驱"
            aria-label="移除光驱"
            @click="model.allowed.splice(index, 1)"
          />
        </div>
        <p
          v-if="model.allowed.length === 0"
          class="m-0 text-[var(--text-sm)] text-[var(--st-text-muted)]"
        >
          当前没有允许的光驱。
        </p>
      </div>
    </div>
  </Card>
</template>
