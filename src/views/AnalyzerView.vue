<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import FilePicker from '@/components/FilePicker.vue'
import NaluTable from '@/components/NaluTable.vue'
import FieldGroups from '@/components/FieldGroups.vue'
import HexView from '@/components/HexView.vue'
import { api, STREAM_EXTS } from '@/bridge'
import { openFileRequest } from '@/store'
import type { NaluDetail, ParseResult } from '@/types'

const path = ref<string | null>(null)
const result = ref<ParseResult | null>(null)
const selected = ref<number | null>(null)
const detail = ref<NaluDetail | null>(null)
const loading = ref(false)
const error = ref('')

// 请求序号守卫：快速切换文件/NALU 时丢弃旧响应，防止旧结果覆盖新结果
let parseSeq = 0
let detailSeq = 0

async function onPicked(p: string): Promise<void> {
  const seq = ++parseSeq
  loading.value = true
  error.value = ''
  try {
    const r = await api.parseStream(p)
    if (seq !== parseSeq) return // 已有更新的请求
    result.value = r
    path.value = p
    detail.value = null
    const first = r.nalus.find((n) => n.nalType === 7) ?? r.nalus[0]
    const next = first ? first.index : null
    const changed = next !== selected.value
    selected.value = next
    // selected/path 都与上次相同（重复打开同一文件）时 watch 不会触发，需要显式加载
    if (!changed) void loadDetail()
  } catch (e) {
    if (seq !== parseSeq) return
    error.value = e instanceof Error ? e.message : String(e)
    result.value = null
    path.value = null
    detail.value = null
  } finally {
    if (seq === parseSeq) loading.value = false
  }
}

async function loadDetail(): Promise<void> {
  const seq = ++detailSeq
  detail.value = null
  const i = selected.value
  if (i === null || !path.value) return
  try {
    const d = await api.naluDetail(path.value, i)
    if (seq === detailSeq) detail.value = d
  } catch (e) {
    if (seq === detailSeq) error.value = e instanceof Error ? e.message : String(e)
  }
}

watch([selected, path], () => void loadDetail())

// 其他视图（如转码完成）请求用码流分析打开文件
watch(openFileRequest, (p) => {
  if (!p) return
  openFileRequest.value = null
  void onPicked(p)
})

const highlight = computed<{ start: number; end: number } | null>(() => {
  if (selected.value === null || !result.value) return null
  const n = result.value.nalus[selected.value]
  if (!n) return null
  return { start: n.offset, end: n.offset + n.startCodeLen + n.size }
})

const fileName = computed(() => path.value?.replace(/\\/g, '/').split('/').pop() ?? '')
</script>

<template>
  <div class="analyzer">
    <div class="toolbar">
      <FilePicker label="打开码流文件…" :exts="STREAM_EXTS" @picked="onPicked" />
      <span v-if="fileName" class="file-name mono" :title="path ?? ''">{{ fileName }}</span>
      <span v-if="loading" class="loading">解析中…</span>
      <template v-if="result">
        <span class="chip">来源 <b>{{ result.stats.sourceFormat === 'mp4' ? 'MP4 (已提取)' : 'AnnexB 裸流' }}</b></span>
        <span v-if="result.stats.profile" class="chip">档次 <b>{{ result.stats.profile }}</b></span>
        <span v-if="result.stats.level" class="chip">级别 <b>{{ result.stats.level }}</b></span>
        <span v-if="result.stats.resolution" class="chip">分辨率 <b>{{ result.stats.resolution }}</b></span>
        <span class="chip">NALU <b>{{ result.stats.naluCount }}</b></span>
        <span class="chip">IDR <b>{{ result.stats.idrCount }}</b></span>
        <span v-if="result.stats.truncated" class="chip warn-chip">⚠ 已达扫描上限，结果截断</span>
      </template>
    </div>

    <div v-if="error" class="error-bar">{{ error }}</div>

    <div v-if="!result && !loading" class="empty-state">
      <div class="big">📡</div>
      <h2>打开码流文件开始分析</h2>
      <p>支持 H.264 裸流与 MP4（自动提取轨道）</p>
    </div>

    <div v-else-if="result" class="layout">
      <div class="left">
        <NaluTable
          :key="path ?? 'none'"
          :nalus="result.nalus"
          :selected-index="selected"
          @select="(i: number) => (selected = i)"
        />
      </div>
      <div class="right">
        <div class="detail">
          <div v-if="detail" class="detail-inner">
            <div class="detail-head">
              <span class="detail-title">NALU #{{ detail.index }}</span>
              <span class="badge" :class="detail.typeName.includes('SPS') ? 'b-sps' : detail.typeName.includes('PPS') ? 'b-pps' : detail.typeName.includes('IDR') ? 'b-idr' : 'b-other'">
                {{ detail.typeName }}
              </span>
            </div>
            <div v-for="(w, i) in detail.warnings" :key="i" class="warn-box">⚠ {{ w }}</div>
            <FieldGroups :groups="detail.groups" />
            <div v-if="detail.rbspHex" class="rbsp">
              <div class="rbsp-title">RBSP 载荷（去仿真预防字节）</div>
              <div class="rbsp-hex mono">{{ detail.rbspHex }}{{ detail.rbspHex.length >= 3 * 4096 - 1 ? ' …' : '' }}</div>
            </div>
          </div>
          <div v-else class="empty-state">选择左侧 NALU 查看详情</div>
        </div>
        <div class="hexwrap">
          <HexView :path="path" :highlight="highlight" />
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.analyzer {
  display: flex;
  flex-direction: column;
  height: 100%;
}
.toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 14px 0;
  flex-wrap: wrap;
  flex-shrink: 0;
}
.file-name {
  color: var(--text-muted);
  font-size: 12px;
  max-width: 240px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.loading {
  color: var(--accent);
  font-size: 12px;
}
.warn-chip {
  color: var(--warn-text);
  border-color: rgba(220, 169, 57, 0.5);
}
.layout {
  flex: 1;
  display: grid;
  grid-template-columns: minmax(420px, 46%) 1fr;
  gap: 12px;
  padding: 12px 14px 14px;
  min-height: 0;
}
.left {
  min-height: 0;
}
.right {
  display: grid;
  grid-template-rows: minmax(0, 1.2fr) minmax(140px, 1fr);
  gap: 12px;
  min-height: 0;
}
.detail {
  overflow-y: auto;
  min-height: 0;
}
.detail-inner {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.detail-head {
  display: flex;
  align-items: center;
  gap: 10px;
}
.detail-title {
  font-weight: 700;
  font-size: 14px;
}
.rbsp {
  background: var(--bg-panel);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 10px 14px;
}
.rbsp-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-muted);
  margin-bottom: 6px;
}
.rbsp-hex {
  font-size: 11px;
  color: var(--text-muted);
  line-height: 1.7;
  word-break: break-all;
}
.hexwrap {
  min-height: 0;
}
</style>
