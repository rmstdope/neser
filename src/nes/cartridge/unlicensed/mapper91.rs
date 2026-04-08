//! Mapper 091 – JY Company pirate board (MMC3-style scanline IRQ)
//!
//! Specifications:
//! - Primary: <https://www.nesdev.org/wiki/INES_Mapper_091>
//! - Fallback: <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/JyCompany/Mapper91.h>
//!
//! Key characteristics:
//! - PRG: 4 × 8 KB slots; slots 2/3 fixed to second-last/last bank; slots 0/1 switchable
//! - CHR: 4 × 2 KB slots covering full 8 KB window
//! - Registers mapped to $6000–$7FFF (not $8000–$FFFF like standard MMC3)
//! - IRQ: MMC3-style A12 scanline counter (Sharp/normal behavior), latch hardcoded to 7
//! - Mirroring: fixed from cartridge header (no mirroring register)
//!
//! Register map (decoded via `addr & 0x7003`):
//! - $6000: CHR bank 0 (2 KB at PPU $0000)
//! - $6001: CHR bank 1 (2 KB at PPU $0800)
//! - $6002: CHR bank 2 (2 KB at PPU $1000)
//! - $6003: CHR bank 3 (2 KB at PPU $1800)
//! - $7000: PRG bank 0 (8 KB at CPU $8000), bits 3–0
//! - $7001: PRG bank 1 (8 KB at CPU $A000), bits 3–0
//! - $7002: IRQ disable + acknowledge (write any value)
//! - $7003: IRQ enable + reload (latch fixed to 7, counter reloaded from 0)
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::BaseMapper;
use crate::cartridge::common::A12RisingEdgeDetector;
use crate::cartridge::{Mapper, MapperCapabilities};

/// Mapper 091 – JY Company / MMC3-style pirate board
pub struct Mapper91 {
    base: BaseMapper,

    prg_regs: [u8; 2],
    chr_regs: [u8; 4],

    // IRQ state (MMC3-style Sharp behavior; latch is hardcoded to 7)
    irq_counter: u8,
    irq_reload: bool,
    irq_enabled: bool,
    irq_asserted: bool,

    a12: A12RisingEdgeDetector,
}

impl Mapper91 {
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KB
    const CHR_BANK_SIZE: usize = 0x0800; // 2 KB
    const IRQ_LATCH: u8 = 7;

    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let mirroring = ctx.mirroring;
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 2,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(Self::PRG_BANK_SIZE);
        base.configure_chr_banking(Self::CHR_BANK_SIZE);

        let mut mapper = Self {
            base,
            prg_regs: [0; 2],
            chr_regs: [0; 4],
            irq_counter: 0,
            irq_reload: false,
            irq_enabled: false,
            irq_asserted: false,
            a12: A12RisingEdgeDetector::new(3),
        };
        mapper.base.set_mirroring(mirroring);
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        self.base.select_prg_page(0, self.prg_regs[0] as i16);
        self.base.select_prg_page(1, self.prg_regs[1] as i16);
        self.base.select_prg_page(2, -2);
        self.base.select_prg_page(3, -1);

        self.base.select_chr_page(0, self.chr_regs[0] as i16);
        self.base.select_chr_page(1, self.chr_regs[1] as i16);
        self.base.select_chr_page(2, self.chr_regs[2] as i16);
        self.base.select_chr_page(3, self.chr_regs[3] as i16);
    }

    fn clock_irq(&mut self) {
        let old_counter = self.irq_counter;
        let was_reload = self.irq_reload;

        if self.irq_counter == 0 || self.irq_reload {
            self.irq_counter = Self::IRQ_LATCH;
            self.irq_reload = false;
        } else {
            self.irq_counter = self.irq_counter.wrapping_sub(1);
        }

        // Sharp (normal) behavior: fire when counter reaches 0
        if self.irq_counter == 0 && self.irq_enabled {
            // Avoid spurious fires when reloading to 0 from already-zero counter
            let decremented = old_counter == 1;
            let reload_triggered = was_reload;
            if decremented || reload_triggered {
                self.irq_asserted = true;
            }
        }
    }
}

impl Mapper for Mapper91 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        91
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 2,
            ..Default::default()
        }
    }

    fn reset(&mut self) {
        self.prg_regs = [0; 2];
        self.chr_regs = [0; 4];
        self.irq_counter = 0;
        self.irq_reload = false;
        self.irq_enabled = false;
        self.irq_asserted = false;
        self.a12 = A12RisingEdgeDetector::new(3);
        self.update_banks();
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if !(0x6000..=0x7FFF).contains(&addr) {
            return;
        }
        match addr & 0x7003 {
            0x6000 => {
                self.chr_regs[0] = value;
                self.update_banks();
            }
            0x6001 => {
                self.chr_regs[1] = value;
                self.update_banks();
            }
            0x6002 => {
                self.chr_regs[2] = value;
                self.update_banks();
            }
            0x6003 => {
                self.chr_regs[3] = value;
                self.update_banks();
            }
            0x7000 => {
                self.prg_regs[0] = value & 0x0F;
                self.update_banks();
            }
            0x7001 => {
                self.prg_regs[1] = value & 0x0F;
                self.update_banks();
            }
            0x7002 => {
                // IRQ disable + acknowledge
                self.irq_enabled = false;
                self.irq_asserted = false;
            }
            0x7003 => {
                // IRQ enable + reload (latch always 7)
                self.irq_counter = 0;
                self.irq_reload = true;
                self.irq_enabled = true;
            }
            _ => {}
        }
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        if self.a12.update(addr) {
            self.clock_irq();
        }
    }

    fn cpu_cycle(&mut self) {
        self.a12.cpu_tick();
    }

    fn irq_pending(&self) -> bool {
        self.irq_asserted
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // [0..=1]:  prg_regs (2-element array: prg_regs[0], prg_regs[1])
        // [2..=5]:  chr_regs (4-element array: chr_regs[0..=3])
        // [6]:      irq_counter
        // [7]:      flags (irq_reload | irq_enabled<<1 | irq_asserted<<2)
        // [8]:      a12 prev_a12
        // [9]:      a12 current_a12
        // [10]:     a12 a12_low_cycles
        let mut snap = Vec::with_capacity(11);
        snap.extend_from_slice(&self.prg_regs);
        snap.extend_from_slice(&self.chr_regs);
        snap.push(self.irq_counter);
        let flags = (self.irq_reload as u8)
            | ((self.irq_enabled as u8) << 1)
            | ((self.irq_asserted as u8) << 2);
        snap.push(flags);
        snap.push(self.a12.prev_a12() as u8);
        snap.push(self.a12.current_a12() as u8);
        snap.push(self.a12.a12_low_cycles());
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 8 {
            self.prg_regs.copy_from_slice(&data[0..2]);
            self.chr_regs.copy_from_slice(&data[2..6]);
            self.irq_counter = data[6];
            let flags = data[7];
            self.irq_reload = (flags & 1) != 0;
            self.irq_enabled = (flags & 2) != 0;
            self.irq_asserted = (flags & 4) != 0;
            self.update_banks();
        }
        if data.len() >= 11 {
            self.a12.set_prev_a12(data[8] != 0);
            self.a12.set_current_a12(data[9] != 0);
            self.a12.set_a12_low_cycles(data[10]);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts to avoid modulo-wrapping false-passes
    const PRG_BANKS: usize = 6; // 6 × 8KB = 48 KB
    const CHR_2K_BANKS: usize = 12; // 12 × 2KB = 24 KB

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(2 * 1024, CHR_2K_BANKS);
        create_mapper(MapperContext::new_for_test(
            91,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 91 should be registered")
    }

    // --- Factory ---

    #[test]
    fn mapper91_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            91,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(2 * 1024, CHR_2K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 91 must be registered in the factory"
        );
    }

    // --- PRG banking ---

    #[test]
    fn prg_slots_2_and_3_are_fixed_to_last_two_banks() {
        let mapper = make_mapper();
        // PRG_BANKS=6: last bank idx=5, second-last=4
        assert_eq!(
            mapper.read_prg(0xC000),
            4,
            "$C000 must be fixed to second-last PRG bank"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            5,
            "$E000 must be fixed to last PRG bank"
        );
    }

    #[test]
    fn prg_slot0_switchable_via_7000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7000, 2);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "$8000 must map to PRG bank 2 after $7000 write"
        );
    }

    #[test]
    fn prg_slot1_switchable_via_7001() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7001, 3);
        assert_eq!(
            mapper.read_prg(0xA000),
            3,
            "$A000 must map to PRG bank 3 after $7001 write"
        );
    }

    #[test]
    fn prg_slot0_masks_to_4_bits() {
        let mut mapper = make_mapper();
        // value = 0xFF, masked to 0x0F = 15, but with 6 banks: 15 % 6 = 3
        mapper.write_prg(0x7000, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "$7000 must mask to low 4 bits (0xFF & 0x0F = 15 → bank 15 % 6 = 3)"
        );
    }

    #[test]
    fn prg_fixed_banks_not_affected_by_slot0_switch() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7000, 1);
        // Fixed banks must remain unchanged
        assert_eq!(mapper.read_prg(0xC000), 4, "$C000 must stay at second-last");
        assert_eq!(mapper.read_prg(0xE000), 5, "$E000 must stay at last");
    }

    // --- CHR banking (2 KB pages) ---

    #[test]
    fn chr_slot0_switchable_via_6000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 3);
        // banked_data(2KB, 12): bank 3 fills with byte 3
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "PPU $0000 must be CHR bank 3 after $6000=3"
        );
    }

    #[test]
    fn chr_slot1_switchable_via_6001() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6001, 5);
        assert_eq!(
            mapper.read_chr(0x0800),
            5,
            "PPU $0800 must be CHR bank 5 after $6001=5"
        );
    }

    #[test]
    fn chr_slot2_switchable_via_6002() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6002, 7);
        assert_eq!(
            mapper.read_chr(0x1000),
            7,
            "PPU $1000 must be CHR bank 7 after $6002=7"
        );
    }

    #[test]
    fn chr_slot3_switchable_via_6003() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6003, 9);
        assert_eq!(
            mapper.read_chr(0x1800),
            9,
            "PPU $1800 must be CHR bank 9 after $6003=9"
        );
    }

    #[test]
    fn chr_slots_are_independent() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 2);
        mapper.write_prg(0x6001, 4);
        mapper.write_prg(0x6002, 6);
        mapper.write_prg(0x6003, 8);

        assert_eq!(mapper.read_chr(0x0000), 2);
        assert_eq!(mapper.read_chr(0x0800), 4);
        assert_eq!(mapper.read_chr(0x1000), 6);
        assert_eq!(mapper.read_chr(0x1800), 8);
    }

    // --- CHR address decoding: addr & 0x7003 ---

    #[test]
    fn chr_register_decoding_uses_addr_mask_0x7003() {
        // $6100 & 0x7003 = $6000 → routes to CHR slot 0
        let mut mapper = make_mapper();
        mapper.write_prg(0x6100, 3); // should act like $6000
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "$6100 write must be decoded same as $6000 (addr & 0x7003)"
        );
    }

    // --- Mirroring ---

    #[test]
    fn mirroring_is_fixed_from_header() {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(2 * 1024, CHR_2K_BANKS);
        let mapper = create_mapper(MapperContext::new_for_test(
            91,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
        .expect("mapper 91 must be registered");
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring must come from header"
        );
    }

    #[test]
    fn mirroring_cannot_be_changed_by_register_writes() {
        let mut mapper = make_mapper(); // Vertical
        // No mirroring register; writes must not change it
        mapper.write_prg(0x6000, 0);
        mapper.write_prg(0x7000, 0);
        mapper.write_prg(0x7002, 0);
        mapper.write_prg(0x7003, 0);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring must remain vertical"
        );
    }

    // --- IRQ: disable / acknowledge ---

    #[test]
    fn irq_not_pending_on_power_on() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending(), "IRQ must not be pending on power-on");
    }

    #[test]
    fn irq_disabled_and_acked_by_7002_write() {
        let mut mapper = make_mapper();
        // Arm the IRQ first: enable via $7003
        mapper.write_prg(0x7003, 0);
        // Disable via $7002
        mapper.write_prg(0x7002, 0);
        assert!(
            !mapper.irq_pending(),
            "IRQ must not be pending after $7002 write"
        );

        // Even after triggering A12 edges, IRQ must not fire because it is disabled
        for _ in 0..10 {
            mapper.ppu_address_changed(0x0FFF);
            for _ in 0..3 {
                mapper.cpu_cycle();
            }
            mapper.ppu_address_changed(0x1000);
        }
        assert!(!mapper.irq_pending(), "IRQ must not fire when disabled");
    }

    // --- IRQ: counter fires after 8 scanlines (latch=7) ---

    #[test]
    fn irq_fires_after_eight_a12_rising_edges() {
        let mut mapper = make_mapper();
        // $7003: set latch=7, reload counter from 0, enable
        mapper.write_prg(0x7003, 0);

        // Helper: generate one A12 rising edge with proper debounce
        let trigger_edge = |m: &mut Box<dyn Mapper>| {
            m.ppu_address_changed(0x0000); // A12 low
            for _ in 0..3 {
                m.cpu_cycle();
            }
            m.ppu_address_changed(0x1000); // A12 high → rising edge
        };

        // First edge: counter reloads from 0 to IRQ_LATCH=7 (due to reload flag)
        trigger_edge(&mut mapper);
        assert!(!mapper.irq_pending(), "No IRQ after edge 1 (reloaded to 7)");

        // Edges 2–8: count down 7→6→5→4→3→2→1→0
        for i in 2..=8 {
            trigger_edge(&mut mapper);
            if i < 8 {
                assert!(!mapper.irq_pending(), "No IRQ after edge {i}");
            }
        }
        assert!(
            mapper.irq_pending(),
            "IRQ must fire after 8 A12 rising edges"
        );
    }

    #[test]
    fn irq_can_be_acknowledged_after_firing() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7003, 0);

        let trigger_edge = |m: &mut Box<dyn Mapper>| {
            m.ppu_address_changed(0x0000);
            for _ in 0..3 {
                m.cpu_cycle();
            }
            m.ppu_address_changed(0x1000);
        };

        for _ in 0..9 {
            trigger_edge(&mut mapper);
        }
        assert!(mapper.irq_pending(), "IRQ must be pending before ack");

        mapper.write_prg(0x7002, 0); // ack
        assert!(!mapper.irq_pending(), "IRQ must be cleared after $7002 ack");
    }

    // --- Save state ---

    #[test]
    fn registers_snapshot_round_trip() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7000, 1); // PRG slot 0 = bank 1
        mapper.write_prg(0x7001, 2); // PRG slot 1 = bank 2
        mapper.write_prg(0x6000, 3); // CHR slot 0 = bank 3
        mapper.write_prg(0x6001, 4); // CHR slot 1 = bank 4
        mapper.write_prg(0x6002, 5); // CHR slot 2 = bank 5
        mapper.write_prg(0x6003, 6); // CHR slot 3 = bank 6

        let snap = mapper.registers_snapshot();

        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&snap);

        assert_eq!(
            mapper2.read_prg(0x8000),
            1,
            "PRG slot 0 must survive snapshot"
        );
        assert_eq!(
            mapper2.read_prg(0xA000),
            2,
            "PRG slot 1 must survive snapshot"
        );
        assert_eq!(
            mapper2.read_chr(0x0000),
            3,
            "CHR slot 0 must survive snapshot"
        );
        assert_eq!(
            mapper2.read_chr(0x0800),
            4,
            "CHR slot 1 must survive snapshot"
        );
        assert_eq!(
            mapper2.read_chr(0x1000),
            5,
            "CHR slot 2 must survive snapshot"
        );
        assert_eq!(
            mapper2.read_chr(0x1800),
            6,
            "CHR slot 3 must survive snapshot"
        );
    }

    #[test]
    fn a12_state_is_included_in_snapshot_round_trip() {
        let mut mapper = make_mapper();
        // Drive A12 high so current_a12=true and a12_low_cycles=0
        mapper.ppu_address_changed(0x1000);
        mapper.cpu_cycle();

        let snap = mapper.registers_snapshot();
        assert!(snap.len() >= 11, "Snapshot must include A12 detector state");

        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&snap);

        // After restore, the A12 state must match (current_a12=true means no spurious
        // edge when the same address is presented again)
        let edge = {
            // Presenting A12 high again should NOT produce another rising edge
            // because current_a12 was already true when saved
            mapper2.ppu_address_changed(0x1000);
            mapper2.irq_pending()
        };
        assert!(
            !edge,
            "Restoring A12 state must not cause a spurious rising edge"
        );
    }

    // --- Reset ---

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        // Change PRG/CHR banks
        mapper.write_prg(0x7000, 2);
        mapper.write_prg(0x7001, 3);
        mapper.write_prg(0x6000, 4);
        mapper.write_prg(0x6001, 5);
        mapper.write_prg(0x6002, 6);
        mapper.write_prg(0x6003, 7);
        // Enable IRQ
        mapper.write_prg(0x7003, 0);

        mapper.reset();

        // PRG slots 0/1 must return to bank 0
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG slot 0 must reset to bank 0"
        );
        assert_eq!(
            mapper.read_prg(0xA000),
            0,
            "PRG slot 1 must reset to bank 0"
        );
        // PRG slots 2/3 must remain fixed
        assert_eq!(
            mapper.read_prg(0xC000),
            4,
            "PRG slot 2 must stay fixed after reset"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            5,
            "PRG slot 3 must stay fixed after reset"
        );
        // CHR slots must return to bank 0
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR slot 0 must reset to bank 0"
        );
        assert_eq!(
            mapper.read_chr(0x0800),
            0,
            "CHR slot 1 must reset to bank 0"
        );
        assert_eq!(
            mapper.read_chr(0x1000),
            0,
            "CHR slot 2 must reset to bank 0"
        );
        assert_eq!(
            mapper.read_chr(0x1800),
            0,
            "CHR slot 3 must reset to bank 0"
        );
        // IRQ must be disabled and not pending
        assert!(!mapper.irq_pending(), "IRQ must not be pending after reset");
    }

    #[test]
    fn reset_clears_irq_state_and_a12_detector() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7003, 0);
        // Trigger enough edges to fire IRQ
        let trigger_a12_rising_edge = |m: &mut Box<dyn Mapper>| {
            m.ppu_address_changed(0x0000);
            for _ in 0..3 {
                m.cpu_cycle();
            }
            m.ppu_address_changed(0x1000);
        };
        for _ in 0..9 {
            trigger_a12_rising_edge(&mut mapper);
        }
        assert!(mapper.irq_pending(), "IRQ must be pending before reset");

        mapper.reset();

        assert!(!mapper.irq_pending(), "IRQ must be cleared by reset");
        // After reset, IRQ is disabled: further edges must not fire
        mapper.write_prg(0x7003, 0); // re-enable to check A12 detector was cleared
        trigger_a12_rising_edge(&mut mapper);
        assert!(
            !mapper.irq_pending(),
            "After reset, first edge must reload counter (not fire immediately)"
        );
    }

    // --- write_prg address range guard ---

    #[test]
    fn write_prg_ignores_addresses_outside_6000_7fff() {
        let mut mapper = make_mapper();
        // $E000 & 0x7003 = $6000 → would alias to CHR reg 0 without range guard
        mapper.write_prg(0xE000, 5);
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "Write to $E000 must be ignored (outside $6000-$7FFF)"
        );
        // $A001 & 0x7003 = $2001 but also ensure $8001 writes are ignored
        mapper.write_prg(0x8000, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Write to $8000 must be ignored (outside $6000-$7FFF)"
        );
    }
}
