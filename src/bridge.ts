/**
 * 后端桥接层：Tauri 环境调用真实命令；
 * 纯浏览器环境（npm run dev 调试）回退到内置模拟数据。
 */
import type { CheckReport, FfProgress, FfmpegEnv, HexData, MediaProbe, ParseResult, SpsCompare } from './types'
import type { NaluDetail, TranscodeResult } from './types'
import { mockInvoke, mockListenProgress, mockPickFile } from './mock'

export const IS_TAURI: boolean =
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

/** 码流文件的打开过滤器（后端当前仅支持 H.264） */
export const STREAM_EXTS = ['264', 'h264', 'h26x', 'annexb', 'bin', 'mp4']

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!IS_TAURI) {
    return mockInvoke<T>(cmd, args ?? {})
  }
  const core = await import('@tauri-apps/api/core')
  return core.invoke<T>(cmd, args)
}

/** 打开文件选择框，返回路径（取消返回 null） */
export async function pickFile(exts: string[]): Promise<string | null> {
  if (!IS_TAURI) return mockPickFile(exts)
  const { open } = await import('@tauri-apps/plugin-dialog')
  const r = await open({
    multiple: false,
    directory: false,
    filters: [{ name: '码流文件', extensions: exts }],
  })
  return typeof r === 'string' ? r : null
}

/** 保存对话框 */
export async function pickSave(defaultName: string, exts: string[]): Promise<string | null> {
  if (!IS_TAURI) return `C:\\demo\\${defaultName}`
  const { save } = await import('@tauri-apps/plugin-dialog')
  return save({
    defaultPath: defaultName,
    filters: [{ name: '码流文件', extensions: exts }],
  })
}

export const api = {
  parseStream: (path: string): Promise<ParseResult> => invoke('parse_stream', { path }),

  naluDetail: (path: string, index: number): Promise<NaluDetail> =>
    invoke('nalu_detail', { path, index }),

  readStreamBytes: (path: string, offset: number, len: number): Promise<HexData> =>
    invoke('read_stream_bytes', { path, offset, len }),

  compareSps: (pathA: string, pathB: string): Promise<SpsCompare> =>
    invoke('compare_sps', { pathA, pathB }),

  // ---- ffmpeg 集成 ----

  ffmpegDetect: (): Promise<FfmpegEnv> => invoke('ffmpeg_detect'),

  ffmpegProbe: (path: string): Promise<MediaProbe> => invoke('ffmpeg_probe', { path }),

  ffmpegDecodeCheck: (path: string): Promise<CheckReport> =>
    invoke('ffmpeg_decode_check', { path }),

  /** 解码指定帧，返回 PNG 的 base64（不含 data: 前缀） */
  ffmpegDecodeFrame: (path: string, frame: number): Promise<string> =>
    invoke('ffmpeg_decode_frame', { path, frame }),

  /** 裸流重封装为 MP4（供浏览器播放），已是 MP4 则原样返回路径 */
  ffmpegRemux: (path: string): Promise<string> => invoke('ffmpeg_remux', { path }),

  /** 启动转码（等待完成返回），进度通过 listenFfProgress 事件推送 */
  ffmpegTranscode: (input: string, output: string, args: string[]): Promise<TranscodeResult> =>
    invoke('ffmpeg_transcode', { request: { input, output, args } }),

  ffmpegCancel: (): Promise<boolean> => invoke('ffmpeg_cancel'),
}

/** 订阅 ffmpeg 进度事件，返回取消订阅函数 */
export async function listenFfProgress(handler: (p: FfProgress) => void): Promise<() => void> {
  if (!IS_TAURI) return mockListenProgress(handler)
  const { listen } = await import('@tauri-apps/api/event')
  return listen<FfProgress>('ff-progress', (e) => handler(e.payload))
}

/** 本地文件的 asset:// URL（供 <video>/<img> 播放）；演示模式返回空串 */
export async function fileSrc(path: string): Promise<string> {
  if (!IS_TAURI) return ''
  const { convertFileSrc } = await import('@tauri-apps/api/core')
  return convertFileSrc(path)
}
