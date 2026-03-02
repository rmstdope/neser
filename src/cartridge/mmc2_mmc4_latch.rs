#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatchTriggerMode {
    Mmc2,
    Mmc4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mmc2Mmc4Latch {
    pub bank_0_fd: u8,
    pub bank_0_fe: u8,
    pub bank_1_fd: u8,
    pub bank_1_fe: u8,
    pub latch0_is_fd: bool,
    pub latch1_is_fd: bool,
    mode: LatchTriggerMode,
}

impl Mmc2Mmc4Latch {
    pub fn new(mode: LatchTriggerMode) -> Self {
        Self {
            bank_0_fd: 0,
            bank_0_fe: 0,
            bank_1_fd: 0,
            bank_1_fe: 0,
            latch0_is_fd: false,
            latch1_is_fd: false,
            mode,
        }
    }

    pub fn selected_bank_0(&self) -> u8 {
        if self.latch0_is_fd {
            self.bank_0_fd
        } else {
            self.bank_0_fe
        }
    }

    pub fn selected_bank_1(&self) -> u8 {
        if self.latch1_is_fd {
            self.bank_1_fd
        } else {
            self.bank_1_fe
        }
    }

    pub fn update_for_chr_read(&mut self, addr: u16) {
        match self.mode {
            LatchTriggerMode::Mmc2 => match addr {
                0x0FD8 => self.latch0_is_fd = true,
                0x0FE8 => self.latch0_is_fd = false,
                0x1FD8..=0x1FDF => self.latch1_is_fd = true,
                0x1FE8..=0x1FEF => self.latch1_is_fd = false,
                _ => {}
            },
            LatchTriggerMode::Mmc4 => match addr {
                0x0FD8..=0x0FDF => self.latch0_is_fd = true,
                0x0FE8..=0x0FEF => self.latch0_is_fd = false,
                0x1FD8..=0x1FDF => self.latch1_is_fd = true,
                0x1FE8..=0x1FEF => self.latch1_is_fd = false,
                _ => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mmc2_latch0_switches_only_on_exact_fd8_and_fe8() {
        let mut latch = Mmc2Mmc4Latch::new(LatchTriggerMode::Mmc2);

        assert!(!latch.latch0_is_fd);

        latch.update_for_chr_read(0x0FDF);
        assert!(!latch.latch0_is_fd);

        latch.update_for_chr_read(0x0FD8);
        assert!(latch.latch0_is_fd);

        latch.update_for_chr_read(0x0FEF);
        assert!(latch.latch0_is_fd);

        latch.update_for_chr_read(0x0FE8);
        assert!(!latch.latch0_is_fd);
    }

    #[test]
    fn mmc4_latch0_switches_on_fd8_fdf_and_fe8_fef_ranges() {
        let mut latch = Mmc2Mmc4Latch::new(LatchTriggerMode::Mmc4);

        assert!(!latch.latch0_is_fd);

        latch.update_for_chr_read(0x0FDF);
        assert!(latch.latch0_is_fd);

        latch.update_for_chr_read(0x0FEF);
        assert!(!latch.latch0_is_fd);
    }

    #[test]
    fn both_modes_use_range_triggers_for_latch1() {
        let mut mmc2 = Mmc2Mmc4Latch::new(LatchTriggerMode::Mmc2);
        let mut mmc4 = Mmc2Mmc4Latch::new(LatchTriggerMode::Mmc4);

        mmc2.update_for_chr_read(0x1FDF);
        mmc4.update_for_chr_read(0x1FDF);
        assert!(mmc2.latch1_is_fd);
        assert!(mmc4.latch1_is_fd);

        mmc2.update_for_chr_read(0x1FEF);
        mmc4.update_for_chr_read(0x1FEF);
        assert!(!mmc2.latch1_is_fd);
        assert!(!mmc4.latch1_is_fd);
    }

    #[test]
    fn selected_banks_follow_latch_states() {
        let mut latch = Mmc2Mmc4Latch::new(LatchTriggerMode::Mmc4);
        latch.bank_0_fd = 1;
        latch.bank_0_fe = 2;
        latch.bank_1_fd = 3;
        latch.bank_1_fe = 4;

        assert_eq!(latch.selected_bank_0(), 2);
        assert_eq!(latch.selected_bank_1(), 4);

        latch.latch0_is_fd = true;
        latch.latch1_is_fd = true;

        assert_eq!(latch.selected_bank_0(), 1);
        assert_eq!(latch.selected_bank_1(), 3);
    }
}
