<script setup lang="ts">
import type { FieldGroup } from '@/types'

defineProps<{
  groups: FieldGroup[]
}>()
</script>

<template>
  <div class="fg-root">
    <section v-for="g in groups" :key="g.title" class="fg-card">
      <header class="fg-title">
        {{ g.title }}
        <span v-if="g.note" class="fg-note">{{ g.note }}</span>
      </header>
      <table class="fg-table">
        <tbody>
          <tr v-for="(f, i) in g.fields" :key="f.name + i">
            <td class="f-name">{{ f.name }}</td>
            <td class="f-val">{{ f.value }}</td>
            <td class="f-bits">{{ f.bits }}</td>
            <td class="f-desc">{{ f.desc }}</td>
          </tr>
        </tbody>
      </table>
    </section>
  </div>
</template>
