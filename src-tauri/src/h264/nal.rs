//! NALU 层：起始码扫描、NALU 头解析、EBSP→RBSP 去仿真预防字节。

/// 一个原始 NALU 在码流中的位置信息
#[derive(Debug, Clone)]
pub struct RawNal {
    /// 起始码起始偏移
    pub start_offset: usize,
    /// 起始码长度（3 或 4 字节）
    pub start_code_len: usize,
    /// NALU 数据（起始码之后、含 NALU 头）起始偏移
    pub data_offset: usize,
    /// NALU 数据长度（含 NALU 头，已去掉尾部连续 0x00）
    pub data_len: usize,
    pub nal_type: u8,
    pub ref_idc: u8,
}

pub fn nal_type_name(t: u8) -> String {
    let name = match t {
        1 => "非IDR切片 (P/B)",
        2 => "切片分区A",
        3 => "切片分区B",
        4 => "切片分区C",
        5 => "IDR切片",
        6 => "SEI",
        7 => "SPS",
        8 => "PPS",
        9 => "AUD 分界符",
        10 => "序列结束",
        11 => "码流结束",
        12 => "填充数据",
        14 => "前缀NALU (MVC/SVC)",
        15 => "子集SPS (MVC/SVC)",
        19 => "切片扩展 (MVC)",
        20 => "切片扩展 (MVC/SVC)",
        13 | 16..=18 | 21..=23 => "保留",
        24..=31 => "保留(RTP聚合等)",
        _ => "非法",
    };
    name.to_string()
}

/// 在 `data[from..]` 中查找下一个起始码，返回 (起始码偏移, 起始码长度)。
/// 4 字节起始码（00 00 00 01）优先于 3 字节（00 00 01）。
pub fn find_start_code(data: &[u8], from: usize) -> Option<(usize, usize)> {
    let mut i = from;
    while i + 2 < data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            if i > from && data[i - 1] == 0 {
                return Some((i - 1, 4));
            }
            return Some((i, 3));
        }
        i += 1;
    }
    None
}

/// 单个码流的最大 NALU 数（防止恶意/损坏文件耗尽内存）
pub const MAX_NALUS: usize = 200_000;

/// 扫描 AnnexB 裸流中的全部 NALU。
/// 约定：NALU 末尾紧邻下一个起始码的连续 0x00 属于尾随零（cabac_zero_words 等），不计入。
pub fn scan_annexb(data: &[u8]) -> Vec<RawNal> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    // 跳过首帧前的垃圾前导字节（一般没有，防个别工具在文件头写 0）
    while let Some((sc_off, sc_len)) = find_start_code(data, cursor) {
        if out.len() >= MAX_NALUS {
            break;
        }
        let data_offset = sc_off + sc_len;
        // 找下一个起始码
        let end = match find_start_code(data, data_offset) {
            Some((next_sc, _)) => next_sc,
            None => data.len(),
        };
        // 去掉尾部连续 0（这些是 trailing zero / 下一帧起始码的前导 0）
        let mut data_end = end;
        while data_end > data_offset && data[data_end - 1] == 0 {
            data_end -= 1;
        }
        if data_end > data_offset {
            let header = data[data_offset];
            out.push(RawNal {
                start_offset: sc_off,
                start_code_len: sc_len,
                data_offset,
                data_len: data_end - data_offset,
                nal_type: header & 0x1F,
                ref_idc: (header >> 5) & 0x3,
            });
        }
        cursor = data_offset;
    }
    out
}

/// EBSP → RBSP：去掉仿真预防字节 0x03（在 00 00 03 且下一个字节 <= 0x03 时）。
pub fn unescape_rbsp(ebsp: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(ebsp.len());
    let mut zero_run = 0usize;
    let mut i = 0;
    while i < ebsp.len() {
        let b = ebsp[i];
        if zero_run >= 2 && b == 0x03 && i + 1 < ebsp.len() && ebsp[i + 1] <= 0x03 {
            // 跳过仿真预防字节
            zero_run = 0;
            i += 1;
            continue;
        }
        if b == 0 {
            zero_run += 1;
        } else {
            zero_run = 0;
        }
        out.push(b);
        i += 1;
    }
    out
}

pub fn is_annexb(data: &[u8]) -> bool {
    find_start_code(data, 0).is_some()
}

pub fn is_mp4(data: &[u8]) -> bool {
    if data.len() < 12 {
        return false;
    }
    matches!(
        &data[4..8],
        b"ftyp" | b"moov" | b"mdat" | b"free" | b"skip" | b"wide"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_finds_all_nalus() {
        // AUD + SPS + IDR 混合 3/4 字节起始码
        let mut data: Vec<u8> = vec![];
        data.extend_from_slice(&[0, 0, 0, 1, 0x09, 0x10]);
        data.extend_from_slice(&[0, 0, 1, 0x67, 0x42, 0xC0, 0x1E]);
        data.extend_from_slice(&[0, 0, 0, 1, 0x65, 0x88, 0x84, 0x00]);
        let nals = scan_annexb(&data);
        assert_eq!(nals.len(), 3);
        assert_eq!(nals[0].start_code_len, 4);
        assert_eq!(nals[0].nal_type, 9);
        assert_eq!(nals[1].start_code_len, 3);
        assert_eq!(nals[1].nal_type, 7);
        assert_eq!(nals[2].nal_type, 5);
        assert_eq!(nals[2].data_len, 3); // 尾部 0x00 被裁掉
    }

    #[test]
    fn trailing_zeros_are_trimmed() {
        let mut data: Vec<u8> = vec![0, 0, 1, 0x65, 0xAA, 0, 0, 0, 0, 0, 0, 0, 1, 0x68, 0xEB];
        data[9] = 0; // 保持视觉上为 7 个连续 0
        let nals = scan_annexb(&data);
        assert_eq!(nals.len(), 2);
        assert_eq!(nals[0].data_len, 2); // 尾部 0 全部裁掉
    }

    #[test]
    fn unescape_removes_emulation_bytes() {
        let ebsp = [0x00, 0x00, 0x03, 0x00, 0xAA, 0x00, 0x00, 0x03, 0x01, 0xBB];
        let rbsp = unescape_rbsp(&ebsp);
        assert_eq!(rbsp, vec![0x00, 0x00, 0x00, 0xAA, 0x00, 0x00, 0x01, 0xBB]);
    }

    #[test]
    fn unescape_keeps_normal_data() {
        let ebsp = [0x12, 0x34, 0x56, 0x78];
        assert_eq!(unescape_rbsp(&ebsp), vec![0x12, 0x34, 0x56, 0x78]);
    }

    #[test]
    fn format_detection() {
        assert!(is_annexb(&[0, 0, 0, 1, 0x67, 0x42]));
        assert!(is_annexb(&[0, 0, 1, 0x67, 0x42]));
        assert!(!is_annexb(b"\x00\x00\x00\x18ftypisom"));
        assert!(is_mp4(b"\x00\x00\x00\x18ftypisom"));
        assert!(!is_mp4(&[0, 0, 0, 1, 0x67, 0x42]));
    }
}
