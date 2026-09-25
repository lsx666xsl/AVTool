<script setup lang="ts">
import { ref } from 'vue'
import { pickFile } from '@/bridge'

const props = withDefaults(
  defineProps<{
    label: string
    exts: string[]
    variant?: 'primary' | 'ghost'
  }>(),
  { variant: 'primary' },
)

const emit = defineEmits<{ (e: 'picked', path: string): void }>()

const loading = ref(false)

async function onClick(): Promise<void> {
  loading.value = true
  try {
    const p = await pickFile(props.exts)
    if (p) emit('picked', p)
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <button
    class="btn"
    :class="props.variant"
    :disabled="loading"
    @click="onClick"
  >
    {{ loading ? '选择中…' : props.label }}
  </button>
</template>
