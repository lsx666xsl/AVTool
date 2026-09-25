/**
 * 轻量跨视图状态：激活的标签页 + "用码流分析打开"请求。
 * 避免 App 与深层视图互相传_props 的样板代码。
 */
import { ref } from 'vue'

export type TabId = 'analyzer' | 'compare' | 'preview' | 'transcode'

export const activeTab = ref<TabId>('analyzer')

/** 非空的路径 = 码流分析页应加载该文件 */
export const openFileRequest = ref<string | null>(null)

export function requestOpenFile(path: string): void {
  activeTab.value = 'analyzer'
  openFileRequest.value = path
}
