<script setup lang="ts">
import { Card, EmptyState, Tag } from '@stellar/ui'
import type { HardwareInventory } from '~/types/admin'
import { formatBytes, formatDateTime } from '~/utils/format'

const props = defineProps<{
  inventory: HardwareInventory | null
  capturedAt: string | null
  receivedAt: string | null
}>()

const summarySections = computed(() => {
  const inventory = props.inventory
  if (!inventory) {
    return []
  }
  return [
    {
      title: '计算机',
      icon: 'ph:desktop',
      fields: [
        ['制造商', inventory.computer.manufacturer],
        ['型号', inventory.computer.model],
        ['序列号', inventory.computer.serial_number],
        ['BIOS 厂商', inventory.computer.bios_vendor],
        ['BIOS 版本', inventory.computer.bios_version],
        ['BIOS 日期', inventory.computer.bios_release_date],
      ],
    },
    {
      title: '操作系统',
      icon: 'ph:windows-logo',
      fields: [
        ['名称', inventory.operating_system.name],
        ['版本', inventory.operating_system.version],
        ['构建', inventory.operating_system.build],
        ['架构', inventory.operating_system.architecture],
        ['区域', inventory.operating_system.locale],
        ['主机名', inventory.operating_system.hostname],
      ],
    },
    {
      title: '处理器',
      icon: 'ph:cpu',
      fields: [
        ['制造商', inventory.cpu.manufacturer],
        ['型号', inventory.cpu.model],
        ['核心', String(inventory.cpu.cores)],
        ['线程', String(inventory.cpu.threads)],
      ],
    },
  ]
})

const listSections = computed(() => {
  const inventory = props.inventory
  if (!inventory) {
    return []
  }
  return [
    {
      title: '内存',
      icon: 'ph:memory',
      empty: '未读取到内存条信息',
      items: inventory.memory_modules.map((item) => ({
        title: item.slot || '内存条',
        fields: [
          ['制造商', item.manufacturer],
          ['型号', item.part_number],
          ['序列号', item.serial_number],
          ['容量', formatBytes(item.capacity_bytes)],
          ['频率', item.speed_mhz ? `${item.speed_mhz} MHz` : '-'],
        ],
      })),
    },
    {
      title: '磁盘',
      icon: 'ph:hard-drives',
      empty: '未读取到磁盘信息',
      items: inventory.disks.map((item) => ({
        title: item.model || '磁盘',
        fields: [
          ['序列号', item.serial_number],
          ['介质', item.media_type],
          ['容量', formatBytes(item.size_bytes)],
        ],
      })),
    },
    {
      title: '卷',
      icon: 'ph:folder',
      empty: '未读取到卷信息',
      items: inventory.volumes.map((item) => ({
        title: item.name || item.label || '卷',
        fields: [
          ['卷标', item.label],
          ['文件系统', item.filesystem],
          ['容量', formatBytes(item.size_bytes)],
          ['可用', formatBytes(item.free_bytes)],
          ['序列号', item.serial_number],
        ],
      })),
    },
    {
      title: '网络适配器',
      icon: 'ph:wifi-high',
      empty: '未读取到网络适配器',
      items: inventory.network_adapters.map((item) => ({
        title: item.name || item.instance_id,
        fields: [
          ['实例 ID', item.instance_id],
          ['MAC', item.mac_address],
          ['连接', item.link_up ? '已连接' : '未连接'],
          ['驱动', item.driver_version],
        ],
      })),
    },
    {
      title: 'USB 设备',
      icon: 'ph:usb',
      empty: '未读取到 USB 设备',
      items: inventory.usb_devices.map((item) => ({
        title: item.name || item.instance_id,
        fields: [
          ['实例 ID', item.instance_id],
          ['制造商', item.manufacturer],
          ['VID / PID', `${item.vendor_id || '-'} / ${item.product_id || '-'}`],
          ['序列号', item.serial_number],
          ['设备类', item.device_class],
        ],
      })),
    },
    {
      title: '光驱',
      icon: 'ph:disc',
      empty: '未读取到光驱',
      items: inventory.optical_drives.map((item) => ({
        title: item.name || item.instance_id,
        fields: [
          ['实例 ID', item.instance_id],
          ['介质', item.media_present ? '已放入' : '无介质'],
        ],
      })),
    },
    {
      title: '显卡',
      icon: 'ph:graphics-card',
      empty: '未读取到显卡',
      items: inventory.gpus.map((item) => ({
        title: item.name || '显卡',
        fields: [
          ['制造商', item.manufacturer],
          ['显存', formatBytes(item.memory_bytes)],
          ['驱动', item.driver_version],
        ],
      })),
    },
  ]
})
</script>

<template>
  <EmptyState
    v-if="!inventory"
    icon="ph:hard-drive"
    title="尚无硬件清单"
    description="设备下一次上报后会显示完整清单。"
  />

  <div v-else class="grid gap-[var(--spacing-lg)]">
    <div class="flex flex-wrap gap-[var(--spacing-sm)]">
      <Tag size="small" icon="ph:clock"
        >采集 {{ formatDateTime(capturedAt) }}</Tag
      >
      <Tag size="small" icon="ph:cloud-check"
        >接收 {{ formatDateTime(receivedAt) }}</Tag
      >
    </div>

    <div class="grid gap-[var(--spacing-lg)] md:grid-cols-2 xl:grid-cols-3">
      <Card
        v-for="section in summarySections"
        :key="section.title"
        :title="section.title"
      >
        <dl
          class="grid grid-cols-[minmax(5.5rem,8rem)_minmax(0,1fr)] gap-x-[var(--spacing-md)] gap-y-[var(--spacing-xs)] text-[var(--text-sm)]"
        >
          <template v-for="[label, value] in section.fields" :key="label">
            <dt class="m-0 text-[var(--st-text-muted)]">{{ label }}</dt>
            <dd class="m-0 break-words">{{ value || '-' }}</dd>
          </template>
        </dl>
      </Card>
    </div>

    <Card
      v-for="section in listSections"
      :key="section.title"
      :title="section.title"
    >
      <EmptyState
        v-if="section.items.length === 0"
        :icon="section.icon"
        :title="section.empty"
        size="small"
      />
      <div v-else class="grid gap-[var(--spacing-md)]">
        <div
          v-for="item in section.items"
          :key="item.title"
          class="grid gap-[var(--spacing-xs)] border-b border-[var(--st-border-divider)] pb-[var(--spacing-md)] last:border-b-0 last:pb-0"
        >
          <strong class="text-[var(--text-sm)]">{{ item.title }}</strong>
          <dl
            class="grid grid-cols-[minmax(5.5rem,8rem)_minmax(0,1fr)] gap-x-[var(--spacing-md)] gap-y-[var(--spacing-xs)] text-[var(--text-sm)]"
          >
            <template v-for="[label, value] in item.fields" :key="label">
              <dt class="m-0 text-[var(--st-text-muted)]">{{ label }}</dt>
              <dd class="m-0 break-words">{{ value || '-' }}</dd>
            </template>
          </dl>
        </div>
      </div>
    </Card>
  </div>
</template>
