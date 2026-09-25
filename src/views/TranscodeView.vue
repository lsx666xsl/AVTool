<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import FilePicker from '@/components/FilePicker.vue'
import { api, listenFfProgress, pickSave, STREAM_EXTS } from '@/bridge'
import { requestOpenFile } from '@/store'
import type { FfProgress, MediaProbe, TranscodeResult } from '@/types'

type PresetId = 'embedded' | 'high' | 'copy' | 'custom'

const PRESETS: Array<{ id: PresetId; label: string; desc: string; ext: string }> = [
  {
    id: 'embedded',
    label: '嵌入式友好',
    desc: 'Constrained Baseline · 4:2:0 · 无B帧 · 每IDR重复参数集',
    ext: 'mp4',
  },
  { id: 'high', label: '通用高质量', desc: 'H.264 High · 4:2:0 · 保持兼容', ext: 'mp4' },
  { id: 'copy', label: '仅重封装', desc: '不重新编码，裸流 ↔ MP4 直接换封装', ext: 'mp4' },
  { id: 'custom', label: '自定义', desc: '手写 ffmpeg 附加参数（空格分隔）', ext: 'mp4' },
]

const input = ref<string | null>(null)
const inputProbe = ref<MediaProbe | null>(null)
const preset = ref<PresetId>('embedded')
const level = ref('3.0')
const gop = ref('30')
const removeAudio = ref(true)
const customArgs = ref('')
const output = ref<string | null>(null)
const running = ref(false)
const result = ref<TranscodeResult | null>(null)
const progress = ref<FfProgress | null>(null)
const error = ref('')
let unlisten: (() => void) | null = null

async function onInputPicked(p: string): Promise<void> {
  input.value = p
  inputProbe.value = null
  result.value = null
  error.value = ''
  output.value = null
  try {
    inputProbe.value = await api.ffmpegProbe(p)
  } catch {
    /* probe 失败不阻塞，转码时仍可尝试 */
  }
  suggestOutput()
}

function suggestOutput(): void {
  if (!input.value) return
  const base = input.value.replace(/\\/g, '/').split('/').pop()?.replace(/\.[^.]+$/, '') ?? 'output'
  const ext = PRESETS.find((p) => p.id === preset.value)?.ext ?? 'mp4'
  output.value = input.value.replace(/[^\\/]+$/, '') + `${base}_${preset.value}.${ext}`
}

async function chooseOutput(): Promise<void> {
  const p = await pickSave(output.value ?? 'output.mp4', ['mp4', '264', 'h264'])
  if (p) output.value = p
}

const args = computed<string[]>(() => {
  switch (preset.value) {
    case 'embedded':
      return [
        '-c:v', 'libx264',
        '-profile:v', 'baseline',
        '-level', level.value || '3.0',
        '-vf', 'format=yuv420p',
        '-bf', '0',
        '-g', gop.value || '30',
        '-preset', 'veryfast',
        '-x264-params', 'repeat-headers=1',
        ...(removeAudio.value ? ['-an'] : []),
      ]
    case 'high':
      return [
        '-c:v', 'libx264',
        '-profile:v', 'high',
        '-level', level.value || '4.0',
        '-vf', 'format=yuv420p',
        '-g', gop.value || '30',
        '-preset', 'veryfast',
        ...(removeAudio.value ? ['-an'] : []),
      ]
    case 'copy':
      return ['-c', 'copy']
    case 'custom':
      return customArgs.value.split(/\s+/).filter(Boolean)
    default:
      return []
  }
})

async function run(): Promise<void> {
  if (!input.value || !output.value || running.value) return
  running.value = true
  result.value = null
  error.value = ''
  progress.value = null
  try {
    result.value = await api.ffmpegTranscode(input.value, output.value, args.value)
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    running.value = false
  }
}

async function cancel(): Promise<void> {
  try {
    await api.ffmpegCancel()
  } catch {
    /* 忽略 */
  }
}

function pickPreset(): void {
  suggestOutput()
}

onMounted(async () => {
  unlisten = await listenFfProgress((p) => {
    if (p.phase === 'transcode') progress.value = p
  })
})
onBeforeUnmount(() => unlisten?.())
</script>

<template>
  <div class="transcode">
    <div class="toolbar">
      <FilePicker label="选择输入文件…" :exts="STREAM_EXTS" @picked="onInputPicked" />
      <button class="btn ghost" :disabled="!input" @click="chooseOutput">输出路径…</button>
      <span v-if="input" class="file-name mono" :title="input ?? ''">{{ input?.replace(/\\/g, '/').split('/').pop() }}</span>
    </div>

    <div v-if="error" class="error-bar">{{ error }}</div>

    <div v-if="!input" class="empty-state">
      <div class="big">🔁</div>
      <h2>按预设重编码 / 重封装</h2>
      <p>调用本机 ffmpeg，按目标解码器能力一键生成新码流</p>
    </div>

    <div v-else class="layout">
      <div class="col">
        <div class="card">
          <div class="panel-title">输入信息</div>
          <div v-if="inputProbe" class="chips">
            <span class="chip">编码 <b>{{ inputProbe.codec }}</b></span>
            <span class="chip">分辨率 <b>{{ inputProbe.width }}x{{ inputProbe.height }}</b></span>
            <span class="chip">像素格式 <b>{{ inputProbe.pixFmt }}</b></span>
            <span class="chip">时长 <b>{{ inputProbe.durationSec.toFixed(2) }}s</b></span>
          </div>
          <div v-else class="card-hint">未能探测输入信息（仍可尝试转码）</div>
        </div>

        <div class="card">
          <div class="panel-title">转码预设</div>
          <label v-for="p in PRESETS" :key="p.id" class="preset-row" :class="{ active: preset === p.id }">
            <input v-model="preset" type="radio" :value="p.id" @change="pickPreset" />
            <span class="preset-label">{{ p.label }}</span>
            <span class="preset-desc">{{ p.desc }}</span>
          </label>
          <div v-if="preset === 'embedded' || preset === 'high'" class="opt-row">
            <label class="opt">
              level
              <input v-model="level" class="opt-input mono" />
            </label>
            <label class="opt">
              GOP
              <input v-model="gop" class="opt-input mono" />
            </label>
            <label class="opt chk">
              <input v-model="removeAudio" type="checkbox" />
              移除音频
            </label>
          </div>
          <div v-if="preset === 'custom'" class="custom-box">
            <textarea v-model="customArgs" class="custom-ta mono" rows="3" placeholder="-c:v libx264 -profile:v main -crf 23 …" />
          </div>
        </div>

        <div class="card">
          <div class="panel-title">输出</div>
          <div class="out-row">
            <span class="out-path mono" :title="output ?? ''">{{ output ?? '（先选择输出路径）' }}</span>
          </div>
          <div class="run-row">
            <button
              v-if="!running"
              class="btn primary"
              :disabled="!output"
              @click="run"
            >
              开始转码
            </button>
            <button v-else class="btn ghost danger" @click="cancel">取消</button>
          </div>
        </div>
      </div>

      <div class="col">
        <div class="card progress-card">
          <div class="panel-title">进度</div>
          <template v-if="running && progress">
            <div class="bar-wrap">
              <div
                class="bar"
                :style="{ width: inputProbe?.frameCount ? Math.min(100, (progress.frame / inputProbe.frameCount) * 100) + '%' : '50%' }"
              />
            </div>
            <div class="prog-rows mono">
              <span>帧 {{ progress.frame }}<template v-if="inputProbe?.frameCount"> / {{ inputProbe.frameCount }}</template></span>
              <span>已输出 {{ progress.outTimeSec.toFixed(2) }}s</span>
              <span v-if="progress.speed">速度 {{ progress.speed }}</span>
            </div>
          </template>
          <div v-else-if="running" class="card-hint">启动转码…</div>
          <template v-else-if="result">
            <div class="check-banner" :class="result.success ? 'no-diff' : 'has-diff'">
              {{ result.success ? '转码完成 ✓' : `转码失败（退出码 ${result.code}）` }}
            </div>
            <div v-if="result.errorTail" class="err-list mono">{{ result.errorTail }}</div>
            <div v-if="result.success" class="done-row">
              <button class="btn primary" @click="requestOpenFile(output!)">用码流分析打开结果</button>
              <button class="btn ghost" @click="input = output; output = null; inputProbe = null">作为新的输入</button>
            </div>
          </template>
          <div v-else class="card-hint">
            转码参数预览：
            <div class="args-preview mono">ffmpeg {{ args.join(' ') }} "{{ output ?? 'output' }}"</div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.transcode {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow-y: auto;
  padding: 14px;
  gap: 12px;
}
.toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}
.file-name {
  color: var(--text-muted);
  font-size: 12px;
  max-width: 260px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.layout {
  display: grid;
  grid-template-columns: minmax(380px, 1fr) minmax(320px, 0.8fr);
  gap: 12px;
  align-items: start;
}
.col {
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-width: 0;
}
.chips {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
  padding: 12px 14px;
}
.card-hint {
  padding: 12px 14px;
  color: var(--text-dim);
  font-size: 12px;
}
.preset-row {
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding: 9px 14px;
  cursor: pointer;
  border-bottom: 1px solid var(--border);
}
.preset-row:hover {
  background: var(--bg-hover);
}
.preset-row.active {
  background: var(--accent-soft);
}
.preset-label {
  font-weight: 600;
  font-size: 13px;
  min-width: 90px;
}
.preset-row.active .preset-label {
  color: var(--accent);
}
.preset-desc {
  font-size: 11.5px;
  color: var(--text-dim);
}
.opt-row {
  display: flex;
  gap: 16px;
  padding: 10px 14px;
  flex-wrap: wrap;
}
.opt {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--text-muted);
}
.opt-input {
  width: 64px;
  background: var(--bg-panel-2);
  border: 1px solid var(--border);
  border-radius: 5px;
  color: var(--text);
  padding: 4px 8px;
  outline: none;
}
.opt.chk {
  cursor: pointer;
}
.custom-box {
  padding: 0 14px 12px;
}
.custom-ta {
  width: 100%;
  background: var(--bg-panel-2);
  border: 1px solid var(--border);
  border-radius: 6px;
  color: var(--text);
  padding: 8px 10px;
  font-size: 12px;
  outline: none;
  resize: vertical;
}
.out-row {
  padding: 12px 14px 6px;
}
.out-path {
  font-size: 12px;
  color: var(--text-muted);
  word-break: break-all;
}
.run-row {
  padding: 8px 14px 14px;
}
.btn.danger {
  border-color: rgba(247, 84, 100, 0.5);
  color: var(--danger-text);
}
.progress-card {
  min-height: 160px;
}
.bar-wrap {
  margin: 16px 14px 10px;
  height: 10px;
  border-radius: 5px;
  background: var(--bg-hover);
  overflow: hidden;
}
.bar {
  height: 100%;
  background: linear-gradient(90deg, var(--accent), #7c5cf0);
  border-radius: 5px;
  transition: width 0.2s;
}
.prog-rows {
  display: flex;
  gap: 18px;
  padding: 0 14px 12px;
  font-size: 12px;
  color: var(--text-muted);
  flex-wrap: wrap;
}
.check-banner {
  margin: 14px;
  border-radius: 8px;
  padding: 9px 12px;
  font-weight: 600;
  font-size: 12.5px;
}
.check-banner.has-diff {
  background: rgba(247, 84, 100, 0.1);
  border: 1px solid rgba(247, 84, 100, 0.4);
  color: var(--danger-text);
}
.check-banner.no-diff {
  background: rgba(95, 173, 101, 0.1);
  border: 1px solid rgba(95, 173, 101, 0.4);
  color: var(--success-text);
}
.err-list {
  margin: 0 14px 14px;
  max-height: 140px;
  overflow: auto;
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 8px 10px;
  font-size: 11px;
  color: var(--danger-text);
  white-space: pre-wrap;
  word-break: break-all;
}
.done-row {
  display: flex;
  gap: 10px;
  padding: 0 14px 14px;
  flex-wrap: wrap;
}
.args-preview {
  margin-top: 6px;
  font-size: 11px;
  color: var(--text-muted);
  word-break: break-all;
  line-height: 1.7;
}
</style>
