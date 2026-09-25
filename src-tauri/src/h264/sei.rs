//! SEI（补充增强信息）解析：v1 输出载荷类型清单与常见载荷的内容摘要。

use super::bitreader::BitReader;
use super::common::{Field, FieldGroup};

pub fn sei_type_name(t: u32) -> String {
    let name = match t {
        0 => "缓冲周期 buffering_period",
        1 => "图像时序 pic_timing",
        2 => "平移扫描 pan_scan_rect",
        3 => "填充数据 filler_data",
        4 => "用户数据 ITU-T T.35",
        5 => "用户数据未注册 user_data_unregistered",
        6 => "恢复点 recovery_point",
        7 => "参考图像标记重复 dec_ref_pic_marking_repetition",
        8 => "SPS/PPS 时序 sps_pps_timing",
        9..=16 => "保留",
        17 => "渐进式细化分段起始",
        18 => "渐进式细化分段结束",
        19 => "运动约束切片组",
        20 => "Film Grain 特性",
        21 => "去块滤波显示",
        22 => "立体帧信息",
        23 => "帧打包",
        24 => "显示方向",
        25 => "3D 位移",
        26 => "深度信息",
        27..=28 => "保留",
        29..=31 => "保留",
        32 => "场景信息",
        33 => "子序列信息",
        34 => "子序列层信息",
        37 => "渐进细化扇区信息",
        45 => "帧间距立体信息",
        46 => "立体适配",
        47 => "内容色域",
        48..=511 => "保留",
        _ => "保留/未定义",
    };
    format!("{t} · {name}")
}

/// 读取 SEI 的 ff 编码字节序列（0xFF 表示 +255，最后小于 0xFF 的字节结束）
fn read_ff_number(br: &mut BitReader) -> Result<u32, String> {
    let mut value = 0u32;
    loop {
        if !br.has_at_least(8) {
            return Err("SEI 编码字节不完整".into());
        }
        let b = br.read_bits(8)?;
        value = value.saturating_add(b);
        if value > (1 << 26) {
            return Err("SEI 编码数值异常过大（>64M），疑似损坏码流".into());
        }
        if b != 0xFF {
            return Ok(value);
        }
    }
}

fn ascii_sniff(data: &[u8]) -> Option<String> {
    if data.is_empty() {
        return None;
    }
    let printable = data.iter().filter(|&&b| (0x20..=0x7E).contains(&b)).count();
    if printable * 10 >= data.len() * 7 {
        let end = data.len().min(96);
        let s: String = data[..end]
            .iter()
            .map(|&b| if (0x20..=0x7E).contains(&b) { b as char } else { '·' })
            .collect();
        let suffix = if data.len() > end { "…" } else { "" };
        return Some(format!("\"{s}{suffix}\""));
    }
    None
}

fn describe_payload(ptype: u32, payload: &[u8]) -> String {
    match ptype {
        5 => {
            let (uuid, rest) = payload.split_at(payload.len().min(16));
            let uuid_hex: String = uuid.iter().map(|b| format!("{b:02X}")).collect();
            let text = ascii_sniff(rest)
                .map(|s| format!("，内容 {s}"))
                .unwrap_or_default();
            format!("UUID={uuid_hex}{text}")
        }
        4 => {
            let country = payload.first().copied().unwrap_or(0);
            let text = ascii_sniff(&payload[1.min(payload.len())..])
                .map(|s| format!("，内容 {s}"))
                .unwrap_or_default();
            format!("国家码={country}{text}")
        }
        3 => {
            let all_zero = payload.iter().all(|&b| b == 0);
            format!("{} 字节{}", payload.len(), if all_zero { "全 0x00" } else { "（非零填充，异常）" })
        }
        6 => {
            let mut br = BitReader::new(payload);
            match (|| -> Result<String, String> {
                let cnt = br.read_ue()?;
                let exact = br.read_bit()?;
                let broken = br.read_bit()?;
                let changing = br.read_bits(2)?;
                Ok(format!("recovery_frame_cnt={cnt}, exact={exact}, broken={broken}, changing={changing}"))
            })() {
                Ok(s) => s,
                Err(_) => format!("{} 字节", payload.len()),
            }
        }
        _ => format!("{} 字节", payload.len()),
    }
}

pub fn parse_sei(body: &[u8]) -> (Vec<FieldGroup>, Vec<String>) {
    let mut br = BitReader::new(body);
    let mut g = FieldGroup::new("SEI · 载荷列表");
    let mut warnings = Vec::new();
    let mut count = 0usize;

    while br.more_rbsp_data() {
        if count >= 32 {
            warnings.push("SEI 载荷超过 32 个，仅列出前 32 个".into());
            break;
        }
        let ptype = match read_ff_number(&mut br) {
            Ok(v) => v,
            Err(e) => {
                warnings.push(e);
                break;
            }
        };
        let psize = match read_ff_number(&mut br) {
            Ok(v) => v,
            Err(e) => {
                warnings.push(e);
                break;
            }
        };
        if br.remaining_bits() < psize as usize * 8 {
            warnings.push(format!("SEI 载荷 {ptype} 声明 {psize} 字节，超出剩余数据，解析停止"));
            break;
        }
        let start_bit = br.bit_pos();
        let payload = &body[start_bit / 8..start_bit / 8 + psize as usize];
        br.skip_bits(psize as usize * 8);
        g.push(Field::new(
            &format!("载荷#{count} {t}", t = sei_type_name(ptype)),
            describe_payload(ptype, payload),
            Some(format!("bit {start_bit} · {}字节", psize)),
            "",
        ));
        count += 1;
    }
    if count == 0 {
        g.note = Some("未解析出任何 SEI 载荷".into());
    }
    (vec![g], warnings)
}
