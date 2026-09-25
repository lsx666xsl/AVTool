<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import FilePicker from '@/components/FilePicker.vue'
import { api, fileSrc, listenFfProgress, STREAM_EXTS } from '@/bridge'
import type { CheckReport, FfmpegEnv, MediaProbe } from '@/types'

const env = ref<FfmpegEnv | null>(null)
const path = ref<string | null>(null)
const probe = ref<MediaProbe | null>(null)
const videoSrc = ref('')
const check = ref<CheckReport | null>(null)
const checking = ref(false)
const frameNo = ref(0)
const frameImg = ref('')
const frameLoading = ref(false)
const frameError = ref('')
const error = ref('')
let unlisten: (() => void) | null = null

const MEDIA_EXTS = [...STREAM_EXTS, 'mov', 'mkv', 'flv', 'ts']

async function onPicked(p: string): Promise<void> {
  error.value = ''
  check.value = null
  frameImg.value = ''
  frameError.value = ''
  path.value = p
  videoSrc.value = ''
  try {
    probe.value = await api.ffmpegProbe(p)
    // 裸流先重封装为 MP4 再交给浏览器播放
    videoSrc.value = await fileSrc(await api.ffmpegRemux(p))
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
    probe.value = null
  }
}

async function runCheck(): Promise<void> {
  if (!path.value) return
  checking.value = true
  error.value = ''
  try {
    check.value = await api.ffmpegDecodeCheck(path.value)
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    checking.value = false
  }
}

async function loadFrame(delta: number): Promise<void> {
  if (!path.value) return
  const target = Math.max(0, frameNo.value + delta)
  frameLoading.value = true
  frameError.value = ''
  try {
    frameImg.value = await api.ffmpegDecodeFrame(path.value, target)
    frameNo.value = target
  } catch (e) {
    frameError.value = e instanceof Error ? e.message : String(e)
  } finally {
    frameLoading.value = false
  }
}

onMounted(async () => {
  env.value = await api.ffmpegDetect()
  unlisten = await listenFfProgress((p) => {
    // 解码验证的进度可以驱动帧数显示（简化：仅转码页使用完整进度）
    if (p.phase === 'check' && checking.value && check.value) {
      check.value.frames = p.frame
    }
  })
})
onBeforeUnmount(() => unlisten?.())
</script>

<template>
  <div class="preview">
    <div class="env-bar">
      <span v-if="!env" class="chip">检测 ffmpeg 中…</span>
      <template v-else-if="env.available">
        <span class="chip ok">ffmpeg 可用</span>
        <span class="env-version mono">{{ env.version }}</span>
      </template>
      <span v-else class="chip bad">未检测到 ffmpeg —— 请安装后重启应用（winget install ffmpeg）</span>
    </div>

    <div class="toolbar">
      <FilePicker label="打开媒体文件…" :exts="MEDIA_EXTS" @picked="onPicked" />
      <button class="btn ghost" :disabled="!path || checking" @click="runCheck">
        {{ checking ? '解码验证中…' : '解码验证' }}
      </button>
      <span v-if="path" class="file-name mono" :title="path ?? ''">{{ path?.replace(/\\/g, '/').split('/').pop() }}</span>
    </div>

    <div v-if="error" class="error-bar">{{ error }}</div>

    <div v-if="!path" class="empty-state">
      <div class="big">▶️</div>
      <h2>解码预览与验证</h2>
      <p>播放码流（裸流自动重封装）、检查解码错误、逐帧预览画面</p>
    </div>

    <div v-else class="grid">
      <div class="col">
        <div class="card">
          <div class="panel-title">媒体信息</div>
          <div v-if="probe" class="probe-chips">
            <span class="chip">编码 <b>{{ probe.codec }}</b></span>
            <span class="chip">分辨率 <b>{{ probe.width }}x{{ probe.height }}</b></span>
            <span class="chip">帧率 <b>{{ probe.fps.toFixed(3) }}</b></span>
            <span class="chip">像素格式 <b>{{ probe.pixFmt }}</b></span>
            <span class="chip">时长 <b>{{ probe.durationSec.toFixed(2) }}s</b></span>
            <span v-if="probe.frameCount" class="chip">帧数 <b>{{ probe.frameCount }}</b></span>
            <span class="chip">音频 <b>{{ probe.hasAudio ? '有' : '无' }}</b></span>
          </div>
        </div>

        <div class="card">
          <div class="panel-title">播放预览</div>
          <video v-if="videoSrc" :src="videoSrc" controls class="player" />
          <div v-else class="player-hint">此文件无法直接播放（重封装失败或演示模式）</div>
          <div class="player-note">注意：此处用系统解码器播放，能播 ≠ 单片机能解，参数是否兼容请看「参数对比」</div>
        </div>

        <div class="card">
          <div class="panel-title">解码验证</div>
          <div v-if="!check" class="card-hint">点击上方「解码验证」，ffmpeg 将完整解码视频轨并报告全部错误</div>
          <template v-else>
            <div class="check-banner" :class="check.errorCount > 0 ? 'has-diff' : 'no-diff'">
              {{ check.errorCount > 0 ? `发现 ${check.errorCount} 条解码错误` : `解码通过：${check.frames} 帧，无错误` }}
            </div>
            <div v-if="check.errors.length" class="err-list mono">
              <div v-for="(e, i) in check.errors" :key="i" class="err-line">{{ e }}</div>
            </div>
          </template>
        </div>
      </div>

      <div class="col">
        <div class="card frame-card">
          <div class="panel-title">逐帧预览</div>
          <div class="frame-controls">
            <button class="btn ghost" :disabled="frameLoading || !path" @click="loadFrame(-1)">◀ 上一帧</button>
            <input v-model.number="frameNo" type="number" min="0" class="frame-input mono" />
            <button class="btn ghost" :disabled="frameLoading || !path" @click="loadFrame(1)">下一帧 ▶</button>
            <button class="btn primary" :disabled="frameLoading || !path" @click="loadFrame(0)">
              {{ frameLoading ? '解码中…' : '解码此帧' }}
            </button>
          </div>
          <div v-if="frameError" class="frame-error">{{ frameError }}</div>
          <div class="frame-view">
            <img v-if="frameImg" :src="'data:image/png;base64,' + frameImg" class="frame-img" alt="帧预览" />
            <div v-else-if="frameLoading" class="frame-hint">解码中…（大帧号需要全量解码，较慢）</div>
            <div v-else class="frame-hint">输入帧号后点击解码</div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.preview {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow-y: auto;
  padding: 14px;
  gap: 12px;
}
.env-bar {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}
.env-version {
  font-size: 11px;
  color: var(--text-dim);
}
.chip.ok {
  color: var(--success-text);
  border-color: rgba(95, 173, 101, 0.4);
}
.chip.bad {
  color: var(--danger-text);
  border-color: rgba(247, 84, 100, 0.45);
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
.grid {
  display: grid;
  grid-template-columns: minmax(380px, 1.1fr) minmax(340px, 1fr);
  gap: 12px;
  align-items: start;
}
.col {
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-width: 0;
}
.probe-chips {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
  padding: 12px 14px;
}
.player {
  width: 100%;
  max-height: 320px;
  background: #000;
  border-radius: 0 0 8px 8px;
  display: block;
}
.player-hint {
  padding: 30px 14px;
  color: var(--text-dim);
  text-align: center;
  font-size: 12px;
}
.player-note {
  padding: 8px 14px;
  font-size: 11.5px;
  color: var(--warn-text);
  background: rgba(220, 169, 57, 0.08);
}
.card-hint {
  padding: 14px;
  color: var(--text-dim);
  font-size: 12px;
}
.check-banner {
  margin: 12px 14px;
  border-radius: 8px;
  padding: 8px 12px;
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
  max-height: 200px;
  overflow: auto;
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 8px 10px;
}
.err-line {
  font-size: 11px;
  color: var(--danger-text);
  line-height: 1.7;
  word-break: break-all;
}
.frame-controls {
  display: flex;
  gap: 8px;
  align-items: center;
  padding: 12px 14px;
  flex-wrap: wrap;
}
.frame-input {
  width: 110px;
  background: var(--bg-panel-2);
  border: 1px solid var(--border);
  border-radius: 5px;
  color: var(--text);
  padding: 6px 10px;
  font-size: 13px;
  outline: none;
}
.frame-input:focus {
  border-color: var(--accent);
}
.frame-error {
  margin: 0 14px 10px;
  color: var(--danger-text);
  font-size: 12px;
  word-break: break-all;
}
.frame-view {
  margin: 0 14px 14px;
  min-height: 220px;
  border: 1px dashed var(--border-strong);
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
}
.frame-img {
  max-width: 100%;
  max-height: 420px;
  display: block;
}
.frame-hint {
  color: var(--text-dim);
  font-size: 12px;
  padding: 40px;
}
</style>
