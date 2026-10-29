<script setup lang="ts">
import {
  Alert,
  Button,
  Card,
  Modal,
  Select,
  Tag,
  TabPanel,
  Tabs,
} from '@stellar/ui'
import type {
  AssignPolicyRequest,
  DeviceSummary,
  PolicyDetail,
} from '~/types/admin'
import { formatDateTime, shortId } from '~/utils/format'

const props = defineProps<{
  detail: PolicyDetail
  activeDevices: DeviceSummary[]
  busy?: boolean
}>()

const emit = defineEmits<{
  sign: []
  assign: [request: AssignPolicyRequest]
  export: []
}>()

const activeTab = ref('rules')
const assignVisible = ref(false)
const targetKind = ref<'default' | 'device'>('default')
const selectedDeviceId = ref('')
const assignError = ref('')

const targetOptions = [
  { value: 'default', label: '默认策略' },
  { value: 'device', label: '指定设备' },
]
const deviceOptions = computed(() =>
  props.activeDevices.map((device) => ({
    value: device.id,
    label: `${device.hostname}（${device.inventory_summary?.computer_model || '未知型号'}）`,
  })),
)
const isSigned = computed(() => props.detail.status === 'signed')

function policyValue(value: 'allow' | 'deny'): string {
  return value === 'allow' ? '允许' : '拒绝'
}

function policySemantic(value: 'allow' | 'deny') {
  return value === 'allow' ? ('success' as const) : ('warning' as const)
}

function openAssign() {
  targetKind.value = 'default'
  selectedDeviceId.value = ''
  assignError.value = ''
  assignVisible.value = true
}

function submitAssign() {
  assignError.value = ''
  if (targetKind.value === 'default') {
    emit('assign', { is_default: true })
    assignVisible.value = false
    return
  }
  if (!selectedDeviceId.value) {
    assignError.value = '请选择一台已批准设备。'
    return
  }
  emit('assign', { device_id: selectedDeviceId.value })
  assignVisible.value = false
}
</script>

<template>
  <div class="grid gap-[var(--spacing-lg)]">
    <div class="flex flex-wrap items-center gap-[var(--spacing-sm)]">
      <Tag :semantic="isSigned ? 'success' : 'info'" size="small">
        {{ isSigned ? `已签名 v${detail.version}` : '草稿' }}
      </Tag>
      <Tag size="small" icon="ph:calendar"
        >到期 {{ formatDateTime(detail.expires_at) }}</Tag
      >
      <div class="ml-auto flex flex-wrap gap-[var(--spacing-xs)]">
        <Button
          v-if="!isSigned"
          variant="solid"
          semantic="primary"
          icon="ph:signature"
          :disabled="busy"
          @click="emit('sign')"
        >
          签名策略
        </Button>
        <template v-else>
          <Button icon="ph:share-network" :disabled="busy" @click="openAssign">
            分配策略
          </Button>
          <Button
            icon="ph:download-simple"
            :disabled="busy"
            @click="emit('export')"
          >
            导出
          </Button>
        </template>
      </div>
    </div>

    <Alert v-if="!isSigned" semantic="info">
      草稿保存后不可修改。签名时服务端会重新校验并分配版本，之后才能分配或导出。
    </Alert>

    <div
      class="grid gap-[var(--spacing-sm)] rounded-[var(--radius-md)] border border-[var(--st-border-divider)] p-[var(--spacing-md)] text-[var(--text-sm)] md:grid-cols-2"
    >
      <div class="flex justify-between gap-[var(--spacing-md)]">
        <span class="text-[var(--st-text-muted)]">策略 ID</span>
        <span class="break-all text-right font-mono">{{ detail.id }}</span>
      </div>
      <div class="flex justify-between gap-[var(--spacing-md)]">
        <span class="text-[var(--st-text-muted)]">版本</span>
        <span>{{ detail.version ?? '签名后分配' }}</span>
      </div>
      <div class="flex justify-between gap-[var(--spacing-md)]">
        <span class="text-[var(--st-text-muted)]">创建时间</span>
        <span>{{ formatDateTime(detail.created_at) }}</span>
      </div>
      <div class="flex justify-between gap-[var(--spacing-md)]">
        <span class="text-[var(--st-text-muted)]">签名时间</span>
        <span>{{ formatDateTime(detail.signed_at) }}</span>
      </div>
      <div class="flex justify-between gap-[var(--spacing-md)]">
        <span class="text-[var(--st-text-muted)]">签名密钥</span>
        <span class="font-mono">{{ shortId(detail.server_key_id, 20) }}</span>
      </div>
    </div>

    <Tabs v-model="activeTab">
      <TabPanel name="rules" label="策略内容" icon="ph:list-checks">
        <div class="grid gap-[var(--spacing-md)]">
          <Card title="USB 存储" :hoverable="false">
            <div class="flex flex-wrap gap-[var(--spacing-sm)]">
              <Tag
                :semantic="policySemantic(detail.policy_json.usb.read)"
                size="small"
              >
                读取：{{ policyValue(detail.policy_json.usb.read) }}
              </Tag>
              <Tag
                :semantic="policySemantic(detail.policy_json.usb.write)"
                size="small"
              >
                写入：{{ policyValue(detail.policy_json.usb.write) }}
              </Tag>
            </div>
            <div
              v-if="detail.policy_json.usb.allowed.length"
              class="mt-[var(--spacing-md)] grid gap-[var(--spacing-xs)]"
            >
              <code
                v-for="(rule, index) in detail.policy_json.usb.allowed"
                :key="index"
                class="break-all text-[var(--text-xs)]"
              >
                {{
                  [
                    rule.instance_id,
                    rule.vendor_id,
                    rule.product_id,
                    rule.serial,
                  ]
                    .filter(Boolean)
                    .join(' / ')
                }}
              </code>
            </div>
            <p
              v-else
              class="m-0 mt-[var(--spacing-sm)] text-[var(--text-sm)] text-[var(--st-text-muted)]"
            >
              没有允许规则。
            </p>
          </Card>

          <Card title="网络" :hoverable="false">
            <Tag
              :semantic="policySemantic(detail.policy_json.network.policy)"
              size="small"
            >
              {{ policyValue(detail.policy_json.network.policy) }}
            </Tag>
            <div
              v-if="detail.policy_json.network.allowed.length"
              class="mt-[var(--spacing-md)] grid gap-[var(--spacing-xs)]"
            >
              <code
                v-for="rule in detail.policy_json.network.allowed"
                :key="rule.adapter_id"
                class="break-all text-[var(--text-xs)]"
              >
                {{ rule.adapter_id }}
              </code>
            </div>
            <p
              v-else
              class="m-0 mt-[var(--spacing-sm)] text-[var(--text-sm)] text-[var(--st-text-muted)]"
            >
              没有允许的网卡。
            </p>
          </Card>

          <Card title="刻录" :hoverable="false">
            <Tag
              :semantic="policySemantic(detail.policy_json.burning.policy)"
              size="small"
            >
              {{ policyValue(detail.policy_json.burning.policy) }}
            </Tag>
            <div
              v-if="detail.policy_json.burning.allowed.length"
              class="mt-[var(--spacing-md)] grid gap-[var(--spacing-xs)]"
            >
              <code
                v-for="rule in detail.policy_json.burning.allowed"
                :key="rule.drive_id"
                class="break-all text-[var(--text-xs)]"
              >
                {{ rule.drive_id }}
              </code>
            </div>
            <p
              v-else
              class="m-0 mt-[var(--spacing-sm)] text-[var(--text-sm)] text-[var(--st-text-muted)]"
            >
              没有允许的光驱。
            </p>
          </Card>

          <Card title="文件扫描" :hoverable="false">
            <dl
              class="grid grid-cols-[minmax(6rem,9rem)_minmax(0,1fr)] gap-x-[var(--spacing-md)] gap-y-[var(--spacing-xs)] text-[var(--text-sm)]"
            >
              <dt class="m-0 text-[var(--st-text-muted)]">受控目录</dt>
              <dd class="m-0 break-all">
                {{
                  detail.policy_json.scan.controlled_dirs.join('、') || '未设置'
                }}
              </dd>
              <dt class="m-0 text-[var(--st-text-muted)]">扩展名</dt>
              <dd class="m-0">
                {{
                  detail.policy_json.scan.file_types.join(', ') ||
                  '全部支持类型'
                }}
              </dd>
            </dl>
            <div
              v-if="detail.policy_json.scan.rules.length"
              class="mt-[var(--spacing-md)] grid gap-[var(--spacing-xs)]"
            >
              <code
                v-for="rule in detail.policy_json.scan.rules"
                :key="rule.rule_id"
                class="break-all text-[var(--text-xs)]"
              >
                {{ rule.rule_id }} · {{ rule.kind }} · {{ rule.pattern }}
              </code>
            </div>
            <p
              v-else
              class="m-0 mt-[var(--spacing-sm)] text-[var(--text-sm)] text-[var(--st-text-muted)]"
            >
              没有扫描规则。
            </p>
          </Card>
        </div>
      </TabPanel>

      <TabPanel name="raw" label="签名数据" icon="ph:code">
        <Card :hoverable="false">
          <pre
            class="m-0 max-h-[28rem] overflow-auto whitespace-pre-wrap break-all font-mono text-[var(--text-xs)] leading-5"
            >{{ JSON.stringify(detail.policy_json, null, 2) }}</pre>
          <div v-if="detail.signature_hex" class="mt-[var(--spacing-md)]">
            <strong class="text-[var(--text-sm)]">签名</strong>
            <pre
              class="m-0 mt-[var(--spacing-xs)] whitespace-pre-wrap break-all font-mono text-[var(--text-xs)]"
              >{{ detail.signature_hex }}</pre>
          </div>
        </Card>
      </TabPanel>
    </Tabs>

    <Modal
      v-model:visible="assignVisible"
      title="分配策略"
      confirm-text="分配"
      :disabled="busy"
      @confirm="submitAssign"
    >
      <div class="grid gap-[var(--spacing-md)]">
        <Alert v-if="assignError" semantic="error">{{ assignError }}</Alert>
        <Select v-model="targetKind" label="目标" :options="targetOptions" />
        <Select
          v-if="targetKind === 'device'"
          v-model="selectedDeviceId"
          label="设备"
          placeholder="选择已批准设备"
          :options="deviceOptions"
        />
        <Alert
          v-if="targetKind === 'device' && deviceOptions.length === 0"
          semantic="warning"
        >
          当前没有已批准设备，请先审批设备。
        </Alert>
        <p class="m-0 text-[var(--text-sm)] text-[var(--st-text-muted)]">
          设备显式分配优先于默认策略；同一目标再次分配会替换现有分配。
        </p>
      </div>
    </Modal>
  </div>
</template>
