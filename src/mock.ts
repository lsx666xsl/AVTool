/**
 * 浏览器演示模式模拟数据：结构与 Rust 后端真实输出完全一致，
 * 用于纯前端 UI 开发与视觉验收（npm run dev 直接在浏览器打开）。
 */
import type {
  CheckReport,
  FieldGroup,
  FfProgress,
  FfmpegEnv,
  MediaProbe,
  NaluDetail,
  NaluSummary,
  ParseResult,
  SpsCompare,
} from './types'

const mockPath = 'C:\\demo\\sample_1920x1080_baseline.264'

export function mockPickFile(_exts: string[]): string | null {
  return mockPath
}

// ============ ffmpeg 演示模式 ============

const progressHandlers = new Set<(p: FfProgress) => void>()
let simTimer: ReturnType<typeof setInterval> | null = null

export function mockListenProgress(handler: (p: FfProgress) => void): () => void {
  progressHandlers.add(handler)
  return () => progressHandlers.delete(handler)
}

function emitSimProgress(total: number): void {
  if (simTimer) clearInterval(simTimer)
  let step = 0
  simTimer = setInterval(() => {
    step += 1
    const done = step >= total
    const p: FfProgress = {
      phase: done ? 'done' : 'transcode',
      frame: Math.round((step / total) * 109),
      outTimeSec: (step / total) * 3.6,
      speed: done ? '' : '3.1x',
      totalSize: Math.round((step / total) * 4188880),
    }
    progressHandlers.forEach((h) => h(p))
    if (done && simTimer) {
      clearInterval(simTimer)
      simTimer = null
    }
  }, 120)
}

/** 演示模式用 canvas 生成带帧号的 PNG（返回纯 base64） */
function mockFramePng(frame: number): string {
  const canvas = document.createElement('canvas')
  canvas.width = 640
  canvas.height = 360
  const ctx = canvas.getContext('2d')
  if (ctx) {
    const grad = ctx.createLinearGradient(0, 0, 640, 360)
    grad.addColorStop(0, '#1e1f22')
    grad.addColorStop(1, '#3574f0')
    ctx.fillStyle = grad
    ctx.fillRect(0, 0, 640, 360)
    ctx.fillStyle = '#dfe1e5'
    ctx.font = 'bold 48px Consolas, monospace'
    ctx.textAlign = 'center'
    ctx.fillText(`FRAME ${frame}`, 320, 168)
    ctx.font = '16px sans-serif'
    ctx.fillStyle = '#9da0a8'
    ctx.fillText('演示模式 · 模拟解码输出', 320, 210)
  }
  return canvas.toDataURL('image/png').replace(/^data:image\/png;base64,/, '')
}

function mockDetect(): FfmpegEnv {
  return { available: true, version: 'ffmpeg version 7.1 (演示模式)', ffprobe: true }
}

function mockProbe(): MediaProbe {
  return {
    codec: 'h264',
    width: 1920,
    height: 1080,
    pixFmt: 'yuv420p',
    fps: 30,
    durationSec: 3.633,
    frameCount: 109,
    formatName: 'mov,mp4,m4a,3gp,3g2,mj2',
    hasAudio: false,
  }
}

function mockCheck(): CheckReport {
  return { frames: 109, errorCount: 0, errors: [] }
}

const PREVIEWS: Record<number, string> = {
  1: '41 9A 42 1C 6B 90 44 58',
  5: '65 88 84 21 A0 FC 00 44',
  6: '06 00 01 C0 60 08 00 40',
  7: '67 42 C0 1E D9 00 D4 3D',
  8: '68 EB EC B2 2C 00 00 01',
  9: '09 10 00 00 00 00 00 00',
}

function buildNalus(): NaluSummary[] {
  const list: NaluSummary[] = []
  let offset = 0
  const push = (nalType: number, typeName: string, size: number, isIdr = false) => {
    list.push({
      index: list.length,
      offset,
      size,
      startCodeLen: 4,
      nalType,
      typeName,
      refIdc: nalType === 9 ? 0 : nalType === 6 ? 0 : 3,
      isIdr,
      preview: PREVIEWS[nalType] ?? '41 9A 42 1C 6B 90 44 58',
    })
    offset += size + 4
  }
  push(9, 'AUD 分界符', 2)
  push(7, 'SPS', 26)
  push(8, 'PPS', 6)
  push(6, 'SEI', 24)
  push(5, 'IDR切片', 84213, true)
  for (let i = 1; i <= 48; i++) {
    push(9, 'AUD 分界符', 2)
    if (i % 10 === 0) {
      push(7, 'SPS', 26)
      push(8, 'PPS', 6)
    }
    if (i % 10 === 0) {
      push(5, 'IDR切片', 61020, true)
    } else {
      push(1, '非IDR切片 (P/B)', 18000 + (i % 7) * 2400)
    }
  }
  return list
}

function countBy(nalus: NaluSummary[]): Array<[string, number]> {
  const m = new Map<string, number>()
  for (const n of nalus) m.set(n.typeName, (m.get(n.typeName) ?? 0) + 1)
  return [...m.entries()].sort((a, b) => b[1] - a[1])
}

function buildParseResult(): ParseResult {
  const nalus = buildNalus()
  const total = nalus[nalus.length - 1]!.offset + nalus[nalus.length - 1]!.size
  return {
    nalus,
    stats: {
      totalSize: total,
      naluCount: nalus.length,
      counts: countBy(nalus),
      idrCount: nalus.filter((n) => n.isIdr).length,
      profile: '66 (Baseline)',
      level: '30 (3.0)',
      resolution: '1920x1080',
      sourceFormat: 'annexb',
      truncated: false,
    },
  }
}

function spsGroupsBaseline(): FieldGroup[] {
  return [
    {
      title: 'SPS · 基础信息',
      note: null,
      fields: [
        { name: 'profile_idc', value: '66 (Baseline)', bits: 'bit 8 · 8位', desc: '档次：决定允许使用的编码工具' },
        { name: 'constraint_set0_flag', value: '1', bits: 'bit 16 · 1位', desc: null },
        { name: 'constraint_set1_flag', value: '1', bits: 'bit 17 · 1位', desc: '1 时为 Constrained Baseline/Main（嵌入式解码器常要求）' },
        { name: 'constraint_set2_flag', value: '0', bits: 'bit 18 · 1位', desc: null },
        { name: 'level_idc', value: '30 (3.0)', bits: 'bit 24 · 8位', desc: '级别：分辨率/码率上限' },
        { name: 'seq_parameter_set_id', value: '0', bits: 'bit 32 · 1位(ue)', desc: null },
      ],
    },
    {
      title: 'SPS · 编码结构',
      note: null,
      fields: [
        { name: 'log2_max_frame_num_minus4', value: '0', bits: 'bit 33 · 1位(ue)', desc: 'frame_num 位宽 = 4' },
        { name: 'pic_order_cnt_type', value: '2 (推断型，无需额外字段)', bits: 'bit 34 · 1位(ue)', desc: 'POC 计算方式（部分硬件仅支持特定类型）' },
        { name: 'max_num_ref_frames', value: '1', bits: 'bit 35 · 1位(ue)', desc: '参考帧数（影响 DPB 内存）' },
        { name: 'gaps_in_frame_num_value_allowed_flag', value: '0', bits: 'bit 36 · 1位', desc: null },
      ],
    },
    {
      title: 'SPS · 图像尺寸',
      note: null,
      fields: [
        { name: 'pic_width_in_mbs_minus1', value: '119', bits: 'bit 37 · 7位(ue)', desc: '= 120 宏块列' },
        { name: 'pic_height_in_map_units_minus1', value: '67', bits: 'bit 44 · 7位(ue)', desc: '= 68 宏块行' },
        { name: 'frame_mbs_only_flag', value: '1', bits: 'bit 51 · 1位', desc: '0 表示允许场编码（隔行）' },
        { name: 'direct_8x8_inference_flag', value: '1', bits: 'bit 52 · 1位', desc: null },
        { name: 'frame_cropping_flag', value: '0', bits: 'bit 53 · 1位', desc: null },
        { name: '分辨率(计算)', value: '1920x1080', bits: null, desc: null },
        { name: 'vui_parameters_present_flag', value: '1', bits: 'bit 54 · 1位', desc: 'VUI 含帧率/宽高比/HRD 等' },
      ],
    },
    {
      title: 'SPS · VUI 视频可用性信息',
      note: null,
      fields: [
        { name: 'timing_info_present_flag', value: '1', bits: 'bit 55 · 1位', desc: '帧率信息' },
        { name: 'num_units_in_tick / time_scale', value: '1 / 30', bits: 'bit 56 · 64位', desc: 'time_scale/num_units_in_tick = 场率' },
        { name: 'fixed_frame_rate_flag', value: '1', bits: 'bit 120 · 1位', desc: null },
        { name: '帧率(计算)', value: '30.000 fps', bits: null, desc: 'time_scale / num_units_in_tick' },
        { name: 'pic_struct_present_flag', value: '0', bits: 'bit 121 · 1位', desc: null },
      ],
    },
  ]
}

function ppsGroups(): FieldGroup[] {
  return [
    {
      title: 'PPS · 基础信息',
      note: null,
      fields: [
        { name: 'pic_parameter_set_id', value: '0', bits: 'bit 8 · 1位(ue)', desc: null },
        { name: 'seq_parameter_set_id', value: '0', bits: 'bit 9 · 1位(ue)', desc: '引用的 SPS' },
      ],
    },
    {
      title: 'PPS · 熵编码与预测',
      note: null,
      fields: [
        { name: 'entropy_coding_mode_flag', value: '0 (CAVLC)', bits: 'bit 10 · 1位', desc: 'CABAC 仅 Main/High 可用，Baseline 必须 CAVLC' },
        { name: 'bottom_field_pic_order_in_frame_present_flag', value: '0', bits: 'bit 11 · 1位', desc: '场编码时 POC 相关' },
        { name: 'num_slice_groups_minus1', value: '0', bits: 'bit 12 · 1位(ue)', desc: 'FMO 多切片组（罕见）' },
        { name: 'num_ref_idx_l0/l1_default_active_minus1', value: '0/0', bits: 'bit 13 · 2位(ue)', desc: '默认参考帧列表长度' },
        { name: 'weighted_pred_flag', value: '0', bits: 'bit 15 · 1位', desc: 'P/SP 切片加权预测（嵌入式常见不支持）' },
        { name: 'weighted_bipred_idc', value: '0 (无)', bits: 'bit 16 · 2位', desc: 'B 切片加权预测模式' },
      ],
    },
    {
      title: 'PPS · 量化',
      note: null,
      fields: [
        { name: 'pic_init_qp_minus26', value: '0', bits: 'bit 18 · 1位(se)', desc: '初始 QP = 26' },
        { name: 'pic_init_qs_minus26', value: '0', bits: 'bit 19 · 1位(se)', desc: null },
        { name: 'chroma_qp_index_offset', value: '0', bits: 'bit 20 · 1位(se)', desc: null },
      ],
    },
    {
      title: 'PPS · 其他控制',
      note: null,
      fields: [
        { name: 'deblocking_filter_control_present_flag', value: '1', bits: 'bit 21 · 1位', desc: '切片头是否携带环路滤波参数' },
        { name: 'constrained_intra_pred_flag', value: '0', bits: 'bit 22 · 1位', desc: null },
        { name: 'redundant_pic_cnt_present_flag', value: '0', bits: 'bit 23 · 1位', desc: null },
        { name: 'transform_8x8_mode_flag', value: '0', bits: 'bit 24 · 1位', desc: '8x8 变换（High 档次，Baseline 无）' },
      ],
    },
  ]
}

function sliceGroups(): FieldGroup[] {
  return [
    {
      title: 'NALU 头',
      note: null,
      fields: [
        { name: 'forbidden_zero_bit', value: '0', bits: 'bit 0 · 1位', desc: '必须为 0' },
        { name: 'nal_ref_idc', value: '3（参考图像）', bits: 'bit 1 · 2位', desc: '参考优先级' },
        { name: 'nal_unit_type', value: 'IDR切片', bits: 'bit 3 · 5位', desc: 'NALU 类型' },
      ],
    },
    {
      title: '切片头 · 基础',
      note: null,
      fields: [
        { name: 'first_mb_in_slice', value: '0', bits: 'bit 8 · 1位(ue)', desc: '0 表示新图像的第一个切片' },
        { name: 'slice_type', value: '7 (I · 该图像全部切片)', bits: 'bit 9 · 3位(ue)', desc: null },
        { name: 'pic_parameter_set_id', value: '0', bits: 'bit 12 · 1位(ue)', desc: null },
        { name: 'frame_num', value: '0', bits: 'bit 13 · 4位', desc: '按解码序计数的图像编号' },
      ],
    },
    {
      title: '切片头 · 图像序号',
      note: null,
      fields: [
        { name: 'idr_pic_id', value: '0', bits: 'bit 17 · 1位(ue)', desc: 'IDR 图像编号（连续 IDR 递增区分）' },
      ],
    },
    {
      title: '切片头 · 滤波与量化',
      note: null,
      fields: [
        { name: 'slice_qp_delta', value: '0', bits: 'bit 20 · 1位(se)', desc: null },
      ],
    },
  ]
}

function seiGroups(): FieldGroup[] {
  return [
    {
      title: 'SEI · 载荷列表',
      note: null,
      fields: [
        {
          name: '载荷#0 5 · 用户数据未注册 user_data_unregistered',
          value: 'UUID=DC45E9BDE6D948B7962CD820D923EEEF，内容 "x264 - core 164 - H.264/MPEG-4 AVC codec"',
          bits: 'bit 8 · 66字节',
          desc: null,
        },
      ],
    },
  ]
}

function buildDetail(args: Record<string, unknown>): NaluDetail {
  const nalus = buildNalus()
  const idx = typeof args.index === 'number' ? args.index : 0
  const nalu = nalus[idx] ?? nalus[0]!
  const typeName = nalu.typeName
  let groups: FieldGroup[] = []
  if (nalu.nalType === 7) {
    groups = spsGroupsBaseline()
  } else if (nalu.nalType === 8) {
    groups = ppsGroups()
  } else if (nalu.nalType === 1 || nalu.nalType === 5) {
    groups = sliceGroups()
  } else if (nalu.nalType === 6) {
    groups = seiGroups()
  } else {
    groups = [
      {
        title: 'NALU 头',
        note: null,
        fields: [
          { name: 'forbidden_zero_bit', value: '0', bits: 'bit 0 · 1位', desc: '必须为 0' },
          { name: 'nal_ref_idc', value: '0（非参考图像）', bits: 'bit 1 · 2位', desc: '参考优先级' },
          { name: 'nal_unit_type', value: 'AUD 分界符', bits: 'bit 3 · 5位', desc: 'NALU 类型' },
        ],
      },
    ]
  }
  return {
    index: nalu.index,
    typeName,
    groups,
    warnings: [],
    rbspHex: '42 C0 1E D9 00 D4 3D A1 00 00 03 00 01 00 00 03 00 32 0F 16 2D 96 10 00',
  }
}

/** 确定性伪随机字节（演示模式的十六进制视图） */
function pseudoBytes(seed: number, len: number): number[] {
  let s = seed | 1
  const out: number[] = []
  for (let i = 0; i < len; i++) {
    s = (s * 1664525 + 1013904223) >>> 0
    out.push(s >>> 24)
  }
  return out
}

function buildCompare(): SpsCompare {
  return {
    diffCount: 9,
    noteA: 'A: 100 (High) / 40 (4.0) / 1920x1080 / POC类型0 / 参考帧4',
    noteB: 'B: 66 (Baseline) / 30 (3.0) / 1920x1080 / POC类型2 / 参考帧1',
    rows: [
      { section: 'SPS · 基础信息', name: 'profile_idc', valueA: '100 (High)', valueB: '66 (Baseline)', same: false, desc: '档次：决定允许使用的编码工具' },
      { section: 'SPS · 基础信息', name: 'constraint_set1_flag', valueA: '0', valueB: '1', same: false, desc: '1 时为 Constrained Baseline/Main（嵌入式解码器常要求）' },
      { section: 'SPS · 基础信息', name: 'level_idc', valueA: '40 (4.0)', valueB: '30 (3.0)', same: false, desc: '级别：分辨率/码率上限' },
      { section: 'SPS · 高档次扩展', name: 'chroma_format_idc', valueA: '1 (4:2:0)', valueB: '（B 侧无此字段）', same: false, desc: '色度采样（高档次才显式出现）' },
      { section: 'SPS · 高档次扩展', name: 'bit_depth_luma/chroma_minus8', valueA: '0/0', valueB: '（B 侧无此字段）', same: false, desc: '位深-8（10bit 时为 2）' },
      { section: 'SPS · 编码结构', name: 'pic_order_cnt_type', valueA: '0 (LSB 编码，最常见)', valueB: '2 (推断型，无需额外字段)', same: false, desc: 'POC 计算方式（部分硬件仅支持特定类型）' },
      { section: 'SPS · 编码结构', name: 'max_num_ref_frames', valueA: '4', valueB: '1', same: false, desc: '参考帧数（影响 DPB 内存）' },
      { section: 'SPS · 图像尺寸', name: '分辨率(计算)', valueA: '1920x1080', valueB: '1920x1080', same: true, desc: null },
      { section: 'SPS · VUI 视频可用性信息', name: 'timing_info_present_flag', valueA: '0', valueB: '1', same: false, desc: '帧率信息' },
      { section: 'PPS · 熵编码与预测', name: 'entropy_coding_mode_flag', valueA: '1 (CABAC)', valueB: '0 (CAVLC)', same: false, desc: 'CABAC 仅 Main/High 可用，Baseline 必须 CAVLC' },
      { section: 'PPS · 其他控制', name: 'transform_8x8_mode_flag', valueA: '1', valueB: '0', same: false, desc: '8x8 变换（High 档次，Baseline 无）' },
    ],
  }
}

export function mockInvoke<T>(cmd: string, args: Record<string, unknown>): T {
  switch (cmd) {
    case 'parse_stream':
      return buildParseResult() as T
    case 'nalu_detail':
      return buildDetail(args) as T
    case 'compare_sps':
      return buildCompare() as T
    case 'read_stream_bytes': {
      const offset = typeof args.offset === 'number' ? args.offset : 0
      const len = typeof args.len === 'number' ? args.len : 16384
      const result = buildParseResult()
      return {
        offset,
        bytes: pseudoBytes(offset * 2654435761, Math.max(0, Math.min(len, result.stats.totalSize - offset))),
        fileSize: result.stats.totalSize,
      } as T
    }
    case 'ffmpeg_detect':
      return mockDetect() as T
    case 'ffmpeg_probe':
      return mockProbe() as T
    case 'ffmpeg_decode_check':
      return mockCheck() as T
    case 'ffmpeg_decode_frame':
      return mockFramePng(typeof args.frame === 'number' ? args.frame : 0) as T
    case 'ffmpeg_remux':
      return String(args.path ?? mockPath) as T
    case 'ffmpeg_transcode':
      emitSimProgress(30)
      return { success: true, code: 0, errorTail: '' } as T
    case 'ffmpeg_cancel':
      return true as T
    default:
      throw new Error(`模拟模式未实现命令: ${cmd}`)
  }
}
