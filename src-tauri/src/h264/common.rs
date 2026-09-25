//! 解析结果中"字段表"的通用结构：每个结构（SPS/PPS/切片头/SEI…）
//! 都以若干 `FieldGroup` 输出，前端按统一组件渲染。

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub name: String,
    pub value: String,
    /// 比特位置，如 "bit 12 · 5位(ue)"，相对于 NALU 首字节（含 NALU 头）
    pub bits: Option<String>,
    pub desc: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldGroup {
    pub title: String,
    pub note: Option<String>,
    pub fields: Vec<Field>,
}

impl FieldGroup {
    pub fn new(title: &str) -> Self {
        FieldGroup {
            title: title.to_string(),
            note: None,
            fields: Vec::new(),
        }
    }

    pub fn push(&mut self, field: Field) {
        self.fields.push(field);
    }
}

impl Field {
    pub fn new(name: &str, value: String, bits: Option<String>, desc: &str) -> Self {
        Field {
            name: name.to_string(),
            value,
            bits,
            desc: if desc.is_empty() {
                None
            } else {
                Some(desc.to_string())
            },
        }
    }
}

/// 记录一个从 `start` 到 `end` 比特区间的字段（相对 NALU 首字节的比特位）。
pub fn bits_range(start: usize, end: usize, base: usize) -> String {
    format!("bit {} · {}位", start + base, end - start)
}

/// 追加一个字段到分组（base 用于把"相对 RBSP 体"的比特位换算为"相对 NALU 头"）。
#[allow(clippy::too_many_arguments)]
pub fn add_field(
    group: &mut FieldGroup,
    name: &str,
    value: String,
    bit_start: usize,
    bit_end: usize,
    base: usize,
    desc: &str,
) {
    group.push(Field::new(
        name,
        value,
        Some(bits_range(bit_start, bit_end, base)),
        desc,
    ));
}

pub fn profile_name(profile_idc: u8) -> String {
    let name = match profile_idc {
        44 => "CAVLC 4:4:4",
        66 => "Baseline",
        77 => "Main",
        83 => "Scalable Baseline",
        86 => "Scalable High",
        88 => "Extended",
        100 => "High",
        110 => "High 10",
        118 => "Multiview High",
        122 => "High 4:2:2",
        128 => "Stereo High",
        134 => "MFC Depth High",
        135 => "MFC High",
        138 => "Multiview Depth High",
        139 => "Multiview Deep 3D High",
        244 => "High 4:4:4 Predictive",
        _ => return format!("未知(P{profile_idc})"),
    };
    format!("{profile_idc} ({name})")
}

pub fn level_name(level_idc: u8) -> String {
    match level_idc {
        9 => format!("{level_idc} (1b)"),
        11 => format!("{level_idc} (1.1 / 1b)"),
        v => format!("{v} ({:.1})", v as f32 / 10.0),
    }
}

pub fn chroma_format_name(v: u32) -> String {
    let name = match v {
        0 => "单色",
        1 => "4:2:0",
        2 => "4:2:2",
        3 => "4:4:4",
        _ => "保留",
    };
    format!("{v} ({name})")
}

pub fn poc_type_name(v: u32) -> String {
    let name = match v {
        0 => "LSB 编码（最常见）",
        1 => "Delta 编码（两类偏移）",
        2 => "推断型（无需额外字段）",
        _ => "保留",
    };
    format!("{v} ({name})")
}

pub fn aspect_ratio_name(idc: u32) -> String {
    let name = match idc {
        0 => "未指定",
        1 => "1:1",
        2 => "12:11",
        3 => "10:11",
        4 => "16:11",
        5 => "40:33",
        6 => "24:11",
        7 => "20:11",
        8 => "32:11",
        9 => "80:33",
        10 => "18:11",
        11 => "15:11",
        12 => "64:33",
        13 => "160:99",
        14 => "4:3",
        15 => "3:2",
        16 => "2:1",
        255 => "扩展(显式宽高)",
        _ => "保留",
    };
    format!("{idc} ({name})")
}

pub fn video_format_name(v: u32) -> String {
    let name = match v {
        0 => "Component",
        1 => "PAL",
        2 => "NTSC",
        3 => "SECAM",
        4 => "MAC",
        5 => "Unspecified",
        6 => "Reserved",
        _ => "Reserved",
    };
    format!("{v} ({name})")
}
