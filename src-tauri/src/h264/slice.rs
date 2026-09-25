//! 切片头解析（I/P/B/SP/SI）。部分字段依赖所属 SPS/PPS，
//! 上下文缺失或遇到不支持的结构时**优雅降级**：返回已解析部分 + 警告。

use super::bitreader::BitReader;
use super::common::*;
use super::pps::Pps;
use super::sps::Sps;

#[derive(Debug, Clone, Default)]
pub struct SliceInfo {
    pub first_mb_in_slice: u32,
    pub slice_type: u32,
    pub slice_type_name: String,
    pub pic_parameter_set_id: u32,
    pub frame_num: Option<u32>,
    pub idr_pic_id: Option<u32>,
    pub pic_order_cnt_lsb: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct SliceParse {
    pub info: SliceInfo,
    pub groups: Vec<FieldGroup>,
    pub warnings: Vec<String>,
}

pub fn slice_type_name(v: u32) -> String {
    let base = if v < 5 { v } else { v - 5 };
    let n = match base {
        0 => "P",
        1 => "B",
        2 => "I",
        3 => "SP",
        4 => "SI",
        _ => "?",
    };
    if v < 5 {
        format!("{v} ({n} · 该图像全部切片)")
    } else {
        format!("{v} ({n} · 仅本切片)")
    }
}

struct Ctx<'a> {
    g1: FieldGroup,
    g2: FieldGroup,
    g3: FieldGroup,
    g4: FieldGroup,
    warnings: Vec<String>,
    sps: Option<&'a Sps>,
    pps: Option<&'a Pps>,
}

impl<'a> Ctx<'a> {
    fn bail(mut self, e: String) -> SliceParse {
        self.warnings.push(format!("解析中断：{e}"));
        self.finish(SliceInfo::default())
    }

    fn finish(self, info: SliceInfo) -> SliceParse {
        let groups = vec![self.g1, self.g2, self.g3, self.g4];
        SliceParse { info, groups, warnings: self.warnings }
    }
}

fn bool_str(v: bool) -> String {
    if v { "1".into() } else { "0".into() }
}

pub fn parse_slice_header(
    body: &[u8],
    nal_ref_idc: u8,
    is_idr: bool,
    sps: Option<&Sps>,
    pps: Option<&Pps>,
) -> SliceParse {
    let mut br = BitReader::new(body);
    let mut ctx = Ctx {
        g1: FieldGroup::new("切片头 · 基础"),
        g2: FieldGroup::new("切片头 · 图像序号"),
        g3: FieldGroup::new("切片头 · 参考与加权"),
        g4: FieldGroup::new("切片头 · 滤波与量化"),
        warnings: Vec::new(),
        sps,
        pps,
    };

    let info = match parse_inner(&mut br, &mut ctx, nal_ref_idc, is_idr) {
        Ok(i) => i,
        Err(e) => return ctx.bail(e),
    };
    ctx.finish(info)
}

fn parse_inner(
    br: &mut BitReader,
    ctx: &mut Ctx,
    nal_ref_idc: u8,
    is_idr: bool,
) -> Result<SliceInfo, String> {
    let mut info = SliceInfo::default();

    let s = br.bit_pos();
    let first_mb = br.read_ue()?;
    info.first_mb_in_slice = first_mb;
    add_field(&mut ctx.g1, "first_mb_in_slice", format!("{first_mb}"), s, br.bit_pos(), 8, "0 表示新图像的第一个切片");

    let s = br.bit_pos();
    let st = br.read_ue()?;
    info.slice_type = st;
    info.slice_type_name = slice_type_name(st);
    add_field(&mut ctx.g1, "slice_type", slice_type_name(st), s, br.bit_pos(), 8, "");

    let s = br.bit_pos();
    let pps_id = br.read_ue()?;
    info.pic_parameter_set_id = pps_id;
    add_field(&mut ctx.g1, "pic_parameter_set_id", format!("{pps_id}"), s, br.bit_pos(), 8, "");

    if let Some(sps) = ctx.sps {
        if sps.separate_colour_plane_flag {
            let s = br.bit_pos();
            let v = br.read_bits(2)?;
            add_field(&mut ctx.g1, "colour_plane_id", format!("{v}"), s, br.bit_pos(), 8, "");
        }
    }

    // frame_num 依赖 SPS 的 log2_max_frame_num
    let sps = match ctx.sps {
        Some(s) => s,
        None => {
            ctx.warnings
                .push("上下文缺少 SPS，frame_num 及之后的字段未解析（可能 SPS 不在本 NALU 之前）".into());
            return Ok(info);
        }
    };
    let frame_num_bits = sps.log2_max_frame_num_minus4 + 4;
    let s = br.bit_pos();
    let frame_num = br.read_bits(frame_num_bits)?;
    info.frame_num = Some(frame_num);
    add_field(&mut ctx.g1, "frame_num", format!("{frame_num}"), s, br.bit_pos(), 8, "按解码序计数的图像编号");

    let mut field_pic = false;
    if !sps.frame_mbs_only_flag {
        let s = br.bit_pos();
        field_pic = br.read_bit()? == 1;
        add_field(&mut ctx.g2, "field_pic_flag", bool_str(field_pic), s, br.bit_pos(), 8, "");
        if field_pic {
            let s = br.bit_pos();
            let bottom = br.read_bit()? == 1;
            add_field(&mut ctx.g2, "bottom_field_flag", bool_str(bottom), s, br.bit_pos(), 8, "");
        }
    }

    if is_idr {
        let s = br.bit_pos();
        let idr_id = br.read_ue()?;
        info.idr_pic_id = Some(idr_id);
        add_field(&mut ctx.g2, "idr_pic_id", format!("{idr_id}"), s, br.bit_pos(), 8, "IDR 图像编号（连续 IDR 递增区分）");
    }

    match sps.pic_order_cnt_type {
        0 => {
            let bits = sps.log2_max_pic_order_cnt_lsb_minus4.unwrap_or(0) + 4;
            let s = br.bit_pos();
            let poc_lsb = br.read_bits(bits)?;
            info.pic_order_cnt_lsb = Some(poc_lsb);
            add_field(&mut ctx.g2, "pic_order_cnt_lsb", format!("{poc_lsb}"), s, br.bit_pos(), 8, "显示序号低位（B 帧排序依据）");
            let bottom_present = ctx.pps.map(|p| p.bottom_field_pic_order_in_frame_present_flag).unwrap_or(false);
            if bottom_present && !field_pic {
                let s = br.bit_pos();
                let delta = br.read_se()?;
                add_field(&mut ctx.g2, "delta_pic_order_cnt_bottom", format!("{delta}"), s, br.bit_pos(), 8, "");
            }
        }
        1 => {
            // delta_pic_order_cnt[0] 仅在 !delta_pic_order_always_zero_flag 时出现
            let always_zero = sps.delta_pic_order_always_zero_flag.unwrap_or(false);
            if !always_zero {
                let s = br.bit_pos();
                let d0 = br.read_se()?;
                add_field(&mut ctx.g2, "delta_pic_order_cnt[0]", format!("{d0}"), s, br.bit_pos(), 8, "");
                let bottom_present = ctx.pps.map(|p| p.bottom_field_pic_order_in_frame_present_flag).unwrap_or(false);
                if bottom_present && !field_pic {
                    let s = br.bit_pos();
                    let d1 = br.read_se()?;
                    add_field(&mut ctx.g2, "delta_pic_order_cnt[1]", format!("{d1}"), s, br.bit_pos(), 8, "");
                }
            }
        }
        _ => {}
    }

    let redundant = ctx.pps.map(|p| p.redundant_pic_cnt_present_flag).unwrap_or(false);
    if redundant {
        let s = br.bit_pos();
        let v = br.read_ue()?;
        add_field(&mut ctx.g1, "redundant_pic_cnt", format!("{v}"), s, br.bit_pos(), 8, "");
    }

    let base_type = if info.slice_type < 5 { info.slice_type } else { info.slice_type - 5 };
    if base_type == 1 {
        // B 切片
        let s = br.bit_pos();
        let spatial = br.read_bit()? == 1;
        add_field(&mut ctx.g3, "direct_spatial_mv_pred_flag", bool_str(spatial), s, br.bit_pos(), 8, "B 帧直接模式预测方式");
    }

    let pps = ctx.pps;
    let (mut num_l0, mut num_l1) = match pps {
        Some(p) => (p.num_ref_idx_l0_default_active_minus1 + 1, p.num_ref_idx_l1_default_active_minus1 + 1),
        None => (1, 1),
    };
    let s = br.bit_pos();
    let override_flag = br.read_bit()? == 1;
    add_field(&mut ctx.g3, "num_ref_idx_active_override_flag", bool_str(override_flag), s, br.bit_pos(), 8, "");
    if override_flag {
        let s = br.bit_pos();
        num_l0 = br.read_ue()? + 1;
        add_field(&mut ctx.g3, "num_ref_idx_l0_active_minus1", format!("{}", num_l0 - 1), s, br.bit_pos(), 8, "");
        if base_type == 1 {
            let s = br.bit_pos();
            num_l1 = br.read_ue()? + 1;
            add_field(&mut ctx.g3, "num_ref_idx_l1_active_minus1", format!("{}", num_l1 - 1), s, br.bit_pos(), 8, "");
        }
    }

    // ref_pic_list_modification
    if base_type != 2 && base_type != 4 {
        let s = br.bit_pos();
        let flag = br.read_bit()? == 1;
        add_field(&mut ctx.g3, "ref_pic_list_modification_flag_l0", bool_str(flag), s, br.bit_pos(), 8, "");
        if flag {
            let mut guard = 0;
            loop {
                let s = br.bit_pos();
                let idc = br.read_ue()?;
                add_field(&mut ctx.g3, "modification_of_pic_nums_idc", format!("{idc}"), s, br.bit_pos(), 8, "");
                if idc == 3 {
                    break;
                }
                let s = br.bit_pos();
                let v = br.read_ue()?;
                add_field(&mut ctx.g3, "abs_diff_pic_num_minus1 / long_term_pic_num", format!("{v}"), s, br.bit_pos(), 8, "");
                guard += 1;
                if guard > 32 {
                    ctx.warnings.push("ref_pic_list_modification 循环异常，停止解析".into());
                    break;
                }
            }
        }
    }
    if base_type == 1 {
        let s = br.bit_pos();
        let flag = br.read_bit()? == 1;
        add_field(&mut ctx.g3, "ref_pic_list_modification_flag_l1", bool_str(flag), s, br.bit_pos(), 8, "");
        if flag {
            let mut guard = 0;
            loop {
                let s = br.bit_pos();
                let idc = br.read_ue()?;
                add_field(&mut ctx.g3, "modification_of_pic_nums_idc_l1", format!("{idc}"), s, br.bit_pos(), 8, "");
                if idc == 3 {
                    break;
                }
                let s = br.bit_pos();
                let v = br.read_ue()?;
                add_field(&mut ctx.g3, "abs_diff/long_term_pic_num_l1", format!("{v}"), s, br.bit_pos(), 8, "");
                guard += 1;
                if guard > 32 {
                    break;
                }
            }
        }
    }

    // 加权预测表
    let wp_p = pps.map(|p| p.weighted_pred_flag).unwrap_or(false);
    let wb_idc = pps.map(|p| p.weighted_bipred_idc).unwrap_or(0);
    let has_chroma = sps.chroma_format_idc != 0;
    if (base_type == 0 || base_type == 3) && wp_p {
        parse_pred_weight_table(br, ctx, num_l0, has_chroma, "l0")?;
    } else if base_type == 1 && wb_idc == 1 {
        parse_pred_weight_table(br, ctx, num_l0, has_chroma, "l0")?;
        parse_pred_weight_table(br, ctx, num_l1, has_chroma, "l1")?;
    }

    // dec_ref_pic_marking
    if nal_ref_idc != 0 {
        let s = br.bit_pos();
        if is_idr {
            let v1 = br.read_bit()? == 1;
            let v2 = br.read_bit()? == 1;
            add_field(&mut ctx.g3, "no_output_of_prior_pics_flag", bool_str(v1), s, br.bit_pos(), 8, "");
            let s = br.bit_pos();
            add_field(&mut ctx.g3, "long_term_reference_flag", bool_str(v2), s, br.bit_pos(), 8, "");
        } else {
            let s = br.bit_pos();
            let adaptive = br.read_bit()? == 1;
            add_field(&mut ctx.g3, "adaptive_ref_pic_marking_mode_flag", bool_str(adaptive), s, br.bit_pos(), 8, "1=显式管理参考帧");
            if adaptive {
                let mut guard = 0;
                loop {
                    let s = br.bit_pos();
                    let op = br.read_ue()?;
                    add_field(&mut ctx.g3, "memory_management_control_operation", format!("{op}"), s, br.bit_pos(), 8, "");
                    if op == 0 {
                        break;
                    }
                    // 7.3.3.3：op=1 读 difference_of_pic_nums_minus1；op=2/6 读 long_term_frame_idx；
                    // op=3 读前两者共两个；op=4 读 max_long_term_frame_idx_plus1；op=5 无参数
                    match op {
                        1 => {
                            let s = br.bit_pos();
                            let v = br.read_ue()?;
                            add_field(&mut ctx.g3, "difference_of_pic_nums_minus1", format!("{v}"), s, br.bit_pos(), 8, "");
                        }
                        2 | 6 => {
                            let s = br.bit_pos();
                            let v = br.read_ue()?;
                            add_field(&mut ctx.g3, "long_term_frame_idx", format!("{v}"), s, br.bit_pos(), 8, "");
                        }
                        3 => {
                            let s = br.bit_pos();
                            let v1 = br.read_ue()?;
                            let v2 = br.read_ue()?;
                            add_field(&mut ctx.g3, "difference_of_pic_nums_minus1", format!("{v1}"), s, br.bit_pos(), 8, "");
                            let s = br.bit_pos();
                            add_field(&mut ctx.g3, "long_term_frame_idx", format!("{v2}"), s, br.bit_pos(), 8, "");
                        }
                        4 => {
                            let s = br.bit_pos();
                            let v = br.read_ue()?;
                            add_field(&mut ctx.g3, "max_long_term_frame_idx_plus1", format!("{v}"), s, br.bit_pos(), 8, "");
                        }
                        5 => {}
                        _ => {
                            ctx.warnings.push(format!("memory_management_control_operation={op} 非法（>6），停止解析"));
                            return Err(format!("MMCO 操作码 {op} 非法"));
                        }
                    }
                    guard += 1;
                    if guard > 64 {
                        ctx.warnings.push("dec_ref_pic_marking 循环异常".into());
                        break;
                    }
                }
            }
        }
    }

    let entropy = pps.map(|p| p.entropy_coding_mode_flag).unwrap_or(false);
    if entropy && base_type != 2 && base_type != 4 {
        let s = br.bit_pos();
        let v = br.read_ue()?;
        add_field(&mut ctx.g4, "cabac_init_idc", format!("{v}"), s, br.bit_pos(), 8, "");
    }
    let s = br.bit_pos();
    let qp_delta = br.read_se()?;
    add_field(&mut ctx.g4, "slice_qp_delta", format!("{qp_delta}"), s, br.bit_pos(), 8, "");
    if base_type == 3 || base_type == 4 {
        let s = br.bit_pos();
        let qs = br.read_se()?;
        add_field(&mut ctx.g4, "slice_qs_delta", format!("{qs}"), s, br.bit_pos(), 8, "");
    }

    let deblock_present = pps.map(|p| p.deblocking_filter_control_present_flag).unwrap_or(false);
    if deblock_present {
        let s = br.bit_pos();
        let idc = br.read_ue()?;
        add_field(&mut ctx.g4, "disable_deblocking_filter_idc", format!("{idc}"), s, br.bit_pos(), 8, "0=启用滤波 1=禁用 2=自定义");
        if idc != 1 {
            let s = br.bit_pos();
            let alpha = br.read_se()?;
            let beta = br.read_se()?;
            add_field(&mut ctx.g4, "slice_alpha_c0/beta_offset_div2", format!("{alpha}/{beta}"), s, br.bit_pos(), 8, "");
        }
    }

    if pps.map(|p| p.num_slice_groups_minus1).unwrap_or(0) > 0 {
        ctx.warnings.push("多切片组(FMO)切片头未解析 slice_group_change_cycle".into());
    }

    Ok(info)
}

fn parse_pred_weight_table(
    br: &mut BitReader,
    ctx: &mut Ctx,
    num_ref: u32,
    has_chroma: bool,
    list: &str,
) -> Result<(), String> {
    if num_ref > 32 {
        return Err(format!("num_ref_idx_active={num_ref} 异常过大（>32），疑似码流损坏"));
    }
    let s = br.bit_pos();
    let luma_denom = br.read_ue()?;
    add_field(&mut ctx.g3, &format!("pred_weight_table[{list}].luma_log2_weight_denom"), format!("{luma_denom}"), s, br.bit_pos(), 8, "");
    let mut chroma_denom = 0;
    if has_chroma {
        let s = br.bit_pos();
        chroma_denom = br.read_ue()?;
        add_field(&mut ctx.g3, &format!("pred_weight_table[{list}].chroma_log2_weight_denom"), format!("{chroma_denom}"), s, br.bit_pos(), 8, "");
    }
    let _ = chroma_denom;
    for i in 0..num_ref {
        let s = br.bit_pos();
        let lw = br.read_bit()? == 1;
        add_field(&mut ctx.g3, &format!("pred_weight_table[{list}].luma_weight_{list}_flag[{i}]"), bool_str(lw), s, br.bit_pos(), 8, "");
        if lw {
            let s = br.bit_pos();
            let dw = br.read_se()?;
            let off = br.read_se()?;
            add_field(&mut ctx.g3, &format!("pred_weight_table[{list}].luma[{i}]"), format!("delta={dw}, offset={off}"), s, br.bit_pos(), 8, "");
        }
        if has_chroma {
            let s = br.bit_pos();
            let cw = br.read_bit()? == 1;
            add_field(&mut ctx.g3, &format!("pred_weight_table[{list}].chroma_flag[{i}]"), bool_str(cw), s, br.bit_pos(), 8, "");
            if cw {
                let s = br.bit_pos();
                let dw = br.read_se()?;
                let off = br.read_se()?;
                let dw2 = br.read_se()?;
                let off2 = br.read_se()?;
                add_field(&mut ctx.g3, &format!("pred_weight_table[{list}].chroma[{i}]"), format!("delta={dw},{dw2} offset={off},{off2}"), s, br.bit_pos(), 8, "");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::h264::test_util::{build_test_sps_nalu, BitWriter};

    /// 构造带 MMCO op=3 的 P 切片头（验证 7.3.3.3 双参数操作码）
    #[test]
    fn mmco_op3_two_params() {
        let sps_nalu = build_test_sps_nalu(30);
        let sps_parse = crate::h264::sps::parse_sps(&sps_nalu[1..]).expect("SPS 解析失败");
        let sps = &sps_parse.sps;

        let mut w = BitWriter::new();
        w.ue(0); // first_mb_in_slice
        w.ue(5); // slice_type = P
        w.ue(0); // pps_id
        w.u(1, 4); // frame_num = 1
        w.u(0, 1); // num_ref_idx_active_override_flag
        w.u(0, 1); // ref_pic_list_modification_flag_l0
        w.u(1, 1); // adaptive_ref_pic_marking_mode_flag = 1
        w.ue(3); // op=3：携带两个参数
        w.ue(2); // difference_of_pic_nums_minus1
        w.ue(1); // long_term_frame_idx
        w.ue(0); // op=0 结束
        w.se(0); // slice_qp_delta
        w.rbsp_trailing();
        let body = w.to_bytes();

        let parsed = parse_slice_header(&body, 2, false, Some(sps), None);
        assert!(
            parsed.warnings.is_empty(),
            "MMCO op=3 应完整解析: {:?}",
            parsed.warnings
        );
        let all: Vec<&Field> = parsed.groups.iter().flat_map(|g| g.fields.iter()).collect();
        let diff = all
            .iter()
            .find(|f| f.name == "difference_of_pic_nums_minus1")
            .expect("缺 difference_of_pic_nums_minus1");
        assert_eq!(diff.value, "2");
        let ltfi = all
            .iter()
            .find(|f| f.name == "long_term_frame_idx")
            .expect("缺 long_term_frame_idx");
        assert_eq!(ltfi.value, "1");
        let qp = all
            .iter()
            .find(|f| f.name == "slice_qp_delta")
            .expect("MMCO 之后应继续解析到 slice_qp_delta");
        assert_eq!(qp.value, "0");
    }
}
