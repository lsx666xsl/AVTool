<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import type { NaluSummary } from '@/types'

const props = defineProps<{
  nalus: NaluSummary[]
  selectedIndex: number | null
}>()

const emit = defineEmits<{ (e: 'select', index: number): void }>()

const ROW_H = 30

type FilterId = 'all' | 'params' | 'idr' | 'nonvcl'
const filter = ref<FilterId>('all')
const search = ref('')

const FILTERS: Array<{ id: FilterId; label: string }> = [
  { id: 'all', label: '全部' },
  { id: 'params', label: '参数集' },
  { id: 'idr', label: '关键帧' },
  { id: 'nonvcl', label: '非VCL' },
]

const filtered = computed<NaluSummary[]>(() =>
  props.nalus.filter((n) => {
    if (filter.value === 'params' && n.nalType !== 7 && n.nalType !== 8) return false
    if (filter.value === 'idr' && !n.isIdr) return false
    if (filter.value === 'nonvcl' && (n.nalType === 1 || n.nalType === 5)) return false
    if (search.value) {
      const q = search.value.toLowerCase()
      if (!`${n.index} ${n.typeName} ${n.preview}`.toLowerCase().includes(q)) return false
    }
    return true
  }),
)

const el = ref<HTMLElement | null>(null)
const scrollTop = ref(0)
const viewportH = ref(480)

function onScroll(): void {
  if (el.value) scrollTop.value = el.value.scrollTop
}
function measure(): void {
  if (el.value) viewportH.value = el.value.clientHeight
}
onMounted(() => {
  measure()
  window.addEventListener('resize', measure)
})
onBeforeUnmount(() => window.removeEventListener('resize', measure))

// 过滤/搜索变化时回到顶部（否则残留的大 scrollTop 会让可视窗口为空）
watch([filter, search], () => {
  if (el.value) el.value.scrollTop = 0
  scrollTop.value = 0
})

const start = computed(() => {
  const maxStart = Math.max(0, filtered.value.length - Math.ceil(viewportH.value / ROW_H))
  return Math.min(Math.max(0, Math.floor(scrollTop.value / ROW_H) - 5), maxStart)
})
const end = computed(() =>
  Math.min(filtered.value.length, start.value + Math.ceil(viewportH.value / ROW_H) + 10),
)
const visible = computed(() => {
  const out: Array<{ pos: number; n: NaluSummary }> = []
  for (let i = start.value; i < end.value; i++) {
    const n = filtered.value[i]
    if (n) out.push({ pos: i, n })
  }
  return out
})

// 选中项变化时滚动到可见位置
watch(
  () => props.selectedIndex,
  (idx) => {
    if (idx === null || !el.value) return
    const pos = filtered.value.findIndex((n) => n.index === idx)
    if (pos < 0) return
    const top = pos * ROW_H
    const viewTop = el.value.scrollTop
    const viewBottom = viewTop + viewportH.value
    if (top < viewTop + ROW_H || top > viewBottom - ROW_H * 2) {
      el.value.scrollTop = Math.max(0, top - viewportH.value / 2)
    }
  },
)

function badgeClass(t: number): string {
  if (t === 7) return 'b-sps'
  if (t === 8) return 'b-pps'
  if (t === 5) return 'b-idr'
  if (t === 1) return 'b-p'
  if (t === 6) return 'b-sei'
  return 'b-other'
}

function hexOffset(v: number): string {
  return v.toString(16).toUpperCase().padStart(8, '0')
}
</script>

<template>
  <div class="nalu-table">
    <div class="nt-toolbar">
      <div class="nt-filters">
        <button
          v-for="f in FILTERS"
          :key="f.id"
          class="nt-filter"
          :class="{ active: filter === f.id }"
          @click="filter = f.id"
        >
          {{ f.label }}
        </button>
      </div>
      <input v-model="search" class="nt-search" placeholder="搜索类型/序号…" />
    </div>
    <div class="nt-head">
      <span class="c-idx">#</span>
      <span class="c-off">偏移</span>
      <span class="c-size">大小</span>
      <span class="c-type">类型</span>
      <span class="c-ref">RefIDC</span>
      <span class="c-prev">首字节</span>
    </div>
    <div ref="el" class="nt-body" @scroll="onScroll">
      <div :style="{ height: filtered.length * ROW_H + 'px', position: 'relative' }">
        <div
          v-for="{ pos, n } in visible"
          :key="n.index"
          class="nt-row"
          :class="{ sel: n.index === selectedIndex, idr: n.isIdr }"
          :style="{ top: pos * ROW_H + 'px', height: ROW_H + 'px' }"
          @click="emit('select', n.index)"
        >
          <span class="c-idx mono">{{ n.index }}</span>
          <span class="c-off mono">{{ hexOffset(n.offset) }}</span>
          <span class="c-size mono">{{ n.size }}</span>
          <span class="c-type"><span class="badge" :class="badgeClass(n.nalType)">{{ n.typeName }}</span></span>
          <span class="c-ref mono">{{ n.refIdc }}</span>
          <span class="c-prev mono">{{ n.preview }}</span>
        </div>
      </div>
    </div>
    <div class="nt-footer">共 {{ filtered.length }} 个 NALU（已过滤自 {{ nalus.length }}）</div>
  </div>
</template>

<style scoped>
.nalu-table {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: 10px;
  overflow: hidden;
}
.nt-toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border-bottom: 1px solid var(--border);
}
.nt-filters {
  display: flex;
  gap: 4px;
}
.nt-filter {
  border: 1px solid transparent;
  background: transparent;
  color: var(--text-muted);
  padding: 3px 10px;
  border-radius: 5px;
  cursor: pointer;
  font-size: 12px;
}
.nt-filter:hover {
  background: var(--bg-hover);
}
.nt-filter.active {
  background: var(--accent-soft);
  color: var(--accent);
  border-color: var(--accent-border);
}
.nt-search {
  flex: 1;
  min-width: 60px;
  background: var(--bg-panel-2);
  border: 1px solid var(--border);
  border-radius: 5px;
  color: var(--text);
  padding: 4px 10px;
  font-size: 12px;
  outline: none;
}
.nt-search:focus {
  border-color: var(--accent-border);
}
.nt-head,
.nt-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 12px;
}
.nt-head {
  height: 30px;
  font-size: 11px;
  color: var(--text-dim);
  border-bottom: 1px solid var(--border);
  background: var(--bg-panel-2);
  flex-shrink: 0;
}
.nt-body {
  flex: 1;
  overflow-y: auto;
  position: relative;
}
.nt-row {
  position: absolute;
  left: 0;
  right: 0;
  cursor: pointer;
  font-size: 12px;
}
.nt-row:hover {
  background: var(--bg-hover);
}
.nt-row.sel {
  background: var(--sel-row);
  box-shadow: inset 2px 0 0 var(--sel-bar);
}
.nt-row.idr .c-idx {
  color: var(--badge-idr-t);
}
.c-idx {
  width: 52px;
  text-align: right;
  color: var(--text-muted);
  flex-shrink: 0;
}
.c-off {
  width: 84px;
  color: var(--text-muted);
  flex-shrink: 0;
}
.c-size {
  width: 64px;
  text-align: right;
  color: var(--text-muted);
  flex-shrink: 0;
}
.c-type {
  width: 150px;
  flex-shrink: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.c-ref {
  width: 48px;
  color: var(--text-dim);
  flex-shrink: 0;
}
.c-prev {
  flex: 1;
  color: var(--text-dim);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11px;
}
.nt-footer {
  padding: 6px 12px;
  font-size: 11px;
  color: var(--text-dim);
  border-top: 1px solid var(--border);
  flex-shrink: 0;
}
</style>
