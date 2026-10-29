<script setup lang="ts">
import { Button, Card, Input, Select } from '@stellar/ui'
import type { PolicyForm } from '~/utils/policy'
import { emptyUsbAllowForm } from '~/utils/policy'

const model = defineModel<PolicyForm['usb']>({ required: true })

const actionOptions = [
  { value: 'deny', label: '拒绝' },
  { value: 'allow', label: '允许' },
]
</script>

<template>
  <Card title="USB 存储" :hoverable="false">
    <div class="grid gap-[var(--spacing-lg)]">
      <p class="m-0 text-[var(--text-sm)] text-[var(--st-text-muted)]">
        允许规则中的所有已填写字段必须同时匹配；留空的行不会提交。
      </p>
      <div class="grid gap-[var(--spacing-md)] md:grid-cols-2">
        <Select v-model="model.read" label="读取" :options="actionOptions" />
        <Select v-model="model.write" label="写入" :options="actionOptions" />
      </div>

      <div class="grid gap-[var(--spacing-sm)]">
        <div class="flex items-center justify-between gap-[var(--spacing-sm)]">
          <strong class="text-[var(--text-sm)]">允许规则</strong>
          <Button
            size="small"
            icon="ph:plus"
            @click="model.allowed.push(emptyUsbAllowForm())"
          >
            添加规则
          </Button>
        </div>
        <div
          v-for="(rule, index) in model.allowed"
          :key="index"
          class="grid gap-[var(--spacing-sm)] rounded-[var(--radius-md)] border border-[var(--st-border-divider)] p-[var(--spacing-md)] md:grid-cols-2 xl:grid-cols-[repeat(4,minmax(0,1fr))_auto]"
        >
          <Input
            v-model="rule.instance_id"
            label="实例 ID"
            placeholder="可选"
          />
          <Input v-model="rule.vendor_id" label="VID" placeholder="可选" />
          <Input v-model="rule.product_id" label="PID" placeholder="可选" />
          <Input v-model="rule.serial" label="序列号" placeholder="可选" />
          <div class="flex items-end">
            <Button
              square
              semantic="error"
              icon="ph:trash"
              title="移除规则"
              aria-label="移除规则"
              @click="model.allowed.splice(index, 1)"
            />
          </div>
        </div>
        <p
          v-if="model.allowed.length === 0"
          class="m-0 text-[var(--text-sm)] text-[var(--st-text-muted)]"
        >
          尚未添加允许规则。读取和写入均为拒绝时，可以保持为空。
        </p>
      </div>
    </div>
  </Card>
</template>
