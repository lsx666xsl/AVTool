//! PPS（图像参数集）解析。

use super::bitreader::BitReader;
use super::common::*;

#[derive(Debug, Clone, Default)]
pub struct Pps {
    pub pic_parameter_set_id: u32,
    pub seq_parameter_set_id: u32,
    pub entropy_coding_mode_flag: bool,
    pub bottom_field_pic_order_in_frame_present_flag: bool,
    pub num_slice_groups_minus1: u32,
    pub num_ref_idx_l0_default_active_minus1: u32,
    pub num_ref_idx_l1_default_active_minus1: u32,
    pub weighted_pred_flag: bool,
    pub weighted_bipred_idc: u32,
    pub pic_init_qp_minus26: i32,
    pub pic_init_qs_minus26: i32,
    pub chroma_qp_index_offset: i32,
    pub deblocking_filter_control_present_flag: bool,
    pub constrained_intra_pred_flag: bool,
    pub redundant_pic_cnt_present_flag: bool,
    pub transform_8x8_mode_flag: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct PpsParse {
    pub pps: Pps,
    pub groups: Vec<FieldGroup>,
    pub warnings: Vec<String>,
}

fn bool_str(v: bool) -> String {
    if v { "1".into() } else { "0".into() }
}

pub fn parse_pps(body: &[u8], sps_chroma_hint: Option<u32>) -> Result<PpsParse, String> {
    if body.is_empty() {
        return Err("PPS 载荷为空".into());
    }
    let mut br = BitReader::new(body);
    let mut pps = Pps::default();
    let mut warnings = Vec::new();

    let mut g1 = FieldGroup::new("PPS · 基础信息");
    let mut g2 = FieldGroup::new("PPS · 熵编码与预测");
    let mut g3 = FieldGroup::new("PPS · 量化");
    let mut g4 = FieldGroup::new("PPS · 其他控制");

    let s = br.bit_pos();
    let pps_id = br.read_ue()?;
    pps.pic_parameter_set_id = pps_id;
    add_field(&mut g1, "pic_parameter_set_id", format!("{pps_id}"), s, br.bit_pos(), 8, "");
    let s = br.bit_pos();
    let sps_id = br.read_ue()?;
    pps.seq_parameter_set_id = sps_id;
    add_field(&mut g1, "seq_parameter_set_id", format!("{sps_id}"), s, br.bit_pos(), 8, "引用的 SPS");

    let s = br.bit_pos();
    let entropy = br.read_bit()? == 1;
    pps.entropy_coding_mode_flag = entropy;
    add_field(
        &mut g2,
        "entropy_coding_mode_flag",
        if entropy { "1 (CABAC)".into() } else { "0 (CAVLC)".into() },
        s,
        br.bit_pos(),
        8,
        "CABAC 仅 Main/High 可用，Baseline 必须 CAVLC",
    );
    let s = br.bit_pos();
    let bottom_order = br.read_bit()? == 1;
    pps.bottom_field_pic_order_in_frame_present_flag = bottom_order;
    add_field(&mut g2, "bottom_field_pic_order_in_frame_present_flag", bool_str(bottom_order), s, br.bit_pos(), 8, "场编码时 POC 相关");

    let s = br.bit_pos();
    let num_groups = br.read_ue()?;
    pps.num_slice_groups_minus1 = num_groups;
    add_field(&mut g1, "num_slice_groups_minus1", format!("{num_groups}"), s, br.bit_pos(), 8, "FMO 多切片组（罕见）");
    if num_groups != 0 {
        warnings.push("该 PPS 使用多切片组（FMO），后续字段未解析（v1 不支持 FMO）".into());
        g1.note = Some("检测到 FMO，解析提前结束".to_string());
        return Ok(PpsParse { pps, groups: vec![g1, g2], warnings });
    }

    let s = br.bit_pos();
    let l0 = br.read_ue()?;
    let l1 = br.read_ue()?;
    pps.num_ref_idx_l0_default_active_minus1 = l0;
    pps.num_ref_idx_l1_default_active_minus1 = l1;
    add_field(&mut g2, "num_ref_idx_l0/l1_default_active_minus1", format!("{l0}/{l1}"), s, br.bit_pos(), 8, "默认参考帧列表长度");

    let s = br.bit_pos();
    let wp = br.read_bit()? == 1;
    pps.weighted_pred_flag = wp;
    add_field(&mut g2, "weighted_pred_flag", bool_str(wp), s, br.bit_pos(), 8, "P/SP 切片加权预测（嵌入式常见不支持）");
    let s = br.bit_pos();
    let wb_idc = br.read_bits(2)?;
    pps.weighted_bipred_idc = wb_idc;
    let wb_name = match wb_idc {
        0 => "0 (无)",
        1 => "1 (显式)",
        2 => "2 (隐式)",
        _ => "?",
    };
    add_field(&mut g2, "weighted_bipred_idc", format!("{wb_idc} {wb_name}"), s, br.bit_pos(), 8, "B 切片加权预测模式");

    let s = br.bit_pos();
    let init_qp = br.read_se()?;
    let init_qs = br.read_se()?;
    pps.pic_init_qp_minus26 = init_qp;
    pps.pic_init_qs_minus26 = init_qs;
    add_field(&mut g3, "pic_init_qp_minus26", format!("{init_qp}"), s, br.bit_pos(), 8, &format!("初始 QP = {}", init_qp + 26));
    add_field(&mut g3, "pic_init_qs_minus26", format!("{init_qs}"), s, br.bit_pos(), 8, "");
    let s = br.bit_pos();
    let cqpo = br.read_se()?;
    pps.chroma_qp_index_offset = cqpo;
    add_field(&mut g3, "chroma_qp_index_offset", format!("{cqpo}"), s, br.bit_pos(), 8, "");

    let s = br.bit_pos();
    let deblock = br.read_bit()? == 1;
    pps.deblocking_filter_control_present_flag = deblock;
    add_field(&mut g4, "deblocking_filter_control_present_flag", bool_str(deblock), s, br.bit_pos(), 8, "切片头是否携带环路滤波参数");
    let s = br.bit_pos();
    let cip = br.read_bit()? == 1;
    pps.constrained_intra_pred_flag = cip;
    add_field(&mut g4, "constrained_intra_pred_flag", bool_str(cip), s, br.bit_pos(), 8, "");
    let s = br.bit_pos();
    let rpc = br.read_bit()? == 1;
    pps.redundant_pic_cnt_present_flag = rpc;
    add_field(&mut g4, "redundant_pic_cnt_present_flag", bool_str(rpc), s, br.bit_pos(), 8, "");

    // 扩展部分（transform_8x8 仅 High 及以上）
    if br.more_rbsp_data() {
        let s = br.bit_pos();
        let t8 = br.read_bit()? == 1;
        pps.transform_8x8_mode_flag = Some(t8);
        add_field(&mut g4, "transform_8x8_mode_flag", bool_str(t8), s, br.bit_pos(), 8, "8x8 变换（High 档次，Baseline 无）");
        let s = br.bit_pos();
        let scaling = br.read_bit()? == 1;
        add_field(&mut g4, "pic_scaling_matrix_present_flag", bool_str(scaling), s, br.bit_pos(), 8, "");
        if scaling {
            let chroma = sps_chroma_hint.unwrap_or(1);
            let count = 6 + if chroma != 3 { 2 } else { 6 };
            for i in 0..count {
                let s = br.bit_pos();
                let present = br.read_bit()? == 1;
                add_field(&mut g4, &format!("pic_scaling_list_present_flag[{i}]"), bool_str(present), s, br.bit_pos(), 8, "");
                if present {
                    warnings.push("PPS 含缩放列表，按 8 个列表（chroma≠3）假设解析".into());
                    let size = if i < 6 { 16 } else { 64 };
                    let mut last_scale = 8i32;
                    let mut next_scale = 8i32;
                    for j in 0..size {
                        if next_scale != 0 {
                            let s = br.bit_pos();
                            let delta = br.read_se()?;
                            next_scale = (last_scale + delta + 256) % 256;
                            add_field(&mut g4, &format!("delta_scale[{j}]"), format!("{delta}"), s, br.bit_pos(), 8, "");
                        }
                        if next_scale != 0 {
                            last_scale = next_scale;
                        }
                    }
                }
            }
        }
        if br.more_rbsp_data() {
            let s = br.bit_pos();
            let cqpo2 = br.read_se()?;
            add_field(&mut g3, "second_chroma_qp_index_offset", format!("{cqpo2}"), s, br.bit_pos(), 8, "");
        }
    }

    Ok(PpsParse { pps, groups: vec![g1, g2, g3, g4], warnings })
}
