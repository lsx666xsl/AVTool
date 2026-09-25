//! SPS（序列参数集）解析：含高档次扩展、缩放列表、完整 VUI。
//!
//! 输入为去除仿真预防字节后的 RBSP 体（不含 NALU 头），
//! 字段比特位以 NALU 首字节为基准（base = 8）。

use super::bitreader::BitReader;
use super::common::*;

#[derive(Debug, Clone, Default)]
pub struct Sps {
    pub profile_idc: u8,
    pub constraint_set0: bool,
    pub constraint_set1: bool,
    pub constraint_set2: bool,
    pub constraint_set3: bool,
    pub constraint_set4: bool,
    pub constraint_set5: bool,
    pub level_idc: u8,
    pub seq_parameter_set_id: u32,
    pub chroma_format_idc: u32,
    pub separate_colour_plane_flag: bool,
    pub bit_depth_luma_minus8: u32,
    pub bit_depth_chroma_minus8: u32,
    pub log2_max_frame_num_minus4: u32,
    pub pic_order_cnt_type: u32,
    pub log2_max_pic_order_cnt_lsb_minus4: Option<u32>,
    pub delta_pic_order_always_zero_flag: Option<bool>,
    pub max_num_ref_frames: u32,
    pub gaps_in_frame_num_value_allowed_flag: bool,
    pub pic_width_in_mbs_minus1: u32,
    pub pic_height_in_map_units_minus1: u32,
    pub frame_mbs_only_flag: bool,
    pub mb_adaptive_frame_field_flag: Option<bool>,
    pub direct_8x8_inference_flag: bool,
    pub frame_cropping_flag: bool,
    pub frame_crop_left_offset: u32,
    pub frame_crop_right_offset: u32,
    pub frame_crop_top_offset: u32,
    pub frame_crop_bottom_offset: u32,
    pub vui_parameters_present_flag: bool,
    pub num_units_in_tick: Option<u32>,
    pub time_scale: Option<u32>,
    pub fixed_frame_rate_flag: Option<bool>,
    pub aspect_ratio_idc: Option<u32>,
    pub sar_width: Option<u32>,
    pub sar_height: Option<u32>,
    // 计算值
    pub width: u32,
    pub height: u32,
    pub fps: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct SpsParse {
    pub sps: Sps,
    pub groups: Vec<FieldGroup>,
}

fn is_high_profile(p: u8) -> bool {
    matches!(p, 100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135)
}

/// 高档次（High/10/4:2:2/4:4:4 等）才有的扩展字段
fn parse_scaling_list(
    br: &mut BitReader,
    size: usize,
    group: &mut FieldGroup,
    list_name: &str,
) -> Result<(), String> {
    let mut last_scale = 8i32;
    let mut next_scale = 8i32;
    for j in 0..size {
        if next_scale != 0 {
            let s = br.bit_pos();
            let delta = br.read_se()?;
            next_scale = (last_scale + delta + 256) % 256;
            add_field(
                group,
                &format!("{list_name}.delta_scale[{j}]"),
                format!("{delta}"),
                s,
                br.bit_pos(),
                8,
                "",
            );
        }
        if next_scale != 0 {
            last_scale = next_scale;
        }
    }
    Ok(())
}

fn parse_hrd(br: &mut BitReader, gv: &mut FieldGroup, prefix: &str) -> Result<(), String> {
    let s0 = br.bit_pos();
    let cpb_cnt_minus1 = br.read_ue()?;
    add_field(
        gv,
        &format!("{prefix}.cpb_cnt_minus1"),
        format!("{cpb_cnt_minus1}"),
        s0,
        br.bit_pos(),
        8,
        "CPB 个数-1",
    );
    let s1 = br.bit_pos();
    let bit_rate_scale = br.read_bits(4)?;
    add_field(
        gv,
        &format!("{prefix}.bit_rate_scale"),
        format!("{bit_rate_scale}"),
        s1,
        br.bit_pos(),
        8,
        "码率上标（×6）",
    );
    let s2 = br.bit_pos();
    let cpb_size_scale = br.read_bits(4)?;
    add_field(
        gv,
        &format!("{prefix}.cpb_size_scale"),
        format!("{cpb_size_scale}"),
        s2,
        br.bit_pos(),
        8,
        "缓冲大小上标（×4）",
    );
    if cpb_cnt_minus1 > 31 {
        return Err(format!("cpb_cnt_minus1={cpb_cnt_minus1} 异常过大（>31），疑似码流损坏"));
    }
    for i in 0..=cpb_cnt_minus1 {
        let s = br.bit_pos();
        let rate = br.read_ue()?;
        let size = br.read_ue()?;
        let cbr = br.read_bit()?;
        add_field(
            gv,
            &format!("{prefix}.cpb[{i}]"),
            format!("rate-1={rate}, size-1={size}, cbr={cbr}"),
            s,
            br.bit_pos(),
            8,
            "",
        );
    }
    for (name, desc) in [
        ("initial_cpb_removal_delay_length_minus1", ""),
        ("cpb_removal_delay_length_minus1", ""),
        ("dpb_output_delay_length_minus1", ""),
        ("time_offset_length", ""),
    ] {
        let s = br.bit_pos();
        let v = br.read_bits(5)?;
        add_field(gv, &format!("{prefix}.{name}"), format!("{v}"), s, br.bit_pos(), 8, desc);
    }
    Ok(())
}

fn parse_vui(br: &mut BitReader, gv: &mut FieldGroup, sps: &mut Sps) -> Result<(), String> {
    let s = br.bit_pos();
    let aspect_present = br.read_bit()? == 1;
    add_field(gv, "aspect_ratio_info_present_flag", bool_str(aspect_present), s, br.bit_pos(), 8, "宽高比信息");
    if aspect_present {
        let s = br.bit_pos();
        let idc = br.read_bits(8)?;
        sps.aspect_ratio_idc = Some(idc);
        add_field(gv, "aspect_ratio_idc", aspect_ratio_name(idc), s, br.bit_pos(), 8, "");
        if idc == 255 {
            let s = br.bit_pos();
            let w = br.read_bits(16)?;
            let h = br.read_bits(16)?;
            sps.sar_width = Some(w);
            sps.sar_height = Some(h);
            add_field(gv, "sar_width/sar_height", format!("{w}x{h}"), s, br.bit_pos(), 8, "SAR 宽高");
        }
    }
    let s = br.bit_pos();
    let overscan_present = br.read_bit()? == 1;
    add_field(gv, "overscan_info_present_flag", bool_str(overscan_present), s, br.bit_pos(), 8, "");
    if overscan_present {
        let s = br.bit_pos();
        let v = br.read_bit()? == 1;
        add_field(gv, "overscan_appropriate_flag", bool_str(v), s, br.bit_pos(), 8, "");
    }
    let s = br.bit_pos();
    let video_signal_present = br.read_bit()? == 1;
    add_field(gv, "video_signal_type_present_flag", bool_str(video_signal_present), s, br.bit_pos(), 8, "");
    if video_signal_present {
        let s = br.bit_pos();
        let vf = br.read_bits(3)?;
        let vf_end = br.bit_pos();
        let full_range = br.read_bit()? == 1;
        add_field(gv, "video_format", video_format_name(vf), s, vf_end, 8, "");
        add_field(gv, "video_full_range_flag", bool_str(full_range), vf_end, br.bit_pos(), 8, "");
        let s = br.bit_pos();
        let colour_desc = br.read_bit()? == 1;
        add_field(gv, "colour_description_present_flag", bool_str(colour_desc), s, br.bit_pos(), 8, "");
        if colour_desc {
            let s = br.bit_pos();
            let p = br.read_bits(8)?;
            let t = br.read_bits(8)?;
            let m = br.read_bits(8)?;
            add_field(
                gv,
                "colour_primaries/transfer/matrix",
                format!("{p}/{t}/{m}"),
                s,
                br.bit_pos(),
                8,
                " primaries / transfer / matrix",
            );
        }
    }
    let s = br.bit_pos();
    let chroma_loc_present = br.read_bit()? == 1;
    add_field(gv, "chroma_loc_info_present_flag", bool_str(chroma_loc_present), s, br.bit_pos(), 8, "");
    if chroma_loc_present {
        let s = br.bit_pos();
        let top = br.read_ue()?;
        let bottom = br.read_ue()?;
        add_field(
            gv,
            "chroma_sample_loc_type(top/bottom)",
            format!("{top}/{bottom}"),
            s,
            br.bit_pos(),
            8,
            "",
        );
    }
    let s = br.bit_pos();
    let timing_present = br.read_bit()? == 1;
    add_field(gv, "timing_info_present_flag", bool_str(timing_present), s, br.bit_pos(), 8, "帧率信息");
    if timing_present {
        let s = br.bit_pos();
        let num_units = br.read_bits(32)?;
        let time_scale = br.read_bits(32)?;
        let fixed = br.read_bit()? == 1;
        sps.num_units_in_tick = Some(num_units);
        sps.time_scale = Some(time_scale);
        sps.fixed_frame_rate_flag = Some(fixed);
        if num_units > 0 && time_scale > 0 {
            // 隔行流（允许场编码）时该比值为场率，帧率为其一半
            let rate = time_scale as f64 / num_units as f64;
            sps.fps = Some(if sps.frame_mbs_only_flag {
                rate
            } else {
                rate / 2.0
            });
        }
        add_field(
            gv,
            "num_units_in_tick / time_scale",
            format!("{num_units} / {time_scale}"),
            s,
            br.bit_pos(),
            8,
            "time_scale/num_units_in_tick = 场率",
        );
        let s = br.bit_pos();
        add_field(gv, "fixed_frame_rate_flag", bool_str(fixed), s, br.bit_pos(), 8, "");
        if let Some(fps) = sps.fps {
            let note = if sps.frame_mbs_only_flag {
                "time_scale / num_units_in_tick"
            } else {
                "隔行流：场率已按 2:1 折算为帧率"
            };
            gv.push(Field::new("帧率(计算)", format!("{fps:.3} fps"), None, note));
        }
    }
    let s = br.bit_pos();
    let nal_hrd = br.read_bit()? == 1;
    add_field(gv, "nal_hrd_parameters_present_flag", bool_str(nal_hrd), s, br.bit_pos(), 8, "");
    if nal_hrd {
        parse_hrd(br, gv, "nal_hrd")?;
    }
    let s = br.bit_pos();
    let vcl_hrd = br.read_bit()? == 1;
    add_field(gv, "vcl_hrd_parameters_present_flag", bool_str(vcl_hrd), s, br.bit_pos(), 8, "");
    if vcl_hrd {
        parse_hrd(br, gv, "vcl_hrd")?;
    }
    if nal_hrd || vcl_hrd {
        let s = br.bit_pos();
        let low_delay = br.read_bit()? == 1;
        add_field(gv, "low_delay_hrd_flag", bool_str(low_delay), s, br.bit_pos(), 8, "");
    }
    let s = br.bit_pos();
    let pic_struct = br.read_bit()? == 1;
    add_field(gv, "pic_struct_present_flag", bool_str(pic_struct), s, br.bit_pos(), 8, "");
    let s = br.bit_pos();
    let restriction = br.read_bit()? == 1;
    add_field(gv, "bitstream_restriction_flag", bool_str(restriction), s, br.bit_pos(), 8, "");
    if restriction {
        let s = br.bit_pos();
        let mv_over_bound = br.read_bit()? == 1;
        add_field(gv, "motion_vectors_over_pic_boundaries_flag", bool_str(mv_over_bound), s, br.bit_pos(), 8, "");
        let s = br.bit_pos();
        let b1 = br.read_ue()?;
        let b2 = br.read_ue()?;
        add_field(
            gv,
            "max_bytes_per_pic_denom / max_bits_per_mb_denom",
            format!("{b1} / {b2}"),
            s,
            br.bit_pos(),
            8,
            "",
        );
        let s = br.bit_pos();
        let b3 = br.read_ue()?;
        let b4 = br.read_ue()?;
        add_field(
            gv,
            "log2_max_mv_length(h/v)",
            format!("{b3} / {b4}"),
            s,
            br.bit_pos(),
            8,
            "",
        );
        let s = br.bit_pos();
        let b5 = br.read_ue()?;
        let b6 = br.read_ue()?;
        add_field(
            gv,
            "max_num_reorder_frames / max_dec_frame_buffering",
            format!("{b5} / {b6}"),
            s,
            br.bit_pos(),
            8,
            "重排帧数 / DPB 深度（B 帧相关）",
        );
    }
    Ok(())
}

fn bool_str(v: bool) -> String {
    if v { "1".into() } else { "0".into() }
}

pub fn parse_sps(body: &[u8]) -> Result<SpsParse, String> {
    if body.is_empty() {
        return Err("SPS 载荷为空".into());
    }
    let mut br = BitReader::new(body);
    let mut sps = Sps::default();
    let mut g1 = FieldGroup::new("SPS · 基础信息");
    let mut g2 = FieldGroup::new("SPS · 高档次扩展");
    let mut g3 = FieldGroup::new("SPS · 编码结构");
    let mut g4 = FieldGroup::new("SPS · 图像尺寸");
    let mut gv = FieldGroup::new("SPS · VUI 视频可用性信息");

    let s = br.bit_pos();
    let profile_idc = br.read_bits(8)? as u8;
    sps.profile_idc = profile_idc;
    add_field(&mut g1, "profile_idc", profile_name(profile_idc), s, br.bit_pos(), 8, "档次：决定允许使用的编码工具");

    for (i, name) in [
        "constraint_set0_flag",
        "constraint_set1_flag",
        "constraint_set2_flag",
        "constraint_set3_flag",
        "constraint_set4_flag",
        "constraint_set5_flag",
    ]
    .iter()
    .enumerate()
    {
        let s = br.bit_pos();
        let v = br.read_bit()? == 1;
        match i {
            0 => sps.constraint_set0 = v,
            1 => sps.constraint_set1 = v,
            2 => sps.constraint_set2 = v,
            3 => sps.constraint_set3 = v,
            4 => sps.constraint_set4 = v,
            5 => sps.constraint_set5 = v,
            _ => {}
        }
        let desc = match i {
            1 => "1 时为 Constrained Baseline/Main（嵌入式解码器常要求）",
            3 => "与 Level 1b 有关",
            5 => "1 表示码流仅含帧（无场）",
            _ => "",
        };
        add_field(&mut g1, name, bool_str(v), s, br.bit_pos(), 8, desc);
    }
    let s = br.bit_pos();
    let reserved = br.read_bits(2)?;
    add_field(&mut g1, "reserved_zero_2bits", format!("{reserved}"), s, br.bit_pos(), 8, "");
    let s = br.bit_pos();
    let level_idc = br.read_bits(8)? as u8;
    sps.level_idc = level_idc;
    add_field(&mut g1, "level_idc", level_name(level_idc), s, br.bit_pos(), 8, "级别：分辨率/码率上限");
    let s = br.bit_pos();
    let sps_id = br.read_ue()?;
    sps.seq_parameter_set_id = sps_id;
    add_field(&mut g1, "seq_parameter_set_id", format!("{sps_id}"), s, br.bit_pos(), 8, "");

    if is_high_profile(profile_idc) {
        let s = br.bit_pos();
        let chroma = br.read_ue()?;
        sps.chroma_format_idc = chroma;
        add_field(&mut g2, "chroma_format_idc", chroma_format_name(chroma), s, br.bit_pos(), 8, "色度采样（高档次才显式出现）");
        if chroma == 3 {
            let s = br.bit_pos();
            let v = br.read_bit()? == 1;
            sps.separate_colour_plane_flag = v;
            add_field(&mut g2, "separate_colour_plane_flag", bool_str(v), s, br.bit_pos(), 8, "");
        }
        let s = br.bit_pos();
        let bd_l = br.read_ue()?;
        let bd_c = br.read_ue()?;
        sps.bit_depth_luma_minus8 = bd_l;
        sps.bit_depth_chroma_minus8 = bd_c;
        add_field(&mut g2, "bit_depth_luma/chroma_minus8", format!("{bd_l}/{bd_c}"), s, br.bit_pos(), 8, "位深-8（10bit 时为 2）");
        let s = br.bit_pos();
        let bypass = br.read_bit()? == 1;
        add_field(&mut g2, "qpprime_y_zero_transform_bypass_flag", bool_str(bypass), s, br.bit_pos(), 8, "");
        let s = br.bit_pos();
        let scaling_present = br.read_bit()? == 1;
        add_field(&mut g2, "seq_scaling_matrix_present_flag", bool_str(scaling_present), s, br.bit_pos(), 8, "");
        if scaling_present {
            let count = if chroma == 3 { 12 } else { 8 };
            for i in 0..count {
                let s = br.bit_pos();
                let present = br.read_bit()? == 1;
                add_field(&mut g2, &format!("seq_scaling_list_present_flag[{i}]"), bool_str(present), s, br.bit_pos(), 8, "");
                if present {
                    let size = if i < 6 { 16 } else { 64 };
                    parse_scaling_list(&mut br, size, &mut g2, &format!("list{i}"))?;
                }
            }
        }
    } else {
        sps.chroma_format_idc = 1;
    }

    let s = br.bit_pos();
    let log2_fn = br.read_ue()?;
    sps.log2_max_frame_num_minus4 = log2_fn;
    add_field(&mut g3, "log2_max_frame_num_minus4", format!("{log2_fn}"), s, br.bit_pos(), 8, &format!("frame_num 位宽 = {}", log2_fn as u64 + 4));

    let s = br.bit_pos();
    let poc_type = br.read_ue()?;
    sps.pic_order_cnt_type = poc_type;
    add_field(&mut g3, "pic_order_cnt_type", poc_type_name(poc_type), s, br.bit_pos(), 8, "POC 计算方式（部分硬件仅支持特定类型）");
    match poc_type {
        0 => {
            let s = br.bit_pos();
            let log2_poc = br.read_ue()?;
            sps.log2_max_pic_order_cnt_lsb_minus4 = Some(log2_poc);
            add_field(&mut g3, "log2_max_pic_order_cnt_lsb_minus4", format!("{log2_poc}"), s, br.bit_pos(), 8, "");
        }
        1 => {
            let s = br.bit_pos();
            let delta_always_zero = br.read_bit()? == 1;
            sps.delta_pic_order_always_zero_flag = Some(delta_always_zero);
            let non_ref = br.read_se()?;
            let top_bottom = br.read_se()?;
            let cycle = br.read_ue()?;
            add_field(&mut g3, "delta_pic_order_always_zero_flag", bool_str(delta_always_zero), s, br.bit_pos(), 8, "");
            add_field(&mut g3, "offset_for_non_ref_pic", format!("{non_ref}"), s, br.bit_pos(), 8, "");
            add_field(&mut g3, "offset_for_top_to_bottom_field", format!("{top_bottom}"), s, br.bit_pos(), 8, "");
            add_field(&mut g3, "num_ref_frames_in_pic_order_cnt_cycle", format!("{cycle}"), s, br.bit_pos(), 8, "");
            if cycle > 255 {
                return Err(format!("num_ref_frames_in_pic_order_cnt_cycle={cycle} 异常过大（>255），疑似码流损坏"));
            }
            for i in 0..cycle {
                let s = br.bit_pos();
                let v = br.read_se()?;
                add_field(&mut g3, &format!("offset_for_ref_frame[{i}]"), format!("{v}"), s, br.bit_pos(), 8, "");
            }
        }
        _ => {}
    }

    let s = br.bit_pos();
    let max_ref = br.read_ue()?;
    sps.max_num_ref_frames = max_ref;
    add_field(&mut g3, "max_num_ref_frames", format!("{max_ref}"), s, br.bit_pos(), 8, "参考帧数（影响 DPB 内存）");
    let s = br.bit_pos();
    let gaps = br.read_bit()? == 1;
    sps.gaps_in_frame_num_value_allowed_flag = gaps;
    add_field(&mut g3, "gaps_in_frame_num_value_allowed_flag", bool_str(gaps), s, br.bit_pos(), 8, "");

    let s = br.bit_pos();
    let w_mbs = br.read_ue()?;
    let h_units = br.read_ue()?;
    sps.pic_width_in_mbs_minus1 = w_mbs;
    sps.pic_height_in_map_units_minus1 = h_units;
    add_field(&mut g4, "pic_width_in_mbs_minus1", format!("{w_mbs}"), s, br.bit_pos(), 8, &format!("= {} 宏块列", w_mbs as u64 + 1));
    add_field(&mut g4, "pic_height_in_map_units_minus1", format!("{h_units}"), s, br.bit_pos(), 8, &format!("= {} 宏块行", h_units as u64 + 1));

    let s = br.bit_pos();
    let frame_only = br.read_bit()? == 1;
    sps.frame_mbs_only_flag = frame_only;
    add_field(&mut g4, "frame_mbs_only_flag", bool_str(frame_only), s, br.bit_pos(), 8, "0 表示允许场编码（隔行）");
    if !frame_only {
        let s = br.bit_pos();
        let adaptive = br.read_bit()? == 1;
        sps.mb_adaptive_frame_field_flag = Some(adaptive);
        add_field(&mut g4, "mb_adaptive_frame_field_flag", bool_str(adaptive), s, br.bit_pos(), 8, "");
    }
    let s = br.bit_pos();
    let direct = br.read_bit()? == 1;
    sps.direct_8x8_inference_flag = direct;
    add_field(&mut g4, "direct_8x8_inference_flag", bool_str(direct), s, br.bit_pos(), 8, "");
    let s = br.bit_pos();
    let cropping = br.read_bit()? == 1;
    sps.frame_cropping_flag = cropping;
    add_field(&mut g4, "frame_cropping_flag", bool_str(cropping), s, br.bit_pos(), 8, "");
    let (mut cl, mut cr, mut ct, mut cb) = (0u32, 0u32, 0u32, 0u32);
    if cropping {
        let s = br.bit_pos();
        cl = br.read_ue()?;
        cr = br.read_ue()?;
        ct = br.read_ue()?;
        cb = br.read_ue()?;
        sps.frame_crop_left_offset = cl;
        sps.frame_crop_right_offset = cr;
        sps.frame_crop_top_offset = ct;
        sps.frame_crop_bottom_offset = cb;
        add_field(&mut g4, "frame_crop offsets(L/R/T/B)", format!("{cl}/{cr}/{ct}/{cb}"), s, br.bit_pos(), 8, "");
    }

    // 计算实际分辨率（u64 饱和运算，损坏码流下不 panic/回绕）
    let crop_unit_x = if sps.chroma_format_idc == 0 { 1u64 } else { 2 };
    let crop_unit_y = crop_unit_x * if frame_only { 1 } else { 2 };
    let width64 = (u64::from(w_mbs) + 1) * 16;
    let height64 = (2 - u64::from(frame_only)) * (u64::from(h_units) + 1) * 16;
    let width64 = if cropping {
        width64.saturating_sub((u64::from(cl) + u64::from(cr)) * crop_unit_x)
    } else {
        width64
    };
    let height64 = if cropping {
        height64.saturating_sub((u64::from(ct) + u64::from(cb)) * crop_unit_y)
    } else {
        height64
    };
    // 超出合理范围（单边 > 65536，远超 Level 6.2 上限）视为码流异常
    let sane = width64 > 0 && height64 > 0 && width64 <= 65536 && height64 <= 65536;
    let (width, height) = if sane {
        (width64 as u32, height64 as u32)
    } else {
        (0, 0)
    };
    sps.width = width;
    sps.height = height;
    if sane {
        g4.push(Field::new("分辨率(计算)", format!("{width}x{height}"), None, ""));
    } else {
        g4.push(Field::new(
            "分辨率(计算)",
            format!("原始值 {width64}x{height64}（超出合理范围，码流可能损坏）"),
            None,
            "",
        ));
    }

    let s = br.bit_pos();
    let vui_present = br.read_bit()? == 1;
    sps.vui_parameters_present_flag = vui_present;
    add_field(&mut g4, "vui_parameters_present_flag", bool_str(vui_present), s, br.bit_pos(), 8, "VUI 含帧率/宽高比/HRD 等");
    if vui_present {
        if let Err(e) = parse_vui(&mut br, &mut gv, &mut sps) {
            gv.note = Some(format!("VUI 解析中断：{e}"));
        }
    }

    let mut groups = vec![g1];
    if is_high_profile(profile_idc) {
        groups.push(g2);
    }
    groups.push(g3);
    groups.push(g4);
    if vui_present {
        groups.push(gv);
    }
    Ok(SpsParse { sps, groups })
}
