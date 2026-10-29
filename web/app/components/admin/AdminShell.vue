<script setup lang="ts">
import {
  Avatar,
  Button,
  DropdownMenu,
  Icon,
  MenuItem,
  NavBar,
  useNotification,
  useTheme,
} from '@stellar/ui'
import PageHeader from '~/components/shell/PageHeader.vue'

const props = defineProps<{
  title?: string
  description?: string
}>()

const route = useRoute()
const slots = useSlots()
const notifications = useNotification()
const { admin, pending, logout } = useAdminApi()
const { theme, toggleTheme } = useTheme()

const navigationItems = [
  { to: '/devices', label: '设备', icon: 'ph:desktop-tower' },
  { to: '/policies', label: '策略', icon: 'ph:shield-check' },
]

const busy = computed(() => pending.value > 0)
const themeLabel = computed(() =>
  theme.value === 'dark' ? '切换为浅色主题' : '切换为深色主题',
)
const showPageHeader = computed(
  () => Boolean(props.title) || Boolean(slots.actions),
)
const navItems = computed(() =>
  navigationItems.map((item) => ({
    label: item.label,
    icon: item.icon,
    active: isActive(item.to),
    disabled: busy.value,
  })),
)

function trimmedPath(path: string): string {
  return path.replace(/\/+$/, '') || '/'
}

function isActive(path: string): boolean {
  const current = trimmedPath(route.path)
  const target = trimmedPath(path)
  return current === target || current.startsWith(`${target}/`)
}

function openNavigation(item: { label: string }) {
  const target = navigationItems.find((entry) => entry.label === item.label)
  if (!target || busy.value) {
    return
  }
  void navigateTo(target.to)
}

async function exitManagement() {
  try {
    await logout()
    notifications.success('已退出管理台')
  } catch {
    notifications.warning('已清理本地会话')
  }
  await navigateTo('/login', { replace: true })
}

function handleExit(close: () => void) {
  close()
  void exitManagement()
}
</script>

<template>
  <div class="flex h-dvh min-h-0 flex-col bg-[var(--st-bg-base)]">
    <NavBar
      title="Anchor Guard"
      variant="solid"
      :items="navItems"
      @item-click="openNavigation"
    >
      <template #actions>
        <Button
          variant="text"
          square
          :icon="theme === 'dark' ? 'ph:sun' : 'ph:moon'"
          :aria-label="themeLabel"
          :title="themeLabel"
          @click="toggleTheme"
        />
        <DropdownMenu placement="bottom" align="right">
          <template #trigger>
            <span
              class="flex items-center gap-[var(--spacing-sm)] rounded-[var(--radius-md)] px-[var(--spacing-xs)] py-[var(--spacing-xs)] hover:bg-[var(--st-bg-surface)]"
            >
              <Avatar icon="ph:user-circle" size="small" shape="circle" />
              <span class="text-[var(--text-sm)]">
                {{ admin?.username ?? '管理员' }}
              </span>
              <Icon
                icon="ph:caret-down"
                class="h-4 w-4 text-[var(--st-text-secondary)]"
              />
            </span>
          </template>
          <template #default="{ close }">
            <MenuItem
              icon="ph:sign-out"
              danger
              label="退出管理"
              :disabled="busy"
              @click="handleExit(close)"
            />
          </template>
        </DropdownMenu>
      </template>
    </NavBar>

    <main
      class="flex min-h-0 min-w-0 flex-1 flex-col gap-[var(--spacing-lg)] overflow-hidden px-[var(--spacing-2xl)] py-[var(--spacing-lg)]"
    >
      <PageHeader
        v-if="showPageHeader"
        :title="title"
        :description="description"
      >
        <template #actions>
          <slot name="actions" />
        </template>
      </PageHeader>
      <div class="flex min-h-0 flex-1 flex-col">
        <slot />
      </div>
    </main>
  </div>
</template>
