//! MP4 / ISO-BMFF 容器解析：盒子树、轨道信息、avcC 参数集提取、样本 → AnnexB 裸流转换。

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mp4BoxNode {
    pub btype: String,
    pub offset: u64,
    pub size: u64,
    pub info: Vec<String>,
    pub children: Vec<Mp4BoxNode>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mp4TrackInfo {
    pub track_id: u32,
    pub handler: String,
    pub format: String,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub timescale: u32,
    pub duration: u64,
    pub duration_seconds: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mp4Info {
    pub boxes: Vec<Mp4BoxNode>,
    pub tracks: Vec<Mp4TrackInfo>,
    pub has_avc: bool,
    pub avc_profile: Option<String>,
    pub avc_level: Option<String>,
    pub length_size: Option<u32>,
    pub sps_count: Option<u32>,
    pub pps_count: Option<u32>,
    pub sps_hex: Vec<String>,
    pub pps_hex: Vec<String>,
}

const CONTAINERS: &[&str] = &[
    "moov", "trak", "mdia", "minf", "stbl", "edts", "dinf", "udta", "mvex", "moof", "traf",
    "sinf", "schi", "meta",
    // 视觉/音频样本描述入口盒内部也携带子盒（如 avcC / btrt / pasp）
    "avc1", "avc2", "avc3", "avc4", "encv", "hvc1", "hev1", "mp4a", "smp4",
];

const MAX_BOXES_PER_LEVEL: usize = 512;
const MAX_DEPTH: u32 = 8;

fn be16(d: &[u8], off: usize) -> u16 {
    u16::from_be_bytes([d[off], d[off + 1]])
}

fn be32(d: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([d[off], d[off + 1], d[off + 2], d[off + 3]])
}

fn be64(d: &[u8], off: usize) -> u64 {
    u64::from_be_bytes([
        d[off],
        d[off + 1],
        d[off + 2],
        d[off + 3],
        d[off + 4],
        d[off + 5],
        d[off + 6],
        d[off + 7],
    ])
}

fn fourcc_at(d: &[u8], off: usize) -> String {
    d[off..off + 4]
        .iter()
        .map(|&b| {
            if (0x20..=0x7E).contains(&b) {
                b as char
            } else {
                '·'
            }
        })
        .collect()
}

/// 盒子内容区（去掉 8/16 字节盒子头）
pub fn box_content<'a>(d: &'a [u8], node: &Mp4BoxNode) -> Option<&'a [u8]> {
    let off = node.offset as usize;
    let size32 = be32(d, off) as u64;
    let header = if size32 == 1 { 16 } else { 8 };
    d.get(off + header..off + node.size as usize)
}

/// 样本入口盒的固定字段长度（子盒从其后开始）：
/// 视觉样本入口 = SampleEntry 8 + 视觉字段 70 = 78；音频 = 8 + 20 = 28。
fn sample_entry_skip(btype: &str) -> usize {
    match btype {
        // stsd 是 FullBox：版本/标志 4 字节 + 入口计数 4 字节
        "stsd" => 8,
        // 视觉样本入口 = SampleEntry 8 + 视觉字段 70 = 78；音频 = 8 + 20 = 28
        "avc1" | "avc2" | "avc3" | "avc4" | "encv" | "hvc1" | "hev1" | "s263" | "jpeg" => 78,
        "mp4a" | "enca" | "ac-3" => 28,
        _ => 0,
    }
}

fn is_container(btype: &str) -> bool {
    CONTAINERS.contains(&btype) || sample_entry_skip(btype) > 0
}

/// 递归解析盒子树
pub fn parse_boxes(d: &[u8], start: usize, end: usize, depth: u32) -> Vec<Mp4BoxNode> {
    let mut out = Vec::new();
    let mut pos = start;
    while pos + 8 <= end && out.len() < MAX_BOXES_PER_LEVEL {
        let size32 = be32(d, pos) as u64;
        let btype = fourcc_at(d, pos + 4);
        let (header_len, size) = if size32 == 1 {
            if pos + 16 > end {
                break;
            }
            (16usize, be64(d, pos + 8))
        } else if size32 == 0 {
            (8usize, (end - pos) as u64)
        } else {
            (8usize, size32)
        };
        if size < header_len as u64 || pos as u64 + size > end as u64 {
            break; // 尺寸异常，视为损坏
        }
        let content_start = pos + header_len;
        let content_end = pos + size as usize;
        let mut info = Vec::new();
        let mut children = Vec::new();
        if is_container(&btype) {
            let skip = if btype == "meta" { 4 } else { sample_entry_skip(&btype) };
            if depth < MAX_DEPTH && content_start + skip <= content_end {
                children = parse_boxes(d, content_start + skip, content_end, depth + 1);
            }
        } else {
            describe_box(d, content_start, content_end, &btype, &mut info);
        }
        out.push(Mp4BoxNode {
            btype,
            offset: pos as u64,
            size,
            info,
            children,
        });
        pos += size as usize;
    }
    out
}

/// 为常见盒子生成摘要信息行（仅用于展示）
fn describe_box(d: &[u8], s: usize, e: usize, btype: &str, info: &mut Vec<String>) {
    let len = e - s;
    match btype {
        "ftyp" if len >= 4 => {
            info.push(format!("品牌 {}", fourcc_at(d, s)));
            if len >= 8 {
                info.push(format!("版本 {}", be32(d, s + 4)));
            }
            info.push(format!("兼容品牌 {} 项", (len - 4) / 4));
        }
        "mvhd" if len >= 20 => {
            let version = d[s];
            // v0: v/f(4)+ctime(4)+mtime(4)+timescale(4)+duration(4)
            if version == 1 && len >= 32 {
                info.push(format!("timescale {}", be32(d, s + 20)));
                info.push(format!("duration {}", be64(d, s + 24)));
            } else {
                info.push(format!("timescale {}", be32(d, s + 12)));
                info.push(format!("duration {}", be32(d, s + 16)));
            }
        }
        "tkhd" if len >= 84 => {
            let version = d[s];
            // v0: v/f(4)+ctime(4)+mtime(4)+track_id(4)…
            let track_id = if version == 1 { be32(d, s + 20) } else { be32(d, s + 12) };
            info.push(format!("track_id {track_id}"));
            let wh = len - 8;
            let w = be32(d, s + wh) as f64 / 65536.0;
            let h = be32(d, s + wh + 4) as f64 / 65536.0;
            if w > 0.0 {
                info.push(format!("显示尺寸 {:.0}x{:.0}", w, h));
            }
        }
        "mdhd" if len >= 20 => {
            let version = d[s];
            let ts = if version == 1 && len >= 32 { be32(d, s + 20) } else { be32(d, s + 12) };
            info.push(format!("timescale {ts}"));
        }
        "hdlr" if len >= 12 => {
            info.push(format!("处理器 {}", fourcc_at(d, s + 8)));
        }
        "stsd" if len >= 16 => {
            info.push(format!("样本描述 {} 项", be32(d, s + 4).min(9999)));
            // 内容 = 版本/标志(4) + 入口计数(4) + 入口盒(尺寸4 + 格式4)
            info.push(format!("首项格式 {}", fourcc_at(d, s + 12)));
        }
        "stsz" if len >= 12 => {
            let ss = be32(d, s + 4);
            let cnt = be32(d, s + 8);
            if ss == 0 {
                info.push(format!("变长样本 {} 个", cnt.min(9_999_999)));
            } else {
                info.push(format!("定长样本 {ss} 字节 × {}", cnt.min(9_999_999)));
            }
        }
        "stco" | "co64" if len >= 8 => {
            info.push(format!("chunk {} 个", be32(d, s + 4).min(9_999_999)));
        }
        "stts" if len >= 8 => {
            info.push(format!("时间-样本映射 {} 项", be32(d, s + 4).min(9_999_999)));
        }
        "avcC" if len >= 8 => {
            let (profile, compat, level, ls, ns, np) = avcc_header_info(d, s, e);
            info.push(format!("Profile {profile} / Compat 0x{compat:02X} / Level {level}"));
            info.push(format!("NALU 长度前缀 {ls} 字节"));
            info.push(format!("SPS {ns} 个 / PPS {np} 个"));
        }
        "hvcC" => info.push("HEVC 配置（v1 暂不解析）".into()),
        "mdat" => info.push(format!("媒体数据 {} 字节", len)),
        _ => {}
    }
}

/// 解析 avcC 头部概要：(profile, compat, level, 长度前缀字节数, SPS 个数, PPS 个数)
fn avcc_header_info(d: &[u8], s: usize, e: usize) -> (u8, u8, u8, u32, u32, u32) {
    if e - s < 6 {
        return (0, 0, 0, 0, 0, 0);
    }
    let profile = d[s + 1];
    let compat = d[s + 2];
    let level = d[s + 3];
    let length_size = (d[s + 4] & 0x03) as u32 + 1;
    let num_sps = (d[s + 5] & 0x1F) as u32;
    // 遍历 SPS 条目后读 PPS 个数
    let mut p = s + 6;
    for _ in 0..num_sps {
        if p + 2 > e {
            return (profile, compat, level, length_size, num_sps, 0);
        }
        p += 2 + be16(d, p) as usize;
    }
    let num_pps = if p < e { d[p] as u32 } else { 0 };
    (profile, compat, level, length_size, num_sps, num_pps)
}

pub struct AvccConfig {
    pub profile: u8,
    pub level: u8,
    pub length_size: usize,
    pub sps: Vec<Vec<u8>>,
    pub pps: Vec<Vec<u8>>,
}

/// 完整解析 avcC，提取全部 SPS/PPS（每个都是完整 NALU，含 NALU 头字节）。
pub fn parse_avcc_full(d: &[u8], s: usize, e: usize) -> Option<AvccConfig> {
    if e - s < 7 {
        return None;
    }
    let profile = d[s + 1];
    let level = d[s + 3];
    let length_size = (d[s + 4] & 0x03) as usize + 1;
    let num_sps = (d[s + 5] & 0x1F) as usize;
    let mut sps = Vec::new();
    let mut pps = Vec::new();
    let mut p = s + 6;
    for _ in 0..num_sps {
        if p + 2 > e {
            return None;
        }
        let n = be16(d, p) as usize;
        p += 2;
        if p + n > e {
            return None;
        }
        sps.push(d[p..p + n].to_vec());
        p += n;
    }
    if p >= e {
        return None;
    }
    let num_pps = d[p] as usize;
    p += 1;
    for _ in 0..num_pps {
        if p + 2 > e {
            return None;
        }
        let n = be16(d, p) as usize;
        p += 2;
        if p + n > e {
            return None;
        }
        pps.push(d[p..p + n].to_vec());
        p += n;
    }
    Some(AvccConfig {
        profile,
        level,
        length_size,
        sps,
        pps,
    })
}

/// 在盒子树中递归查找第一个指定类型的盒子
pub fn find_first<'a>(nodes: &'a [Mp4BoxNode], btype: &str) -> Option<&'a Mp4BoxNode> {
    for n in nodes {
        if n.btype == btype {
            return Some(n);
        }
        if let Some(found) = find_first(&n.children, btype) {
            return Some(found);
        }
    }
    None
}

/// 在盒子树中递归收集所有指定类型的盒子
pub fn find_all<'a>(nodes: &'a [Mp4BoxNode], btype: &str, out: &mut Vec<&'a Mp4BoxNode>) {
    for n in nodes {
        if n.btype == btype {
            out.push(n);
        }
        find_all(&n.children, btype, out);
    }
}

fn handler_type(d: &[u8], node: &Mp4BoxNode) -> String {
    box_content(d, node)
        .filter(|c| c.len() >= 12)
        .map(|c| fourcc_at(c, 8))
        .unwrap_or_else(|| "?".into())
}

fn build_track(d: &[u8], trak: &Mp4BoxNode) -> Mp4TrackInfo {
    let hdlr = find_first(&trak.children, "hdlr");
    let mdhd = find_first(&trak.children, "mdhd");
    let tkhd = find_first(&trak.children, "tkhd");
    let stsd = find_first(&trak.children, "stsd");

    let handler = hdlr.map(|n| handler_type(d, n)).unwrap_or_else(|| "?".into());

    let (timescale, duration) = mdhd
        .and_then(|n| box_content(d, n))
        .map(|c| {
            // v0: v/f(4)+ctime(4)+mtime(4)+timescale(4)+duration(4)
            if c.len() >= 32 && c[0] == 1 {
                (be32(c, 20), be64(c, 24))
            } else if c.len() >= 20 {
                (be32(c, 12), be32(c, 16) as u64)
            } else {
                (0, 0)
            }
        })
        .unwrap_or((0, 0));

    let (track_id, width, height) = tkhd
        .and_then(|n| box_content(d, n))
        .filter(|c| !c.is_empty())
        .map(|c| {
            let is_v1 = c[0] == 1;
            let track_id = if is_v1 && c.len() >= 24 {
                be32(c, 20)
            } else if c.len() >= 16 {
                be32(c, 12)
            } else {
                0
            };
            // 宽高位于内容区最后 8 字节（16.16 定点数）
            let mut w = None;
            let mut h = None;
            if c.len() >= 8 {
                let base = c.len() - 8;
                let wf = be32(c, base) as f64 / 65536.0;
                let hf = be32(c, base + 4) as f64 / 65536.0;
                if wf > 0.0 {
                    w = Some(wf);
                    h = Some(hf);
                }
            }
            (track_id, w, h)
        })
        .unwrap_or((0, None, None));

    let format = stsd
        .and_then(|n| box_content(d, n))
        .filter(|c| c.len() >= 16)
        .map(|c| fourcc_at(c, 12))
        .unwrap_or_else(|| "?".into());

    Mp4TrackInfo {
        track_id,
        handler,
        format,
        width,
        height,
        timescale,
        duration,
        duration_seconds: if timescale > 0 {
            duration as f64 / timescale as f64
        } else {
            0.0
        },
    }
}

pub fn hex_bytes(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" ")
}

/// 解析 MP4 文件内容，输出盒子树 + 轨道信息 + avcC 摘要与参数集
pub fn parse_mp4_bytes(d: &[u8]) -> Result<Mp4Info, String> {
    let boxes = parse_boxes(d, 0, d.len(), 0);
    if boxes.is_empty() {
        return Err("未能解析出任何 MP4 盒子（文件损坏或不是 MP4）".into());
    }

    let mut traks = Vec::new();
    find_all(&boxes, "trak", &mut traks);
    let tracks: Vec<Mp4TrackInfo> = traks.iter().map(|t| build_track(d, t)).collect();

    let avcc = find_first(&boxes, "avcC").and_then(|node| {
        let content = box_content(d, node)?;
        parse_avcc_full(content, 0, content.len())
    });

    let (
        has_avc,
        avc_profile,
        avc_level,
        length_size,
        sps_count,
        pps_count,
        sps_hex,
        pps_hex,
    ) = match &avcc {
        Some(cfg) => (
            true,
            Some(crate::h264::common::profile_name(cfg.profile)),
            Some(crate::h264::common::level_name(cfg.level)),
            Some(cfg.length_size as u32),
            Some(cfg.sps.len() as u32),
            Some(cfg.pps.len() as u32),
            cfg.sps.iter().map(|b| hex_bytes(b)).collect(),
            cfg.pps.iter().map(|b| hex_bytes(b)).collect(),
        ),
        None => (false, None, None, None, None, None, vec![], vec![]),
    };

    Ok(Mp4Info {
        boxes,
        tracks,
        has_avc,
        avc_profile,
        avc_level,
        length_size,
        sps_count,
        pps_count,
        sps_hex,
        pps_hex,
    })
}

/// 把 MP4 的 H.264 样本转换为 AnnexB 裸流（前置 SPS/PPS + 每样本按起始码拼接）。
/// 只使用包含 avcC 的那个视频轨道的样本表，避免多轨道文件错用音轨的 stbl。
pub fn extract_annexb_bytes(d: &[u8]) -> Result<(Vec<u8>, usize), String> {
    let boxes = parse_boxes(d, 0, d.len(), 0);
    let mut traks = Vec::new();
    find_all(&boxes, "trak", &mut traks);
    let video_trak = traks
        .iter()
        .find(|t| find_first(&t.children, "avcC").is_some())
        .ok_or("未找到含 H.264 (avcC) 的视频轨道（该 MP4 可能不是 H.264 编码）")?;
    let avcc_node = find_first(&video_trak.children, "avcC").ok_or("avcC 查找失败")?;
    let avcc_content = box_content(d, avcc_node).ok_or("avcC 内容越界")?;
    let avcc = parse_avcc_full(avcc_content, 0, avcc_content.len()).ok_or("avcC 配置解析失败")?;

    let stsz = find_first(&video_trak.children, "stsz").ok_or("缺少样本尺寸表 stsz")?;
    let stsc = find_first(&video_trak.children, "stsc").ok_or("缺少样本-块映射表 stsc")?;
    let stco = find_first(&video_trak.children, "stco")
        .or_else(|| find_first(&video_trak.children, "co64"));
    let stco = stco.ok_or("缺少 chunk 偏移表 stco")?;

    // stsz → 样本尺寸
    let sz_c = box_content(d, stsz).ok_or("stsz 内容越界")?;
    if sz_c.len() < 12 {
        return Err("stsz 表异常".into());
    }
    let sample_size = be32(sz_c, 4) as usize;
    let sample_count = be32(sz_c, 8) as usize;
    if sample_count == 0 || sample_count > 5_000_000 {
        return Err(format!("样本数量异常: {sample_count}"));
    }
    let mut sizes: Vec<usize> = Vec::with_capacity(sample_count);
    if sample_size > 0 {
        sizes.resize(sample_count, sample_size);
    } else if sz_c.len() >= 12 + sample_count * 4 {
        for i in 0..sample_count {
            sizes.push(be32(sz_c, 12 + i * 4) as usize);
        }
    } else {
        return Err("stsz 表越界".into());
    }

    // stco / co64 → chunk 偏移
    let co_c = box_content(d, stco).ok_or("stco 内容越界")?;
    if co_c.len() < 8 {
        return Err("stco 表异常".into());
    }
    let chunk_count = be32(co_c, 4) as usize;
    let is_co64 = stco.btype == "co64";
    let entry_size = if is_co64 { 8 } else { 4 };
    if co_c.len() < 8 + chunk_count * entry_size {
        return Err("stco 表越界".into());
    }
    let mut chunk_offsets: Vec<u64> = Vec::with_capacity(chunk_count);
    for i in 0..chunk_count {
        chunk_offsets.push(if is_co64 {
            be64(co_c, 8 + i * 8)
        } else {
            be32(co_c, 8 + i * 4) as u64
        });
    }

    // stsc → 每个 chunk 的样本数
    let sc_c = box_content(d, stsc).ok_or("stsc 内容越界")?;
    if sc_c.len() < 8 {
        return Err("stsc 表异常".into());
    }
    let entry_count = be32(sc_c, 4) as usize;
    if sc_c.len() < 8 + entry_count * 12 {
        return Err("stsc 表越界".into());
    }
    let mut stsc_entries: Vec<(u32, u32)> = Vec::with_capacity(entry_count);
    for i in 0..entry_count {
        stsc_entries.push((be32(sc_c, 8 + i * 12), be32(sc_c, 12 + i * 12)));
    }
    if chunk_count > 0 && stsc_entries.is_empty() {
        return Err("stsc 表为空但存在 chunk，文件可能损坏".into());
    }
    let mut per_chunk: Vec<u32> = Vec::with_capacity(chunk_count);
    let mut entry_idx = 0usize;
    for c in 0..chunk_count {
        while entry_idx + 1 < stsc_entries.len() && stsc_entries[entry_idx + 1].0 <= (c + 1) as u32 {
            entry_idx += 1;
        }
        per_chunk.push(stsc_entries[entry_idx].1);
    }

    // 前置 SPS/PPS
    let mut out: Vec<u8> = Vec::new();
    for sps in &avcc.sps {
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(sps);
    }
    for pps in &avcc.pps {
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(pps);
    }

    let mut sample_idx = 0usize;
    for (c, &chunk_off) in chunk_offsets.iter().enumerate() {
        let mut pos = chunk_off as usize;
        for _ in 0..per_chunk[c] {
            if sample_idx >= sizes.len() {
                break;
            }
            let size = sizes[sample_idx];
            sample_idx += 1;
            if size == 0 || pos.saturating_add(size) > d.len() {
                return Err(format!("样本 #{sample_idx} 越界（size={size}），文件可能损坏"));
            }
            convert_length_prefixed(&d[pos..pos + size], avcc.length_size, &mut out)?;
            pos += size;
            if out.len() > 2 * 1024 * 1024 * 1024 {
                return Err("输出超过 2GB 上限，已中止".into());
            }
        }
    }
    if sample_idx == 0 {
        return Err("未提取到任何样本".into());
    }
    Ok((out, sample_idx))
}

/// 把一个“长度前缀 NALU 序列”样本转换为起始码格式
fn convert_length_prefixed(sample: &[u8], length_size: usize, out: &mut Vec<u8>) -> Result<(), String> {
    let mut p = 0usize;
    while p < sample.len() {
        if p + length_size > sample.len() {
            return Err("样本内长度前缀越界".into());
        }
        let n = match length_size {
            1 => sample[p] as usize,
            2 => be16(sample, p) as usize,
            3 => ((sample[p] as usize) << 16) | (be16(sample, p + 1) as usize),
            _ => be32(sample, p) as usize,
        };
        p += length_size;
        if n == 0 || p.saturating_add(n) > sample.len() {
            return Err("样本内 NALU 长度异常".into());
        }
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&sample[p..p + n]);
        p += n;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box4(btype: &str, payload: &[u8]) -> Vec<u8> {
        let mut v = ((payload.len() as u32 + 8).to_be_bytes()).to_vec();
        v.extend_from_slice(btype.as_bytes());
        v.extend_from_slice(payload);
        v
    }

    /// 构造一个最小可用的 H.264 MP4：ftyp + moov(mvhd/trak(tkhd/mdia(hdlr/mdhd/minf/stbl(stsd(avc1(avcC))/stsc/stsz/stco)))) + mdat
    /// 样本布局：2 个 chunk × 2 个样本，每样本含 2 个长度前缀 NALU。
    pub(super) fn build_test_mp4(sps: &[u8], pps: &[u8]) -> Vec<u8> {
        let sample = |a: u8, b: u8| {
            let mut s = Vec::new();
            s.extend_from_slice(&2u32.to_be_bytes()); // NALU1 长度
            s.extend_from_slice(&[0x41, a]); // 非IDR切片
            s.extend_from_slice(&2u32.to_be_bytes()); // NALU2 长度
            s.extend_from_slice(&[0x41, b]);
            s
        };
        let samples: Vec<Vec<u8>> = vec![
            sample(0x01, 0x11),
            sample(0x02, 0x22),
            sample(0x03, 0x33),
            sample(0x04, 0x44),
        ];

        // avcC
        let mut avcc_payload = vec![1u8, 66, 0xC0, 30, 0xFF, 0xE1];
        avcc_payload.extend_from_slice(&(sps.len() as u16).to_be_bytes());
        avcc_payload.extend_from_slice(sps);
        avcc_payload.push(1); // 1 个 PPS
        avcc_payload.extend_from_slice(&(pps.len() as u16).to_be_bytes());
        avcc_payload.extend_from_slice(pps);
        let avcc = box4("avcC", &avcc_payload);

        // avc1 样本描述（VisualSampleEntry：78 字节固定字段 + 子盒）
        let mut avc1_body = vec![0u8; 78];
        avc1_body[6..8].copy_from_slice(&1u16.to_be_bytes()); // data_reference_index
        avc1_body[24..26].copy_from_slice(&320u16.to_be_bytes()); // width
        avc1_body[26..28].copy_from_slice(&240u16.to_be_bytes()); // height
        avc1_body[28..32].copy_from_slice(&0x0048_0000u32.to_be_bytes()); // hres 72dpi
        avc1_body[32..36].copy_from_slice(&0x0048_0000u32.to_be_bytes()); // vres 72dpi
        avc1_body[40..42].copy_from_slice(&1u16.to_be_bytes()); // frame_count
        avc1_body[74..76].copy_from_slice(&0x0018u16.to_be_bytes()); // depth 24
        avc1_body.extend_from_slice(&avcc);
        let avc1 = box4("avc1", &avc1_body);

        let stsd = {
            let mut p = 0u32.to_be_bytes().to_vec(); // version/flags
            p.extend_from_slice(&1u32.to_be_bytes()); // entry_count
            p.extend_from_slice(&avc1);
            box4("stsd", &p)
        };

        let stsz = {
            let mut p = vec![0u8; 4]; // version/flags
            p.extend_from_slice(&0u32.to_be_bytes()); // 变长样本
            p.extend_from_slice(&(samples.len() as u32).to_be_bytes());
            for s in &samples {
                p.extend_from_slice(&(s.len() as u32).to_be_bytes());
            }
            box4("stsz", &p)
        };

        let stsc = {
            let mut p = vec![0u8; 4];
            p.extend_from_slice(&1u32.to_be_bytes()); // 1 条目
            p.extend_from_slice(&1u32.to_be_bytes()); // first_chunk=1
            p.extend_from_slice(&2u32.to_be_bytes()); // samples_per_chunk=2
            p.extend_from_slice(&1u32.to_be_bytes()); // sample_desc_idx
            box4("stsc", &p)
        };

        let ftyp = box4("ftyp", b"isom\x00\x00\x02\x00isomiso2avc1mp41");
        let mvhd = {
            let mut p = vec![0u8];
            p.extend_from_slice(&[0, 0, 0]);
            p.extend_from_slice(&[0u8; 8]);
            p.extend_from_slice(&1000u32.to_be_bytes());
            p.extend_from_slice(&4000u32.to_be_bytes());
            p.extend_from_slice(&[0u8; 80]);
            box4("mvhd", &p)
        };
        let hdlr = {
            let mut p = vec![0u8; 4];
            p.extend_from_slice(&0u32.to_be_bytes());
            p.extend_from_slice(b"vide");
            p.extend_from_slice(&[0u8; 12]);
            box4("hdlr", &p)
        };
        let mdhd = {
            let mut p = vec![0u8];
            p.extend_from_slice(&[0, 0, 0]);
            p.extend_from_slice(&[0u8; 8]);
            p.extend_from_slice(&1000u32.to_be_bytes());
            p.extend_from_slice(&4000u32.to_be_bytes());
            box4("mdhd", &p)
        };
        let tkhd = {
            let mut p = vec![0u8];
            p.extend_from_slice(&[0, 0, 7]);
            p.extend_from_slice(&[0u8; 8]);
            p.extend_from_slice(&1u32.to_be_bytes()); // track_id
            p.extend_from_slice(&0u32.to_be_bytes());
            p.extend_from_slice(&4000u32.to_be_bytes()); // duration
            p.extend_from_slice(&[0u8; 8]);
            p.extend_from_slice(&[0u8; 2]); // layer
            p.extend_from_slice(&[0u8; 2]); // alt group
            p.extend_from_slice(&[0u8; 2]); // volume
            p.extend_from_slice(&[0u8; 2]); // reserved
            p.extend_from_slice(&[0u8; 36]); // matrix
            p.extend_from_slice(&(320u32 << 16).to_be_bytes());
            p.extend_from_slice(&(240u32 << 16).to_be_bytes());
            box4("tkhd", &p)
        };

        // stco 为 32 位偏移；先算出最终布局再填值
        let stco_size = 8 + 4 + 4 + 8;
        let first_pass = {
            let stbl = box4("stbl", &[stsd.clone(), stsc.clone(), stsz.clone()].concat());
            let minf = box4("minf", &stbl);
            let mdia = box4("mdia", &[hdlr.clone(), mdhd.clone(), minf].concat());
            let trak = box4("trak", &[tkhd.clone(), mdia].concat());
            box4("moov", &[mvhd.clone(), trak].concat())
        };
        let mdat_data_start = (ftyp.len() + first_pass.len() + stco_size + 8) as u64;
        let chunk1_end = mdat_data_start
            + (samples[0].len() + samples[1].len()) as u64;
        let stco = {
            let mut p = vec![0u8; 4];
            p.extend_from_slice(&2u32.to_be_bytes());
            p.extend_from_slice(&(mdat_data_start as u32).to_be_bytes());
            p.extend_from_slice(&(chunk1_end as u32).to_be_bytes());
            box4("stco", &p)
        };
        assert_eq!(stco.len(), stco_size);

        let stbl = box4("stbl", &[stsd, stsc, stsz, stco].concat());
        let minf = box4("minf", &stbl);
        let mdia = box4("mdia", &[hdlr, mdhd, minf].concat());
        let trak = box4("trak", &[tkhd, mdia].concat());
        let moov = box4("moov", &[mvhd, trak].concat());
        assert_eq!(moov.len(), first_pass.len() + stco_size);

        let mut mdat_payload = Vec::new();
        for s in &samples {
            mdat_payload.extend_from_slice(s);
        }
        let mdat = box4("mdat", &mdat_payload);

        [ftyp, moov, mdat].concat()
    }

    #[test]
    fn mp4_parse_tree_and_tracks() {
        let sps = crate::h264::test_util::build_test_sps_nalu(30);
        let pps = crate::h264::test_util::build_test_pps_nalu();
        let data = crate::mp4::tests::build_test_mp4(&sps, &pps);
        let info = parse_mp4_bytes(&data).expect("解析失败");
        assert!(info.has_avc);
        assert_eq!(info.sps_count, Some(1));
        assert_eq!(info.pps_count, Some(1));
        assert_eq!(info.length_size, Some(4));
        assert_eq!(info.tracks.len(), 1);
        assert_eq!(info.tracks[0].handler, "vide");
        assert_eq!(info.tracks[0].format, "avc1");
        assert_eq!(info.tracks[0].timescale, 1000);
        assert_eq!(info.tracks[0].duration, 4000);
        assert_eq!(info.tracks[0].duration_seconds, 4.0);
        assert_eq!(info.tracks[0].width, Some(320.0));
        // ftyp / moov / mdat 三大顶层盒子
        assert_eq!(info.boxes.len(), 3);
        assert_eq!(info.boxes[0].btype, "ftyp");
        assert_eq!(info.boxes[2].btype, "mdat");
    }

    #[test]
    fn mp4_extract_annexb() {
        let sps = crate::h264::test_util::build_test_sps_nalu(30);
        let pps = crate::h264::test_util::build_test_pps_nalu();
        let data = crate::mp4::tests::build_test_mp4(&sps, &pps);
        let (stream, count) = extract_annexb_bytes(&data).expect("提取失败");
        assert_eq!(count, 4);
        // 前 4 字节是起始码，然后是 SPS
        assert_eq!(&stream[0..4], &[0, 0, 0, 1]);
        assert_eq!(stream[4], 0x67);
        // 流中应能扫描出 SPS + PPS + 4 样本 × 2 NALU = 10 个 NALU
        let nals = crate::h264::nal::scan_annexb(&stream);
        assert_eq!(nals.len(), 10);
        // 每个 NALU 应与长度前缀声明一致（起始码开头）
        for n in &nals {
            assert!(n.data_len >= 1);
        }
    }
}

