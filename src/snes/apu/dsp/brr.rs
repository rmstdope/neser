#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedBrrBlock {
    pub samples: [i16; 16],
    pub loop_flag: bool,
    pub end_flag: bool,
}

#[must_use]
pub fn decode_brr_block(header: u8, data: [u8; 8], prev1: i16, prev2: i16) -> DecodedBrrBlock {
    let shift = (header >> 4) & 0x0F;
    let filter = (header >> 2) & 0x03;
    let mut hist1 = prev1;
    let mut hist2 = prev2;
    let mut samples = [0i16; 16];
    for (i, byte) in data.iter().copied().enumerate() {
        let hi = ((byte >> 4) & 0x0F) as i8;
        let lo = (byte & 0x0F) as i8;
        let s0 = decode_brr_nibble(sign_extend_nibble(hi), shift, filter, hist1, hist2);
        hist2 = hist1;
        hist1 = s0;
        samples[i * 2] = s0;

        let s1 = decode_brr_nibble(sign_extend_nibble(lo), shift, filter, hist1, hist2);
        hist2 = hist1;
        hist1 = s1;
        samples[i * 2 + 1] = s1;
    }
    DecodedBrrBlock {
        samples,
        loop_flag: header & 0x02 != 0,
        end_flag: header & 0x01 != 0,
    }
}

fn sign_extend_nibble(value: i8) -> i16 {
    let widened = (value << 4) >> 4;
    i16::from(widened)
}

fn decode_brr_nibble(raw: i16, shift: u8, filter: u8, prev1: i16, prev2: i16) -> i16 {
    let base = if shift > 12 {
        if raw >= 0 { 0 } else { -2048 }
    } else {
        i32::from(raw) << shift
    };
    let predict = match filter {
        0 => 0,
        1 => (i32::from(prev1) * 15) >> 4,
        2 => ((i32::from(prev1) * 61) >> 5) - ((i32::from(prev2) * 15) >> 4),
        _ => ((i32::from(prev1) * 115) >> 6) - ((i32::from(prev2) * 13) >> 4),
    };
    (base + predict).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}
