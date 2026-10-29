<script setup lang="ts">
import { NotificationContainer, useTheme } from '@stellar/ui'

useTheme()
</script>

<template>
  <NotificationContainer position="top-right" :max="4" />
  <NuxtPage />
</template>
