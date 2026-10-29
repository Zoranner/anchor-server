<script setup lang="ts">
defineProps<{
  title?: string
  description?: string
}>()
</script>

<template>
  <header
    class="flex flex-none flex-wrap items-start justify-between gap-[var(--spacing-md)] border-b border-[var(--st-border-divider)] pb-[var(--spacing-sm)]"
  >
    <div class="grid gap-[var(--spacing-2xs)]">
      <h1
        v-if="title"
        class="m-0 text-[var(--text-xl)] font-semibold leading-7"
      >
        {{ title }}
      </h1>
      <p
        v-if="description"
        class="m-0 text-[var(--text-sm)] text-[var(--st-text-muted)]"
      >
        {{ description }}
      </p>
    </div>
    <div class="flex flex-wrap items-center gap-[var(--spacing-sm)]">
      <slot name="actions" />
    </div>
  </header>
</template>
