//! H.264 解析模块入口：码流扫描 → 汇总统计 → NALU 详情 → SPS 对比。

pub mod bitreader;
pub mod common;
pub mod nal;
pub mod pps;
pub mod sei;
pub mod slice;
pub mod sps;
#[cfg(test)]
pub mod test_util;

use std::collections::HashMap;

use crate::model::{DiffRow, NaluDetail, NaluSummary, ParseResult, SpsCompare, StreamStats};
use common::{Field, FieldGroup};
use nal::RawNal;

/// 取一个 NALU 去除仿真预防字节后的完整 RBSP（含 NALU 头字节）。
fn nalu_rbsp(data: &[u8], n: &RawNal) -> Option<Vec<u8>> {
    let ebsp = data.get(n.data_offset..n.data_offset.checked_add(n.data_len)?)?;
    Some(nal::unescape_rbsp(ebsp))
}

fn hex_str(bytes: &[u8], cap: usize) -> String {
    bytes
        .iter()
        .take(cap)
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// 解析 AnnexB 裸流为 NALU 列表 + 统计。
/// `nals` 为调用方预扫描的 NALU 定位表（通常来自缓存，避免重复扫描）。
pub fn parse_stream_bytes(data: &[u8], source_format: &str, nals: &[RawNal]) -> ParseResult {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut idr_count = 0usize;
    let mut first_sps: Option<sps::Sps> = None;
    let mut nalus = Vec::with_capacity(nals.len());

    for (i, n) in nals.iter().enumerate() {
        let type_name = nal::nal_type_name(n.nal_type);
        *counts.entry(type_name.clone()).or_insert(0) += 1;
        if n.nal_type == 5 {
            idr_count += 1;
        }
        if n.nal_type == 7 && first_sps.is_none() {
            if let Some(rbsp) = nalu_rbsp(data, n) {
                if rbsp.len() > 1 {
                    if let Ok(p) = sps::parse_sps(&rbsp[1..]) {
                        first_sps = Some(p.sps);
                    }
                }
            }
        }
        let preview = hex_str(&data[n.data_offset..n.data_offset + n.data_len.min(8)], 8);
        nalus.push(NaluSummary {
            index: i,
            offset: n.start_offset,
            size: n.data_len,
            start_code_len: n.start_code_len,
            nal_type: n.nal_type,
            type_name,
            ref_idc: n.ref_idc,
            is_idr: n.nal_type == 5,
            preview,
        });
    }

    let mut counts: Vec<(String, usize)> = counts.into_iter().collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let (profile, level, resolution) = match &first_sps {
        Some(s) => (
            Some(common::profile_name(s.profile_idc)),
            Some(common::level_name(s.level_idc)),
            Some(format!("{}x{}", s.width, s.height)),
        ),
        None => (None, None, None),
    };

    ParseResult {
        nalus,
        stats: StreamStats {
            total_size: data.len(),
            nalu_count: nals.len(),
            counts,
            idr_count,
            profile,
            level,
            resolution,
            source_format: source_format.to_string(),
            truncated: nals.len() >= nal::MAX_NALUS,
        },
    }
}

/// 扫描 `nals[..upto]`，收集最新的 SPS/PPS 上下文（按 id 索引）。
fn build_context(
    data: &[u8],
    nals: &[RawNal],
    upto: usize,
) -> (HashMap<u32, sps::Sps>, HashMap<u32, pps::Pps>) {
    let mut sps_map = HashMap::new();
    let mut pps_map = HashMap::new();
    for n in nals.iter().take(upto.min(nals.len())) {
        let Some(rbsp) = nalu_rbsp(data, n) else { continue };
        if rbsp.len() < 2 {
            continue;
        }
        let body = &rbsp[1..];
        match n.nal_type {
            7 => {
                if let Ok(p) = sps::parse_sps(body) {
                    sps_map.insert(p.sps.seq_parameter_set_id, p.sps);
                }
            }
            8 => {
                if let Ok(p) = pps::parse_pps(body, None) {
                    pps_map.insert(p.pps.pic_parameter_set_id, p.pps);
                }
            }
            _ => {}
        }
    }
    (sps_map, pps_map)
}

fn header_group(n: &RawNal) -> FieldGroup {
    let mut g = FieldGroup::new("NALU 头");
    common::add_field(&mut g, "forbidden_zero_bit", "0".into(), 0, 1, 0, "必须为 0");
    common::add_field(
        &mut g,
        "nal_ref_idc",
        match n.ref_idc {
            0 => "0（非参考图像）".to_string(),
            v => format!("{v}（参考图像）"),
        },
        1,
        3,
        0,
        "参考优先级",
    );
    common::add_field(&mut g, "nal_unit_type", nal::nal_type_name(n.nal_type), 3, 8, 0, "NALU 类型");
    g
}

/// 解析指定 NALU 的详细内容（带 SPS/PPS 上下文）。
pub fn nalu_detail_bytes(data: &[u8], nals: &[RawNal], index: usize) -> Result<NaluDetail, String> {
    let n = nals
        .get(index)
        .ok_or_else(|| format!("NALU 序号 {index} 越界（共 {} 个）", nals.len()))?;
    let rbsp = nalu_rbsp(data, n).ok_or("NALU 偏移越界")?;
    let type_name = nal::nal_type_name(n.nal_type);
    let mut warnings = Vec::new();

    let groups = if rbsp.len() < 2 {
        warnings.push("NALU 数据不足 2 字节".into());
        vec![header_group(n)]
    } else {
        let body = &rbsp[1..];
        match n.nal_type {
            7 => match sps::parse_sps(body) {
                Ok(p) => p.groups,
                Err(e) => {
                    warnings.push(format!("SPS 解析失败：{e}"));
                    vec![header_group(n)]
                }
            },
            8 => match pps::parse_pps(body, None) {
                Ok(p) => {
                    warnings.extend(p.warnings);
                    p.groups
                }
                Err(e) => {
                    warnings.push(format!("PPS 解析失败：{e}"));
                    vec![header_group(n)]
                }
            },
            1 | 5 => {
                // 两遍解析：第一遍拿到 pps_id，第二遍带完整 SPS/PPS 上下文
                let first = slice::parse_slice_header(body, n.ref_idc, n.nal_type == 5, None, None);
                let (sps_map, pps_map) = build_context(data, nals, index);
                let pps = pps_map.get(&first.info.pic_parameter_set_id);
                let sps = pps.and_then(|p| sps_map.get(&p.seq_parameter_set_id));
                let parsed = slice::parse_slice_header(body, n.ref_idc, n.nal_type == 5, sps, pps);
                warnings.extend(parsed.warnings);
                let mut groups = vec![header_group(n)];
                groups.extend(parsed.groups);
                groups
            }
            6 => {
                let (g, w) = sei::parse_sei(body);
                warnings.extend(w);
                let mut groups = vec![header_group(n)];
                groups.extend(g);
                groups
            }
            9 => {
                let mut br = bitreader::BitReader::new(body);
                let mut g = FieldGroup::new("AUD 内容");
                let s = br.bit_pos();
                let t = br.read_bits(3).unwrap_or(0);
                let name = match t {
                    0 => "P 帧",
                    1 => "B 帧",
                    2 => "I 帧",
                    3 => "SP 帧",
                    4 => "SI 帧",
                    _ => "保留",
                };
                common::add_field(
                    &mut g,
                    "primary_pic_type",
                    format!("{t} ({name})"),
                    s,
                    br.bit_pos(),
                    8,
                    "",
                );
                vec![header_group(n), g]
            }
            12 => {
                let mut g = FieldGroup::new("填充数据");
                g.note = Some(format!("{} 字节填充（FF 后接 0x00 序列）", n.data_len - 1));
                vec![header_group(n), g]
            }
            14 | 15 | 19 | 20 => {
                let mut g = FieldGroup::new("扩展类型");
                g.note = Some("MVC/SVC 扩展类型，v1 仅识别类型，不解析内容".into());
                vec![header_group(n), g]
            }
            _ => {
                let mut g = FieldGroup::new("内容");
                g.note = Some("该类型暂无详细解析".into());
                vec![header_group(n), g]
            }
        }
    };

    let payload = if rbsp.len() > 1 { &rbsp[1..] } else { &rbsp[..0] };
    Ok(NaluDetail {
        index,
        type_name,
        groups,
        warnings,
        rbsp_hex: hex_str(payload, 4096),
    })
}

/// 取码流中第一个 SPS（含分组）与第一个 PPS 分组，用于对比功能。
fn first_parameter_sets(
    data: &[u8],
    nals: &[RawNal],
) -> Result<(sps::Sps, Vec<FieldGroup>, Vec<FieldGroup>), String> {
    let mut sps_groups: Option<Vec<FieldGroup>> = None;
    let mut sps_val: Option<sps::Sps> = None;
    let mut pps_groups: Option<Vec<FieldGroup>> = None;
    for n in nals {
        let Some(rbsp) = nalu_rbsp(data, n) else { continue };
        if rbsp.len() < 2 {
            continue;
        }
        let body = &rbsp[1..];
        match n.nal_type {
            7 if sps_val.is_none() => {
                if let Ok(p) = sps::parse_sps(body) {
                    sps_val = Some(p.sps);
                    sps_groups = Some(p.groups);
                }
            }
            8 if pps_groups.is_none() => {
                if let Ok(p) = pps::parse_pps(body, None) {
                    pps_groups = Some(p.groups);
                }
            }
            _ => {}
        }
        if sps_val.is_some() && pps_groups.is_some() {
            break;
        }
    }
    let sps = sps_val.ok_or("码流中未找到 SPS（不是 H.264 裸流，或参数集缺失）")?;
    Ok((
        sps,
        sps_groups.unwrap_or_default(),
        pps_groups.unwrap_or_default(),
    ))
}

/// 对比两个码流的 SPS/PPS 字段差异（按分组标题 + 字段名匹配）。
pub fn compare_sps_bytes(
    data_a: &[u8],
    nals_a: &[RawNal],
    data_b: &[u8],
    nals_b: &[RawNal],
) -> Result<SpsCompare, String> {
    let (sps_a, mut groups_a, pps_a) = first_parameter_sets(data_a, nals_a)?;
    let (sps_b, mut groups_b, pps_b) = first_parameter_sets(data_b, nals_b)?;
    groups_a.extend(pps_a);
    groups_b.extend(pps_b);

    let note = |s: &sps::Sps, tag: &str| {
        format!(
            "{tag}: {} / {} / {}x{} / POC类型{} / 参考帧{}",
            common::profile_name(s.profile_idc),
            common::level_name(s.level_idc),
            s.width,
            s.height,
            s.pic_order_cnt_type,
            s.max_num_ref_frames
        )
    };

    let mut b_index: HashMap<&str, HashMap<&str, &Field>> = HashMap::new();
    for g in &groups_b {
        let m: HashMap<&str, &Field> = g.fields.iter().map(|f| (f.name.as_str(), f)).collect();
        b_index.insert(g.title.as_str(), m);
    }

    let mut rows = Vec::new();
    let mut a_field_names: HashMap<String, Vec<String>> = HashMap::new();
    for g in &groups_a {
        let names = g.fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>();
        a_field_names.insert(g.title.clone(), names);
        let b_fields = b_index.get(g.title.as_str());
        for f in &g.fields {
            let (vb, same) = match b_fields.and_then(|m| m.get(f.name.as_str())) {
                Some(bf) => (bf.value.clone(), bf.value == f.value),
                None => ("（B 侧无此字段）".to_string(), false),
            };
            rows.push(DiffRow {
                section: g.title.clone(),
                name: f.name.clone(),
                value_a: f.value.clone(),
                value_b: vb,
                same,
                desc: f.desc.clone(),
            });
        }
    }
    // B 侧独有字段（A 侧按字段名不存在时补充显示）
    for g in &groups_b {
        let a_names = a_field_names.get(g.title.as_str()).cloned().unwrap_or_default();
        for f in &g.fields {
            if !a_names.contains(&f.name) {
                rows.push(DiffRow {
                    section: g.title.clone(),
                    name: f.name.clone(),
                    value_a: "（A 侧无此字段）".to_string(),
                    value_b: f.value.clone(),
                    same: false,
                    desc: f.desc.clone(),
                });
            }
        }
    }

    let diff_count = rows.iter().filter(|r| !r.same).count();
    Ok(SpsCompare {
        diff_count,
        rows,
        note_a: note(&sps_a, "A"),
        note_b: note(&sps_b, "B"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::h264::test_util::build_test_stream;

    #[test]
    fn stream_scan_and_stats() {
        let data = build_test_stream(30);
        let nals = nal::scan_annexb(&data);
        let r = parse_stream_bytes(&data, "annexb", &nals);
        assert_eq!(r.stats.nalu_count, 6); // AUD + SPS + PPS + IDR + P + P
        assert_eq!(r.stats.idr_count, 1);
        assert_eq!(r.stats.resolution.as_deref(), Some("320x240"));
        assert!(r.stats.profile.as_deref().unwrap_or("").contains("Baseline"));
        assert_eq!(r.stats.source_format, "annexb");
        // 统计里应有各类型
        let sps_count = r.stats.counts.iter().find(|(k, _)| k.starts_with("SPS")).map(|(_, v)| *v);
        assert_eq!(sps_count, Some(1));
    }

    #[test]
    fn sps_detail_fields() {
        let data = build_test_stream(30);
        let nals = nal::scan_annexb(&data);
        let d = nalu_detail_bytes(&data, &nals, 1).expect("SPS 详情失败");
        assert_eq!(d.type_name, "SPS");
        let all: Vec<&Field> = d.groups.iter().flat_map(|g| g.fields.iter()).collect();
        let prof = all.iter().find(|f| f.name == "profile_idc").expect("缺 profile_idc");
        assert!(prof.value.contains("Baseline"), "profile 值: {}", prof.value);
        let res = all.iter().find(|f| f.name == "分辨率(计算)").expect("缺分辨率");
        assert_eq!(res.value, "320x240");
        let c1 = all.iter().find(|f| f.name == "constraint_set1_flag").expect("缺约束位");
        assert_eq!(c1.value, "1");
        assert!(d.warnings.is_empty(), "SPS 不应有警告: {:?}", d.warnings);
    }

    #[test]
    fn slice_detail_with_context() {
        let data = build_test_stream(30);
        let nals = nal::scan_annexb(&data);
        // index 3 = IDR 切片
        let d = nalu_detail_bytes(&data, &nals, 3).expect("切片详情失败");
        assert_eq!(d.type_name, "IDR切片");
        let all: Vec<&Field> = d.groups.iter().flat_map(|g| g.fields.iter()).collect();
        let st = all.iter().find(|f| f.name == "slice_type").expect("缺 slice_type");
        assert!(st.value.starts_with("7 (I"));
        let fn_ = all.iter().find(|f| f.name == "frame_num").expect("缺 frame_num");
        assert_eq!(fn_.value, "0");
        let idr = all.iter().find(|f| f.name == "idr_pic_id").expect("缺 idr_pic_id");
        assert_eq!(idr.value, "0");
        assert!(d.warnings.is_empty(), "切片不应有警告: {:?}", d.warnings);

        // index 4 = P 切片，frame_num=1
        let d2 = nalu_detail_bytes(&data, &nals, 4).expect("P 切片详情失败");
        let all2: Vec<&Field> = d2.groups.iter().flat_map(|g| g.fields.iter()).collect();
        let fn2 = all2.iter().find(|f| f.name == "frame_num").expect("缺 frame_num");
        assert_eq!(fn2.value, "1");
    }

    #[test]
    fn pps_detail() {
        let data = build_test_stream(30);
        let nals = nal::scan_annexb(&data);
        let d = nalu_detail_bytes(&data, &nals, 2).expect("PPS 详情失败");
        let all: Vec<&Field> = d.groups.iter().flat_map(|g| g.fields.iter()).collect();
        let ent = all.iter().find(|f| f.name == "entropy_coding_mode_flag").expect("缺熵编码");
        assert!(ent.value.contains("CAVLC"));
    }

    #[test]
    fn compare_same_stream_no_diff() {
        let data = build_test_stream(30);
        let nals = nal::scan_annexb(&data);
        let c = compare_sps_bytes(&data, &nals, &data, &nals).expect("对比失败");
        assert_eq!(c.diff_count, 0, "相同码流不应有差异");
    }

    #[test]
    fn compare_different_level_finds_diff() {
        let a = build_test_stream(30);
        let b = build_test_stream(40);
        let (na, nb) = (nal::scan_annexb(&a), nal::scan_annexb(&b));
        let c = compare_sps_bytes(&a, &na, &b, &nb).expect("对比失败");
        assert!(c.diff_count > 0);
        let level_row = c.rows.iter().find(|r| r.name == "level_idc").expect("缺 level 行");
        assert!(!level_row.same);
        assert!(level_row.value_a.contains("3.0"));
        assert!(level_row.value_b.contains("4.0"));
    }

    #[test]
    fn out_of_range_index_errors() {
        let data = build_test_stream(30);
        let nals = nal::scan_annexb(&data);
        assert!(nalu_detail_bytes(&data, &nals, 999).is_err());
    }
}

