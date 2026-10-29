<script setup lang="ts">
import {
  Alert,
  Button,
  Card,
  Checkbox,
  Icon,
  Input,
  Textarea,
  useNotification,
} from '@stellar/ui'
import { errorMessage, useAdminApi } from '~/composables/useAdminApi'

const api = useAdminApi()
const notifications = useNotification()

const mode = ref<'login' | 'setup'>('login')
const submitting = ref(false)
const errorText = ref('')
const username = ref('')
const password = ref('')
const setupToken = ref('')
const confirmPassword = ref('')
const importSigningKey = ref(false)
const signingKeyHex = ref('')

const busy = computed(() => submitting.value || api.pending.value > 0)
const canSubmitLogin = computed(
  () =>
    !busy.value &&
    username.value.trim().length > 0 &&
    password.value.length > 0,
)
const canSubmitSetup = computed(
  () =>
    !busy.value &&
    setupToken.value.trim().length > 0 &&
    username.value.trim().length >= 3 &&
    password.value.length >= 12 &&
    confirmPassword.value.length >= 12,
)

onMounted(() => {
  if (api.authenticated.value) {
    void navigateTo('/devices', { replace: true })
  }
})

function switchMode(next: 'login' | 'setup') {
  mode.value = next
  errorText.value = ''
  password.value = ''
  confirmPassword.value = ''
  signingKeyHex.value = ''
  importSigningKey.value = false
}

async function submitLogin() {
  if (!canSubmitLogin.value) {
    return
  }
  submitting.value = true
  errorText.value = ''
  try {
    await api.login(username.value.trim(), password.value)
    notifications.success('已登录管理台')
    await navigateTo('/devices', { replace: true })
  } catch (error) {
    errorText.value = errorMessage(error, '登录失败')
  } finally {
    submitting.value = false
  }
}

async function submitSetup() {
  if (!canSubmitSetup.value) {
    return
  }
  if (password.value !== confirmPassword.value) {
    errorText.value = '两次输入的密码不一致。'
    return
  }
  if (password.value.length < 12) {
    errorText.value = '管理员密码至少需要 12 个字符。'
    return
  }

  submitting.value = true
  errorText.value = ''
  try {
    await api.setup({
      setup_token: setupToken.value.trim(),
      username: username.value.trim(),
      password: password.value,
      signing_key_hex: importSigningKey.value ? signingKeyHex.value : undefined,
    })
    notifications.success('初始化完成，已进入管理台')
    await navigateTo('/devices', { replace: true })
  } catch (error) {
    errorText.value = errorMessage(error, '初始化失败')
  } finally {
    submitting.value = false
  }
}
</script>

<template>
  <main
    class="grid min-h-dvh place-items-center bg-[var(--st-bg-base)] p-[var(--spacing-xl)]"
  >
    <div class="w-full max-w-[var(--max-width-md)]">
      <div
        class="mb-[var(--spacing-lg)] flex items-center gap-[var(--spacing-sm)]"
      >
        <div
          class="grid h-10 w-10 place-items-center rounded-[var(--radius-md)] bg-[var(--st-primary)]/10 text-[var(--st-primary)]"
        >
          <Icon icon="ph:shield-check" class="h-6 w-6" />
        </div>
        <div>
          <h1 class="m-0 text-[var(--text-xl)] font-semibold">Anchor Guard</h1>
          <p class="m-0 text-[var(--text-sm)] text-[var(--st-text-muted)]">
            设备与策略管理台
          </p>
        </div>
      </div>

      <Card :hoverable="false">
        <div class="mb-[var(--spacing-lg)] flex gap-[var(--spacing-xs)]">
          <Button
            :variant="mode === 'login' ? 'solid' : 'text'"
            :semantic="mode === 'login' ? 'primary' : 'info'"
            size="small"
            @click="switchMode('login')"
          >
            管理员登录
          </Button>
          <Button
            :variant="mode === 'setup' ? 'solid' : 'text'"
            :semantic="mode === 'setup' ? 'primary' : 'info'"
            size="small"
            @click="switchMode('setup')"
          >
            首次初始化
          </Button>
        </div>

        <Alert
          v-if="api.restoreError.value && mode === 'login'"
          semantic="warning"
          class="mb-[var(--spacing-lg)]"
        >
          {{ api.restoreError.value }}
        </Alert>
        <Alert v-if="errorText" semantic="error" class="mb-[var(--spacing-lg)]">
          {{ errorText }}
        </Alert>

        <form
          v-if="mode === 'login'"
          class="grid gap-[var(--spacing-lg)]"
          @submit.prevent="submitLogin"
        >
          <Input
            v-model="username"
            label="用户名"
            autocomplete="username"
            autofocus
            required
          />
          <Input
            v-model="password"
            label="密码"
            type="password"
            autocomplete="current-password"
            required
          />
          <Button
            type="submit"
            variant="solid"
            semantic="primary"
            icon="ph:sign-in"
            :disabled="!canSubmitLogin"
          >
            登录
          </Button>
        </form>

        <form
          v-else
          class="grid gap-[var(--spacing-lg)]"
          @submit.prevent="submitSetup"
        >
          <p class="m-0 text-[var(--text-sm)] text-[var(--st-text-muted)]">
            初始化只能执行一次，需要部署时配置的
            <code>ANCHOR_SETUP_TOKEN</code>。完成后该入口会关闭。
          </p>
          <Input
            v-model="setupToken"
            label="初始化令牌"
            type="password"
            autocomplete="off"
            required
          />
          <Input
            v-model="username"
            label="管理员用户名"
            autocomplete="username"
            help-text="3 到 64 个字符，仅限字母、数字、点、下划线和连字符"
            required
          />
          <Input
            v-model="password"
            label="管理员密码"
            type="password"
            autocomplete="new-password"
            help-text="至少 12 个字符"
            required
          />
          <Input
            v-model="confirmPassword"
            label="确认密码"
            type="password"
            autocomplete="new-password"
            required
          />
          <div class="grid gap-[var(--spacing-sm)]">
            <Checkbox v-model="importSigningKey" label="导入已有签名私钥" />
            <Textarea
              v-if="importSigningKey"
              v-model="signingKeyHex"
              label="签名私钥（32 字节 hex）"
              :rows="3"
              placeholder="留空或取消勾选时由服务端生成新密钥"
            />
          </div>
          <Button
            type="submit"
            variant="solid"
            semantic="primary"
            icon="ph:rocket-launch"
            :disabled="!canSubmitSetup"
          >
            完成初始化
          </Button>
        </form>
      </Card>
      <p
        class="mt-[var(--spacing-md)] text-center text-[var(--text-xs)] text-[var(--st-text-muted)]"
      >
        会话 Cookie 仅在 HTTPS 下生效。
      </p>
    </div>
  </main>
</template>
