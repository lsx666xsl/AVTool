//! RBSP 比特级读取器：H.264 的语法元素都是比特流上的变长字段。

#[derive(Debug)]
pub struct BitReader<'a> {
    data: &'a [u8],
    /// 当前读取位置（比特）
    pos: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        BitReader { data, pos: 0 }
    }

    pub fn bit_pos(&self) -> usize {
        self.pos
    }

    pub fn total_bits(&self) -> usize {
        self.data.len() * 8
    }

    pub fn remaining_bits(&self) -> usize {
        self.total_bits().saturating_sub(self.pos)
    }

    pub fn has_at_least(&self, bits: usize) -> bool {
        self.remaining_bits() >= bits
    }

    pub fn read_bit(&mut self) -> Result<u32, String> {
        if self.pos >= self.total_bits() {
            return Err(format!(
                "比特流越界: 位置 {} 超出总长 {}",
                self.pos,
                self.total_bits()
            ));
        }
        let byte = self.data[self.pos / 8];
        let bit = (byte >> (7 - (self.pos % 8))) & 1;
        self.pos += 1;
        Ok(bit as u32)
    }

    /// 读取 n 位（大端序），n <= 32。
    pub fn read_bits(&mut self, n: u32) -> Result<u32, String> {
        if n > 32 {
            return Err(format!("单次最多读 32 位，收到 {n}"));
        }
        let mut v: u32 = 0;
        for _ in 0..n {
            v = (v << 1) | self.read_bit()?;
        }
        Ok(v)
    }

    /// 前进 n 比特（不校验内容），用于跳过整段载荷。
    pub fn skip_bits(&mut self, n: usize) {
        self.pos = (self.pos + n).min(self.total_bits());
    }

    /// 无符号指数哥伦布编码 ue(v)
    pub fn read_ue(&mut self) -> Result<u32, String> {
        let mut zeros = 0u32;
        while self.read_bit()? == 0 {
            zeros += 1;
            if zeros >= 32 {
                return Err("ue(v) 前导零超过 31，疑似损坏码流".into());
            }
        }
        if zeros == 0 {
            return Ok(0);
        }
        let suffix = self.read_bits(zeros)?;
        Ok((1u32 << zeros) - 1 + suffix)
    }

    /// 有符号指数哥伦布编码 se(v)
    pub fn read_se(&mut self) -> Result<i32, String> {
        let ue = self.read_ue()? as i32;
        let k = (ue + 1) / 2;
        Ok(if ue % 2 == 0 { -k } else { k })
    }

    /// `more_rbsp_data()`：当前位置之后是否还有除 rbsp_trailing_bits 之外的数据。
    /// rbsp_trailing_bits = 最后一个为 1 的比特（stop bit）+ 其后的 0。
    /// 比特位采用 MSB-first 约定：字节内最高位是 bit 0。
    pub fn more_rbsp_data(&self) -> bool {
        let mut last_set: Option<usize> = None;
        for (i, &b) in self.data.iter().enumerate() {
            if b != 0 {
                // 读取顺序（MSB-first）中最后一个为 1 的比特 = 字节内 7 - trailing_zeros
                last_set = Some(i * 8 + 7 - b.trailing_zeros() as usize);
            }
        }
        match last_set {
            Some(last) => self.pos < last,
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_bits_basic() {
        let data = [0b1011_0010, 0b0100_0000];
        let mut br = BitReader::new(&data);
        assert_eq!(br.read_bit().unwrap(), 1);
        assert_eq!(br.read_bits(3).unwrap(), 0b011);
        assert_eq!(br.read_bits(4).unwrap(), 0b0010);
        assert_eq!(br.read_bits(8).unwrap(), 0b0100_0000);
    }

    #[test]
    fn ue_se_roundtrip() {
        use crate::h264::test_util::BitWriter;
        let mut w = BitWriter::new();
        let ue_vals: Vec<u32> = (0..=40).collect();
        let se_vals: Vec<i32> = (-20..=20).collect();
        for &v in &ue_vals {
            w.ue(v);
        }
        for &v in &se_vals {
            w.se(v);
        }
        w.rbsp_trailing();
        let bytes = w.to_bytes();
        let mut br = BitReader::new(&bytes);
        for &v in &ue_vals {
            assert_eq!(br.read_ue().unwrap(), v, "ue roundtrip {v}");
        }
        for &v in &se_vals {
            assert_eq!(br.read_se().unwrap(), v, "se roundtrip {v}");
        }
        assert!(!br.more_rbsp_data());
    }

    #[test]
    fn more_rbsp_data_semantics() {
        // 1 0 1 0 0000：stop bit 在 bit2；读掉两个 ue(0)、ue(1) 后应无更多数据
        let data = [0b1010_0000];
        let mut br = BitReader::new(&data);
        assert!(br.more_rbsp_data()); // stop bit (bit2) 在前方
        assert_eq!(br.read_ue().unwrap(), 0); // bit0
        assert!(br.more_rbsp_data()); // bit2 的 1 还在
        assert_eq!(br.read_ue().unwrap(), 1); // bits1-2: 0,1
        assert!(!br.more_rbsp_data()); // 只剩 trailing zeros
    }

    #[test]
    fn more_rbsp_data_at_stop_bit() {
        // 仅 stop bit：[1] + 补零
        let data = [0b1000_0000];
        let mut br = BitReader::new(&data);
        assert!(!br.more_rbsp_data()); // 当前就停在 stop bit 前？
        // 注：pos=0 时 stop bit 位于 bit0，"当前位之后"没有数据
        br.read_bit().unwrap();
        assert!(!br.more_rbsp_data());
    }

    #[test]
    fn more_rbsp_data_multibyte() {
        // 第二字节最高位（全局 bit 8）是 stop bit
        let data = [0b1010_1010, 0b1000_0000];
        let mut br = BitReader::new(&data);
        assert!(br.more_rbsp_data());
        br.read_bits(8).unwrap();
        assert!(!br.more_rbsp_data()); // pos=8 恰好停在 stop bit
    }
}
