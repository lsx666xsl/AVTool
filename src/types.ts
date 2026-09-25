/** 前后端共享类型：与 Rust 侧 model.rs / FieldGroup 严格一一对应（serde camelCase）。 */

export interface ParsedField {
  name: string
  value: string
  bits?: string | null
  desc?: string | null
}

export interface FieldGroup {
  title: string
  note?: string | null
  fields: ParsedField[]
}

export interface NaluSummary {
  index: number
  /** 起始码在码流中的偏移 */
  offset: number
  /** NALU 数据长度（不含起始码） */
  size: number
  startCodeLen: number
  nalType: number
  typeName: string
  refIdc: number
  isIdr: boolean
  preview: string
}

export type NaluCounts = Array<[string, number]>

export interface StreamStats {
  totalSize: number
  naluCount: number
  counts: NaluCounts
  idrCount: number
  profile?: string | null
  level?: string | null
  resolution?: string | null
  /** "annexb" 裸流，或 "mp4"（已自动提取） */
  sourceFormat: string
  /** NALU 数量超过扫描上限，结果被截断 */
  truncated: boolean
}

export interface ParseResult {
  nalus: NaluSummary[]
  stats: StreamStats
}

export interface NaluDetail {
  index: number
  typeName: string
  groups: FieldGroup[]
  warnings: string[]
  /** 去仿真预防后的载荷十六进制（空格分隔） */
  rbspHex: string
}

export interface HexData {
  offset: number
  bytes: number[]
  fileSize: number
}

export interface DiffRow {
  section: string
  name: string
  valueA: string
  valueB: string
  same: boolean
  desc?: string | null
}

export interface SpsCompare {
  diffCount: number
  rows: DiffRow[]
  noteA: string
  noteB: string
}

// ============ ffmpeg 集成 ============

export interface FfmpegEnv {
  available: boolean
  version: string
  ffprobe: boolean
}

export interface MediaProbe {
  codec: string
  width: number
  height: number
  pixFmt: string
  fps: number
  durationSec: number
  frameCount: number | null
  formatName: string
  hasAudio: boolean
}

export interface CheckReport {
  frames: number
  errorCount: number
  errors: string[]
}

export interface FfProgress {
  /** check | transcode | done | failed */
  phase: string
  frame: number
  outTimeSec: number
  speed: string
  totalSize: number
}

export interface TranscodeResult {
  success: boolean
  code: number
  errorTail: string
}

// ============ 应用信息与更新检查 ============

export interface AppInfo {
  version: string
  gitSha: string
}

export interface UpdateInfo {
  available: boolean
  latestSha: string
  message: string
  date: string
}
