<script setup lang="ts">
import { Button, Card, Input, Select, Textarea } from '@stellar/ui'
import type { PolicyForm } from '~/utils/policy'
import { emptyScanRuleForm } from '~/utils/policy'

const model = defineModel<PolicyForm['scan']>({ required: true })

const kindOptions = [
  { value: 'keyword', label: '关键字' },
  { value: 'regex', label: '正则表达式' },
]
</script>

<template>
  <Card title="文件扫描" :hoverable="false">
    <div class="grid gap-[var(--spacing-lg)]">
      <p class="m-0 text-[var(--text-sm)] text-[var(--st-text-muted)]">
        扫描只记录风险，不隔离或删除文件。扩展名不区分大小写，不需要带点。
      </p>
      <Textarea
        v-model="model.controlledDirs"
        label="受控目录"
        :rows="3"
        placeholder="每行一个目录，例如 D:\\data"
      />
      <Input
        v-model="model.fileTypes"
        label="扫描扩展名"
        placeholder="例如 docx, xlsx, pdf；留空表示扫描所有支持类型"
      />

      <div class="grid gap-[var(--spacing-sm)]">
        <div class="flex items-center justify-between gap-[var(--spacing-sm)]">
          <strong class="text-[var(--text-sm)]">识别规则</strong>
          <Button
            size="small"
            icon="ph:plus"
            @click="model.rules.push(emptyScanRuleForm(model.rules.length))"
          >
            添加规则
          </Button>
        </div>
        <div
          v-for="(rule, index) in model.rules"
          :key="index"
          class="grid gap-[var(--spacing-sm)] rounded-[var(--radius-md)] border border-[var(--st-border-divider)] p-[var(--spacing-md)] lg:grid-cols-[minmax(7rem,10rem)_minmax(8rem,12rem)_minmax(0,1fr)_auto]"
        >
          <Input v-model="rule.rule_id" label="规则 ID" />
          <Select v-model="rule.kind" label="类型" :options="kindOptions" />
          <Input
            v-model="rule.pattern"
            label="匹配内容"
            placeholder="关键字或正则表达式"
          />
          <div class="flex items-end">
            <Button
              square
              semantic="error"
              icon="ph:trash"
              title="移除规则"
              aria-label="移除规则"
              @click="model.rules.splice(index, 1)"
            />
          </div>
        </div>
        <p
          v-if="model.rules.length === 0"
          class="m-0 text-[var(--text-sm)] text-[var(--st-text-muted)]"
        >
          尚未添加扫描规则。
        </p>
      </div>
    </div>
  </Card>
</template>
