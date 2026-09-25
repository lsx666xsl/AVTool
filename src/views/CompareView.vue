<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import FilePicker from '@/components/FilePicker.vue'
import { api, STREAM_EXTS } from '@/bridge'
import type { SpsCompare } from '@/types'

const pathA = ref<string | null>(null)
const pathB = ref<string | null>(null)
const cmp = ref<SpsCompare | null>(null)
const loading = ref(false)
const error = ref('')
const onlyDiff = ref(true)

// 请求序号守卫：连续切换文件时只采用最新一次对比结果
let cmpSeq = 0

async function run(): Promise<void> {
  if (!pathA.value || !pathB.value) return
  const seq = ++cmpSeq
  loading.value = true
  error.value = ''
  cmp.value = null
  try {
    const r = await api.compareSps(pathA.value, pathB.value)
    if (seq !== cmpSeq) return
    cmp.value = r
  } catch (e) {
    if (seq !== cmpSeq) return
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    if (seq === cmpSeq) loading.value = false
  }
}

watch([pathA, pathB], () => {
  cmp.value = null
  void run()
})

const fileName = (p: string | null): string =>
  p?.replace(/\\/g, '/').split('/').pop() ?? ''

const rows = computed(() =>
  cmp.value ? (onlyDiff.value ? cmp.value.rows.filter((r) => !r.same) : cmp.value.rows) : [],
)
</script>

<template>
  <div class="compare">
    <div class="picker-row">
      <div class="picker-card">
        <div class="picker-tag tag-a">A</div>
        <div class="picker-body">
          <FilePicker label="选择文件 A" :exts="STREAM_EXTS" variant="ghost" @picked="(p: string) => (pathA = p)" />
          <div class="picker-file mono" :title="pathA ?? ''">{{ fileName(pathA) || '未选择' }}</div>
        </div>
      </div>
      <div class="vs">对比</div>
      <div class="picker-card">
        <div class="picker-tag tag-b">B</div>
        <div class="picker-body">
          <FilePicker label="选择文件 B" :exts="STREAM_EXTS" variant="ghost" @picked="(p: string) => (pathB = p)" />
          <div class="picker-file mono" :title="pathB ?? ''">{{ fileName(pathB) || '未选择' }}</div>
        </div>
      </div>
    </div>

    <div v-if="error" class="error-bar">{{ error }}</div>

    <div v-if="loading" class="empty-state">对比中…</div>

    <div v-else-if="!cmp" class="empty-state">
      <div class="big">⚖️</div>
      <h2>选择两个码流进行参数对比</h2>
    </div>

    <template v-else>
      <div class="summary">
        <div class="sum-banner" :class="cmp.diffCount > 0 ? 'has-diff' : 'no-diff'">
          {{ cmp.diffCount > 0 ? `发现 ${cmp.diffCount} 处参数差异` : '两个码流的 SPS/PPS 参数完全一致' }}
        </div>
        <div class="sum-notes">
          <div class="sum-note"><b>A：</b>{{ cmp.noteA.slice(2) }}</div>
          <div class="sum-note"><b>B：</b>{{ cmp.noteB.slice(2) }}</div>
        </div>
        <label class="only-diff">
          <input v-model="onlyDiff" type="checkbox" />
          只看差异项
        </label>
      </div>

      <div class="diff-scroll">
        <table class="diff-table">
          <thead>
            <tr>
              <th class="col-sec">分组</th>
              <th class="col-name">字段</th>
              <th class="col-val">A 值</th>
              <th class="col-val">B 值</th>
              <th class="col-desc">说明</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(r, i) in rows" :key="r.section + r.name + i" :class="{ diff: !r.same }">
              <td class="col-sec">{{ r.section }}</td>
              <td class="col-name mono">{{ r.name }}</td>
              <td class="col-val mono">{{ r.valueA }}</td>
              <td class="col-val mono" :class="{ 'val-b': !r.same }">{{ r.valueB }}</td>
              <td class="col-desc">{{ r.desc }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </template>
  </div>
</template>

<style scoped>
.compare {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}
.picker-row {
  display: flex;
  align-items: stretch;
  gap: 14px;
  padding: 14px 14px 0;
  flex-shrink: 0;
}
.picker-card {
  flex: 1;
  display: flex;
  gap: 12px;
  align-items: center;
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 12px 14px;
}
.picker-tag {
  width: 34px;
  height: 34px;
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 800;
  font-size: 15px;
  flex-shrink: 0;
}
.tag-a {
  background: rgba(59, 130, 246, 0.18);
  color: #93c5fd;
  border: 1px solid rgba(59, 130, 246, 0.4);
}
.tag-b {
  background: rgba(20, 184, 166, 0.16);
  color: #5eead4;
  border: 1px solid rgba(20, 184, 166, 0.4);
}
.picker-body {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}
.picker-file {
  font-size: 11px;
  color: var(--text-dim);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 320px;
}
.vs {
  align-self: center;
  color: var(--text-dim);
  font-weight: 600;
  flex-shrink: 0;
}
.summary {
  padding: 12px 14px 0;
  flex-shrink: 0;
}
.sum-banner {
  border-radius: 8px;
  padding: 9px 14px;
  font-weight: 600;
  font-size: 13px;
}
.sum-banner.has-diff {
  background: rgba(247, 84, 100, 0.1);
  border: 1px solid rgba(247, 84, 100, 0.4);
  color: var(--danger-text);
}
.sum-banner.no-diff {
  background: rgba(95, 173, 101, 0.1);
  border: 1px solid rgba(95, 173, 101, 0.4);
  color: var(--success-text);
}
.sum-notes {
  display: flex;
  gap: 18px;
  margin-top: 8px;
  flex-wrap: wrap;
}
.sum-note {
  font-size: 12px;
  color: var(--text-muted);
}
.sum-note b {
  color: var(--text);
}
.only-diff {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  margin-top: 8px;
  font-size: 12px;
  color: var(--text-muted);
  cursor: pointer;
}
.diff-scroll {
  flex: 1;
  overflow: auto;
  margin: 12px 14px 14px;
  border: 1px solid var(--border);
  border-radius: 10px;
}
.diff-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
}
.diff-table th {
  position: sticky;
  top: 0;
  background: var(--bg-panel-2);
  color: var(--text-dim);
  text-align: left;
  padding: 8px 12px;
  font-size: 11px;
  border-bottom: 1px solid var(--border);
  z-index: 1;
}
.diff-table td {
  padding: 6px 12px;
  border-bottom: 1px solid rgba(30, 42, 68, 0.5);
  vertical-align: top;
}
.diff-table tr:hover td {
  background: rgba(24, 35, 58, 0.6);
}
.diff-table tr.diff td {
  background: rgba(247, 84, 100, 0.06);
}
.diff-table tr.diff .col-name {
  color: var(--danger-text);
}
.val-b {
  color: var(--diff-val);
  font-weight: 600;
}
.col-sec {
  width: 150px;
  color: var(--text-muted);
}
.col-name {
  width: 240px;
  color: var(--code-name);
  word-break: break-all;
}
.col-val {
  width: 200px;
  word-break: break-all;
}
.col-desc {
  color: var(--text-dim);
}
</style>
