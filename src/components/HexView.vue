<script setup lang="ts">
import { computed, onMounted, ref, watch, nextTick } from 'vue'
import { api } from '@/bridge'
import type { HexData } from '@/types'

const props = defineProps<{
  path: string | null
  highlight: { start: number; end: number } | null
}>()

const WINDOW = 16384
const ROW_H = 19

const data = ref<HexData | null>(null)
const error = ref('')
const bodyEl = ref<HTMLElement | null>(null)

// 请求序号守卫：快速切换 NALU/文件时丢弃旧响应
let loadSeq = 0

async function load(): Promise<void> {
  const seq = ++loadSeq
  error.value = ''
  if (!props.path) {
    data.value = null
    return
  }
  try {
    const anchor = props.highlight ? props.highlight.start : 0
    const winStart = Math.max(0, anchor - 2048)
    const d = await api.readStreamBytes(props.path, winStart, WINDOW)
    if (seq !== loadSeq) return
    data.value = d
    await nextTick()
    if (seq === loadSeq) scrollToHighlight()
  } catch (e) {
    if (seq !== loadSeq) return
    error.value = String(e)
    data.value = null
  }
}

function scrollToHighlight(): void {
  if (!props.highlight || !data.value || !bodyEl.value) return
  const rel = Math.max(0, props.highlight.start - data.value.offset)
  const row = Math.floor(rel / 16)
  bodyEl.value.scrollTop = Math.max(0, row * ROW_H - 120)
}

onMounted(load)
watch(() => [props.path, props.highlight?.start], load)

interface HexRow {
  offset: number
  cells: Array<{ byte: number; cls: string }>
  ascii: string
}

const rows = computed<HexRow[]>(() => {
  const d = data.value
  if (!d) return []
  const hl = props.highlight
  const out: HexRow[] = []
  for (let i = 0; i < d.bytes.length; i += 16) {
    const cells: HexRow['cells'] = []
    let ascii = ''
    for (let j = 0; j < 16; j++) {
      const k = i + j
      if (k >= d.bytes.length) {
        cells.push({ byte: -1, cls: '' })
        continue
      }
      const b = d.bytes[k]!
      const global = d.offset + k
      let cls = ''
      if (hl && global >= hl.start && global < hl.end) cls = 'hx-hl'
      // 起始码高亮（00 00 01，可选前导 00）
      if (
        b === 0 &&
        k + 2 < d.bytes.length &&
        d.bytes[k + 1] === 0 &&
        d.bytes[k + 2] === 1 &&
        (!hl || global + 3 >= hl.start)
      ) {
        cls = 'hx-sc'
      }
      cells.push({ byte: b, cls })
      ascii += b >= 0x20 && b <= 0x7e ? String.fromCharCode(b) : '·'
    }
    out.push({ offset: d.offset + i, cells, ascii })
  }
  return out
})

function hex2(b: number): string {
  return b.toString(16).toUpperCase().padStart(2, '0')
}
</script>

<template>
  <div class="hex-view">
    <div class="hv-header">
      <span>十六进制视图</span>
      <span v-if="data" class="hv-range mono">
        0x{{ data.offset.toString(16).toUpperCase() }} ~
        0x{{ (data.offset + data.bytes.length).toString(16).toUpperCase() }} / 共
        {{ data.fileSize.toLocaleString() }} 字节
      </span>
    </div>
    <div v-if="error" class="hv-error">{{ error }}</div>
    <div v-if="!path" class="hv-empty">选择文件后显示字节内容</div>
    <div v-else-if="!data && !error" class="hv-empty">加载中…</div>
    <div v-else ref="bodyEl" class="hv-body mono">
      <div v-for="row in rows" :key="row.offset" class="hv-row">
        <span class="hv-off">{{ row.offset.toString(16).toUpperCase().padStart(8, '0') }}</span>
        <span class="hv-bytes">
          <span
            v-for="(c, j) in row.cells"
            :key="j"
            class="hx-byte"
            :class="c.cls"
          >{{ c.byte >= 0 ? hex2(c.byte) : '  ' }}</span>
        </span>
        <span class="hv-ascii">{{ row.ascii }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.hex-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: 10px;
  overflow: hidden;
}
.hv-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 7px 12px;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-muted);
  border-bottom: 1px solid var(--border);
  background: var(--bg-panel-2);
  flex-shrink: 0;
}
.hv-range {
  font-weight: 400;
  font-size: 11px;
  color: var(--text-dim);
}
.hv-body {
  flex: 1;
  overflow: auto;
  padding: 6px 0;
}
.hv-row {
  display: flex;
  gap: 12px;
  padding: 0 12px;
  font-size: 11.5px;
  line-height: 19px;
  white-space: nowrap;
}
.hv-off {
  color: var(--text-dim);
}
.hv-bytes {
  display: inline-flex;
  gap: 3px;
}
.hx-byte {
  width: 20px;
  text-align: center;
  color: var(--text);
}
.hx-hl {
  background: var(--hl-bg);
  border-radius: 2px;
  color: var(--diff-val);
}
.hx-sc {
  background: var(--sc-bg);
  color: var(--sc-text);
  border-radius: 2px;
}
.hv-ascii {
  color: var(--text-dim);
}
.hv-empty {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-dim);
  font-size: 12px;
}
.hv-error {
  padding: 6px 12px;
  color: var(--danger-text);
  font-size: 12px;
}
</style>
