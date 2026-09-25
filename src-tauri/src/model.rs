//! 前后端共享的可序列化数据模型（serde 命名风格统一为 camelCase）。

use serde::Serialize;

use crate::h264::common::FieldGroup;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NaluSummary {
    pub index: usize,
    /// 起始码在文件中的偏移
    pub offset: usize,
    /// NALU 数据长度（不含起始码，含 NALU 头）
    pub size: usize,
    pub start_code_len: usize,
    pub nal_type: u8,
    pub type_name: String,
    pub ref_idc: u8,
    pub is_idr: bool,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamStats {
    pub total_size: usize,
    pub nalu_count: usize,
    /// 按类型名统计，如 "SPS": 2
    pub counts: Vec<(String, usize)>,
    pub idr_count: usize,
    pub profile: Option<String>,
    pub level: Option<String>,
    pub resolution: Option<String>,
    /// "annexb"（裸流）或 "mp4"（已自动提取为裸流）
    pub source_format: String,
    /// NALU 数量超过扫描上限，结果被截断
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseResult {
    pub nalus: Vec<NaluSummary>,
    pub stats: StreamStats,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NaluDetail {
    pub index: usize,
    pub type_name: String,
    pub groups: Vec<FieldGroup>,
    pub warnings: Vec<String>,
    /// 去除仿真预防字节后的 NALU 载荷（不含 NALU 头），十六进制展示
    pub rbsp_hex: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HexData {
    pub offset: usize,
    pub bytes: Vec<u8>,
    pub file_size: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffRow {
    pub section: String,
    pub name: String,
    pub value_a: String,
    pub value_b: String,
    pub same: bool,
    pub desc: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpsCompare {
    pub diff_count: usize,
    pub rows: Vec<DiffRow>,
    pub note_a: String,
    pub note_b: String,
}
