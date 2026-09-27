//! Mapper 105 - NES-EVENT (Nintendo World Championships)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/NES-EVENT> (with Disch's notes on the same page)
//! - Implementation reference: Mesen2 `Core/NES/Mappers/Nintendo/MMC1_105.h`
//!
//! An MMC1 whose CHR bank 0 register ($A000) is rewired:
//!
//! ```text
//! $A000: [...I OAA.]
//!         I = 0: run timer, 1: reset timer (and acknowledge its IRQ)
//!         O = 0: 32 KiB bank AA from the first 128 KiB chip
//!             1: normal MMC1 PRG banking ($E000 bits 0-2) in the second chip
//! ```
//!
//! "The first 32 KiB is hardwired until the timer is started (write 0 then 1 to bit 4)
//! for the first time", and again after reset. The timer is a 30-bit up counter clocked
//! by M2; the IRQ fires when it reaches `$20000000 | dip << 25`.
//!
//! Known Limitations:
//! - The four DIP switches are fixed open (a 5:00 timer on NTSC); there is no setting.
//! - The board's seven-segment timer display is not shown.

use crate::nes::cartridge::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};
use crate::nes::cartridge::mmc1::MMC1Mapper;

/// How far the board is from releasing its power-on 32 KiB PRG lock: the timer bit
/// must go to 0 and then back to 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrgLock {
    Locked = 0,
    TimerRan = 1,
    Unlocked = 2,
}

impl PrgLock {
    fn from_byte(value: u8) -> Self {
        match value {
            1 => Self::TimerRan,
            2 => Self::Unlocked,
            _ => Self::Locked,
        }
    }

    fn after_timer_bit(self, timer_bit_set: bool) -> Self {
        match (self, timer_bit_set) {
            (Self::Locked, false) => Self::TimerRan,
            (Self::TimerRan, true) => Self::Unlocked,
            (state, _) => state,
        }
    }
}

/// Mapper 105 (NES-EVENT): MMC1 registers, mirroring and PRG-RAM, with the board's
/// own PRG banking and 30-bit IRQ timer layered on top.
///
/// PRG-RAM at $6000-$7FFF is gated by the $E000 WRAM disable bit only; CHR bank bit 4
/// never gates it here, unlike on MMC1 SNROM.
pub struct NesEventMapper {
    inner: MMC1Mapper,
    prg_lock: PrgLock,
    irq_running: bool,
    irq_pending: bool,
    irq_counter: u32,
}

impl NesEventMapper {
    /// `[irq_running, irq_pending, prg_lock, irq_counter (u32 LE)]`, then MMC1's.
    const SNAPSHOT_SIZE: usize = 7;
    const IRQ_COUNTER_IDX: usize = 3;
    const TIMER_BIT: u8 = 0x10;
    const CHIP_SELECT_BIT: u8 = 0x08;
    /// DIP switches 1-4 set counter bits 25-28 of the target; all open here.
    const DIP_SWITCHES: u32 = 0;
    const IRQ_THRESHOLD: u32 = 0x2000_0000 | (Self::DIP_SWITCHES << 25);
    /// 16 KiB banks of the second PRG chip start at bank 8.
    const SECOND_CHIP_FIRST_BANK: i16 = 8;
    const SECOND_CHIP_LAST_BANK: i16 = 15;

    pub fn new(mut ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        // Board exception: NES-EVENT carries 8 KiB of PRG-RAM whatever the header says.
        ctx.set_board_prg_ram(1);
        let mut inner = MMC1Mapper::new(ctx);
        // The board passes MMC1's SNROM test (CHR-RAM, 256 KiB PRG, 8 KiB RAM), but its
        // CHR bank 0 bit 4 is the timer control (nesdev NES-EVENT: "0: Run timer /
        // 1: Reset timer"), not PRG-RAM /CE. Only $E000's "W = WRAM disable (same as
        // MMC1)" gates the RAM.
        inner.without_chr_a16_prg_ram_gate();
        let mut mapper = Self {
            inner,
            prg_lock: PrgLock::Locked,
            irq_running: false,
            irq_pending: false,
            irq_counter: 0,
        };
        mapper.power_on_state();
        mapper
    }

    /// Power-on and reset: the timer bit reads as set (timer held at 0), and the first
    /// 32 KiB of the first chip is hardwired again.
    fn power_on_state(&mut self) {
        let mut regs = self.inner.registers_snapshot();
        if regs.len() > 3 {
            regs[3] |= Self::TIMER_BIT;
            self.inner.restore_registers(&regs);
        }
        self.prg_lock = PrgLock::Locked;
        self.irq_running = false;
        self.irq_pending = false;
        self.irq_counter = 0;
        self.update_banks();
    }

    /// Applies the level of the timer bit: set holds the counter at 0 and acknowledges
    /// the IRQ; clear lets it count. Each level also advances the PRG lock.
    fn apply_timer_bit(&mut self) {
        let timer_bit_set = self.inner.chr_bank_0() & Self::TIMER_BIT != 0;
        self.prg_lock = self.prg_lock.after_timer_bit(timer_bit_set);
        if timer_bit_set {
            self.irq_running = false;
            self.irq_pending = false;
            self.irq_counter = 0;
        } else {
            self.irq_running = true;
        }
    }

    /// Overrides the inner MMC1's PRG and CHR mapping with the board's.
    fn update_banks(&mut self) {
        let (low, high) = self.prg_banks_16k();
        let base = self.inner.base_mut();
        base.select_prg_page(0, low);
        base.select_prg_page(1, high);
        // One fixed 8 KiB of CHR-RAM; $A000 bit 0 is "Not used".
        base.select_chr_page(0, 0);
        base.select_chr_page(1, 1);
    }

    fn prg_banks_16k(&self) -> (i16, i16) {
        if self.prg_lock != PrgLock::Unlocked {
            return (0, 1);
        }
        let chr_bank_0 = self.inner.chr_bank_0();
        if chr_bank_0 & Self::CHIP_SELECT_BIT == 0 {
            let bank_32k = ((chr_bank_0 >> 1) & 0x03) as i16;
            return (bank_32k * 2, bank_32k * 2 + 1);
        }
        let bank = Self::SECOND_CHIP_FIRST_BANK | (self.inner.prg_bank() & 0x07) as i16;
        match (self.inner.control() >> 2) & 0x03 {
            0 | 1 => (bank & !1, (bank & !1) + 1),
            2 => (Self::SECOND_CHIP_FIRST_BANK, bank),
            _ => (bank, Self::SECOND_CHIP_LAST_BANK),
        }
    }

    fn tick_irq_timer(&mut self) {
        if !self.irq_running {
            return;
        }
        self.irq_counter = self.irq_counter.wrapping_add(1);
        if self.irq_counter >= Self::IRQ_THRESHOLD {
            self.irq_pending = true;
            self.irq_running = false;
        }
    }
}

impl Mapper for NesEventMapper {
    fn base(&self) -> &BaseMapper {
        self.inner.base()
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        self.inner.base_mut()
    }

    fn read_prg(&self, addr: u16) -> u8 {
        self.inner.read_prg(addr)
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        self.inner.read_prg_open_bus(addr, open_bus)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        self.inner.write_prg(addr, value);
        if addr >= 0x8000 {
            self.apply_timer_bit();
            self.update_banks();
        }
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.inner.write_chr(addr, value);
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.inner.read_chr(addr)
    }

    fn cpu_cycle(&mut self) {
        self.inner.cpu_cycle();
        self.tick_irq_timer();
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn get_mirroring(&self) -> crate::nes::cartridge::NametableLayout {
        self.inner.get_mirroring()
    }

    fn wram_size(&self) -> usize {
        self.inner.wram_size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.inner.wram_snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.inner.load_wram_snapshot(data);
    }

    fn initialize_ram(&mut self, mode: crate::nes::console::RamInitMode) {
        self.inner.initialize_ram(mode);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let inner = self.inner.registers_snapshot();
        let mut snap = Vec::with_capacity(Self::SNAPSHOT_SIZE + inner.len());
        snap.push(self.irq_running as u8);
        snap.push(self.irq_pending as u8);
        snap.push(self.prg_lock as u8);
        snap.extend_from_slice(&self.irq_counter.to_le_bytes());
        snap.extend(inner);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < Self::SNAPSHOT_SIZE {
            return;
        }
        self.irq_running = data[0] != 0;
        self.irq_pending = data[1] != 0;
        self.prg_lock = PrgLock::from_byte(data[2]);
        let counter = &data[Self::IRQ_COUNTER_IDX..Self::IRQ_COUNTER_IDX + 4];
        self.irq_counter = u32::from_le_bytes([counter[0], counter[1], counter[2], counter[3]]);
        self.inner.restore_registers(&data[Self::SNAPSHOT_SIZE..]);
        self.update_banks();
    }

    fn reset(&mut self) {
        // Disch's notes: "On powerup and reset, the first 32k of PRG (from the first
        // PRG chip) is selected at $8000 *no matter what*."
        self.inner.reset();
        self.power_on_state();
    }

    fn capabilities(&self) -> MapperCapabilities {
        let mut caps = self.inner.capabilities();
        caps.has_irq = true;
        caps.has_chr_banking = false;
        caps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    #[test]
    fn nes_event_board_carries_8k_prg_ram_whatever_the_header_says() {
        // nesdev NES-EVENT: the board has "8K of PRG RAM", fixed on the board.
        for ctx in [
            MapperContext::new_for_test(
                105,
                vec![0; 256 * 1024],
                vec![],
                NametableLayout::Horizontal,
            )
            .with_unspecified_prg_ram_size(),
            MapperContext::new_for_test(
                105,
                vec![0; 256 * 1024],
                vec![],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        ] {
            assert_eq!(NesEventMapper::new(ctx).wram_size(), 8 * 1024);
        }
    }

    /// A NES-EVENT image as the board is built: 256 KiB PRG-ROM and CHR-RAM
    /// (nesdev NES-EVENT: "8K of CHR RAM"), with the board's 8 KiB PRG-RAM.
    fn nes_event_board() -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            105,
            banked_data(16 * 1024, 16),
            vec![],
            NametableLayout::Horizontal,
        ))
        .expect("Mapper 105 should be implemented")
    }

    const OPEN_BUS: u8 = 0x5A;

    fn assert_prg_ram_read_write(mapper: &mut dyn Mapper, value: u8, when: &str) {
        mapper.write_prg(0x6000, value);
        mapper.write_prg(0x7FFF, !value);
        assert_eq!(
            mapper.read_prg_open_bus(0x6000, OPEN_BUS),
            value,
            "$6000 must read back {when}"
        );
        assert_eq!(
            mapper.read_prg_open_bus(0x7FFF, OPEN_BUS),
            !value,
            "$7FFF must read back {when}"
        );
    }

    #[test]
    fn nes_event_prg_ram_is_readable_and_writable_while_timer_is_reset() {
        // nesdev NES-EVENT, $A000 bit 4: "0: Run timer / 1: Reset timer". It is the
        // timer control, not SNROM's PRG-RAM /CE, so the RAM answers while it is set.
        let mut mapper = nes_event_board();
        assert_prg_ram_read_write(mapper.as_mut(), 0x11, "at power-on (timer reset)");

        write_mmc1_register(mapper.as_mut(), 0xA000, 0b10000);
        assert_prg_ram_read_write(mapper.as_mut(), 0x22, "with the timer bit set");

        write_mmc1_register(mapper.as_mut(), 0xA000, 0b00000);
        assert_prg_ram_read_write(mapper.as_mut(), 0x33, "with the timer running");
    }

    #[test]
    fn nes_event_prg_ram_follows_e000_wram_disable_bit() {
        // nesdev NES-EVENT, $E000: "W = WRAM disable (same as MMC1)".
        let mut mapper = nes_event_board();
        assert_prg_ram_read_write(mapper.as_mut(), 0x44, "while WRAM is enabled");

        write_mmc1_register(mapper.as_mut(), 0xE000, 0b10000);
        assert_eq!(mapper.read_prg_open_bus(0x6000, OPEN_BUS), OPEN_BUS);
        assert_eq!(mapper.read_prg_open_bus(0x7FFF, OPEN_BUS), OPEN_BUS);
        mapper.write_prg(0x6000, 0x99);

        write_mmc1_register(mapper.as_mut(), 0xE000, 0b00000);
        assert_eq!(
            mapper.read_prg_open_bus(0x6000, OPEN_BUS),
            0x44,
            "re-enabled WRAM keeps its data and ignored the disabled write"
        );
    }

    #[test]
    fn nes_event_prg_ram_stays_enabled_after_reset() {
        // Reset forces the timer bit again; the RAM must still answer.
        let mut mapper = nes_event_board();
        write_mmc1_register(mapper.as_mut(), 0xA000, 0b00000);
        mapper.reset();
        assert_eq!(
            mapper.registers_snapshot()[NesEventMapper::SNAPSHOT_SIZE + 3] & 0x10,
            0x10,
            "reset re-forces the timer bit"
        );
        assert_prg_ram_read_write(mapper.as_mut(), 0x55, "after reset");
    }

    const PRG_BANKS_16K: usize = 11;
    const CHR_BANKS_4K: usize = 9;

    fn write_mmc1_register<M: Mapper + ?Sized>(mapper: &mut M, addr: u16, value: u8) {
        // MMC1 serial protocol in this codebase requires two cpu_cycle() ticks
        // between writes so consecutive-write filtering does not drop the bit.
        for bit in 0..5 {
            mapper.cpu_cycle();
            mapper.cpu_cycle();
            mapper.write_prg(addr, (value >> bit) & 0x01);
        }
    }

    fn make_mapper() -> Box<dyn Mapper> {
        let prg_rom = banked_data(16 * 1024, PRG_BANKS_16K);
        let chr_rom = banked_data(4 * 1024, CHR_BANKS_4K);
        create_mapper(MapperContext::new_for_test(
            105,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ))
        .expect("Mapper 105 should be implemented")
    }

    #[test]
    fn mapper_105_is_registered() {
        let prg_rom = banked_data(16 * 1024, PRG_BANKS_16K);
        let chr_rom = banked_data(4 * 1024, CHR_BANKS_4K);
        let result = create_mapper(MapperContext::new_for_test(
            105,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 105 must be registered");
    }

    #[test]
    fn mapper_105_mirroring_modes_are_selectable_via_control_register() {
        let mut mapper = make_mapper();

        write_mmc1_register(mapper.as_mut(), 0x8000, 0b00000);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);

        write_mmc1_register(mapper.as_mut(), 0x8000, 0b00001);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);

        write_mmc1_register(mapper.as_mut(), 0x8000, 0b00010);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        write_mmc1_register(mapper.as_mut(), 0x8000, 0b00011);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    /// The two 16 KiB banks mapped at $8000 and $C000, read through `banked_data`,
    /// whose every byte is its own 16 KiB bank number.
    fn prg_banks(mapper: &dyn Mapper) -> (u8, u8) {
        (mapper.read_prg(0x8000), mapper.read_prg(0xC000))
    }

    /// nesdev NES-EVENT: "The first 32 KiB is hardwired until the timer is started
    /// (write 0 then 1 to bit 4) for the first time."
    fn unlock<M: Mapper + ?Sized>(mapper: &mut M) {
        write_mmc1_register(mapper, 0xA000, 0b00000);
        write_mmc1_register(mapper, 0xA000, 0b10000);
    }

    #[test]
    fn nes_event_power_on_maps_first_32k_of_first_chip() {
        let mapper = nes_event_board();
        assert_eq!(prg_banks(mapper.as_ref()), (0, 1));
    }

    #[test]
    fn nes_event_stays_locked_until_timer_bit_goes_0_then_1() {
        let mut mapper = nes_event_board();
        // $E000 and $8000 writes that would bank a plain MMC1 change nothing.
        write_mmc1_register(mapper.as_mut(), 0xE000, 0b00101);
        write_mmc1_register(mapper.as_mut(), 0x8000, 0b01100);
        assert_eq!(prg_banks(mapper.as_ref()), (0, 1), "locked at power-on");

        // O=0, bank 2, with the timer bit still at its power-on 1.
        write_mmc1_register(mapper.as_mut(), 0xA000, 0b10100);
        assert_eq!(prg_banks(mapper.as_ref()), (0, 1), "I never went to 0");

        // O=0, bank 2, timer running: I has gone to 0 but not back to 1.
        write_mmc1_register(mapper.as_mut(), 0xA000, 0b00100);
        assert_eq!(prg_banks(mapper.as_ref()), (0, 1), "I went to 0 only");

        write_mmc1_register(mapper.as_mut(), 0xA000, 0b10100);
        assert_eq!(
            prg_banks(mapper.as_ref()),
            (4, 5),
            "unlocked after 0 then 1"
        );

        // Once unlocked it stays unlocked whatever I does.
        write_mmc1_register(mapper.as_mut(), 0xA000, 0b00110);
        assert_eq!(prg_banks(mapper.as_ref()), (6, 7));
    }

    #[test]
    fn nes_event_unlocked_o0_selects_32k_from_first_chip() {
        // nesdev: $A000 bits 1-2 "Select 32 KiB bank in $8000-$FFFF from lower 128KB ROM".
        let mut mapper = nes_event_board();
        unlock(mapper.as_mut());
        for bank in 0..4u8 {
            write_mmc1_register(mapper.as_mut(), 0xA000, 0b10000 | (bank << 1));
            assert_eq!(prg_banks(mapper.as_ref()), (bank * 2, bank * 2 + 1));
        }
        // Bit 0 is "Not used": it must not move the PRG bank.
        write_mmc1_register(mapper.as_mut(), 0xA000, 0b10011);
        assert_eq!(prg_banks(mapper.as_ref()), (2, 3));
    }

    #[test]
    fn nes_event_unlocked_o1_uses_mmc1_modes_in_second_chip() {
        // nesdev: with bit 3 set, "Normal MMC1 behavior from upper 128KB ROM".
        let mut mapper = nes_event_board();
        unlock(mapper.as_mut());
        write_mmc1_register(mapper.as_mut(), 0xA000, 0b11000);
        write_mmc1_register(mapper.as_mut(), 0xE000, 0b00101);

        write_mmc1_register(mapper.as_mut(), 0x8000, 0b01100); // mode 3
        assert_eq!(
            prg_banks(mapper.as_ref()),
            (13, 15),
            "switch $8000, fix last"
        );

        write_mmc1_register(mapper.as_mut(), 0x8000, 0b01000); // mode 2
        assert_eq!(
            prg_banks(mapper.as_ref()),
            (8, 13),
            "fix first, switch $C000"
        );

        write_mmc1_register(mapper.as_mut(), 0x8000, 0b00000); // mode 0: 32 KiB
        assert_eq!(prg_banks(mapper.as_ref()), (12, 13), "32 KiB ignores bit 0");
    }

    #[test]
    fn nes_event_reset_relocks_prg() {
        // Disch's notes on nesdev: "On powerup and reset, the first 32k of PRG (from
        // the first PRG chip) is selected at $8000 *no matter what*."
        let mut mapper = nes_event_board();
        unlock(mapper.as_mut());
        write_mmc1_register(mapper.as_mut(), 0xA000, 0b10110);
        assert_eq!(prg_banks(mapper.as_ref()), (6, 7));

        mapper.reset();
        assert_eq!(prg_banks(mapper.as_ref()), (0, 1), "reset locks again");
        write_mmc1_register(mapper.as_mut(), 0xA000, 0b10110);
        assert_eq!(
            prg_banks(mapper.as_ref()),
            (0, 1),
            "and needs 0 then 1 again"
        );
    }

    #[test]
    fn nes_event_chr_ram_is_not_banked() {
        // nesdev: $A000 bit 0 "Not used"; the board has one fixed 8 KiB of CHR-RAM.
        let mut mapper = nes_event_board();
        mapper.write_chr(0x0000, 0xAB);
        mapper.write_chr(0x1000, 0xCD);
        write_mmc1_register(mapper.as_mut(), 0x8000, 0b11100); // 4 KiB CHR mode
        write_mmc1_register(mapper.as_mut(), 0xA000, 0b10001);
        write_mmc1_register(mapper.as_mut(), 0xC000, 0b00000);
        assert_eq!(mapper.read_chr(0x0000), 0xAB);
        assert_eq!(mapper.read_chr(0x1000), 0xCD);
        assert!(!mapper.capabilities().has_chr_banking);
    }

    /// nesdev NES-EVENT: the counter fires "when it reaches a high enough value"; with
    /// every DIP switch open (Disch's notes) that value is $20000000.
    const IRQ_THRESHOLD_ALL_DIPS_OPEN: u32 = 0x2000_0000;

    /// Puts the 30-bit counter at `value` through a snapshot round trip, so the test
    /// does not have to run half a billion cycles.
    fn set_irq_counter(mapper: &mut dyn Mapper, value: u32) {
        let mut regs = mapper.registers_snapshot();
        regs[NesEventMapper::IRQ_COUNTER_IDX..NesEventMapper::IRQ_COUNTER_IDX + 4]
            .copy_from_slice(&value.to_le_bytes());
        mapper.restore_registers(&regs);
    }

    #[test]
    fn nes_event_irq_fires_when_counter_reaches_threshold() {
        let mut mapper = nes_event_board();
        write_mmc1_register(mapper.as_mut(), 0xA000, 0b00000); // run timer
        set_irq_counter(mapper.as_mut(), IRQ_THRESHOLD_ALL_DIPS_OPEN - 2);

        mapper.cpu_cycle();
        assert!(!mapper.irq_pending(), "one cycle short of the threshold");
        mapper.cpu_cycle();
        assert!(mapper.irq_pending(), "fires on reaching $20000000");
        mapper.cpu_cycle();
        assert!(mapper.irq_pending(), "stays pending until acknowledged");

        write_mmc1_register(mapper.as_mut(), 0xA000, 0b10000);
        assert!(!mapper.irq_pending(), "setting the timer bit acknowledges");
    }

    #[test]
    fn nes_event_timer_does_not_fire_early() {
        // The old model fired within 16 cycles; a real run takes five minutes.
        let mut mapper = nes_event_board();
        write_mmc1_register(mapper.as_mut(), 0xA000, 0b00000);
        for _ in 0..100_000 {
            mapper.cpu_cycle();
        }
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn nes_event_timer_bit_set_holds_counter_at_zero() {
        // Disch's notes: "When set, the IRQ counter is reset to 0 and stays there".
        let mut mapper = nes_event_board();
        write_mmc1_register(mapper.as_mut(), 0xA000, 0b00000);
        set_irq_counter(mapper.as_mut(), IRQ_THRESHOLD_ALL_DIPS_OPEN - 1);
        write_mmc1_register(mapper.as_mut(), 0xA000, 0b10000);
        for _ in 0..10 {
            mapper.cpu_cycle();
        }
        assert!(!mapper.irq_pending(), "held while the bit is set");

        write_mmc1_register(mapper.as_mut(), 0xA000, 0b00000);
        mapper.cpu_cycle();
        assert!(
            !mapper.irq_pending(),
            "restarted from 0, not from where it was"
        );
    }

    #[test]
    fn nes_event_snapshot_round_trips_unlock_and_counter() {
        let mut mapper = nes_event_board();
        unlock(mapper.as_mut());
        write_mmc1_register(mapper.as_mut(), 0xA000, 0b00110); // bank 3, timer running
        for _ in 0..1234 {
            mapper.cpu_cycle();
        }
        let saved = mapper.registers_snapshot();

        let mut restored = nes_event_board();
        restored.restore_registers(&saved);
        assert_eq!(
            prg_banks(restored.as_ref()),
            (6, 7),
            "unlock and bank restored"
        );
        assert_eq!(restored.registers_snapshot(), saved, "counter restored");
    }
}
