<script setup lang="ts">
import {
  Alert,
  Button,
  Drawer,
  Select,
  useConfirm,
  useNotification,
} from '@stellar/ui'
import AdminShell from '~/components/admin/AdminShell.vue'
import PolicyDetailPanel from '~/components/policy/PolicyDetailPanel.vue'
import PolicyEditor from '~/components/policy/PolicyEditor.vue'
import PolicyTable from '~/components/policy/PolicyTable.vue'
import { errorMessage, useAdminApi } from '~/composables/useAdminApi'
import type {
  AssignPolicyRequest,
  DeviceSummary,
  PolicyDetail,
  PolicyStatus,
  PolicySummary,
} from '~/types/admin'
import type { PolicyForm } from '~/utils/policy'
import { buildPolicyDraft, emptyPolicyForm } from '~/utils/policy'

const api = useAdminApi()
const notifications = useNotification()
const { confirm } = useConfirm()

const policies = ref<PolicySummary[]>([])
const statusFilter = ref<PolicyStatus | ''>('')
const nextCursor = ref<string | null>(null)
const loading = ref(false)
const loadingMore = ref(false)
const errorText = ref('')
const activeDevices = ref<DeviceSummary[]>([])

const editorVisible = ref(false)
const editorMode = ref<'new' | 'detail'>('new')
const drawerTitle = ref('新建策略')
const form = ref<PolicyForm>(emptyPolicyForm())
const formErrors = ref<string[]>([])
const detail = ref<PolicyDetail | null>(null)
const detailLoading = ref(false)
const saving = ref(false)

const statusOptions = [
  { value: '', label: '全部状态' },
  { value: 'draft', label: '草稿' },
  { value: 'signed', label: '已签名' },
]
const busy = computed(
  () =>
    loading.value ||
    loadingMore.value ||
    detailLoading.value ||
    saving.value ||
    api.pending.value > 0,
)

watch(statusFilter, () => {
  void loadPolicies(true)
})

onMounted(() => {
  void loadPolicies(true)
  void loadDevices()
})

async function loadPolicies(reset: boolean) {
  if (reset) {
    loading.value = true
    nextCursor.value = null
  } else {
    loadingMore.value = true
  }
  errorText.value = ''
  try {
    const page = await api.loadPolicies(
      statusFilter.value || null,
      reset ? null : nextCursor.value,
    )
    policies.value = reset ? page.items : [...policies.value, ...page.items]
    nextCursor.value = page.next_cursor
  } catch (error) {
    errorText.value = errorMessage(error, '无法读取策略列表')
  } finally {
    loading.value = false
    loadingMore.value = false
  }
}

async function loadDevices() {
  try {
    activeDevices.value = await api.loadActiveDevices()
  } catch {
    // 分配策略时仍可只用默认策略；设备列表读取失败不阻塞策略编辑。
  }
}

function openNew() {
  editorMode.value = 'new'
  drawerTitle.value = '新建策略'
  form.value = emptyPolicyForm()
  formErrors.value = []
  detail.value = null
  editorVisible.value = true
  void loadDevices()
}

async function openPolicy(policy: PolicySummary) {
  editorMode.value = 'detail'
  drawerTitle.value = `策略 ${policy.version ? `v${policy.version}` : '草稿'}`
  formErrors.value = []
  detail.value = null
  detailLoading.value = true
  editorVisible.value = true
  void loadDevices()
  try {
    detail.value = await api.loadPolicy(policy.id)
  } catch (error) {
    notifications.error(errorMessage(error, '无法读取策略详情'))
  } finally {
    detailLoading.value = false
  }
}

async function saveDraft() {
  if (saving.value) {
    return
  }
  const result = buildPolicyDraft(form.value)
  formErrors.value = result.errors
  if (!result.request) {
    return
  }

  saving.value = true
  try {
    detail.value = await api.createPolicy(result.request)
    editorMode.value = 'detail'
    drawerTitle.value = '策略草稿'
    formErrors.value = []
    notifications.success('策略草稿已保存')
    await loadPolicies(true)
  } catch (error) {
    formErrors.value = [errorMessage(error, '保存策略草稿失败')]
  } finally {
    saving.value = false
  }
}

async function signPolicy() {
  if (!detail.value || busy.value) {
    return
  }
  const confirmed = await confirm({
    title: '签名策略',
    message: `签名后会生成不可修改的版本并立即对已分配设备生效。继续？`,
    confirmText: '签名',
  })
  if (!confirmed) {
    return
  }
  saving.value = true
  try {
    detail.value = await api.signPolicy(detail.value.id)
    drawerTitle.value = `策略 v${detail.value.version}`
    notifications.success('策略已签名')
    await loadPolicies(true)
  } catch (error) {
    notifications.error(errorMessage(error, '签名失败'))
  } finally {
    saving.value = false
  }
}

async function assignPolicy(request: AssignPolicyRequest) {
  if (!detail.value || busy.value) {
    return
  }
  saving.value = true
  try {
    await api.assignPolicy(detail.value.id, request)
    notifications.success(
      request.is_default ? '已设为默认策略' : '策略已分配给设备',
    )
  } catch (error) {
    notifications.error(errorMessage(error, '分配策略失败'))
  } finally {
    saving.value = false
  }
}

async function exportPolicy() {
  if (!detail.value || busy.value) {
    return
  }
  saving.value = true
  try {
    await api.exportPolicy(detail.value.id)
    notifications.success('策略包已导出')
  } catch (error) {
    notifications.error(errorMessage(error, '导出策略失败'))
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <AdminShell title="策略" description="编辑、签名并分配设备策略">
    <template #actions>
      <Select v-model="statusFilter" :options="statusOptions" size="small" />
      <Button
        icon="ph:arrows-clockwise"
        :disabled="busy"
        @click="loadPolicies(true)"
      >
        刷新
      </Button>
      <Button
        variant="solid"
        semantic="primary"
        icon="ph:plus"
        :disabled="busy"
        @click="openNew"
      >
        新建策略
      </Button>
    </template>

    <div class="flex min-h-0 flex-1 flex-col gap-[var(--spacing-md)]">
      <Alert v-if="errorText" semantic="error">{{ errorText }}</Alert>
      <div
        class="flex items-center justify-between text-[var(--text-sm)] text-[var(--st-text-muted)]"
      >
        <span>已加载 {{ policies.length }} 条策略</span>
        <span v-if="nextCursor">还有更多策略</span>
      </div>
      <div class="min-h-0 flex-1 overflow-auto">
        <PolicyTable
          :policies="policies"
          :loading="loading"
          :busy="busy"
          @select="openPolicy"
        />
      </div>
      <div v-if="nextCursor" class="flex justify-center">
        <Button :disabled="busy" @click="loadPolicies(false)">
          {{ loadingMore ? '加载中…' : '加载更多' }}
        </Button>
      </div>
    </div>

    <Drawer
      v-model:visible="editorVisible"
      :title="drawerTitle"
      size="xlarge"
      :show-footer="false"
    >
      <div class="grid gap-[var(--spacing-lg)] pb-[var(--spacing-xl)]">
        <div
          v-if="detailLoading"
          class="py-[var(--spacing-2xl)] text-center text-[var(--st-text-muted)]"
        >
          正在读取策略详情…
        </div>

        <template v-else-if="editorMode === 'new'">
          <PolicyEditor v-model="form" :errors="formErrors" />
          <div class="flex justify-end gap-[var(--spacing-sm)]">
            <Button :disabled="busy" @click="editorVisible = false"
              >取消</Button
            >
            <Button
              variant="solid"
              semantic="primary"
              icon="ph:floppy-disk"
              :disabled="busy"
              @click="saveDraft"
            >
              保存草稿
            </Button>
          </div>
        </template>

        <PolicyDetailPanel
          v-else-if="detail"
          :detail="detail"
          :active-devices="activeDevices"
          :busy="busy"
          @sign="signPolicy"
          @assign="assignPolicy"
          @export="exportPolicy"
        />
      </div>
    </Drawer>
  </AdminShell>
</template>
