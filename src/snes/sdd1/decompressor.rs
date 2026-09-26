//! The S-DD1 decompressor: fullsnes "SNES Cart S-DD1 Decompression Algorithm", ported
//! routine for routine (`decompress_init`, `decompress_byte`, `GetBit`, `ProbGetBit`,
//! `GetCodeword`) with its four tables.
//!
//! Input bytes are fetched through a caller-supplied reader, so a compressed stream that
//! crosses a 1 MiB bank follows the S-DD1's own bank registers, as on the cartridge.

use serde::{Deserialize, Serialize};

/// fullsnes `EvolutionCodeSize`: the Golomb code order used in each probability state.
const EVOLUTION_CODE_SIZE: [u8; 33] = [
    0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, //
    4, 4, 5, 5, 6, 6, 7, 7, 0, 1, 2, 3, 4, 5, 6, 7,
];

/// fullsnes `EvolutionMpsNext`: the state after a run ends on the most probable symbol.
const EVOLUTION_MPS_NEXT: [u8; 33] = [
    25, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, //
    18, 19, 20, 21, 22, 23, 24, 24, 26, 27, 28, 29, 30, 31, 32, 24,
];

/// fullsnes `EvolutionLpsNext`: the state after a run ends on the least probable symbol.
const EVOLUTION_LPS_NEXT: [u8; 33] = [
    25, 1, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, //
    16, 17, 18, 19, 20, 21, 22, 23, 1, 2, 4, 8, 12, 16, 18, 22,
];

/// fullsnes `RunTable`: run length (plus the LPS marker convention of `ProbGetBit`) for each
/// 7-bit Golomb suffix.
#[rustfmt::skip]
const RUN_TABLE: [u8; 128] = [
    128, 64, 96, 32, 112, 48, 80, 16, 120, 56, 88, 24, 104, 40, 72, 8,
    124, 60, 92, 28, 108, 44, 76, 12, 116, 52, 84, 20, 100, 36, 68, 4,
    126, 62, 94, 30, 110, 46, 78, 14, 118, 54, 86, 22, 102, 38, 70, 6,
    122, 58, 90, 26, 106, 42, 74, 10, 114, 50, 82, 18,  98, 34, 66, 2,
    127, 63, 95, 31, 111, 47, 79, 15, 119, 55, 87, 23, 103, 39, 71, 7,
    123, 59, 91, 27, 107, 43, 75, 11, 115, 51, 83, 19,  99, 35, 67, 3,
    125, 61, 93, 29, 109, 45, 77, 13, 117, 53, 85, 21, 101, 37, 69, 5,
    121, 57, 89, 25, 105, 41, 73,  9, 113, 49, 81, 17,  97, 33, 65, 1,
];

/// Decompressor state between output bytes. Every field is plain data so a save state can
/// resume a transfer mid-stream. Field names follow fullsnes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sdd1Decompressor {
    /// Address of the next compressed byte to fetch.
    src: u32,
    /// The 16-bit bit window; the next bit is shifted into bit 15.
    input: u16,
    /// Bits still to shift before the low byte of `input` is empty.
    valid_bits: i8,
    /// 2, 4 or 8 bitplanes, or 0 for 8-bit raw bytes.
    num_planes: u8,
    high_context_bits: u16,
    low_context_bits: u16,
    /// Remaining run count per Golomb code order (bit 7 marks a run that ends on the MPS).
    bit_ctr: [u8; 8],
    prev_bits: [u16; 8],
    context_states: [u8; 32],
    context_mps: [u8; 32],
    plane: u8,
    yloc: u8,
    raw: u8,
}

impl Sdd1Decompressor {
    /// `decompress_init(src)`: reads the header byte at `addr` and primes the bit window.
    pub fn init(&mut self, addr: u32, read: &mut impl FnMut(u32) -> u8) {
        let header = read(addr);
        self.num_planes = match header & 0xC0 {
            0x00 => 2,
            0x40 => 8,
            0x80 => 4,
            _ => 0,
        };
        (self.high_context_bits, self.low_context_bits) = match header & 0x30 {
            0x00 => (0x01C0, 0x0001),
            0x10 => (0x0180, 0x0001),
            0x20 => (0x00C0, 0x0001),
            _ => (0x0180, 0x0003),
        };
        // fullsnes writes this `input=(input SHL 11) OR ([src+1] SHL 3)` after `src=src+1`;
        // the byte meant is the one straight after the header (the next reload then reads
        // `addr + 2`), which is what `valid_bits=5` accounts for.
        self.input = (u16::from(header) << 11) | (u16::from(read(addr.wrapping_add(1))) << 3);
        self.src = addr.wrapping_add(2);
        self.valid_bits = 5;
        self.bit_ctr = [0; 8];
        self.prev_bits = [0; 8];
        self.context_states = [0; 32];
        self.context_mps = [0; 32];
        self.plane = 0;
        self.yloc = 0;
        self.raw = 0;
    }

    /// `decompress_byte`: the next output byte.
    pub fn next_byte(&mut self, read: &mut impl FnMut(u32) -> u8) -> u8 {
        if self.num_planes == 0 {
            for plane in 0..8 {
                self.get_bit(plane, read);
            }
            self.raw
        } else if self.plane & 1 == 0 {
            for _ in 0..8 {
                self.get_bit(self.plane, read);
                self.get_bit(self.plane + 1, read);
            }
            let byte = self.prev_bits[usize::from(self.plane)] as u8;
            self.plane += 1;
            byte
        } else {
            let byte = self.prev_bits[usize::from(self.plane)] as u8;
            self.plane -= 1;
            self.yloc += 1;
            if self.yloc == 8 {
                self.yloc = 0;
                self.plane = (self.plane + 2) & (self.num_planes - 1);
            }
            byte
        }
    }

    /// `GetBit(plane)`.
    fn get_bit(&mut self, plane: u8, read: &mut impl FnMut(u32) -> u8) {
        let prev = self.prev_bits[usize::from(plane)];
        let context = (u16::from(plane & 1) << 4)
            | ((prev & self.high_context_bits) >> 5)
            | (prev & self.low_context_bits);
        let pbit = self.prob_get_bit(usize::from(context), read);
        self.prev_bits[usize::from(plane)] = (prev << 1) | u16::from(pbit);
        if self.num_planes == 0 {
            self.raw = (self.raw >> 1) | (pbit << 7);
        }
    }

    /// `ProbGetBit(context)`.
    fn prob_get_bit(&mut self, context: usize, read: &mut impl FnMut(u32) -> u8) -> u8 {
        let state = self.context_states[context];
        let code_size = EVOLUTION_CODE_SIZE[usize::from(state)];
        let counter = usize::from(code_size);
        if self.bit_ctr[counter] & 0x7F == 0 {
            self.bit_ctr[counter] = self.get_codeword(code_size, read);
        }
        let mut pbit = self.context_mps[context];
        self.bit_ctr[counter] = self.bit_ctr[counter].wrapping_sub(1);
        if self.bit_ctr[counter] == 0x00 {
            self.context_states[context] = EVOLUTION_LPS_NEXT[usize::from(state)];
            pbit ^= 1;
            if state < 2 {
                self.context_mps[context] = pbit;
            }
        } else if self.bit_ctr[counter] == 0x80 {
            self.context_states[context] = EVOLUTION_MPS_NEXT[usize::from(state)];
        }
        pbit
    }

    /// `GetCodeword(code_size)`.
    fn get_codeword(&mut self, code_size: u8, read: &mut impl FnMut(u32) -> u8) -> u8 {
        if self.valid_bits == 0 {
            self.input |= u16::from(self.fetch(read));
            self.valid_bits = 8;
        }
        self.input <<= 1;
        self.valid_bits -= 1;
        if self.input & 0x8000 == 0 {
            // A 128-bit run at order 7 is 80h+80h, which wraps the 8-bit counter to 00h.
            // `ProbGetBit` decrements before it compares, so 00h counts down FFh..80h and
            // ends the run after exactly 128 bits, as the unwrapped 100h would.
            return 0x80u8.wrapping_add(1 << code_size);
        }
        let index = ((self.input >> 8) & 0x7F) as u8 | (0x7F >> code_size);
        self.input <<= code_size;
        self.valid_bits -= code_size as i8;
        if self.valid_bits < 0 {
            self.input |= u16::from(self.fetch(read)) << (-self.valid_bits);
            self.valid_bits += 8;
        }
        RUN_TABLE[usize::from(index)]
    }

    fn fetch(&mut self, read: &mut impl FnMut(u32) -> u8) -> u8 {
        let byte = read(self.src);
        self.src = self.src.wrapping_add(1);
        byte
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decompress(input: &[u8], len: usize) -> Vec<u8> {
        let mut read = |addr: u32| input.get(addr as usize).copied().unwrap_or(0);
        let mut decompressor = Sdd1Decompressor::default();
        decompressor.init(0, &mut read);
        (0..len)
            .map(|_| decompressor.next_byte(&mut read))
            .collect()
    }

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
            .collect()
    }

    /// One vector per header high nibble: every plane count (2/8/4/raw) with every context
    /// selection. Input is pseudo-random; the expected output was produced by Mesen2's
    /// `Sdd1Decomp` (Andreas Naive's decoder, an implementation independent of the fullsnes
    /// formulation ported here), compiled standalone. No vector reads past its 96 bytes:
    /// each gives the same 64 output bytes when the stream is extended.
    const MESEN2_VECTORS: [(&str, &str); 16] = [
        (
            "03CD406C472661B26EB3B0B28B878B1C6711DFF272C201D997F62AD5CA9B7D3E6A287065260C01C574ED8375F31A18DEC488540C87F775B9F40EE65ED0A7BDD7E0298DEF263AA70BE958F26FECC32D4A8239682F833249A326297F31E5D80995",
            "777F283E45D983016EDA17958871552A8A1A89AEC09633F495790BAAEF985BDDAD251B7486EB7A49DE1321E23E83A3EAFECA597B231C9A93357F6D7FB5BFFBC0",
        ),
        (
            "1E930241D7345511EE02BBB868D279030722A0F5694D289984F6925E2F43B299306411B865383D3052AE22E0751081A36249C65BD8AFCDA4563A41F6DD9C970C2290CDA4609EE3263F585BF21E496BEA09F64BE550F30AB7327784DA10023D8E",
            "D9B5492179CACA007217C2D0D219D4DDD1E0461D7460B50D2A10EA2424CFB918A5E2AA0694D8A9E3A614296BE3807B3E1BB06AF642079CB83901EEFC6401F1F6",
        ),
        (
            "22C28D2C3766F810065CC8C2EDB5205574B905A7112445CEFC3043F1057F02043DE99D638B36C5CBECF45A58D3ECAB51AD8B0CB9F37328B6D01898559C82BFF94899A80DAD41744899A38BB0748B7D160B2985450BB4E83CAD21C3D68CF3513E",
            "7F165E6484DFF67AED6F4AF76B6D2AAF1508D179ADF62A7F8AFF67E4179B456F71CE2C3DAAFFAAF2927EDDFF553E57F46B372A1EB29B827F7BE7EAA14AD7AD9E",
        ),
        (
            "310D946BE3F392DF1308B7D74EABCB64EF99452713988AE1D61602ED92CE18A9348DE1C7AE72FD348FB5BF90BA9053CDB5E0155578484C3B3AB3EFC5750070FA7242B940778D22C3A743ECD9773A5671819D81D4E6232B40D2BA8AB47576B113",
            "004D17481D81731B1F44A7F6BDE6BC7012301820B41A82A10EC07C559D01FC61CC90CCBED58292A8E9AE64651DC1FC30FA2885104300320019004A0024004400",
        ),
        (
            "4652A1365248DC30C95A28D679C197A3269D543CA56AEE016F7A1B92EB819C6D2E86199343715424B5BC2E3A162A16DA7D28D0C5BDE3638091A1654824235D686903BBED62935BCAE3D0DC30740478882E30EE7B9C07CBA912BF628B0AD18673",
            "4780135826914CAD0299A96CA0EDF2CDDEF033A97481FDEBBA84F7A1115BA91240C98A5B27519005C1AA32A1E989F2D3194CFB2BF713CB83D7A1FD13FFA2F805",
        ),
        (
            "579EB5B79407176EECAB2FBEA600120C8A01D2F00438B5C3AFB19D4138673D3E2AF2D68CC9AA8093BBA756E12C31368A46F218960158E8237EED676CCAB6CA1394C43C0120D5C4062ED5DCA9585C169D963D6C50013A4CC6168D2F982ACEE234",
            "75ED3BF09176B0B39EFF0ACED0E742FF989F2BFF85A7A8F086763AB89967C8FFD4B550CA00E78F7385B99A1E8DFF27FE6E6FB49F18E76EFFB6FF15BF30DF19EF",
        ),
        (
            "61521C3E822F0E1512CE0AC9575F7F279B14BF0A3A6460BAE2A898FD8D0E7A15267AC66A96A1B577ED92C01DD4B099B3DD2B638B1C621AF00BC1F8D05BA75272962FDFFB4C6ABEDBA03F5059C3EC66FE0B7BB902F7FD58400A57E460E329DF0E",
            "01784AFB98FBA9F73BFF52FF14FF9DFF02D992B440BA00FAA876E17E64FEE8FF4181A78C6BD86589AB12DB243E284459485A11CD219B0272021402BB0C3199AF",
        ),
        (
            "736F9BACFEFF396AC6A8055B99FC65E5F02E37103AA1A7039F25C7F8704FD2C4800EF7249F3258950C348E04E7ACFA8414889AD0A64B99F37BB46AF271A8C1DCBDBB3419197572D9E122B38D1D4981EFDE43CBBC54A37C01E28FBA25B46B503E",
            "5D6AEBCB91B6D6B8FEDDFB8FDA8627621845DCB73856BE0B5C191DB5A6B0A2BB7F3166BAED61FFCCF7D8AF40C0D8F3160036DC3D0BB37D4E96BBC48EC10EC5C7",
        ),
        (
            "81FC05219559FBA8787AA25DC2905AEDF5862E6716D875FF4E738BF42BC4BB82961C703B8113A709CD232F753C9E1F440F9819A24142F3919CA2449340C4F9B0693A9C589582A480D9D9E2EE6E9A6CA5FEE32E0CA32398785A43A6C41C428281",
            "3060278C93DDC7F4287891EA22EE90635C810B69E091AE8CCE4E4AAF85CF76C7416320F094104208A44053370B0E8217B3E8C314242B965747280DAD164DA2DF",
        ),
        (
            "999E4C05FA675D7C2401707217A47A3ADB63A7BD35D63EEA03E1430F681D19E9DFB428E3D492D72D31557EDCACF3999FCDA69F25AC171207FC589ED0EEABF15DFCA625714571B21248540B0537292465ED4C0CCC00B595B64F4E55DE7DB6BC08",
            "9F58B38FE33217B476D9F762C9045F1526DD434F90855F84C8E058E2B8B760325470CF9CC0B8772AC0C075F2482C414D732A4CB760DD71764A5F9E5770F9C6D0",
        ),
        (
            "AF9E3A20F50A81CE8635629BDAA0CFDA9451A48A66770473458D7DBFBA1937EF80470E19D170E939832B70A7CD3A46C7346FF1061D3FB6C1EEB09D30F3A843D1799E22DA7A3EF3BE9B87E548731E0F9882D41B6BBF58DF1F39A990CE9D6054FC",
            "D5E528F1D5F12AF05507AA67D21F9D807645ADE15AE795E359F796F0AD7C757F7834767CA97C51E3AE2155685A236513A203DD1126B84ABDB5F556FD8DFF1AFD",
        ),
        (
            "BE28E6EF7CBA0EEDEBCFF22AF3D069DD14E4BF32A9C95D5B6FB1D434E3AEF523B24E8381C26E25607CC1A52BE1C10B1CCE717038471662DEFEA84625E9189C4B2922BEB3129D62C7808306FC20352E64736B394F2FAE71C989DB7CBC577E560C",
            "D591BEF1BF9322CA90A051E596D38293E789B92846641E09A75735FD4C6B7212CE25EAA1B2092C4FAC12CD9F36D0167F918AD0A1873F23F44D6D0E61DF5F36D5",
        ),
        (
            "CBF480B62BE1B08D4C506323E063ACB5CAB08DE9F54D9A79777794310E39BC37CE232717E10581585648CDF7DA6EFD365BF10CFFAA3C3BF4F71ADB5219EFDE0458858AD6AFA36B17F0D0FA198B6CD0E275B9D2733D6FFEC304A7B9F241EC0EBB",
            "6986D9206AA049AEFB0AFF93EE8BA6F1A3F5BBC8C6FC076EDF8F53D42D7EACB082E9A7BEC2C25C2BEA9EADFF5449A3E38C693E17EFAEDBEBB819C2EE722BC692",
        ),
        (
            "D8B445D241318AC9A8CF9A275722A2A109993EF6385072820C991617126AC125D79D34790409024683667B42FC394255DBBD5F1EA121D2148EF3CD9A5E813452D83DCBC99DC01523C14AE001D7799D35FEA40CEF9286CA25B45A50683F75A89C",
            "35D50B4C552065D905A15A54D27D60C810F41E75FBEDC5E84FECCD46D69C04BC8FB7A1338A016190433054529D2315D54B60456B85679D75FE2D09031577957F",
        ),
        (
            "E0CDE3B01E3F42D2E15A5F3298ABF62A2BFBEC69D864ECFDCD6425F6415075F4A6942405B3D7CBE15B1BD266200E2506F3FFAEE931E928C387B1ADE62759442935E5CFDB1C3C8B87EE860B92C87C44ADD632E89789E370218157E4193CB65DD1",
            "103D2361BB915CC33AFBBEA63E93C95C773D1BB9286E07EDDE332EC47BA88840BF5CFACA4406D8B9F3BA6193860B059E24EA597D803F872ADA9BD43421ED8335",
        ),
        (
            "F12C73B982CEBE7BA4CDD559402D99DBE75081BA6A2E8643953414A3B7B33DA09F4702F10BAE34B82FCF1345AE423FCED95AB77AFC353AED1684C2F77D58E2EECBC58F75E7D5D7F609BFFC4743051144664F3ED3DED9603252F69EE3E5BC353C",
            "88AAA822A83A9339828D05A737D644DE7D17B60EA0FF17B9A0D21B88165CC8D1607BC1680E7A705310582AB5D671E02B8B78E05A435D0B97B1F171E3CFB6209B",
        ),
    ];

    #[test]
    fn decompressor_matches_mesen2_for_every_plane_and_context_mode() {
        let failures: Vec<String> = MESEN2_VECTORS
            .iter()
            .filter_map(|&(input, expected)| {
                let actual = decompress(&hex(input), 64);
                (actual != hex(expected)).then(|| format!("header ${}: {actual:02X?}", &input[..2]))
            })
            .collect();
        assert!(failures.is_empty(), "mismatching modes: {failures:#?}");
    }

    /// Sparse streams (about one nonzero byte in 32) give long most-probable-symbol runs, so
    /// the probability states climb to Golomb order 7, where a run is 128 bits and the
    /// 8-bit run counter wraps. Expected output from Mesen2's `Sdd1Decomp`, as above; 512
    /// bytes each, the second in raw-byte mode.
    const MESEN2_LONG_RUN_VECTORS: [(&str, &str); 2] = [
        (
            concat!(
                "00000000000000000000000000000000000700000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000820000000000",
                "0000000000000000000000000100000000000000006E000000980000000000000000000000000000",
                "0000000000000000",
            ),
            concat!(
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "0000000000000000000000000000000000000000000000000000000000000000",
            ),
        ),
        (
            concat!(
                "C000000000000000000000005D0000000000000000000000000000000000000075000000764C0000",
                "000000000000000000000000000000000000000000000099000000C1000000000021000000000000",
                "00000000000000000000008F00000000001900000000000000000000000000006A00000000000000",
                "0000000000000000",
            ),
            concat!(
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "00000000000000000000000000000000000000000000000000000000000000000000000000000000",
                "0000000000000000000000000000000000000000000000000000000000000000",
            ),
        ),
    ];

    #[test]
    fn decompressor_matches_mesen2_through_order_7_runs() {
        for &(input, expected) in &MESEN2_LONG_RUN_VECTORS {
            assert_eq!(
                decompress(&hex(input), 512),
                hex(expected),
                "header ${}",
                &input[..2]
            );
        }
    }

    /// With an all-zero stream every codeword is a lone 0 bit, which fullsnes' `GetCodeword`
    /// turns into a full run of the most probable symbol, and every context starts with MPS 0.
    #[test]
    fn all_zero_input_decompresses_to_zero_bytes() {
        for header in [0x00, 0x40, 0x80, 0xC0] {
            let mut input = vec![0u8; 16];
            input[0] = header;
            assert_eq!(
                decompress(&input, 16),
                vec![0u8; 16],
                "header ${header:02X}"
            );
        }
    }
}
