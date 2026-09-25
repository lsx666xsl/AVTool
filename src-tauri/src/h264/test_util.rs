//! 测试辅助：比特写入器与合成测试码流（仅在 cargo test 下编译）。

pub struct BitWriter {
    bits: Vec<bool>,
}

impl Default for BitWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl BitWriter {
    pub fn new() -> Self {
        BitWriter { bits: Vec::new() }
    }

    /// 写 n 位无符号整数
    pub fn u(&mut self, v: u32, n: u32) {
        for i in (0..n).rev() {
            self.bits.push(((v >> i) & 1) == 1);
        }
    }

    /// 无符号指数哥伦布 ue(v)
    pub fn ue(&mut self, v: u32) {
        let val = v + 1;
        let n = 32 - val.leading_zeros();
        for _ in 0..(n - 1) {
            self.bits.push(false);
        }
        self.u(val, n);
    }

    /// 有符号指数哥伦布 se(v)
    pub fn se(&mut self, v: i32) {
        let ue_val = if v <= 0 { (-v) as u32 * 2 } else { v as u32 * 2 - 1 };
        self.ue(ue_val);
    }

    /// 追加 rbsp_trailing_bits（stop bit + 对齐 0）
    pub fn rbsp_trailing(&mut self) {
        self.bits.push(true);
        while self.bits.len() % 8 != 0 {
            self.bits.push(false);
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = vec![0u8; self.bits.len().div_ceil(8)];
        for (i, &b) in self.bits.iter().enumerate() {
            if b {
                out[i / 8] |= 1 << (7 - (i % 8));
            }
        }
        out
    }
}

fn nal(nal_header: u8, body: &[u8]) -> Vec<u8> {
    let mut v = vec![nal_header];
    v.extend_from_slice(body);
    v
}

/// SPS：Constrained Baseline / Level 3.0 / 320x240 / POC type 2 / 1 参考帧
pub fn build_test_sps_nalu(level_idc: u8) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.u(66, 8); // profile_idc = Baseline
    w.u(1, 1); // constraint_set0
    w.u(1, 1); // constraint_set1 -> Constrained Baseline
    w.u(0, 4); // constraint_set2..5
    w.u(0, 2); // reserved
    w.u(level_idc as u32, 8);
    w.ue(0); // sps_id
    w.ue(0); // log2_max_frame_num_minus4
    w.ue(2); // pic_order_cnt_type = 2
    w.ue(1); // max_num_ref_frames
    w.ue(0); // gaps
    w.ue(19); // pic_width_in_mbs_minus1 -> 320
    w.ue(14); // pic_height_in_map_units_minus1 -> 240
    w.u(1, 1); // frame_mbs_only_flag
    w.u(1, 1); // direct_8x8_inference_flag
    w.u(0, 1); // frame_cropping_flag
    w.u(0, 1); // vui_parameters_present_flag
    w.rbsp_trailing();
    nal(0x67, &w.to_bytes())
}

/// PPS：CAVLC / 单切片组 / 无加权预测
pub fn build_test_pps_nalu() -> Vec<u8> {
    let mut w = BitWriter::new();
    w.ue(0); // pps_id
    w.ue(0); // sps_id
    w.u(0, 1); // entropy_coding_mode_flag = CAVLC
    w.u(0, 1); // bottom_field_pic_order_in_frame_present_flag
    w.ue(0); // num_slice_groups_minus1
    w.ue(0); // num_ref_idx_l0_default_active_minus1
    w.ue(0); // num_ref_idx_l1_default_active_minus1
    w.u(0, 1); // weighted_pred_flag
    w.u(0, 2); // weighted_bipred_idc
    w.se(0); // pic_init_qp_minus26
    w.se(0); // pic_init_qs_minus26
    w.se(0); // chroma_qp_index_offset
    w.u(0, 1); // deblocking_filter_control_present_flag
    w.u(0, 1); // constrained_intra_pred_flag
    w.u(0, 1); // redundant_pic_cnt_present_flag
    w.rbsp_trailing();
    nal(0x68, &w.to_bytes())
}

/// IDR 切片（I 帧全部切片）
pub fn build_test_idr_slice_nalu() -> Vec<u8> {
    let mut w = BitWriter::new();
    w.ue(0); // first_mb_in_slice
    w.ue(7); // slice_type = I（全部切片）
    w.ue(0); // pps_id
    w.u(0, 4); // frame_num = 0
    w.ue(0); // idr_pic_id
    // poc type 2: 无 POC 字段
    w.u(0, 1); // no_output_of_prior_pics_flag
    w.u(0, 1); // long_term_reference_flag
    w.se(0); // slice_qp_delta
    w.rbsp_trailing();
    nal(0x65, &w.to_bytes())
}

/// P 切片（非 IDR 参考帧）
pub fn build_test_p_slice_nalu(frame_num: u32) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.ue(0); // first_mb_in_slice
    w.ue(5); // slice_type = P（仅本切片）
    w.ue(0); // pps_id
    w.u(frame_num, 4); // frame_num
    // P 切片: num_ref_idx_active_override_flag
    w.u(0, 1);
    // P 切片必有 ref_pic_list_modification_flag_l0
    w.u(0, 1);
    // dec_ref_pic_marking（nal_ref_idc != 0，非 IDR）
    w.u(0, 1); // adaptive_ref_pic_marking_mode_flag
    w.se(0); // slice_qp_delta
    w.rbsp_trailing();
    nal(0x41, &w.to_bytes())
}

/// 合成一个完整码流：AUD + SPS + PPS + IDR + 2×P
pub fn build_test_stream(level_idc: u8) -> Vec<u8> {
    let mut v: Vec<u8> = Vec::new();
    v.extend_from_slice(&[0x00, 0x00, 0x00, 0x01, 0x09, 0x10]); // AUD
    v.extend_from_slice(&[0x00, 0x00, 0x01]);
    v.extend(build_test_sps_nalu(level_idc));
    v.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
    v.extend(build_test_pps_nalu());
    v.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
    v.extend(build_test_idr_slice_nalu());
    for fn_ in 1..3u32 {
        v.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
        v.extend(build_test_p_slice_nalu(fn_));
    }
    v
}
