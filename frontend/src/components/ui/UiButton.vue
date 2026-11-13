<script setup lang="ts">
import { Primitive, type PrimitiveProps } from 'reka-ui'
import { LoaderCircle } from 'lucide-vue-next'
import { computed } from 'vue'

const props = withDefaults(
  defineProps<
    PrimitiveProps & {
      variant?: 'primary' | 'secondary' | 'ghost' | 'danger'
      size?: 'sm' | 'md' | 'icon'
      loading?: boolean
      disabled?: boolean
    }
  >(),
  { as: 'button', variant: 'primary', size: 'md' },
)

const classes = computed(() => [
  'inline-flex shrink-0 items-center justify-center gap-2 rounded-lg font-medium whitespace-nowrap transition outline-none',
  'focus-visible:ring-4 focus-visible:ring-brand-500/25 disabled:pointer-events-none disabled:opacity-50',
  '[&_svg]:size-4 [&_svg]:shrink-0',
  {
    primary: 'bg-zinc-900 text-white shadow-sm hover:bg-zinc-700 dark:bg-white dark:text-zinc-900 dark:hover:bg-zinc-200',
    secondary:
      'border border-zinc-200 bg-white text-zinc-800 shadow-sm hover:bg-zinc-50 dark:border-zinc-800 dark:bg-zinc-900 dark:text-zinc-200 dark:hover:bg-zinc-800',
    ghost: 'text-zinc-600 hover:bg-zinc-100 hover:text-zinc-900 dark:text-zinc-400 dark:hover:bg-zinc-800 dark:hover:text-zinc-100',
    danger: 'bg-red-600 text-white shadow-sm hover:bg-red-500',
  }[props.variant],
  { sm: 'h-8 px-3 text-xs', md: 'h-9 px-4 text-sm', icon: 'size-8' }[props.size],
])
</script>

<template>
  <Primitive
    :as="as"
    :as-child="asChild"
    :class="classes"
    :disabled="disabled || loading || undefined"
    :type="as === 'button' ? 'button' : undefined"
  >
    <LoaderCircle v-if="loading" class="animate-spin" />
    <slot />
  </Primitive>
</template>
