<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
import AnalyzerView from '@/views/AnalyzerView.vue'
import CompareView from '@/views/CompareView.vue'
import PreviewView from '@/views/PreviewView.vue'
import TranscodeView from '@/views/TranscodeView.vue'
import { api, IS_TAURI } from '@/bridge'
import { activeTab, type TabId } from '@/store'
import type { AppInfo, UpdateInfo } from '@/types'

const TABS: Array<{ id: TabId; icon: string; label: string }> = [
  { id: 'analyzer', icon: '📡', label: '码流分析' },
  { id: 'compare', icon: '⚖️', label: '参数对比' },
  { id: 'preview', icon: '▶️', label: '预览验证' },
  { id: 'transcode', icon: '🔁', label: '转码' },
]

const tab = ref<TabId>(activeTab.value)
watch(activeTab, (v) => (tab.value = v))
watch(tab, (v) => (activeTab.value = v))

const theme = ref<'dark' | 'light'>(
  document.documentElement.dataset.theme === 'light' ? 'light' : 'dark',
)

function toggleTheme(): void {
  theme.value = theme.value === 'dark' ? 'light' : 'dark'
  document.documentElement.dataset.theme = theme.value
  localStorage.setItem('avtool-theme', theme.value)
}

// ---- 更新检查：启动时静默查一次，也可手动触发 ----
const version = ref('…')
const update = ref<UpdateInfo | null>(null)
const updateDismissed = ref(false)
const checking = ref(false)
const checkNote = ref('')

async function doCheck(notify: boolean): Promise<void> {
  if (checking.value) return
  checking.value = true
  checkNote.value = ''
  try {
    const u = await api.updateCheck()
    update.value = u
    updateDismissed.value = false
    if (!u.available && notify) checkNote.value = '已是最新版本'
  } catch (e) {
    if (notify) checkNote.value = `检查失败：${e instanceof Error ? e.message : String(e)}`
  } finally {
    checking.value = false
  }
}

onMounted(async () => {
  const info: AppInfo = await api.appInfo()
  version.value = info.version
  // 无 git 提交号的构建（异常环境）不启用自动检查；演示模式展示通知效果
  if (IS_TAURI && !info.gitSha) return
  void doCheck(false)
})
</script>

<template>
  <div class="app-shell">
    <aside class="sidebar">
      <div class="brand">
        <div class="brand-logo">AV</div>
        <div class="brand-name">AVTool</div>
        <button class="theme-btn" :title="theme === 'dark' ? '切换浅色' : '切换深色'" @click="toggleTheme">
          {{ theme === 'dark' ? '☀' : '☾' }}
        </button>
      </div>
      <nav class="nav">
        <button
          v-for="t in TABS"
          :key="t.id"
          class="nav-item"
          :class="{ active: tab === t.id }"
          @click="tab = t.id"
        >
          <span class="nav-icon">{{ t.icon }}</span>
          <span class="nav-label">{{ t.label }}</span>
        </button>
      </nav>
      <div class="sidebar-footer">
        <div>v{{ version }}</div>
        <button class="check-btn" :disabled="checking" @click="doCheck(true)">
          {{ checking ? '检查中…' : '检查更新' }}
        </button>
        <div v-if="checkNote" class="check-note">{{ checkNote }}</div>
      </div>
    </aside>
    <main class="content">
      <div v-if="update?.available && !updateDismissed" class="update-bar">
        <span>
          🔔 有更新（{{ update.latestSha }} · {{ update.date }}）：{{ update.message }}
        </span>
        <button class="ub-close" @click="updateDismissed = true">✕</button>
      </div>
      <AnalyzerView v-show="tab === 'analyzer'" />
      <CompareView v-show="tab === 'compare'" />
      <PreviewView v-show="tab === 'preview'" />
      <TranscodeView v-show="tab === 'transcode'" />
    </main>
  </div>
</template>

<style scoped>
.app-shell {
  display: flex;
  height: 100%;
}
.sidebar {
  width: 200px;
  flex-shrink: 0;
  background: var(--bg-panel-2);
  border-right: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  padding: 14px 10px;
}
.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 4px 6px 16px;
}
.brand-logo {
  width: 34px;
  height: 34px;
  border-radius: 8px;
  background: linear-gradient(135deg, #3574f0, #7c5cf0);
  color: #ffffff;
  font-weight: 800;
  font-size: 14px;
  display: flex;
  align-items: center;
  justify-content: center;
  letter-spacing: 0.5px;
  flex-shrink: 0;
}
.brand-name {
  font-weight: 700;
  font-size: 15px;
  flex: 1;
}
.theme-btn {
  width: 30px;
  height: 30px;
  border-radius: 8px;
  border: 1.5px solid var(--border-strong);
  background: var(--bg-panel);
  color: var(--text-muted);
  font-size: 15px;
  line-height: 1;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: border-color 0.15s, color 0.15s;
}
.theme-btn:hover {
  border-color: var(--accent);
  color: var(--accent);
}
.nav {
  display: flex;
  flex-direction: column;
  gap: 4px;
  flex: 1;
}
.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 10px;
  background: transparent;
  border: 1px solid transparent;
  border-radius: 8px;
  cursor: pointer;
  text-align: left;
  color: var(--text);
  transition: background 0.15s;
}
.nav-item:hover {
  background: var(--bg-hover);
}
.nav-item.active {
  background: var(--accent-soft);
  border-color: var(--accent-border);
}
.nav-icon {
  font-size: 16px;
}
.nav-label {
  font-weight: 600;
  font-size: 13px;
}
.nav-item.active .nav-label {
  color: var(--accent);
}
.sidebar-footer {
  padding: 10px 8px 2px;
  font-size: 10.5px;
  color: var(--text-dim);
  border-top: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  gap: 5px;
  align-items: flex-start;
}
.check-btn {
  background: transparent;
  border: 1px solid var(--border-strong);
  border-radius: 5px;
  color: var(--text-muted);
  font-size: 10.5px;
  padding: 2px 8px;
  cursor: pointer;
}
.check-btn:hover:not(:disabled) {
  color: var(--accent);
  border-color: var(--accent);
}
.check-note {
  font-size: 10.5px;
  color: var(--text-muted);
  word-break: break-all;
}
.update-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 7px 14px;
  font-size: 12px;
  background: var(--accent-soft);
  border-bottom: 1px solid var(--accent-border);
  color: var(--text);
  flex-shrink: 0;
}
.ub-close {
  background: transparent;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  font-size: 13px;
  padding: 0 4px;
}
.ub-close:hover {
  color: var(--text);
}
.content {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
</style>
