use super::*;
use crate::nes::cartridge::{Cartridge, NametableLayout};
use crate::nes::cpu::opcode;
use crate::nes::cpu::opcode::*;
use std::cell::RefCell;
use std::rc::Rc;

fn map_minimal_cartridge_for_reset_vector(cpu: &mut Cpu) {
    // Minimal cartridge: reset vector -> $8000.
    let mut prg_rom = vec![0; 0x4000];
    prg_rom[0x3FFC] = 0x00;
    prg_rom[0x3FFD] = 0x80;
    prg_rom[0x0000] = 0xEA; // NOP at $8000
    let chr_rom = vec![0; 0x2000];
    let cartridge = Cartridge::from_parts(prg_rom, chr_rom, NametableLayout::Horizontal);
    cpu.bus.borrow_mut().map_cartridge(cartridge);
}

#[test]
fn test_read_byte_from_pc_wraps_program_counter_at_ffff() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    map_minimal_cartridge_for_reset_vector(&mut cpu);

    cpu.pc = 0xFFFF;

    let value = cpu.read_byte_from_pc();

    assert_eq!(value, 0x00);
    assert_eq!(cpu.pc, 0x0000);
}

#[test]
fn test_reset_ticks_apu_for_7_cpu_cycles() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    map_minimal_cartridge_for_reset_vector(&mut cpu);

    let apu_before = apu.borrow().frame_counter().get_cycle_counter();
    cpu.reset(true);
    let apu_after = apu.borrow().frame_counter().get_cycle_counter();

    // Reset takes 7 CPU cycles; the APU frame counter is clocked every CPU cycle.
    assert_eq!(
        apu_after - apu_before,
        7,
        "CPU reset should tick the APU for 7 cycles"
    );
}

#[test]
fn test_soft_reset_preserves_registers_but_sets_i_and_adjusts_sp() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    map_minimal_cartridge_for_reset_vector(&mut cpu);

    // Arrange: put CPU in a non-power-on state.
    cpu.a = 0x12;
    cpu.x = 0x34;
    cpu.y = 0x56;
    cpu.sp = 0x80;
    cpu.p = 0x00; // I flag cleared

    // Act: soft reset (reset button behavior).
    cpu.reset(true);

    // Assert: A/X/Y preserved, SP adjusted by 3, and I flag set.
    assert_eq!(cpu.a, 0x12);
    assert_eq!(cpu.x, 0x34);
    assert_eq!(cpu.y, 0x56);
    assert_eq!(cpu.sp, 0x7D);
    assert_ne!(cpu.p & FLAG_INTERRUPT, 0);
}

#[test]
fn test_cpu_state_snapshot() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    cpu.a = 0x11;
    cpu.x = 0x22;
    cpu.y = 0x33;
    cpu.sp = 0x44;
    cpu.pc = 0x5566;
    cpu.p = 0x77;

    let state = cpu.state();
    assert_eq!(state.a, 0x11);
    assert_eq!(state.x, 0x22);
    assert_eq!(state.y, 0x33);
    assert_eq!(state.sp, 0x44);
    assert_eq!(state.pc, 0x5566);
    assert_eq!(state.p, 0x77);
}

#[test]
fn test_cpu_save_state_roundtrip_includes_internal_flags() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    cpu.a = 0x11;
    cpu.x = 0x22;
    cpu.y = 0x33;
    cpu.sp = 0x44;
    cpu.pc = 0x5566;
    cpu.p = 0x77;
    cpu.halted = true;
    cpu.set_total_cycles(1234);
    cpu.delayed_i_flag = Some(true);
    cpu.nmi_pending = true;
    cpu.prev_need_nmi = true;
    cpu.prev_run_irq = true;
    cpu.run_irq = true;
    cpu.irq_pending = true;
    cpu.forced_irq_pending = true;
    cpu.skip_interrupt_latch_this_cycle = true;
    cpu.master_clock.set_master_cycles(111);
    cpu.master_clock.set_ppu_cycles(222);
    cpu.dmc_dma_phase = DmcDmaPhase::Halt;
    cpu.interrupt_stack = vec![InterruptKind::Irq, InterruptKind::Nmi];
    cpu.current_tick_info = Some((2, 5));

    let state = cpu.capture_state();

    let mut restored = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    restored.restore_state(&state);

    assert_eq!(restored.a, cpu.a);
    assert_eq!(restored.x, cpu.x);
    assert_eq!(restored.y, cpu.y);
    assert_eq!(restored.sp, cpu.sp);
    assert_eq!(restored.pc, cpu.pc);
    assert_eq!(restored.p, cpu.p);
    assert_eq!(restored.halted, cpu.halted);
    assert_eq!(restored.total_cycles, cpu.total_cycles);
    assert_eq!(restored.delayed_i_flag, cpu.delayed_i_flag);
    assert_eq!(restored.nmi_pending, cpu.nmi_pending);
    assert_eq!(restored.prev_need_nmi, cpu.prev_need_nmi);
    assert_eq!(restored.prev_run_irq, cpu.prev_run_irq);
    assert_eq!(restored.run_irq, cpu.run_irq);
    assert_eq!(restored.irq_pending, cpu.irq_pending);
    assert_eq!(restored.forced_irq_pending, cpu.forced_irq_pending);
    assert_eq!(
        restored.skip_interrupt_latch_this_cycle,
        cpu.skip_interrupt_latch_this_cycle
    );
    assert_eq!(
        restored.master_clock.master_cycles(),
        cpu.master_clock.master_cycles()
    );
    assert_eq!(
        restored.master_clock.ppu_cycles(),
        cpu.master_clock.ppu_cycles()
    );
    assert_eq!(restored.dmc_dma_phase, cpu.dmc_dma_phase);
    assert_eq!(restored.interrupt_stack, cpu.interrupt_stack);
    assert_eq!(restored.current_tick_info, cpu.current_tick_info);
}

#[test]
fn test_set_pc_for_tests() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    cpu.set_pc(0xC0DE);
    assert_eq!(cpu.pc, 0xC0DE);
}

#[test]
fn test_hard_reset_restores_power_on_register_defaults() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    map_minimal_cartridge_for_reset_vector(&mut cpu);

    // Arrange: dirty state.
    cpu.a = 0xFF;
    cpu.x = 0xEE;
    cpu.y = 0xDD;
    cpu.sp = 0x10;
    cpu.p = 0x00;

    // Act: hard reset (power-cycle behavior).
    cpu.reset(false);

    // Assert: power-on defaults restored (as per Cpu::new()) and reset sequence applied.
    // Cpu::new() initializes A/X/Y = 0, SP = 0x00, P = 0x20; the reset sequence subtracts
    // 3 from SP and sets I.
    assert_eq!(cpu.a, 0x00);
    assert_eq!(cpu.x, 0x00);
    assert_eq!(cpu.y, 0x00);
    assert_eq!(cpu.sp, 0xFD);
    assert_eq!(cpu.p & FLAG_UNUSED, FLAG_UNUSED);
    assert_ne!(cpu.p & FLAG_INTERRUPT, 0);
}

#[test]
fn test_soft_reset_sets_i_without_changing_other_flags() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    // Match blargg cpu_reset/registers.s (test #3):
    // Before reset: A/X/Y set, S=$12, P pulled from stack=$FB (i.e., D=1, I=0).
    map_minimal_cartridge_for_reset_vector(&mut cpu);
    cpu.a = 0x34;
    cpu.x = 0x56;
    cpu.y = 0x78;
    cpu.sp = 0x12;
    cpu.p = 0xFB;

    cpu.reset(true);

    // Reset-button behavior: set I, decrement S by 3, and otherwise preserve registers/flags.
    assert_eq!(cpu.a, 0x34);
    assert_eq!(cpu.x, 0x56);
    assert_eq!(cpu.y, 0x78);
    assert_eq!(cpu.sp, 0x0F);
    assert_eq!(cpu.p, 0xFF);
}

#[test]
fn test_execute_captures_pending_ppu_nmi_edge() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    // Minimal cartridge: reset vector -> $8000, NMI vector -> $9000.
    // Put NOP at both $8000 and $9000.
    let mut prg_rom = vec![0; 0x4000];
    // NMI vector ($FFFA)
    prg_rom[0x3FFA] = 0x00;
    prg_rom[0x3FFB] = 0x90;
    // Reset vector ($FFFC)
    prg_rom[0x3FFC] = 0x00;
    prg_rom[0x3FFD] = 0x80;
    // Code at $8000 and $9000
    prg_rom[0x0000] = 0xEA; // NOP at $8000
    prg_rom[0x1000] = 0xEA; // NOP at $9000
    let chr_rom = vec![0; 0x2000];
    let cartridge = Cartridge::from_parts(prg_rom, chr_rom, NametableLayout::Horizontal);
    cpu.bus.borrow_mut().map_cartridge(cartridge);

    cpu.reset(true);

    // Arrange: make PPU enter VBlank with NMI enabled, so poll_nmi() becomes true.
    ppu.borrow_mut().write_control(0x80);
    ppu.borrow_mut().run_ppu_cycles(241 * 341 + 1);

    // Act: execute an instruction. During CPU cycles, the CPU should poll the PPU for NMI.
    cpu.execute();

    // Assert: NMI was latched and then serviced (PC jumps to NMI vector), and the PPU NMI flag was consumed.
    assert_eq!(cpu.pc, 0x9000, "CPU should service NMI after execute()");
    assert_eq!(
        cpu.current_interrupt(),
        Some(InterruptKind::Nmi),
        "CPU should report being in NMI handler after vectoring"
    );
    assert!(
        !ppu.borrow_mut().poll_nmi(),
        "PPU NMI flag should be cleared once CPU polls it"
    );
}

#[test]
fn test_rti_clears_interrupt_indicator_after_nmi() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    // Minimal cartridge: reset vector -> $8000, NMI vector -> $9000.
    // Put NOP at $8000 and RTI at $9000.
    let mut prg_rom = vec![0; 0x4000];
    // NMI vector ($FFFA)
    prg_rom[0x3FFA] = 0x00;
    prg_rom[0x3FFB] = 0x90;
    // Reset vector ($FFFC)
    prg_rom[0x3FFC] = 0x00;
    prg_rom[0x3FFD] = 0x80;
    // Code at $8000 and $9000
    prg_rom[0x0000] = 0xEA; // NOP at $8000
    prg_rom[0x1000] = 0x40; // RTI at $9000
    let chr_rom = vec![0; 0x2000];
    let cartridge = Cartridge::from_parts(prg_rom, chr_rom, NametableLayout::Horizontal);
    cpu.bus.borrow_mut().map_cartridge(cartridge);

    cpu.reset(true);

    // Arrange: make PPU enter VBlank with NMI enabled.
    ppu.borrow_mut().write_control(0x80);
    ppu.borrow_mut().run_ppu_cycles(241 * 341 + 1);

    // Act: execute NOP, then the CPU should service NMI and jump to $9000.
    cpu.execute();
    assert_eq!(cpu.pc, 0x9000);
    assert_eq!(cpu.current_interrupt(), Some(InterruptKind::Nmi));

    // Act: execute RTI at $9000.
    cpu.execute();

    assert_eq!(
        cpu.current_interrupt(),
        None,
        "RTI should clear the interrupt indicator"
    );
    assert_eq!(
        cpu.pc, 0x8001,
        "RTI should return to the instruction after the interrupted one"
    );
}

#[test]
fn test_cpu_new_stores_provided_ppu_and_apu_instances() {
    let ppu = Rc::new(RefCell::new(crate::nes::ppu::Ppu::new_for_testing(
        TimingMode::Ntsc,
    )));
    let apu = Rc::new(RefCell::new(crate::nes::apu::Apu::new()));
    let app_context = Rc::new(RefCell::new(
        crate::platform::app_context::AppContext::new_with_config(
            crate::nes::console::Config::default(),
        ),
    ));
    let memory = Rc::new(RefCell::new(Bus::new(
        Rc::clone(&ppu),
        Rc::clone(&apu),
        app_context,
    )));

    let cpu = Cpu::new(TimingMode::Ntsc, memory, Rc::clone(&ppu), Rc::clone(&apu));

    assert!(Rc::ptr_eq(&cpu.ppu, &ppu));
    assert!(Rc::ptr_eq(&cpu.apu, &apu));
}

#[test]
fn test_oam_dma_even_cycle_costs_513_cycles() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, Rc::clone(&memory), ppu, apu);

    cpu.set_total_cycles(8);
    assert!(
        cpu.get_total_cycles().is_multiple_of(2),
        "Should start on even cycle"
    );

    memory.borrow_mut().write(0x4014, 0x02, false);

    let cycles_before = cpu.get_total_cycles();
    let dma_cycles = cpu
        .handle_oam_dma_if_pending()
        .expect("DMA should be pending");

    assert_eq!(dma_cycles, 514);
    assert_eq!(cpu.get_total_cycles() - cycles_before, 514);
}

#[test]
fn test_oam_dma_odd_cycle_costs_514_cycles() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, Rc::clone(&memory), ppu, apu);

    cpu.set_total_cycles(7);
    assert!(
        !cpu.get_total_cycles().is_multiple_of(2),
        "Should start on odd cycle"
    );

    memory.borrow_mut().write(0x4014, 0x02, false);

    let cycles_before = cpu.get_total_cycles();
    let dma_cycles = cpu
        .handle_oam_dma_if_pending()
        .expect("DMA should be pending");

    assert_eq!(dma_cycles, 513);
    assert_eq!(cpu.get_total_cycles() - cycles_before, 513);
}

#[test]
fn test_oam_dma_transfers_256_bytes_from_requested_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, Rc::clone(&memory), ppu, apu);

    // Set up test data in RAM at page $02 ($0200-$02FF)
    for i in 0..256u16 {
        memory
            .borrow_mut()
            .write(0x0200 + i, (i & 0xFF) as u8, false);
    }

    // Trigger OAM DMA from page $02
    memory.borrow_mut().write(0x4014, 0x02, false);
    cpu.handle_oam_dma_if_pending()
        .expect("DMA should be pending");

    // Verify all 256 bytes were copied to OAM by reading through $2004
    for i in 0..256u16 {
        memory.borrow_mut().write(0x2003, i as u8, false);
        let oam_byte = memory.borrow_mut().read(0x2004, false);
        let expected = if (i & 0x03) == 2 {
            ((i & 0xFF) as u8) & 0xE3
        } else {
            (i & 0xFF) as u8
        };
        assert_eq!(
            oam_byte, expected,
            "OAM byte {} should match source data (with attribute masking)",
            i
        );
    }
}

#[test]
fn test_oam_dma_ticks_mapper_expansion_audio() {
    // During OAM DMA, the CPU core is stalled but M2 keeps running; cartridge hardware
    // (mapper IRQ counters + expansion audio) continues advancing.
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, Rc::clone(&memory), ppu, apu);

    // Map a minimal iNES ROM with mapper 24 (VRC6).
    let mut rom = vec![
        b'N', b'E', b'S', 0x1A, 2,    // 32KB PRG
        1,    // 8KB CHR
        0x80, // flags6: mapper low nibble=8 (0x8<<4)
        0x10, // flags7: mapper high nibble=1 (0x1<<4)
        0, 0, 0, 0, 0, 0, 0, 0,
    ];
    rom.extend(vec![0u8; 2 * 16 * 1024]);
    rom.extend(vec![0u8; 8 * 1024]);

    let cart = crate::nes::cartridge::Cartridge::load_from_file(&rom, "cpu-reset-test.nes", None)
        .expect("cartridge should parse");
    memory.borrow_mut().map_cartridge(cart);

    // Configure VRC6 saw: rate=8, period=0, enable.
    // Output starts at 0 and becomes non-zero after a couple of mapper CPU cycles.
    memory.borrow_mut().write(0xB000, 0b0000_1000, false);
    memory.borrow_mut().write(0xB001, 0x00, false);
    memory.borrow_mut().write(0xB002, 0b1000_0000, false);

    let before = memory.borrow().mapper_expansion_audio_sample();
    assert_eq!(before, 0.0);

    // Trigger and run OAM DMA.
    memory.borrow_mut().write(0x4014, 0x02, false);
    cpu.handle_oam_dma_if_pending()
        .expect("DMA should be pending");

    let after = memory.borrow().mapper_expansion_audio_sample();
    assert!(
        after > 0.0,
        "expected expansion audio to advance during DMA"
    );
}

#[test]
fn test_oam_dma_returns_none_when_not_pending() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    assert_eq!(cpu.handle_oam_dma_if_pending(), None);
}

#[test]
fn test_cancelled_dmc_dma_after_halt_does_not_complete_sample_read() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    setup_pending_dmc_dma_with_sample(&mut cpu, &apu, 0xA5);

    cpu.set_total_cycles(1);
    cpu.start_dmc_dma();

    let halted_read_addr = 0x1234;
    let cycles_before_halt = cpu.get_total_cycles();

    cpu.before_cpu_cycle(false);
    let _ = cpu.bus.borrow_mut().read(halted_read_addr, false);
    cpu.after_cpu_cycle(false);

    assert_eq!(cpu.get_total_cycles(), cycles_before_halt + 1);

    apu.borrow_mut().write_enable(0);
    cpu.process_pending_dmc_dma(halted_read_addr);

    let dmc_state = apu.borrow().dmc().capture_state();
    assert!(
        dmc_state.sample_buffer.is_none(),
        "cancelled DMC DMA should discard the queued sample fetch"
    );
    assert!(
        !dmc_state.dma_pending,
        "cancelled DMC DMA should not remain pending after the discard"
    );
}

#[test]
fn test_dmc_dma_overlap_4016_exercises_halt_and_dummy_cycles() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    setup_pending_dmc_dma_with_sample(&mut cpu, &apu, 0xA5);

    cpu.set_total_cycles(0);
    let before = cpu.get_total_cycles();

    let _ = cpu.read(0x4016);

    let after = cpu.get_total_cycles();
    assert_eq!(
        after - before,
        4,
        "DMC overlap on $4016 should consume halt + dummy + get + retried CPU read"
    );
}

#[test]
fn test_dmc_dma_overlap_4017_get_cycle_returns_dmc_sample_value() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    setup_pending_dmc_dma_with_sample(&mut cpu, &apu, 0xA5);

    cpu.set_total_cycles(0);
    let value = cpu.read(0x4017);

    assert_eq!(
        value, 0xA5,
        "On the DMC get cycle, $4017 should observe the DMC sample byte on the bus"
    );
}

#[test]
fn test_should_skip_first_input_clock_for_aliasing_4016_read() {
    assert!(Cpu::should_skip_first_input_clock(0x4016, 0xC016));
}

#[test]
fn test_should_not_skip_first_input_clock_for_non_aliasing_4016_read() {
    assert!(!Cpu::should_skip_first_input_clock(0x4016, 0xC000));
}

#[test]
fn test_execute_does_not_service_irq_when_not_asserted() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    // Map a minimal cartridge so vectors and opcode fetches are valid.
    // Point reset vector ($FFFC) to $8000 and IRQ vector ($FFFE) to $9000.
    // Put NOP at $8000.
    let mut prg_rom = vec![0; 0x4000];
    prg_rom[0x3FFC] = 0x00;
    prg_rom[0x3FFD] = 0x80;
    prg_rom[0x3FFE] = 0x00;
    prg_rom[0x3FFF] = 0x90;
    prg_rom[0x0000] = 0xEA; // NOP at $8000
    let chr_rom = vec![0; 0x2000];
    let cartridge = Cartridge::from_parts(prg_rom, chr_rom, NametableLayout::Horizontal);
    cpu.bus.borrow_mut().map_cartridge(cartridge);

    cpu.reset(true);
    cpu.p &= !FLAG_INTERRUPT;
    let pc_before = cpu.pc;
    let sp_before = cpu.sp;
    let cycles_before = cpu.get_total_cycles();

    cpu.execute();

    assert_eq!(cpu.get_total_cycles(), cycles_before + 2);
    assert_eq!(cpu.pc, pc_before + 1);
    assert_eq!(cpu.sp, sp_before);
}

#[test]
fn test_execute_services_irq_after_instruction_and_sets_pc_and_stack() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    // Map a minimal cartridge so IRQ vector reads are valid.
    // Reset vector -> $8000, IRQ vector -> $9000.
    // Put NOP at $8000 and NOP at $9000.
    let mut prg_rom = vec![0; 0x4000];
    // IRQ vector ($FFFE)
    prg_rom[0x3FFE] = 0x00;
    prg_rom[0x3FFF] = 0x90;
    // Reset vector ($FFFC)
    prg_rom[0x3FFC] = 0x00;
    prg_rom[0x3FFD] = 0x80;
    // Code at $8000 and $9000
    prg_rom[0x0000] = 0xEA; // NOP at $8000
    prg_rom[0x1000] = 0xEA; // NOP at $9000
    let chr_rom = vec![0; 0x2000];
    let cartridge = Cartridge::from_parts(prg_rom, chr_rom, NametableLayout::Horizontal);
    cpu.bus.borrow_mut().map_cartridge(cartridge);

    cpu.reset(true);

    // Ensure interrupts are enabled
    cpu.p &= !FLAG_INTERRUPT;

    // Force an asserted IRQ line for this test by setting CPU state directly.
    // (APU polling integration is covered by NES/Blargg tests; here we focus on CPU-owned IRQ handling.)
    cpu.set_irq_pending(true);

    let pc_before = cpu.pc;
    let sp_before = cpu.sp;
    let p_before = cpu.p;
    let cycles_before = cpu.get_total_cycles();

    cpu.execute();

    // NOP (2 cycles) + IRQ sequence (7 cycles) = 9
    assert_eq!(cpu.get_total_cycles(), cycles_before + 9);

    // PC loaded from IRQ vector
    assert_eq!(cpu.pc, 0x9000);

    assert_eq!(
        cpu.current_interrupt(),
        Some(InterruptKind::Irq),
        "CPU should report being in IRQ handler after vectoring"
    );

    // I flag set
    assert_ne!(cpu.p & FLAG_INTERRUPT, 0);

    // Stack pointer decremented by 3
    assert_eq!(cpu.sp, sp_before.wrapping_sub(3));

    // Verify pushed PC and status in memory (high, low, P) at original stack addresses.
    // PC pushed is the address after the completed instruction.
    let pch_addr = 0x0100 | (sp_before as u16);
    let pcl_addr = 0x0100 | (sp_before.wrapping_sub(1) as u16);
    let p_addr = 0x0100 | (sp_before.wrapping_sub(2) as u16);

    let pch = memory.borrow_mut().read(pch_addr, false);
    let pcl = memory.borrow_mut().read(pcl_addr, false);
    let pushed_p = memory.borrow_mut().read(p_addr, false);

    let expected_return_pc = pc_before.wrapping_add(1);
    assert_eq!(pch, (expected_return_pc >> 8) as u8);
    assert_eq!(pcl, (expected_return_pc & 0xFF) as u8);

    let expected_pushed_p = (p_before & !FLAG_BREAK) | FLAG_UNUSED;
    assert_eq!(pushed_p, expected_pushed_p);
}

#[test]
fn test_execute_services_mapper_irq_after_instruction() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    // Build a minimal iNES ROM for MMC3 (mapper 4):
    // - 16KB PRG ROM (1 bank)
    // - 8KB CHR ROM (1 bank)
    // Vectors are stored at the end of the (fixed last) PRG bank.
    let mut prg_rom = vec![0; 0x4000];
    // Reset vector ($FFFC) -> $8000
    prg_rom[0x3FFC] = 0x00;
    prg_rom[0x3FFD] = 0x80;
    // IRQ vector ($FFFE) -> $9000
    prg_rom[0x3FFE] = 0x00;
    prg_rom[0x3FFF] = 0x90;
    // Code at $8000 and $9000
    prg_rom[0x0000] = 0xEA; // NOP at $8000
    prg_rom[0x1000] = 0xEA; // NOP at $9000
    let chr_rom = vec![0; 0x2000];

    let flags6 = 0x40; // mapper 4 in upper nibble, horizontal mirroring
    let mut rom = vec![
        b'N', b'E', b'S', 0x1A,   // iNES header
        1,      // PRG ROM size (16KB units)
        1,      // CHR ROM size (8KB units)
        flags6, // flags 6
        0,      // flags 7
        0, 0, 0, 0, 0, 0, 0, 0, // padding
    ];
    rom.extend_from_slice(&prg_rom);
    rom.extend_from_slice(&chr_rom);

    let cartridge = Cartridge::load_from_file(&rom, "cpu-mmc3-test.nes", None)
        .expect("MMC3 iNES ROM should parse");
    cpu.bus.borrow_mut().map_cartridge(cartridge);
    cpu.update_mapper_capability_flags();

    cpu.reset(true);
    cpu.p &= !FLAG_INTERRUPT; // allow IRQs

    // Program MMC3 scanline IRQ: latch=1, reload, enable.
    memory.borrow_mut().write_for_testing(0xC000, 1);
    memory.borrow_mut().write_for_testing(0xC001, 0);
    memory.borrow_mut().write_for_testing(0xE001, 0);

    // Generate two valid A12 rising edges (requires 8 low cycles each) so MMC3 asserts IRQ.
    for _ in 0..8 {
        memory
            .borrow_mut()
            .mapper_ppu_address_changed_for_test(0x0FFF);
    }
    memory
        .borrow_mut()
        .mapper_ppu_address_changed_for_test(0x1000);

    for _ in 0..8 {
        memory
            .borrow_mut()
            .mapper_ppu_address_changed_for_test(0x0FFF);
    }
    memory
        .borrow_mut()
        .mapper_ppu_address_changed_for_test(0x1000);

    assert!(
        memory.borrow().mapper_irq_pending(),
        "Mapper should have asserted IRQ before CPU executes"
    );

    cpu.execute();

    // Expect CPU to take IRQ after completing the instruction.
    assert_eq!(cpu.pc, 0x9000);
}

#[test]
fn test_execute_ticks_full_instruction_cycles_for_nop_ntsc() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    // Minimal cartridge: reset vector -> $8000, code at $8000 is NOP
    let mut prg_rom = vec![0; 0x4000];
    prg_rom[0x3FFC] = 0x00;
    prg_rom[0x3FFD] = 0x80;
    prg_rom[0x0000] = 0xEA; // NOP at $8000
    let chr_rom = vec![0; 0x2000];
    let cartridge = Cartridge::from_parts(prg_rom, chr_rom, NametableLayout::Horizontal);
    cpu.bus.borrow_mut().map_cartridge(cartridge);

    cpu.reset(true);

    let cpu_cycles_before = cpu.get_total_cycles();
    let ppu_before = ppu_dot(&ppu.borrow());
    let apu_before = apu.borrow().frame_counter().get_cycle_counter();

    cpu.execute();

    let cpu_cycles_after = cpu.get_total_cycles();
    let ppu_after = ppu_dot(&ppu.borrow());
    let apu_after = apu.borrow().frame_counter().get_cycle_counter();

    // NOP is 2 CPU cycles.
    assert_eq!(cpu_cycles_after - cpu_cycles_before, 2);

    // NTSC: 3 PPU cycles per CPU cycle.
    assert_eq!(ppu_after - ppu_before, 6);
    assert_eq!(apu_after - apu_before, 2);
}

#[test]
fn test_oam_dma_ticks_ppu_and_apu_for_dma_cycles_ntsc() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    cpu.set_total_cycles(8); // even cycle start => 513
    memory.borrow_mut().write(0x4014, 0x02, false);

    let ppu_before = ppu_dot(&ppu.borrow());
    let apu_before = apu.borrow().frame_counter().get_cycle_counter();

    let dma_cycles = cpu
        .handle_oam_dma_if_pending()
        .expect("DMA should be pending");

    let ppu_after = ppu_dot(&ppu.borrow());
    let apu_after = apu.borrow().frame_counter().get_cycle_counter();

    assert_eq!(ppu_after - ppu_before, dma_cycles as u64 * 3);
    assert_eq!(apu_after - apu_before, dma_cycles as u32);
}

#[test]
fn test_oam_dma_advances_master_clock_for_synthetic_cycles() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    cpu.set_total_cycles(8); // even cycle start => 514
    cpu.master_clock.set_master_cycles(0);
    cpu.master_clock.set_ppu_cycles(0);

    memory.borrow_mut().write(0x4014, 0x02, false);

    let dma_cycles = cpu
        .handle_oam_dma_if_pending()
        .expect("DMA should be pending");

    let expected_master_ticks = cpu.master_clock.cpu_divider() * dma_cycles as u64;
    assert_eq!(cpu.master_clock.master_cycles(), expected_master_ticks);
}

#[test]
fn test_oam_dma_pal_ticks_ppu_and_tracks_fractional_cycles() {
    let (ppu, apu, memory) = create_test_memory_for(TimingMode::Pal);
    let mut cpu = Cpu::new(
        TimingMode::Pal,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    cpu.set_total_cycles(8); // even cycle start => 513
    memory.borrow_mut().write(0x4014, 0x02, false);

    let ppu_before = ppu_dot(&ppu.borrow());
    let apu_before = apu.borrow().frame_counter().get_cycle_counter();

    let dma_cycles = cpu
        .handle_oam_dma_if_pending()
        .expect("DMA should be pending");

    let expected_total_ppu = dma_cycles as f64 * TimingMode::Pal.ppu_cycles_per_cpu_cycle();
    let expected_run_ppu = expected_total_ppu.floor() as u64;

    let ppu_after = ppu_dot(&ppu.borrow());
    let apu_after = apu.borrow().frame_counter().get_cycle_counter();

    assert_eq!(ppu_after - ppu_before, expected_run_ppu);
    assert_eq!(apu_after - apu_before, dma_cycles as u32);
}

#[test]
fn test_oam_dma_services_nmi_after_dma_and_ticks_ppu_apu_for_nmi_cycles() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    // Map a minimal cartridge so NMI vector reads are valid.
    // Point NMI vector ($FFFA) to $8000.
    let mut prg_rom = vec![0; 0x4000];
    prg_rom[0x3FFA] = 0x00;
    prg_rom[0x3FFB] = 0x80;
    // Also set reset vector to $8000 for completeness.
    prg_rom[0x3FFC] = 0x00;
    prg_rom[0x3FFD] = 0x80;
    let chr_rom = vec![0; 0x2000];
    let cartridge = Cartridge::from_parts(prg_rom, chr_rom, NametableLayout::Horizontal);
    cpu.bus.borrow_mut().map_cartridge(cartridge);

    // Enable NMI on VBlank (PPUCTRL bit 7)
    memory.borrow_mut().write(0x2000, 0x80, false);

    // Move PPU to just before VBlank start (scanline 241, pixel 0)
    let vblank_start_minus_one = 241u64 * 341u64;
    ppu.borrow_mut().run_ppu_cycles(vblank_start_minus_one);
    assert_eq!(ppu.borrow().scanline(), 241);
    assert_eq!(ppu.borrow().pixel(), 0);
    assert!(!ppu.borrow_mut().poll_nmi());

    cpu.set_total_cycles(8); // even cycle start => 513
    memory.borrow_mut().write(0x4014, 0x02, false);

    let cpu_before = cpu.get_total_cycles();
    let ppu_before = ppu_dot(&ppu.borrow());
    let apu_before = apu.borrow().frame_counter().get_cycle_counter();

    let dma_cycles = cpu
        .handle_oam_dma_if_pending()
        .expect("DMA should be pending");

    let cpu_after = cpu.get_total_cycles();
    let ppu_after = ppu_dot(&ppu.borrow());
    let apu_after = apu.borrow().frame_counter().get_cycle_counter();

    // NMI should have been taken (adds 7 CPU cycles) and PPU/APU should be ticked for them.
    assert_eq!(cpu_after - cpu_before, dma_cycles as u64 + 7);
    assert_eq!(ppu_after - ppu_before, dma_cycles as u64 * 3 + 7 * 3);
    assert_eq!(apu_after - apu_before, dma_cycles as u32 + 7);
}

type TestMemory = (Rc<RefCell<Ppu>>, Rc<RefCell<Apu>>, Rc<RefCell<Bus>>);

// Test helper function to create a Memory instance with a PPU/APU for testing
fn create_test_memory() -> TestMemory {
    let ppu = Rc::new(RefCell::new(crate::nes::ppu::Ppu::new_for_testing(
        TimingMode::Ntsc,
    )));
    let apu = Rc::new(RefCell::new(crate::nes::apu::Apu::new()));
    let config = crate::nes::console::Config {
        frontend: crate::platform::config::FrontendConfig {
            ram_init_mode: crate::nes::console::RamInitMode::Zero,
            ..Default::default()
        },
        ..Default::default()
    };
    let app_context = Rc::new(RefCell::new(
        crate::platform::app_context::AppContext::new_with_config(config),
    ));
    let memory = Rc::new(RefCell::new(Bus::new(
        Rc::clone(&ppu),
        Rc::clone(&apu),
        app_context,
    )));
    (ppu, apu, memory)
}

fn create_test_memory_for(tv_system: TimingMode) -> TestMemory {
    let ppu = Rc::new(RefCell::new(crate::nes::ppu::Ppu::new_for_testing(
        tv_system,
    )));
    let apu = Rc::new(RefCell::new(crate::nes::apu::Apu::new()));
    let config = crate::nes::console::Config {
        frontend: crate::platform::config::FrontendConfig {
            ram_init_mode: crate::nes::console::RamInitMode::Zero,
            ..Default::default()
        },
        ..Default::default()
    };
    let app_context = Rc::new(RefCell::new(
        crate::platform::app_context::AppContext::new_with_config(config),
    ));
    let memory = Rc::new(RefCell::new(Bus::new(
        Rc::clone(&ppu),
        Rc::clone(&apu),
        app_context,
    )));
    (ppu, apu, memory)
}

fn ppu_dot(ppu: &Ppu) -> u64 {
    (ppu.scanline() as u64) * 341 + (ppu.pixel() as u64)
}

// Test helper function to run the CPU until halted (KIL instruction)
fn run(cpu: &mut Cpu) {
    while !cpu.halted {
        cpu.execute();
    }
}

// Test helper function to load a program into memory and set reset vector
fn fake_cartridge(cpu: &mut Cpu, program: &[u8]) {
    // Create a fake cartridge with the program in PRG ROM
    // PRG ROM is 16KB (0x4000 bytes), mapped at $8000-$BFFF (and mirrored at $C000-$FFFF)
    let mut prg_rom = vec![0; 0x4000]; // 16KB

    // Place the program at the beginning of PRG ROM
    for (i, &byte) in program.iter().enumerate() {
        prg_rom[i] = byte;
    }

    // Set reset vector to point to 0x8000 (which is index 0x0000 in PRG ROM)
    // Reset vector is at 0xFFFC-0xFFFD
    // For 16KB ROM: (0xFFFC - 0x8000) % 0x4000 = 0x7FFC % 0x4000 = 0x3FFC
    prg_rom[0x3FFC] = 0x00; // Low byte of 0x8000
    prg_rom[0x3FFD] = 0x80; // High byte of 0x8000

    // Create CHR ROM with zeros only (8KB)
    let chr_rom = vec![0; 0x2000];

    let cartridge = Cartridge::from_parts(prg_rom, chr_rom, NametableLayout::Horizontal);

    cpu.bus.borrow_mut().map_cartridge(cartridge);
}

fn setup_pending_dmc_dma_with_sample(
    cpu: &mut Cpu,
    apu: &Rc<RefCell<crate::nes::apu::Apu>>,
    sample_byte: u8,
) {
    fake_cartridge(cpu, &[sample_byte]);

    let mut apu = apu.borrow_mut();
    apu.dmc_mut().write_sample_address(0x00);
    apu.dmc_mut().write_sample_length(0x00);
    apu.write_enable(0b0001_0000);
    apu.dmc_mut().debug_set_transfer_start_delay(0);
    apu.dmc_mut().debug_set_dma_pending(true);
}

#[test]
fn test_cpu_new() {
    let (ppu, apu, memory) = create_test_memory();
    let cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    assert_eq!(cpu.a, 0);
    assert_eq!(cpu.x, 0);
    assert_eq!(cpu.y, 0);
    assert_eq!(cpu.sp, 0x00); // SP starts at 0x00 before reset
    assert_eq!(cpu.pc, 0);
    assert_eq!(cpu.p, 0x20); // P starts at 0x20 before reset (only unused bit set)
}

#[test]
fn test_cpu_reset() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // Load a minimal program so reset vector is set up
    let program = vec![KIL];
    fake_cartridge(&mut cpu, &program);

    cpu.a = 0xFF;
    cpu.x = 0xFF;
    cpu.y = 0xFF;
    cpu.sp = 0x00;
    cpu.p = 0xFF;

    cpu.reset(true);

    // Reset should NOT modify A, X, Y
    assert_eq!(cpu.a, 0xFF);
    assert_eq!(cpu.x, 0xFF);
    assert_eq!(cpu.y, 0xFF);
    // Reset should subtract 3 from SP: 0x00 - 3 = 0xFD
    assert_eq!(cpu.sp, 0xFD);
    // Reset should set I flag: 0xFF | 0x04 = 0xFF (all flags set)
    assert_eq!(cpu.p, 0xFF);
}

#[test]
fn test_execute_kil_halts_cpu() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.execute();

    assert!(cpu.halted, "KIL should halt the CPU when executed");
    assert_eq!(cpu.pc, 0x8000, "KIL should not advance PC");
}

#[test]
fn test_adc_immediate() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_IMM, 0x20, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x10;
    run(&mut cpu);
    assert_eq!(cpu.a, 0x30);
    assert_eq!(cpu.p & FLAG_CARRY, 0); // Carry flag should be clear
    assert_eq!(cpu.p & FLAG_ZERO, 0); // Zero flag should be clear
    assert_eq!(cpu.p & FLAG_OVERFLOW, 0); // Overflow flag should be clear
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0); // Negative flag should be clear
}

#[test]
fn test_adc_immediate_with_carry() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_IMM, 0x20, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x10;
    cpu.p |= FLAG_CARRY; // Set carry flag
    run(&mut cpu);
    assert_eq!(cpu.a, 0x31); // 0x10 + 0x20 + 1 (carry)
    assert_eq!(cpu.p & FLAG_CARRY, 0); // Carry flag should be clear
}

#[test]
fn test_adc_immediate_carry_flag() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_IMM, 0x01, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0xFF;
    run(&mut cpu);
    assert_eq!(cpu.a, 0x00); // Wraps around
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // Carry flag should be set
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO); // Zero flag should be set
}

#[test]
fn test_adc_immediate_overflow_flag() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_IMM, 0x50, KIL]; // Add another positive
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x50; // Positive number
    run(&mut cpu);
    assert_eq!(cpu.a, 0xA0); // Result is negative in two's complement
    assert_eq!(cpu.p & FLAG_OVERFLOW, FLAG_OVERFLOW); // Overflow flag should be set
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE); // Negative flag should be set
}

#[test]
fn test_adc_immediate_negative_overflow() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_IMM, 0x80, KIL]; // Add -128
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x80; // -128 in two's complement
    run(&mut cpu);
    assert_eq!(cpu.a, 0x00);
    assert_eq!(cpu.p & FLAG_OVERFLOW, FLAG_OVERFLOW); // Overflow flag should be set
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // Carry flag should be set
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO); // Zero flag should be set
}

#[test]
fn test_adc_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x10;
    cpu.bus.borrow_mut().write(0x42, 0x33, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0x43);
}

#[test]
fn test_adc_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_ABS, 0x34, 0x12, KIL]; // Little-endian
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x20;
    cpu.bus.borrow_mut().write(0x1234, 0x55, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0x75);
}

#[test]
fn test_adc_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_ABSX, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x10;
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x1239, 0x44, false); // 0x1234 + 0x05
    run(&mut cpu);
    assert_eq!(cpu.a, 0x54);
}

#[test]
fn test_adc_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_ZPX, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x15;
    cpu.x = 0x03;
    cpu.bus.borrow_mut().write(0x45, 0x22, false); // 0x42 + 0x03
    run(&mut cpu);
    assert_eq!(cpu.a, 0x37);
}

#[test]
fn test_adc_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_ABSY, 0x00, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x08;
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x1010, 0x17, false); // 0x1000 + 0x10
    run(&mut cpu);
    assert_eq!(cpu.a, 0x1F);
}

#[test]
fn test_adc_indirect_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_INDX, 0x20, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x05;
    cpu.x = 0x04;
    cpu.bus.borrow_mut().write(0x24, 0x74, false); // Pointer at 0x20 + 0x04: low byte
    cpu.bus.borrow_mut().write(0x25, 0x10, false); // Pointer at 0x20 + 0x04: high byte
    cpu.bus.borrow_mut().write(0x1074, 0x33, false); // Value at address 0x1074
    run(&mut cpu);
    assert_eq!(cpu.a, 0x38);
}

#[test]
fn test_adc_indirect_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_INDY, 0x86, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x0A;
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x86, 0x28, false); // Pointer at 0x86: low byte
    cpu.bus.borrow_mut().write(0x87, 0x10, false); // Pointer at 0x86: high byte
    cpu.bus.borrow_mut().write(0x1038, 0x06, false); // Value at 0x1028 + 0x10
    run(&mut cpu);
    assert_eq!(cpu.a, 0x10);
}

#[test]
fn test_and_immediate() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![AND_IMM, 0b1010_1010, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b1111_0000;
    run(&mut cpu);
    assert_eq!(cpu.a, 0b1010_0000);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_and_immediate_zero_flag() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![AND_IMM, 0b0000_1111, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b1111_0000;
    run(&mut cpu);
    assert_eq!(cpu.a, 0);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_and_immediate_clears_negative_flag() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![AND_IMM, 0b0111_1111, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b1111_1111;
    cpu.p = FLAG_NEGATIVE; // Set negative flag initially
    run(&mut cpu);
    assert_eq!(cpu.a, 0b0111_1111);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_and_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![AND_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b1100_1100;
    cpu.bus.borrow_mut().write(0x42, 0b1010_1010, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0b1000_1000);
}

#[test]
fn test_and_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![AND_ZPX, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b1111_0000;
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x47, 0b0011_1111, false); // 0x42 + 0x05
    run(&mut cpu);
    assert_eq!(cpu.a, 0b0011_0000);
}

#[test]
fn test_and_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![AND_ABS, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b1010_1010;
    cpu.bus.borrow_mut().write(0x1234, 0b1100_1100, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0b1000_1000);
}

#[test]
fn test_and_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![AND_ABSX, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b1111_1111;
    cpu.x = 0x10;
    cpu.bus.borrow_mut().write(0x1244, 0b0101_0101, false); // 0x1234 + 0x10
    run(&mut cpu);
    assert_eq!(cpu.a, 0b0101_0101);
}

#[test]
fn test_and_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![AND_ABSY, 0x00, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b1100_0011;
    cpu.y = 0x20;
    cpu.bus.borrow_mut().write(0x1020, 0b0011_1100, false); // 0x1000 + 0x20
    run(&mut cpu);
    assert_eq!(cpu.a, 0b0000_0000);
}

#[test]
fn test_and_indirect_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![AND_INDX, 0x20, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b1111_0000;
    cpu.x = 0x04;
    cpu.bus.borrow_mut().write(0x24, 0x74, false); // Pointer at 0x20 + 0x04: low byte
    cpu.bus.borrow_mut().write(0x25, 0x10, false); // Pointer at 0x20 + 0x04: high byte
    cpu.bus.borrow_mut().write(0x1074, 0b0000_1111, false); // Value at address 0x1074
    run(&mut cpu);
    assert_eq!(cpu.a, 0b0000_0000);
}

#[test]
fn test_and_indirect_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![AND_INDY, 0x86, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b1010_1010;
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x86, 0x28, false); // Pointer at 0x86: low byte
    cpu.bus.borrow_mut().write(0x87, 0x10, false); // Pointer at 0x86: high byte
    cpu.bus.borrow_mut().write(0x1038, 0b1111_0000, false); // Value at 0x1028 + 0x10
    run(&mut cpu);
    assert_eq!(cpu.a, 0b1010_0000);
}

#[test]
fn test_asl_accumulator() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ASL_A, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b0100_0010;
    run(&mut cpu);
    assert_eq!(cpu.a, 0b1000_0100);
    assert_eq!(cpu.p & FLAG_CARRY, 0);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_asl_accumulator_sets_carry() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ASL_A, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b1000_0001;
    run(&mut cpu);
    assert_eq!(cpu.a, 0b0000_0010);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_asl_accumulator_sets_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ASL_A, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b1000_0000;
    run(&mut cpu);
    assert_eq!(cpu.a, 0);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_asl_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ASL_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x42, 0b0011_0011, false);
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x42, false), 0b0110_0110);
    assert_eq!(cpu.p & FLAG_CARRY, 0);
}

#[test]
fn test_asl_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ASL_ZPX, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x47, 0b1010_0101, false); // 0x42 + 0x05
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x47, false), 0b0100_1010);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
}

#[test]
fn test_asl_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ASL_ABS, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x1234, 0b0100_0001, false);
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x1234, false), 0b1000_0010);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_asl_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ASL_ABSXW, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x10;
    cpu.bus.borrow_mut().write(0x1244, 0b0000_0001, false); // 0x1234 + 0x10
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x1244, false), 0b0000_0010);
    assert_eq!(cpu.p & FLAG_CARRY, 0);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
}

#[test]
fn test_bit_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BIT_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b1111_0000;
    cpu.bus.borrow_mut().write(0x42, 0b1100_0011, false);
    run(&mut cpu);
    // A & memory = 0b1111_0000 & 0b1100_0011 = 0b1100_0000 (not zero)
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    // Bit 7 of memory is 1
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    // Bit 6 of memory is 1
    assert_eq!(cpu.p & FLAG_OVERFLOW, FLAG_OVERFLOW);
}

#[test]
fn test_bit_zero_page_sets_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BIT_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b0000_1111;
    cpu.bus.borrow_mut().write(0x42, 0b1111_0000, false);
    run(&mut cpu);
    // A & memory = 0b0000_1111 & 0b1111_0000 = 0b0000_0000 (zero)
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    // Bit 7 of memory is 1
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    // Bit 6 of memory is 1
    assert_eq!(cpu.p & FLAG_OVERFLOW, FLAG_OVERFLOW);
}

#[test]
fn test_bit_zero_page_clears_flags() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BIT_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b1111_1111;
    cpu.bus.borrow_mut().write(0x42, 0b0011_1111, false);
    run(&mut cpu);
    // A & memory = 0b1111_1111 & 0b0011_1111 = 0b0011_1111 (not zero)
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    // Bit 7 of memory is 0
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    // Bit 6 of memory is 0
    assert_eq!(cpu.p & FLAG_OVERFLOW, 0);
}

#[test]
fn test_bit_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BIT_ABS, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b1010_1010;
    cpu.bus.borrow_mut().write(0x1234, 0b0101_1010, false);
    run(&mut cpu);
    // A & memory = 0b1010_1010 & 0b0101_1010 = 0b0000_1010 (not zero)
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    // Bit 7 of memory is 0
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    // Bit 6 of memory is 1
    assert_eq!(cpu.p & FLAG_OVERFLOW, FLAG_OVERFLOW);
}

#[test]
fn test_bcc_branch_taken() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BCC, 0x02, 0x00, 0x00, KIL]; // Branch forward 2 bytes to skip padding
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p &= !FLAG_CARRY; // Ensure carry is clear
    run(&mut cpu);
    // PC should be at 0x8000 + 2 (after reading BCC and offset) + 2 (offset) = 0x8004
    assert_eq!(cpu.pc, 0x8004);
}

#[test]
fn test_bcc_branch_not_taken() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BCC, 0x05, KIL]; // Should not branch
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p |= FLAG_CARRY; // Set carry flag
    run(&mut cpu);
    // PC should be at 0x8000 + 2 (instruction) = 0x8002
    assert_eq!(cpu.pc, 0x8002);
}

#[test]
fn test_bcc_branch_backward() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // BRK at start, then BCC at offset 3 that branches back -5 bytes to the BRK
    let program = vec![KIL, 0x00, 0x00, BCC, 0xFB];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p &= !FLAG_CARRY; // Ensure carry is clear
    cpu.pc = 0x8003; // Start at offset 3 (the BCC instruction)
    run(&mut cpu);
    // Should branch back to 0x8000 where the BRK is
    assert_eq!(cpu.pc, 0x8000);
}

#[test]
fn test_bcs_branch_taken() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BCS, 0x01, 0x00, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p |= FLAG_CARRY; // Set carry flag
    run(&mut cpu);
    assert_eq!(cpu.pc, 0x8003);
}

#[test]
fn test_bcs_branch_not_taken() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BCS, 0x03, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p &= !FLAG_CARRY; // Clear carry flag
    run(&mut cpu);
    assert_eq!(cpu.pc, 0x8002);
}

#[test]
fn test_beq_branch_taken() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BEQ, 0x01, 0x00, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p |= FLAG_ZERO; // Set zero flag
    run(&mut cpu);
    assert_eq!(cpu.pc, 0x8003);
}

#[test]
fn test_beq_branch_not_taken() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BEQ, 0x02, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p &= !FLAG_ZERO; // Clear zero flag
    run(&mut cpu);
    assert_eq!(cpu.pc, 0x8002);
}

#[test]
fn test_bmi_branch_taken() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BMI, 0x01, 0x00, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p |= FLAG_NEGATIVE; // Set negative flag
    run(&mut cpu);
    assert_eq!(cpu.pc, 0x8003);
}

#[test]
fn test_bmi_branch_not_taken() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BMI, 0x04, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p &= !FLAG_NEGATIVE; // Clear negative flag
    run(&mut cpu);
    assert_eq!(cpu.pc, 0x8002);
}

#[test]
fn test_bne_branch_taken() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BNE, 0x01, 0x00, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p &= !FLAG_ZERO; // Clear zero flag (not equal)
    run(&mut cpu);
    assert_eq!(cpu.pc, 0x8003);
}

#[test]
fn test_bne_branch_not_taken() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BNE, 0x06, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p |= FLAG_ZERO; // Set zero flag (equal)
    run(&mut cpu);
    assert_eq!(cpu.pc, 0x8002);
}

#[test]
fn test_bpl_branch_taken() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BPL, 0x01, 0x00, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p &= !FLAG_NEGATIVE; // Clear negative flag (positive)
    run(&mut cpu);
    assert_eq!(cpu.pc, 0x8003);
}

#[test]
fn test_bpl_branch_not_taken() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BPL, 0x07, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p |= FLAG_NEGATIVE; // Set negative flag
    run(&mut cpu);
    assert_eq!(cpu.pc, 0x8002);
}

#[test]
fn test_bvc_branch_taken() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BVC, 0x01, 0x00, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p &= !FLAG_OVERFLOW; // Clear overflow flag
    run(&mut cpu);
    assert_eq!(cpu.pc, 0x8003);
}

#[test]
fn test_bvc_branch_not_taken() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BVC, 0x05, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p |= FLAG_OVERFLOW; // Set overflow flag
    run(&mut cpu);
    assert_eq!(cpu.pc, 0x8002);
}

#[test]
fn test_bvs_branch_taken() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BVS, 0x01, 0x00, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p |= FLAG_OVERFLOW; // Set overflow flag
    run(&mut cpu);
    assert_eq!(cpu.pc, 0x8003);
}

#[test]
fn test_bvs_branch_not_taken() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BVS, 0x08, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p &= !FLAG_OVERFLOW; // Clear overflow flag
    run(&mut cpu);
    assert_eq!(cpu.pc, 0x8002);
}

#[test]
fn test_cmp_immediate_equal() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CMP_IMM, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x42;
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO); // A == value
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // A >= value
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0); // Result is 0, bit 7 = 0
}

#[test]
fn test_cmp_immediate_greater() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CMP_IMM, 0x30, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x50;
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_ZERO, 0); // A != value
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // A >= value
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0); // Result is positive
}

#[test]
fn test_cmp_immediate_less() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CMP_IMM, 0x50, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x30;
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_ZERO, 0); // A != value
    assert_eq!(cpu.p & FLAG_CARRY, 0); // A < value
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE); // Result is negative (0x30 - 0x50 = 0xE0)
}

#[test]
fn test_cmp_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CMP_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x80;
    cpu.bus.borrow_mut().write(0x42, 0x80, false);
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
}

#[test]
fn test_cmp_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CMP_ZPX, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x10;
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x47, 0x05, false); // 0x42 + 0x05
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // 0x10 >= 0x05
}

#[test]
fn test_cmp_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CMP_ABS, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x20;
    cpu.bus.borrow_mut().write(0x1234, 0x30, false);
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_CARRY, 0); // 0x20 < 0x30
}

#[test]
fn test_cmp_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CMP_ABSX, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0xFF;
    cpu.x = 0x10;
    cpu.bus.borrow_mut().write(0x1244, 0xFF, false);
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
}

#[test]
fn test_cmp_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CMP_ABSY, 0x00, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x55;
    cpu.y = 0x20;
    cpu.bus.borrow_mut().write(0x1020, 0x44, false);
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // 0x55 >= 0x44
}

#[test]
fn test_cmp_indirect_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CMP_INDX, 0x20, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x33;
    cpu.x = 0x04;
    cpu.bus.borrow_mut().write(0x24, 0x74, false);
    cpu.bus.borrow_mut().write(0x25, 0x10, false);
    cpu.bus.borrow_mut().write(0x1074, 0x33, false);
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
}

#[test]
fn test_cmp_indirect_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CMP_INDY, 0x86, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x77;
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x86, 0x28, false);
    cpu.bus.borrow_mut().write(0x87, 0x10, false);
    cpu.bus.borrow_mut().write(0x1038, 0x88, false);
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_CARRY, 0); // 0x77 < 0x88
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_cpx_immediate_equal() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CPX_IMM, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x42;
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_cpx_immediate_greater() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CPX_IMM, 0x30, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x50;
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_cpx_immediate_less() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CPX_IMM, 0x50, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x30;
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_CARRY, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_cpx_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CPX_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x80;
    cpu.bus.borrow_mut().write(0x42, 0x80, false);
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
}

#[test]
fn test_cpx_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CPX_ABS, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x20;
    cpu.bus.borrow_mut().write(0x1234, 0x30, false);
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_CARRY, 0); // 0x20 < 0x30
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_cpy_immediate_equal() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CPY_IMM, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x42;
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_cpy_immediate_greater() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CPY_IMM, 0x30, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x50;
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_cpy_immediate_less() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CPY_IMM, 0x50, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x30;
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_CARRY, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_cpy_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CPY_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x80;
    cpu.bus.borrow_mut().write(0x42, 0x80, false);
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
}

#[test]
fn test_cpy_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CPY_ABS, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x20;
    cpu.bus.borrow_mut().write(0x1234, 0x30, false);
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_CARRY, 0); // 0x20 < 0x30
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_dec_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DEC_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x42, 0x50, false);
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x42, false), 0x4F);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_dec_zero_page_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DEC_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x42, 0x01, false);
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x42, false), 0x00);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_dec_zero_page_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DEC_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x42, 0x00, false);
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x42, false), 0xFF);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_dec_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DEC_ZPX, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x47, 0x80, false);
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x47, false), 0x7F);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_dec_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DEC_ABS, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x1234, 0x30, false);
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x1234, false), 0x2F);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_dec_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DEC_ABSXW, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x10;
    cpu.bus.borrow_mut().write(0x1244, 0x90, false);
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x1244, false), 0x8F);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_eor_immediate() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![EOR_IMM, 0b1111_0000, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b1010_1010;
    run(&mut cpu);
    assert_eq!(cpu.a, 0b0101_1010);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_eor_immediate_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![EOR_IMM, 0b1010_1010, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b1010_1010;
    run(&mut cpu);
    assert_eq!(cpu.a, 0x00);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
}

#[test]
fn test_eor_immediate_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![EOR_IMM, 0b1111_0000, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b0101_0101;
    run(&mut cpu);
    assert_eq!(cpu.a, 0b1010_0101);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_eor_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![EOR_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0xFF;
    cpu.bus.borrow_mut().write(0x42, 0x0F, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0xF0);
}

#[test]
fn test_eor_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![EOR_ZPX, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0xFF;
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x47, 0x55, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0xAA);
}

#[test]
fn test_eor_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![EOR_ABS, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x12;
    cpu.bus.borrow_mut().write(0x1234, 0x34, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0x26);
}

#[test]
fn test_eor_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![EOR_ABSX, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0xAA;
    cpu.x = 0x10;
    cpu.bus.borrow_mut().write(0x1244, 0x55, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0xFF);
}

#[test]
fn test_eor_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![EOR_ABSY, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0xF0;
    cpu.y = 0x20;
    cpu.bus.borrow_mut().write(0x1254, 0x0F, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0xFF);
}

#[test]
fn test_eor_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![EOR_INDX, 0x20, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b1100_0011;
    cpu.x = 0x04;
    cpu.bus.borrow_mut().write(0x24, 0x74, false);
    cpu.bus.borrow_mut().write(0x25, 0x10, false);
    cpu.bus.borrow_mut().write(0x1074, 0b0011_1100, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0b1111_1111);
}

#[test]
fn test_eor_indirect_indexed() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![EOR_INDY, 0x86, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b1010_0101;
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x86, 0x28, false);
    cpu.bus.borrow_mut().write(0x87, 0x10, false);
    cpu.bus.borrow_mut().write(0x1038, 0b0101_1010, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0xFF);
}

#[test]
fn test_clc() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CLC, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p = FLAG_CARRY;
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_CARRY, 0);
}

#[test]
fn test_cld() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CLD, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p = FLAG_DECIMAL;
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_DECIMAL, 0);
}

#[test]
fn test_cli() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CLI, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p = FLAG_INTERRUPT;
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_INTERRUPT, 0);
}

#[test]
fn test_clv() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CLV, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p = FLAG_OVERFLOW;
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_OVERFLOW, 0);
}

#[test]
fn test_sec() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SEC, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p = 0;
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
}

#[test]
fn test_sed() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SED, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p = 0;
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_DECIMAL, FLAG_DECIMAL);
}

#[test]
fn test_sei() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SEI, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p = 0;
    run(&mut cpu);
    assert_eq!(cpu.p & FLAG_INTERRUPT, FLAG_INTERRUPT);
}

#[test]
fn test_inc_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![INC_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x42, 0x50, false);
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x42, false), 0x51);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_inc_zero_page_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![INC_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x42, 0xFF, false);
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x42, false), 0x00);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_inc_zero_page_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![INC_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x42, 0x7F, false);
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x42, false), 0x80);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_inc_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![INC_ZPX, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x47, 0x20, false);
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x47, false), 0x21);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_inc_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![INC_ABS, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x1234, 0x30, false);
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x1234, false), 0x31);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_inc_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![INC_ABSXW, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x10;
    cpu.bus.borrow_mut().write(0x1244, 0x8F, false);
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x1244, false), 0x90);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_jmp_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    fake_cartridge(&mut cpu, &[]);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x0600, JMP_ABS, false);
    cpu.bus.borrow_mut().write(0x0601, 0x34, false);
    cpu.bus.borrow_mut().write(0x0602, 0x12, false);
    cpu.bus.borrow_mut().write(0x1234, KIL, false);
    cpu.pc = 0x0600;
    run(&mut cpu);
    assert_eq!(cpu.pc, 0x1234); // BRK at 0x1234
}

#[test]
fn test_jmp_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    fake_cartridge(&mut cpu, &[]);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x0600, JMP_IND, false);
    cpu.bus.borrow_mut().write(0x0601, 0x20, false);
    cpu.bus.borrow_mut().write(0x0602, 0x10, false);
    cpu.bus.borrow_mut().write(0x1020, 0x56, false);
    cpu.bus.borrow_mut().write(0x1021, 0x18, false);
    cpu.bus.borrow_mut().write(0x1856, KIL, false);
    cpu.pc = 0x0600;
    run(&mut cpu);
    assert_eq!(cpu.pc, 0x1856); // BRK at 0x1856
}

#[test]
fn test_jmp_indirect_page_boundary_bug() {
    // The 6502 has a bug where if the indirect address is on a page boundary
    // (e.g., 0x10FF), it doesn't cross the page boundary to read the high byte
    // Instead of reading from 0x1100, it wraps around to 0x1000
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    fake_cartridge(&mut cpu, &[]);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x0600, JMP_IND, false);
    cpu.bus.borrow_mut().write(0x0601, 0xFF, false);
    cpu.bus.borrow_mut().write(0x0602, 0x10, false);
    cpu.bus.borrow_mut().write(0x10FF, 0x34, false);
    cpu.bus.borrow_mut().write(0x1000, 0x12, false); // Wraps to start of page, not 0x1100
    cpu.bus.borrow_mut().write(0x1234, KIL, false);
    cpu.pc = 0x0600;
    run(&mut cpu);
    assert_eq!(cpu.pc, 0x1234); // Should jump to 0x1234 (low=0x34, high=0x12)
}

#[test]
fn test_jsr() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    fake_cartridge(&mut cpu, &[]);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x0600, JSR, false);
    cpu.bus.borrow_mut().write(0x0601, 0x34, false);
    cpu.bus.borrow_mut().write(0x0602, 0x12, false);
    cpu.bus.borrow_mut().write(0x1234, KIL, false);
    cpu.pc = 0x0600;
    cpu.sp = 0xFF;
    run(&mut cpu);
    assert_eq!(cpu.pc, 0x1234); // BRK at 0x1234
    assert_eq!(cpu.sp, 0xFD); // SP decremented by 2 (pushed 2 bytes)
    // Return address should be 0x0602 (address of last byte of JSR instruction)
    assert_eq!(cpu.bus.borrow_mut().read(0x01FF, false), 0x06); // High byte of return address
    assert_eq!(cpu.bus.borrow_mut().read(0x01FE, false), 0x02); // Low byte of return address
}

#[test]
fn test_lda_immediate() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_IMM, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    run(&mut cpu);
    assert_eq!(cpu.a, 0x42);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_lda_immediate_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_IMM, 0x00, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    run(&mut cpu);
    assert_eq!(cpu.a, 0x00);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
}

#[test]
fn test_lda_immediate_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_IMM, 0x80, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    run(&mut cpu);
    assert_eq!(cpu.a, 0x80);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_lda_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x42, 0x55, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0x55);
}

#[test]
fn test_lda_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_ZPX, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x47, 0xAA, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0xAA);
}

#[test]
fn test_lda_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_ABS, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x1234, 0x77, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0x77);
}

#[test]
fn test_lda_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_ABSX, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x10;
    cpu.bus.borrow_mut().write(0x1244, 0x88, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0x88);
}

#[test]
fn test_lda_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_ABSY, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x20;
    cpu.bus.borrow_mut().write(0x1254, 0x99, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0x99);
}

#[test]
fn test_lda_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_INDX, 0x20, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x04;
    cpu.bus.borrow_mut().write(0x24, 0x74, false);
    cpu.bus.borrow_mut().write(0x25, 0x10, false);
    cpu.bus.borrow_mut().write(0x1074, 0xCC, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0xCC);
}

#[test]
fn test_lda_indirect_indexed() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_INDY, 0x86, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x86, 0x28, false);
    cpu.bus.borrow_mut().write(0x87, 0x10, false);
    cpu.bus.borrow_mut().write(0x1038, 0xDD, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0xDD);
}

#[test]
fn test_ldx_immediate() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDX_IMM, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    run(&mut cpu);
    assert_eq!(cpu.x, 0x42);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_ldx_immediate_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDX_IMM, 0x00, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    run(&mut cpu);
    assert_eq!(cpu.x, 0x00);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
}

#[test]
fn test_ldx_immediate_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDX_IMM, 0x80, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    run(&mut cpu);
    assert_eq!(cpu.x, 0x80);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_ldx_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDX_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x42, 0x55, false);
    run(&mut cpu);
    assert_eq!(cpu.x, 0x55);
}

#[test]
fn test_ldx_zero_page_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDX_ZPY, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x05;
    cpu.bus.borrow_mut().write(0x47, 0xAA, false);
    run(&mut cpu);
    assert_eq!(cpu.x, 0xAA);
}

#[test]
fn test_ldx_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDX_ABS, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x1234, 0x77, false);
    run(&mut cpu);
    assert_eq!(cpu.x, 0x77);
}

#[test]
fn test_ldx_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDX_ABSY, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x20;
    cpu.bus.borrow_mut().write(0x1254, 0x99, false);
    run(&mut cpu);
    assert_eq!(cpu.x, 0x99);
}

#[test]
fn test_ldy_immediate() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDY_IMM, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    run(&mut cpu);
    assert_eq!(cpu.y, 0x42);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_ldy_immediate_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDY_IMM, 0x00, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    run(&mut cpu);
    assert_eq!(cpu.y, 0x00);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
}

#[test]
fn test_ldy_immediate_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDY_IMM, 0x80, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    run(&mut cpu);
    assert_eq!(cpu.y, 0x80);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_ldy_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDY_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x42, 0x55, false);
    run(&mut cpu);
    assert_eq!(cpu.y, 0x55);
}

#[test]
fn test_ldy_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDY_ZPX, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x47, 0xAA, false);
    run(&mut cpu);
    assert_eq!(cpu.y, 0xAA);
}

#[test]
fn test_ldy_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDY_ABS, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x1234, 0x77, false);
    run(&mut cpu);
    assert_eq!(cpu.y, 0x77);
}

#[test]
fn test_ldy_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDY_ABSX, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x10;
    cpu.bus.borrow_mut().write(0x1244, 0x88, false);
    run(&mut cpu);
    assert_eq!(cpu.y, 0x88);
}

#[test]
fn test_lsr_accumulator() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LSR_ACC, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b10110101;
    run(&mut cpu);
    assert_eq!(cpu.a, 0b01011010);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_lsr_accumulator_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LSR_ACC, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b00000001;
    run(&mut cpu);
    assert_eq!(cpu.a, 0);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
}

#[test]
fn test_lsr_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LSR_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x42, 0b11001100, false);
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x42, false), 0b01100110);
    assert_eq!(cpu.p & FLAG_CARRY, 0);
}

#[test]
fn test_lsr_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LSR_ZPX, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x47, 0b10101011, false);
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x47, false), 0b01010101);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
}

#[test]
fn test_lsr_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LSR_ABS, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x1234, 0b01010100, false);
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x1234, false), 0b00101010);
    assert_eq!(cpu.p & FLAG_CARRY, 0);
}

#[test]
fn test_lsr_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LSR_ABSXW, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x10;
    cpu.bus.borrow_mut().write(0x1244, 0b00000011, false);
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x1244, false), 0b00000001);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
}

#[test]
fn test_nop() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![NOP, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x42;
    cpu.x = 0x33;
    cpu.y = 0x24;
    cpu.p = 0xFF;
    run(&mut cpu);
    // NOP should not affect any registers or flags
    assert_eq!(cpu.a, 0x42);
    assert_eq!(cpu.x, 0x33);
    assert_eq!(cpu.y, 0x24);
    assert_eq!(cpu.p, 0xFF);
}

#[test]
fn test_ora_immediate() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ORA_IMM, 0b01010101, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b10101010;
    run(&mut cpu);
    assert_eq!(cpu.a, 0b11111111);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_ora_immediate_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ORA_IMM, 0x00, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x00;
    run(&mut cpu);
    assert_eq!(cpu.a, 0x00);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
}

#[test]
fn test_ora_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ORA_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b11110000;
    cpu.bus.borrow_mut().write(0x42, 0b00001111, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0b11111111);
}

#[test]
fn test_ora_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ORA_ZPX, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b10000000;
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x47, 0b01000000, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0b11000000);
}

#[test]
fn test_ora_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ORA_ABS, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b00110011;
    cpu.bus.borrow_mut().write(0x1234, 0b11001100, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0b11111111);
}

#[test]
fn test_ora_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ORA_ABSX, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b00001111;
    cpu.x = 0x10;
    cpu.bus.borrow_mut().write(0x1244, 0b11110000, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0b11111111);
}

#[test]
fn test_ora_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ORA_ABSY, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b01010101;
    cpu.y = 0x20;
    cpu.bus.borrow_mut().write(0x1254, 0b10101010, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0b11111111);
}

#[test]
fn test_ora_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ORA_INDX, 0x82, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b00110011;
    cpu.x = 0x04;
    cpu.bus.borrow_mut().write(0x86, 0x34, false);
    cpu.bus.borrow_mut().write(0x87, 0x12, false);
    cpu.bus.borrow_mut().write(0x1234, 0b11001100, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0b11111111);
}

#[test]
fn test_ora_indirect_indexed() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ORA_INDY, 0x86, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b10101010;
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x86, 0x28, false);
    cpu.bus.borrow_mut().write(0x87, 0x10, false);
    cpu.bus.borrow_mut().write(0x1038, 0b01010101, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0b11111111);
}

#[test]
fn test_ora_negative_flag() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ORA_IMM, 0x80, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x00;
    run(&mut cpu);
    assert_eq!(cpu.a, 0x80);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
}

#[test]
fn test_dex() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DEX, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x42;
    run(&mut cpu);
    assert_eq!(cpu.x, 0x41);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_dex_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DEX, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x01;
    run(&mut cpu);
    assert_eq!(cpu.x, 0x00);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
}

#[test]
fn test_dex_wrap() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DEX, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x00;
    run(&mut cpu);
    assert_eq!(cpu.x, 0xFF);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_dey() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DEY, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x42;
    run(&mut cpu);
    assert_eq!(cpu.y, 0x41);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_inx() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![INX, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x42;
    run(&mut cpu);
    assert_eq!(cpu.x, 0x43);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_inx_wrap() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![INX, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0xFF;
    run(&mut cpu);
    assert_eq!(cpu.x, 0x00);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
}

#[test]
fn test_iny() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![INY, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x42;
    run(&mut cpu);
    assert_eq!(cpu.y, 0x43);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_tax() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TAX, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x42;
    run(&mut cpu);
    assert_eq!(cpu.x, 0x42);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_tax_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TAX, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x00;
    run(&mut cpu);
    assert_eq!(cpu.x, 0x00);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
}

#[test]
fn test_tax_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TAX, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x80;
    run(&mut cpu);
    assert_eq!(cpu.x, 0x80);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_tay() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TAY, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x42;
    run(&mut cpu);
    assert_eq!(cpu.y, 0x42);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_txa() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TXA, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x42;
    run(&mut cpu);
    assert_eq!(cpu.a, 0x42);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_tya() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TYA, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x42;
    run(&mut cpu);
    assert_eq!(cpu.a, 0x42);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_rol_accumulator() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROL_ACC, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b10110101;
    cpu.p = 0; // Clear carry
    run(&mut cpu);
    assert_eq!(cpu.a, 0b01101010);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_rol_accumulator_with_carry() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROL_ACC, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b01010101;
    cpu.p = FLAG_CARRY; // Set carry
    run(&mut cpu);
    assert_eq!(cpu.a, 0b10101011);
    assert_eq!(cpu.p & FLAG_CARRY, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_rol_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROL_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x42, 0b11001100, false);
    cpu.p = 0;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x42, false), 0b10011000);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
}

#[test]
fn test_rol_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROL_ZPX, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x47, 0b10101011, false);
    cpu.p = FLAG_CARRY;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x47, false), 0b01010111);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
}

#[test]
fn test_rol_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROL_ABS, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x1234, 0b01010100, false);
    cpu.p = 0;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x1234, false), 0b10101000);
    assert_eq!(cpu.p & FLAG_CARRY, 0);
}

#[test]
fn test_rol_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROL_ABSXW, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x10;
    cpu.bus.borrow_mut().write(0x1244, 0b00000011, false);
    cpu.p = 0;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x1244, false), 0b00000110);
    assert_eq!(cpu.p & FLAG_CARRY, 0);
}

#[test]
fn test_ror_accumulator() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROR_ACC, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b10110101;
    cpu.p = 0; // Clear carry
    run(&mut cpu);
    assert_eq!(cpu.a, 0b01011010);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_ror_accumulator_with_carry() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROR_ACC, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b01010101;
    cpu.p = FLAG_CARRY; // Set carry
    run(&mut cpu);
    assert_eq!(cpu.a, 0b10101010);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_ror_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROR_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x42, 0b11001100, false);
    cpu.p = 0;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x42, false), 0b01100110);
    assert_eq!(cpu.p & FLAG_CARRY, 0);
}

#[test]
fn test_ror_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROR_ZPX, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x47, 0b10101011, false);
    cpu.p = FLAG_CARRY;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x47, false), 0b11010101);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
}

#[test]
fn test_ror_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROR_ABS, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x1234, 0b01010100, false);
    cpu.p = 0;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x1234, false), 0b00101010);
    assert_eq!(cpu.p & FLAG_CARRY, 0);
}

#[test]
fn test_ror_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROR_ABSXW, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x10;
    cpu.bus.borrow_mut().write(0x1244, 0b00000011, false);
    cpu.p = 0;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x1244, false), 0b00000001);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
}

#[test]
fn test_rti() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![RTI, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    // Set up stack with saved processor status and return address
    cpu.sp = 0xFC;
    cpu.bus.borrow_mut().write(0x01FD, 0b11010011, false); // Saved status flags
    cpu.bus.borrow_mut().write(0x01FE, 0x34, false); // PC low byte
    cpu.bus.borrow_mut().write(0x01FF, 0x12, false); // PC high byte
    cpu.bus.borrow_mut().write(0x1234, KIL, false); // BRK at return address
    run(&mut cpu);
    // RTI should behave like PLP - ignore B flag and unused bit, always set unused to 1
    // 0b11010011 with B flag cleared and unused set: 0b11100011 = 0xE3
    assert_eq!(cpu.p, 0b11100011);
    assert_eq!(cpu.pc, 0x1234); // BRK instruction
    assert_eq!(cpu.sp, 0xFF);
}

#[test]
fn test_rts() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![RTS, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    // Set up stack with saved return address (PC-1)
    cpu.sp = 0xFD;
    cpu.bus.borrow_mut().write(0x01FE, 0x33, false); // PC-1 low byte (0x1233)
    cpu.bus.borrow_mut().write(0x01FF, 0x12, false); // PC-1 high byte
    cpu.bus.borrow_mut().write(0x1234, KIL, false); // BRK at return address
    run(&mut cpu);
    assert_eq!(cpu.pc, 0x1234); // BRK instruction (0x1234)
    assert_eq!(cpu.sp, 0xFF);
}

#[test]
fn test_sbc_immediate() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_IMM, 0x30, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x50;
    cpu.p |= FLAG_CARRY; // Set carry (no borrow)
    run(&mut cpu);
    assert_eq!(cpu.a, 0x20);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_sbc_immediate_with_borrow() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_IMM, 0x30, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x50;
    cpu.p &= !FLAG_CARRY; // Clear carry (borrow)
    run(&mut cpu);
    assert_eq!(cpu.a, 0x1F);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
}

#[test]
fn test_sbc_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x80;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x42, 0x40, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0x40);
}

#[test]
fn test_sbc_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_ZPX, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x50;
    cpu.x = 0x05;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x47, 0x10, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0x40);
}

#[test]
fn test_sbc_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_ABS, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x60;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x1234, 0x20, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0x40);
}

#[test]
fn test_sbc_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_ABSX, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x70;
    cpu.x = 0x10;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x1244, 0x30, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0x40);
}

#[test]
fn test_sbc_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_ABSY, 0x34, 0x12, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x90;
    cpu.y = 0x20;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x1254, 0x50, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0x40);
}

#[test]
fn test_sbc_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_INDX, 0x82, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0xA0;
    cpu.x = 0x04;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x86, 0x34, false);
    cpu.bus.borrow_mut().write(0x87, 0x12, false);
    cpu.bus.borrow_mut().write(0x1234, 0x60, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0x40);
}

#[test]
fn test_sbc_indirect_indexed() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_INDY, 0x86, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0xB0;
    cpu.y = 0x10;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x86, 0x28, false);
    cpu.bus.borrow_mut().write(0x87, 0x10, false);
    cpu.bus.borrow_mut().write(0x1038, 0x70, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0x40);
}

#[test]
fn test_sbc_overflow() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_IMM, 0xB0, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x50;
    cpu.p |= FLAG_CARRY;
    run(&mut cpu);
    assert_eq!(cpu.a, 0xA0);
    assert_eq!(cpu.p & FLAG_OVERFLOW, FLAG_OVERFLOW);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_sta_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STA_ZP, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x42;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x10, false), 0x42);
}

#[test]
fn test_sta_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STA_ZPX, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x42;
    cpu.x = 0x05;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x15, false), 0x42);
}

#[test]
fn test_sta_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STA_ABS, 0x00, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x42;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x1000, false), 0x42);
}

#[test]
fn test_sta_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STA_ABSXW, 0x00, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x42;
    cpu.x = 0x05;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x1005, false), 0x42);
}

#[test]
fn test_sta_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STA_ABSYW, 0x00, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x42;
    cpu.y = 0x05;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x1005, false), 0x42);
}

#[test]
fn test_sta_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STA_INDX, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x42;
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x15, 0x00, false);
    cpu.bus.borrow_mut().write(0x16, 0x10, false);
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x1000, false), 0x42);
}

#[test]
fn test_sta_indirect_indexed() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STA_INDYW, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x42;
    cpu.y = 0x05;
    cpu.bus.borrow_mut().write(0x10, 0x00, false);
    cpu.bus.borrow_mut().write(0x11, 0x10, false);
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x1005, false), 0x42);
}

#[test]
fn test_txs() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TXS, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0xFF;
    run(&mut cpu);
    assert_eq!(cpu.sp, 0xFF);
}

#[test]
fn test_tsx() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TSX, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.sp = 0xAB;
    run(&mut cpu);
    assert_eq!(cpu.x, 0xAB);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_tsx_zero_flag() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TSX, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.sp = 0x00;
    run(&mut cpu);
    assert_eq!(cpu.x, 0x00);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_pha() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![PHA, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x42;
    cpu.sp = 0xFD;
    run(&mut cpu);
    assert_eq!(cpu.sp, 0xFC);
    assert_eq!(cpu.bus.borrow_mut().read(0x01FD, false), 0x42);
}

#[test]
fn test_pla() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![PLA, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.sp = 0xFC;
    cpu.bus.borrow_mut().write(0x01FD, 0x42, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0x42);
    assert_eq!(cpu.sp, 0xFD);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_pla_zero_flag() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![PLA, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.sp = 0xFC;
    cpu.bus.borrow_mut().write(0x01FD, 0x00, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0x00);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
}

#[test]
fn test_pla_negative_flag() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![PLA, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.sp = 0xFC;
    cpu.bus.borrow_mut().write(0x01FD, 0x80, false);
    run(&mut cpu);
    assert_eq!(cpu.a, 0x80);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_php() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![PHP, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.p = 0xFF;
    cpu.sp = 0xFD;
    run(&mut cpu);
    assert_eq!(cpu.sp, 0xFC);
    // PHP should push P with B flag (bit 4) and unused bit (bit 5) set to 1
    assert_eq!(cpu.bus.borrow_mut().read(0x01FD, false), 0xFF);
}

#[test]
fn test_php_sets_break_and_unused_bits() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![PHP, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    // Set status to 0xC0 (only N and V flags set, B and unused are 0)
    cpu.p = 0xC0;
    cpu.sp = 0xFD;
    run(&mut cpu);
    assert_eq!(cpu.sp, 0xFC);
    // Should push 0xF0 (0xC0 | 0x30) - B flag and unused bit both set
    assert_eq!(cpu.bus.borrow_mut().read(0x01FD, false), 0xF0);
}

#[test]
fn test_plp() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![PLP, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.sp = 0xFC;
    cpu.bus.borrow_mut().write(0x01FD, 0xC3, false);
    run(&mut cpu);
    // PLP should load flags but ignore B flag and always set unused bit (bit 5)
    // 0xC3 = 0b11000011, after PLP with unused bit set: 0b11100011 = 0xE3
    assert_eq!(cpu.p, 0xE3);
    assert_eq!(cpu.sp, 0xFD);
}

#[test]
fn test_plp_ignores_break_and_unused_bits() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![PLP, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.sp = 0xFC;
    // Stack has 0xFF (all bits set including B and unused)
    cpu.bus.borrow_mut().write(0x01FD, 0xFF, false);
    // But P register starts with B and unused cleared
    cpu.p = 0xC0; // Only N and V set
    run(&mut cpu);
    // After PLP, P should be 0xEF (all bits except B flag)
    // B flag (bit 4) should remain at its previous state
    // Unused bit (bit 5) should remain set (always 1)
    assert_eq!(cpu.p, 0xEF);
    assert_eq!(cpu.sp, 0xFD);
}

#[test]
fn test_stx_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STX_ZP, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x42;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x10, false), 0x42);
}

#[test]
fn test_stx_zero_page_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STX_ZPY, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x42;
    cpu.y = 0x05;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x15, false), 0x42);
}

#[test]
fn test_stx_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STX_ABS, 0x00, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x42;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x1000, false), 0x42);
}

#[test]
fn test_sty_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STY_ZP, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x42;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x10, false), 0x42);
}

#[test]
fn test_sty_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STY_ZPX, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x42;
    cpu.x = 0x05;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x15, false), 0x42);
}

#[test]
fn test_sty_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STY_ABS, 0x00, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x42;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x1000, false), 0x42);
}

#[test]
fn test_load_program_at_custom_address() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_IMM, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    run(&mut cpu);
    assert_eq!(cpu.a, 0x42);
    // Verify program was loaded at 0x8000
    assert_eq!(cpu.bus.borrow_mut().read(0x8000, false), LDA_IMM);
    assert_eq!(cpu.bus.borrow_mut().read(0x8001, false), 0x42);
    assert_eq!(cpu.bus.borrow_mut().read(0x8002, false), KIL);
}

#[test]
fn test_aac_sets_carry_when_bit7_set() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![AAC_IMM, 0b11000000, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b11111111;
    cpu.p = 0x00;
    run(&mut cpu);
    assert_eq!(cpu.a, 0b11000000);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_aac_clears_carry_when_bit7_clear() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![AAC_IMM, 0b01000000, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b11111111;
    cpu.p = FLAG_CARRY;
    run(&mut cpu);
    assert_eq!(cpu.a, 0b01000000);
    assert_eq!(cpu.p & FLAG_CARRY, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_sax_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SAX_ZP, 0x50, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b11110000;
    cpu.x = 0b10101010;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x0050, false), 0b10100000);
}

#[test]
fn test_sax_zero_page_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SAX_ZPY, 0x50, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b11110000;
    cpu.x = 0b10101010;
    cpu.y = 0x05;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x0055, false), 0b10100000);
}

#[test]
fn test_sax_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SAX_ABS, 0x00, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b11110000;
    cpu.x = 0b10101010;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x1000, false), 0b10100000);
}

#[test]
fn test_sax_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SAX_INDX, 0x40, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b11111111;
    cpu.x = 0b10101010;
    // The pointer is at 0x40 + X (wrapping in zero page)
    // So we need to set up the pointer at 0x40 + 0xAA = 0xEA
    cpu.bus.borrow_mut().write(0x00EA, 0x00, false);
    cpu.bus.borrow_mut().write(0x00EB, 0x10, false);
    run(&mut cpu);
    // Should store A & X = 0b11111111 & 0b10101010 = 0b10101010 at 0x1000
    assert_eq!(cpu.bus.borrow_mut().read(0x1000, false), 0b10101010);
}

#[test]
fn test_arr_basic() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ARR_IMM, 0b11110000, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b11111111;
    cpu.p = 0x00; // No carry
    run(&mut cpu);
    // A = 0b11111111 AND 0b11110000 = 0b11110000
    // Then shift right: 0b11110000 >> 1 = 0b01111000
    assert_eq!(cpu.a, 0b01111000);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0); // bit 7 is 0
    assert_eq!(cpu.p & FLAG_ZERO, 0);
}

#[test]
fn test_arr_with_carry() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ARR_IMM, 0b11110000, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b11111111;
    cpu.p = FLAG_CARRY; // Carry set
    run(&mut cpu);
    // A = 0b11111111 AND 0b11110000 = 0b11110000
    // Then shift right with carry: (0b11110000 >> 1) | 0b10000000 = 0b11111000
    assert_eq!(cpu.a, 0b11111000);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE); // bit 7 is 1
}

#[test]
fn test_arr_sets_carry_and_overflow() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ARR_IMM, 0b01100001, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b11111111;
    cpu.p = 0x00;
    run(&mut cpu);
    // A = 0b11111111 AND 0b01100001 = 0b01100001
    // Then shift right: 0b01100001 >> 1 = 0b00110000
    assert_eq!(cpu.a, 0b00110000);
    // Carry = bit 6 of result = bit 6 of 0b00110000 = 0
    assert_eq!(cpu.p & FLAG_CARRY, 0);
    // Overflow = bit 6 XOR bit 5 of result
    // Result is 0b00110000: bit 6 = 0, bit 5 = 1, so 0 XOR 1 = 1
    assert_eq!(cpu.p & FLAG_OVERFLOW, FLAG_OVERFLOW);
}

#[test]
fn test_asr_basic() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ASR_IMM, 0b11110000, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b11111111;
    cpu.p = FLAG_CARRY; // Carry should be ignored for LSR
    run(&mut cpu);
    // A = 0b11111111 AND 0b11110000 = 0b11110000
    // Then LSR (logical shift right): 0b11110000 >> 1 = 0b01111000
    assert_eq!(cpu.a, 0b01111000);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0); // bit 7 is 0
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_CARRY, 0); // bit 0 of AND result was 0
}

#[test]
fn test_asr_sets_carry() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ASR_IMM, 0b11110001, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b11111111;
    cpu.p = 0x00;
    run(&mut cpu);
    // A = 0b11111111 AND 0b11110001 = 0b11110001
    // Then LSR: 0b11110001 >> 1 = 0b01111000
    assert_eq!(cpu.a, 0b01111000);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // bit 0 of AND result was 1
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_asr_zero_result() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ASR_IMM, 0b00000001, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b00000001;
    cpu.p = 0x00;
    run(&mut cpu);
    // A = 0b00000001 AND 0b00000001 = 0b00000001
    // Then LSR: 0b00000001 >> 1 = 0b00000000
    assert_eq!(cpu.a, 0b00000000);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // bit 0 was 1
}

#[test]
fn test_atx_basic() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ATX_IMM, 0b11110000, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b11111111;
    cpu.x = 0x00;
    run(&mut cpu);
    // A,X = immediate value (stable behavior for blargg tests)
    assert_eq!(cpu.a, 0b11110000);
    assert_eq!(cpu.x, 0b11110000);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
}

#[test]
fn test_atx_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ATX_IMM, 0b00001111, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b11110000;
    cpu.x = 0xFF;
    run(&mut cpu);
    // A,X = immediate value (0b00001111 is not zero)
    assert_eq!(cpu.a, 0b00001111);
    assert_eq!(cpu.x, 0b00001111);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_atx_preserves_result() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ATX_IMM, 0b10101010, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b11001100;
    cpu.x = 0x33;
    run(&mut cpu);
    // A,X = immediate value
    assert_eq!(cpu.a, 0b10101010);
    assert_eq!(cpu.x, 0b10101010);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
}

#[test]
fn test_axa_indirect_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // Set up indirect address at ZP location 0x20
    cpu.bus.borrow_mut().write(0x20, 0x00, false); // Low byte
    cpu.bus.borrow_mut().write(0x21, 0x10, false); // High byte = 0x10, so address is 0x1000
    let program = vec![AXA_INDY, 0x20, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0xFF;
    cpu.x = 0x7F;
    cpu.y = 0x05; // Add 5 to base address, final address = 0x1005
    run(&mut cpu);
    // Value = A AND X AND (high byte of address + 1)
    // high byte of final address 0x1005 is 0x10
    // Value = 0xFF AND 0x7F AND (0x10 + 1) = 0xFF AND 0x7F AND 0x11 = 0x11
    let stored_value = cpu.bus.borrow_mut().read(0x1005, false);
    assert_eq!(stored_value, 0x11);
}

#[test]
fn test_axa_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![AXA_ABSY, 0x00, 0x10, KIL]; // Base address 0x1000
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0xFF;
    cpu.x = 0x3F;
    cpu.y = 0x10; // Final address = 0x1010
    run(&mut cpu);
    // Value = A AND X AND (high byte of address + 1)
    // high byte of final address 0x1010 is 0x10
    // Value = 0xFF AND 0x3F AND (0x10 + 1) = 0xFF AND 0x3F AND 0x11 = 0x11
    let stored_value = cpu.bus.borrow_mut().read(0x1010, false);
    assert_eq!(stored_value, 0x11);
}

#[test]
fn test_axa_page_boundary() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![AXA_ABSY, 0xFF, 0x10, KIL]; // Base address 0x10FF
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0xFF;
    cpu.x = 0xFF;
    cpu.y = 0x01; // Final address = 0x1100
    run(&mut cpu);
    // Value = A AND X AND (high byte of address + 1)
    // high byte of final address 0x1100 is 0x11
    // Value = 0xFF AND 0xFF AND (0x11 + 1) = 0xFF AND 0xFF AND 0x12 = 0x12
    let stored_value = cpu.bus.borrow_mut().read(0x1100, false);
    assert_eq!(stored_value, 0x12);
}

#[test]
fn test_axs_basic() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![AXS_IMM, 0x05, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x0F;
    cpu.x = 0xFF;
    run(&mut cpu);
    // X = (A AND X) - immediate (without borrow)
    // X = (0x0F AND 0xFF) - 0x05 = 0x0F - 0x05 = 0x0A
    assert_eq!(cpu.x, 0x0A);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // No borrow occurred
}

#[test]
fn test_axs_with_borrow() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![AXS_IMM, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x0F;
    cpu.x = 0x0F;
    run(&mut cpu);
    // X = (A AND X) - immediate (without borrow)
    // X = (0x0F AND 0x0F) - 0x10 = 0x0F - 0x10 = 0xFF (wraps around)
    assert_eq!(cpu.x, 0xFF);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_CARRY, 0); // Borrow occurred
}

#[test]
fn test_axs_zero_result() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![AXS_IMM, 0x08, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x0F;
    cpu.x = 0xF8;
    run(&mut cpu);
    // X = (A AND X) - immediate
    // X = (0x0F AND 0xF8) - 0x08 = 0x08 - 0x08 = 0x00
    assert_eq!(cpu.x, 0x00);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // No borrow
}

#[test]
fn test_dcp_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DCP_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x42, 0x10, false);
    cpu.a = 0x0F;
    run(&mut cpu);
    // Memory at 0x42: 0x10 - 1 = 0x0F
    assert_eq!(cpu.bus.borrow_mut().read(0x42, false), 0x0F);
    // Compare A (0x0F) with memory (0x0F)
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO); // Equal
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // A >= memory
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_dcp_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DCP_ABSXW, 0x00, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x1005, 0x20, false);
    cpu.a = 0x30;
    run(&mut cpu);
    // Memory at 0x1005: 0x20 - 1 = 0x1F
    assert_eq!(cpu.bus.borrow_mut().read(0x1005, false), 0x1F);
    // Compare A (0x30) with memory (0x1F): 0x30 > 0x1F
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // A >= memory
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_dcp_indirect_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    cpu.bus.borrow_mut().write(0x20, 0x00, false);
    cpu.bus.borrow_mut().write(0x21, 0x10, false);
    let program = vec![DCP_INDYW, 0x20, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x1010, 0x05, false);
    cpu.a = 0x03;
    run(&mut cpu);
    // Memory at 0x1010: 0x05 - 1 = 0x04
    assert_eq!(cpu.bus.borrow_mut().read(0x1010, false), 0x04);
    // Compare A (0x03) with memory (0x04): 0x03 < 0x04
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_CARRY, 0); // A < memory (borrow)
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE); // Result would be negative
}

#[test]
fn test_dop_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DOP_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x42, 0xFF, false);
    cpu.a = 0x10;
    cpu.x = 0x20;
    cpu.y = 0x30;
    let saved_status = cpu.p;
    run(&mut cpu);
    // DOP does nothing - just reads memory and discards
    assert_eq!(cpu.bus.borrow_mut().read(0x42, false), 0xFF); // Memory unchanged
    assert_eq!(cpu.a, 0x10); // A unchanged
    assert_eq!(cpu.x, 0x20); // X unchanged
    assert_eq!(cpu.y, 0x30); // Y unchanged
    assert_eq!(cpu.p, saved_status); // Status unchanged
}

#[test]
fn test_dop_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DOP_ZPX, 0x40, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x45, 0xAA, false);
    cpu.a = 0x10;
    cpu.y = 0x30;
    let saved_status = cpu.p;
    run(&mut cpu);
    // DOP does nothing - just reads memory at 0x40 + X = 0x45 and discards
    assert_eq!(cpu.bus.borrow_mut().read(0x45, false), 0xAA); // Memory unchanged
    assert_eq!(cpu.a, 0x10); // A unchanged
    assert_eq!(cpu.x, 0x05); // X unchanged
    assert_eq!(cpu.y, 0x30); // Y unchanged
    assert_eq!(cpu.p, saved_status); // Status unchanged
}

#[test]
fn test_dop_immediate() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DOP_IMM, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x10;
    cpu.x = 0x20;
    cpu.y = 0x30;
    let saved_status = cpu.p;
    run(&mut cpu);
    // DOP does nothing - just reads immediate value and discards
    assert_eq!(cpu.a, 0x10); // A unchanged
    assert_eq!(cpu.x, 0x20); // X unchanged
    assert_eq!(cpu.y, 0x30); // Y unchanged
    assert_eq!(cpu.p, saved_status); // Status unchanged
}

#[test]
fn test_isb_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ISB_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x42, 0x10, false);
    cpu.a = 0x50;
    cpu.p |= FLAG_CARRY; // Set carry (no borrow)
    run(&mut cpu);
    // Memory at 0x42: 0x10 + 1 = 0x11
    assert_eq!(cpu.bus.borrow_mut().read(0x42, false), 0x11);
    // Then SBC: A = 0x50 - 0x11 - (1 - carry) = 0x50 - 0x11 - 0 = 0x3F
    assert_eq!(cpu.a, 0x3F);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // No borrow
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_isb_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ISB_ABSXW, 0x00, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x1005, 0xFF, false);
    cpu.a = 0x00;
    cpu.p |= FLAG_CARRY; // Set carry (no borrow)
    run(&mut cpu);
    // Memory at 0x1005: 0xFF + 1 = 0x00 (wraps)
    assert_eq!(cpu.bus.borrow_mut().read(0x1005, false), 0x00);
    // Then SBC: A = 0x00 - 0x00 - 0 = 0x00
    assert_eq!(cpu.a, 0x00);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // No borrow
}

#[test]
fn test_isb_indirect_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    cpu.bus.borrow_mut().write(0x20, 0x00, false);
    cpu.bus.borrow_mut().write(0x21, 0x10, false);
    let program = vec![ISB_INDYW, 0x20, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x1010, 0x05, false);
    cpu.a = 0x10;
    cpu.p |= FLAG_CARRY; // Set carry (no borrow)
    run(&mut cpu);
    // Memory at 0x1010: 0x05 + 1 = 0x06
    assert_eq!(cpu.bus.borrow_mut().read(0x1010, false), 0x06);
    // Then SBC: A = 0x10 - 0x06 - 0 = 0x0A
    assert_eq!(cpu.a, 0x0A);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
}

#[test]
fn test_kil_opcode_0x02() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.execute();
    assert!(cpu.halted);
}

#[test]
fn test_kil_opcode_0x12() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![KIL2];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.execute();
    assert!(cpu.halted);
}

#[test]
fn test_kil_opcode_0xf2() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![KIL12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.execute();
    assert!(cpu.halted);
}

#[test]
fn test_kil_halts_until_reset() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![KIL, NOP, NOP];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    // Execute KIL - should halt
    cpu.execute();
    assert!(cpu.halted);
    // Try to execute next opcode - should still be halted
    cpu.execute();
    assert!(cpu.halted);
    // Reset should clear halt
    cpu.reset(true);
    assert!(!cpu.halted);
    // Load a simple NOP program and verify we can execute it
    let program2 = vec![NOP];
    fake_cartridge(&mut cpu, &program2);
    cpu.reset(true);
    cpu.execute();
    assert!(!cpu.halted);
}

#[test]
fn test_lar_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LAR_ABSY, 0x00, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x05;
    cpu.sp = 0xFD;
    cpu.bus.borrow_mut().write(0x1005, 0xAB, false);
    run(&mut cpu);
    // LAR: SP & M -> A, X, SP
    // 0xFD & 0xAB = 0xA9
    assert_eq!(cpu.a, 0xA9);
    assert_eq!(cpu.x, 0xA9);
    assert_eq!(cpu.sp, 0xA9);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE); // Bit 7 is set
    assert_eq!(cpu.p & FLAG_ZERO, 0);
}

#[test]
fn test_lax_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LAX_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x42, 0x55, false);
    run(&mut cpu);
    // LAX: Load both A and X with memory value
    assert_eq!(cpu.a, 0x55);
    assert_eq!(cpu.x, 0x55);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_lax_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LAX_ABSY, 0x00, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x1010, 0x80, false);
    run(&mut cpu);
    // LAX: Load both A and X with memory value
    assert_eq!(cpu.a, 0x80);
    assert_eq!(cpu.x, 0x80);
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE); // Bit 7 is set
    assert_eq!(cpu.p & FLAG_ZERO, 0);
}

#[test]
fn test_lax_indirect_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    cpu.bus.borrow_mut().write(0x20, 0x00, false);
    cpu.bus.borrow_mut().write(0x21, 0x10, false);
    let program = vec![LAX_INDY, 0x20, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x05;
    cpu.bus.borrow_mut().write(0x1005, 0x00, false);
    run(&mut cpu);
    // LAX: Load both A and X with memory value (0x00)
    assert_eq!(cpu.a, 0x00);
    assert_eq!(cpu.x, 0x00);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO); // Zero flag set
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_nop_undocumented_0x1a() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![NOP_IMP, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    let a_before = cpu.a;
    let x_before = cpu.x;
    let y_before = cpu.y;
    let p_before = cpu.p;
    run(&mut cpu);
    // NOP should not change any registers or flags
    assert_eq!(cpu.a, a_before);
    assert_eq!(cpu.x, x_before);
    assert_eq!(cpu.y, y_before);
    assert_eq!(cpu.p, p_before);
}

#[test]
fn test_nop_undocumented_0xda() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![NOP_IMP5, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x42;
    cpu.x = 0x55;
    let a_before = cpu.a;
    let x_before = cpu.x;
    run(&mut cpu);
    // NOP should not change any registers
    assert_eq!(cpu.a, a_before);
    assert_eq!(cpu.x, x_before);
}

#[test]
fn test_rla_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![RLA_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x42, 0b0110_1010, false); // 0x6A
    cpu.a = 0b1111_0000; // 0xF0
    cpu.p &= !FLAG_CARRY; // Clear carry
    run(&mut cpu);
    // RLA: ROL memory (0x6A << 1 = 0xD4), then AND with A
    // Memory should be 0xD4, A should be 0xF0 & 0xD4 = 0xD0
    assert_eq!(cpu.bus.borrow_mut().read(0x42, false), 0xD4);
    assert_eq!(cpu.a, 0xD0);
    assert_eq!(cpu.p & FLAG_CARRY, 0); // Carry clear (bit 7 was 0)
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE); // Negative set
    assert_eq!(cpu.p & FLAG_ZERO, 0);
}

#[test]
fn test_rla_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![RLA_ABSXW, 0x00, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x1005, 0b1000_0001, false); // 0x81
    cpu.a = 0xFF;
    cpu.p |= FLAG_CARRY; // Set carry
    run(&mut cpu);
    // RLA: ROL memory (0x81 << 1 + carry = 0x03), then AND with A
    // Memory should be 0x03, A should be 0xFF & 0x03 = 0x03
    assert_eq!(cpu.bus.borrow_mut().read(0x1005, false), 0x03);
    assert_eq!(cpu.a, 0x03);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // Carry set (bit 7 was 1)
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
    assert_eq!(cpu.p & FLAG_ZERO, 0);
}

#[test]
fn test_rla_indirect_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    cpu.bus.borrow_mut().write(0x20, 0x00, false);
    cpu.bus.borrow_mut().write(0x21, 0x10, false);
    let program = vec![RLA_INDYW, 0x20, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x1010, 0x01, false);
    cpu.a = 0x01;
    cpu.p &= !FLAG_CARRY;
    run(&mut cpu);
    // RLA: ROL memory (0x01 << 1 = 0x02), then AND with A
    // Memory should be 0x02, A should be 0x01 & 0x02 = 0x00
    assert_eq!(cpu.bus.borrow_mut().read(0x1010, false), 0x02);
    assert_eq!(cpu.a, 0x00);
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO); // Zero flag set
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_rra_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![RRA_ZP, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x10, 0b1010_1010, false); // 0xAA
    cpu.a = 0x10;
    cpu.p &= !FLAG_CARRY; // Clear carry
    run(&mut cpu);
    // RRA: ROR memory (0xAA >> 1 = 0x55), then ADC with A (0x10 + 0x55 = 0x65)
    assert_eq!(cpu.bus.borrow_mut().read(0x10, false), 0x55);
    assert_eq!(cpu.a, 0x65);
    assert_eq!(cpu.p & FLAG_CARRY, 0); // No carry from addition
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_rra_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![RRA_ABSXW, 0x00, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x1005, 0b0000_0001, false); // 0x01
    cpu.a = 0xFF;
    cpu.p |= FLAG_CARRY; // Set carry
    run(&mut cpu);
    // RRA: ROR memory (0x01 >> 1 with carry = 0x80), then ADC with A (0xFF + 0x80 + carry=1)
    // Memory rotates to 0x80 (carry goes into bit 7), bit 0 goes to carry
    // Then: 0xFF + 0x80 + 1 (carry from ROR) = 0x180 = 0x80 with carry set
    assert_eq!(cpu.bus.borrow_mut().read(0x1005, false), 0x80);
    assert_eq!(cpu.a, 0x80);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // Carry from addition
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE); // Result is negative
}

#[test]
fn test_rra_indirect_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    cpu.bus.borrow_mut().write(0x20, 0x00, false);
    cpu.bus.borrow_mut().write(0x21, 0x10, false);
    let program = vec![RRA_INDYW, 0x20, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x1010, 0b0000_0010, false); // 0x02
    cpu.a = 0x00;
    cpu.p &= !FLAG_CARRY;
    run(&mut cpu);
    // RRA: ROR memory (0x02 >> 1 = 0x01), then ADC with A (0x00 + 0x01 = 0x01)
    assert_eq!(cpu.bus.borrow_mut().read(0x1010, false), 0x01);
    assert_eq!(cpu.a, 0x01);
    assert_eq!(cpu.p & FLAG_CARRY, 0); // No carry
    assert_eq!(cpu.p & FLAG_ZERO, 0);
}

#[test]
fn test_sbc_immediate_undocumented() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_IMM2, 0x01, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x05;
    cpu.p |= FLAG_CARRY; // Set carry (no borrow)
    run(&mut cpu);
    // Undocumented SBC: same as legal SBC #byte
    // 0x05 - 0x01 = 0x04
    assert_eq!(cpu.a, 0x04);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // No borrow
    assert_eq!(cpu.p & FLAG_ZERO, 0);
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_slo_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SLO_ZP, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.bus.borrow_mut().write(0x10, 0b0101_0101, false); // 0x55
    cpu.a = 0b0000_1111; // 0x0F
    run(&mut cpu);
    // SLO: ASL memory (0x55 << 1 = 0xAA), then ORA with A (0x0F | 0xAA = 0xAF)
    assert_eq!(cpu.bus.borrow_mut().read(0x10, false), 0xAA);
    assert_eq!(cpu.a, 0xAF);
    assert_eq!(cpu.p & FLAG_CARRY, 0); // No carry from shift
    assert_eq!(cpu.p & FLAG_NEGATIVE, FLAG_NEGATIVE); // Result is negative
}

#[test]
fn test_slo_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SLO_ABSXW, 0x00, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x1005, 0b1000_0001, false); // 0x81
    cpu.a = 0b0000_0010; // 0x02
    run(&mut cpu);
    // SLO: ASL memory (0x81 << 1 = 0x02, carry set), then ORA with A (0x02 | 0x02 = 0x02)
    assert_eq!(cpu.bus.borrow_mut().read(0x1005, false), 0x02);
    assert_eq!(cpu.a, 0x02);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // Carry from shift
    assert_eq!(cpu.p & FLAG_ZERO, 0);
}

#[test]
fn test_slo_indirect_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    cpu.bus.borrow_mut().write(0x20, 0x00, false);
    cpu.bus.borrow_mut().write(0x21, 0x10, false);
    let program = vec![SLO_INDYW, 0x20, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x1010, 0b0000_0001, false); // 0x01
    cpu.a = 0b0000_0000; // 0x00
    run(&mut cpu);
    // SLO: ASL memory (0x01 << 1 = 0x02), then ORA with A (0x00 | 0x02 = 0x02)
    assert_eq!(cpu.bus.borrow_mut().read(0x1010, false), 0x02);
    assert_eq!(cpu.a, 0x02);
    assert_eq!(cpu.p & FLAG_CARRY, 0); // No carry
    assert_eq!(cpu.p & FLAG_ZERO, 0);
}

#[test]
fn test_sre_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    cpu.bus.borrow_mut().write(0x42, 0b0000_0110, false); // 0x06
    let program = vec![SRE_ZP, 0x42, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0b0000_0001; // 0x01
    run(&mut cpu);
    // SRE: LSR memory (0x06 >> 1 = 0x03), then EOR with A (0x01 ^ 0x03 = 0x02)
    assert_eq!(cpu.bus.borrow_mut().read(0x42, false), 0x03);
    assert_eq!(cpu.a, 0x02);
    assert_eq!(cpu.p & FLAG_CARRY, 0); // No carry from shift
    assert_eq!(cpu.p & FLAG_ZERO, 0);
}

#[test]
fn test_sre_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    cpu.bus.borrow_mut().write(0x20, 0x00, false);
    cpu.bus.borrow_mut().write(0x21, 0x10, false);
    let program = vec![SRE_ABSXW, 0x00, 0x10, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x10;
    cpu.bus.borrow_mut().write(0x1010, 0b0000_0101, false); // 0x05
    cpu.a = 0b0000_0011; // 0x03
    run(&mut cpu);
    // SRE: LSR memory (0x05 >> 1 = 0x02 with carry), then EOR with A (0x03 ^ 0x02 = 0x01)
    assert_eq!(cpu.bus.borrow_mut().read(0x1010, false), 0x02);
    assert_eq!(cpu.a, 0x01);
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY); // Carry from LSR
    assert_eq!(cpu.p & FLAG_ZERO, 0);
}

#[test]
fn test_sre_indirect_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    cpu.bus.borrow_mut().write(0x20, 0x00, false);
    cpu.bus.borrow_mut().write(0x21, 0x10, false);
    let program = vec![SRE_INDYW, 0x20, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x1010, 0b0000_1000, false); // 0x08
    cpu.a = 0b0000_0100; // 0x04
    run(&mut cpu);
    // SRE: LSR memory (0x08 >> 1 = 0x04), then EOR with A (0x04 ^ 0x04 = 0x00)
    assert_eq!(cpu.bus.borrow_mut().read(0x1010, false), 0x04);
    assert_eq!(cpu.a, 0x00);
    assert_eq!(cpu.p & FLAG_CARRY, 0); // No carry
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO); // Result is zero
}

#[test]
fn test_sxa_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // Test SXA with Absolute,Y addressing
    // SXA stores X AND (HIGH(addr) + 1) at the target address
    // If addr = 0x1000 and Y = 0x10, target = 0x1010
    // HIGH(0x1000) + 1 = 0x10 + 1 = 0x11
    // If X = 0xFF, result = 0xFF AND 0x11 = 0x11
    let program = vec![SXA_ABSY, 0x00, 0x10, KIL]; // SXA $1000,Y
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0xFF;
    cpu.y = 0x10;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x1010, false), 0x11); // X AND (0x10 + 1)
}

#[test]
fn test_sya_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // Test SYA with Absolute,X addressing
    // SYA stores Y AND (HIGH(addr) + 1) at the target address
    // If addr = 0x1000 and X = 0x10, target = 0x1010
    // HIGH(0x1000) + 1 = 0x10 + 1 = 0x11
    // If Y = 0xFF, result = 0xFF AND 0x11 = 0x11
    let program = vec![SYA_ABSX, 0x00, 0x10, KIL]; // SYA $1000,X
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.y = 0xFF;
    cpu.x = 0x10;
    run(&mut cpu);
    assert_eq!(cpu.bus.borrow_mut().read(0x1010, false), 0x11); // Y AND (0x10 + 1)
}

#[test]
fn test_top_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // Test TOP with Absolute addressing - should do nothing
    let program = vec![TOP_ABS, 0x00, 0x30, KIL]; // TOP $3000
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0x42;
    run(&mut cpu);
    // TOP should not affect any registers or memory
    assert_eq!(cpu.a, 0x42);
}

#[test]
fn test_top_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // Test TOP with Absolute,X addressing - should do nothing
    let program = vec![TOP_ABSX, 0x00, 0x30, KIL]; // TOP $3000,X
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.x = 0x10;
    cpu.a = 0x42;
    run(&mut cpu);
    // TOP should not affect any registers or memory
    assert_eq!(cpu.a, 0x42);
    assert_eq!(cpu.x, 0x10);
}

#[test]
fn test_xaa_immediate() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // XAA performs: A = (A | MAGIC) & X & immediate
    // Using MAGIC = 0xEE (common value)
    // A = 0xFF, X = 0xF0, immediate = 0x0F
    // Result: (0xFF | 0xEE) & 0xF0 & 0x0F = 0xFF & 0xF0 & 0x0F = 0x00
    let program = vec![XAA_IMM, 0x0F, KIL];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0xFF;
    cpu.x = 0xF0;
    run(&mut cpu);
    assert_eq!(cpu.a, 0x00);
    // Zero flag should be set
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO);
    // Negative flag should be clear
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0);
}

#[test]
fn test_xas_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // XAS performs: SP = A & X, then M = SP & (HIGH(addr) + 1)
    // A = 0xFF, X = 0xF0 -> SP = 0xF0
    // addr = 0x1000, Y = 0x10 -> effective addr = 0x1010
    // HIGH(0x1000) = 0x10, so result = 0xF0 & 0x11 = 0x10
    let program = vec![XAS_ABSY, 0x00, 0x10, KIL]; // XAS $1000,Y
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0xFF;
    cpu.x = 0xF0;
    cpu.y = 0x10;
    run(&mut cpu);
    // SP should be A & X
    assert_eq!(cpu.sp, 0xF0);
    // Memory at $1010 should be SP & (HIGH(addr) + 1) = 0xF0 & 0x11 = 0x10
    assert_eq!(cpu.bus.borrow_mut().read(0x1010, false), 0x10);
}

// Cycle counter tests
#[test]
fn test_cycle_counter_starts_at_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    assert_eq!(cpu.get_total_cycles(), 0);
}

#[test]
fn test_master_clock_ntsc_read_cycle_ticks_master_clock() {
    let mut clock = crate::nes::cpu::MasterClock::new(TimingMode::Ntsc);

    assert_eq!(clock.master_cycles(), 0);

    // Updated timing model:
    // before_cpu_cycle: read uses (before - 1)
    // after_cpu_cycle:  read uses (after + 1)
    // For NTSC: (6-1) + (6+1) = 12
    clock.before_cpu_cycle(false);
    clock.after_cpu_cycle(false);

    assert_eq!(clock.master_cycles(), 12);
}

#[test]
fn test_master_clock_ntsc_write_cycle_ticks_master_clock() {
    let mut clock = crate::nes::cpu::MasterClock::new(TimingMode::Ntsc);

    assert_eq!(clock.master_cycles(), 0);

    // Updated timing model:
    // before_cpu_cycle: write uses (before + 1)
    // after_cpu_cycle:  write uses (after - 1)
    // For NTSC: (6+1) + (6-1) = 12
    clock.before_cpu_cycle(true);
    clock.after_cpu_cycle(true);

    assert_eq!(clock.master_cycles(), 12);
}

#[test]
fn test_rmw_on_ppu_register_preserves_w_flag() {
    // This test verifies that RMW instructions on PPU registers (specifically $2006)
    // handle both writes correctly. According to Blargg's test ROM analysis:
    // - Dummy write: Writes UNMODIFIED value, toggles w flag, modifies t/v registers
    // - Real write: Writes MODIFIED value, toggles w flag, modifies t/v registers
    //
    // For INC $2006 with open bus $25 when w starts false:
    //   - Read gets $25
    //   - Dummy write $25: w false→true, t high byte = $25
    //   - Real write $26: w true→false, t low byte = $26, v = $2526

    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // Setup: Write a value to PPUADDR to set it up and clear w flag
    // First write $20 to $2006 (sets high byte, w becomes true)
    cpu.bus.borrow_mut().write(0x2006, 0x20, false);
    // Second write $00 to $2006 (sets low byte, w becomes false, v = $2000)
    cpu.bus.borrow_mut().write(0x2006, 0x00, false);

    // The w flag should now be false (after two writes)
    // PPUADDR should be $2000

    // Now execute an RMW instruction on $2006
    // We'll use INC $2006 to match the test ROM
    let program = vec![0xEE, 0x06, 0x20, 0x02]; // INC $2006, KIL
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Re-setup PPUADDR after reset
    cpu.bus.borrow_mut().write(0x2006, 0x20, false);
    cpu.bus.borrow_mut().write(0x2006, 0x00, false);

    // Execute the INC $2006 instruction
    // This will:
    // 1. Read $2006 (gets open bus value, let's say $20)
    // 2. Dummy write $20 to $2006 (w: false→true, t high = $20)
    // 3. Real write $21 to $2006 (w: true→false, t low = $21, v = $2021)
    cpu.execute();

    // After the RMW on $2006:
    // - PPUADDR should be $2021 (or thereabouts, depending on open bus)
    // To verify, we write a test value and read it back

    // Since we don't know the exact open bus value, let's just verify
    // that the dummy write affected the state by checking that subsequent
    // writes work correctly
    cpu.bus.borrow_mut().write(0x2006, 0x30, false);
    cpu.bus.borrow_mut().write(0x2006, 0x40, false);
    cpu.bus.borrow_mut().write(0x2007, 0xAB, false);

    // Read back from PPU memory at $3040 to verify
    cpu.bus.borrow_mut().write(0x2006, 0x30, false);
    cpu.bus.borrow_mut().write(0x2006, 0x40, false);
    let _ = cpu.bus.borrow_mut().read(0x2007, false); // Dummy read (buffered)
    let value = cpu.bus.borrow_mut().read(0x2007, false); // Actual value

    assert_eq!(
        value, 0xAB,
        "Value at PPU $3040 should be $AB - dummy writes do affect PPU state"
    );
}

// ==================== get_operand Tests ====================

#[test]
fn test_get_operand_implied() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Test NOP (Implied)
    let op = opcode::lookup(0xEA);
    assert_eq!(cpu.get_operand(*op), 0, "Implied mode should return 0");

    // Test INX (Implied)
    let op = opcode::lookup(0xE8);
    assert_eq!(cpu.get_operand(*op), 0, "Implied mode should return 0");
}

#[test]
fn test_get_operand_accumulator() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Test ASL A (Accumulator)
    let op = opcode::lookup(0x0A);
    assert_eq!(cpu.get_operand(*op), 0, "Accumulator mode should return 0");

    // Test LSR A (Accumulator)
    let op = opcode::lookup(0x4A);
    assert_eq!(cpu.get_operand(*op), 0, "Accumulator mode should return 0");
}

#[test]
fn test_get_operand_immediate() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Set up: Write immediate value at PC
    cpu.pc = 0x0100;
    cpu.bus.borrow_mut().write(0x0100, 0x42, false);

    // Test LDA #$42 (Immediate)
    let op = opcode::lookup(0xA9);
    assert_eq!(
        cpu.get_operand(*op),
        0x42,
        "Immediate mode should return the byte at PC"
    );
}

#[test]
fn test_get_operand_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Set up: Write ZP address at PC
    cpu.pc = 0x0100;
    cpu.bus.borrow_mut().write(0x0100, 0x80, false);

    // Test LDA $80 (Zero Page)
    let op = opcode::lookup(0xA5);
    assert_eq!(
        cpu.get_operand(*op),
        0x80,
        "Zero Page mode should return ZP address"
    );
}

#[test]
fn test_get_operand_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Set up: Write ZP base address at PC, set X register
    cpu.pc = 0x0100;
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x0100, 0x80, false);

    // Test LDA $80,X (Zero Page,X)
    let op = opcode::lookup(0xB5);
    assert_eq!(
        cpu.get_operand(*op),
        0x85,
        "Zero Page,X mode should return (ZP + X) & 0xFF"
    );
}

#[test]
fn test_get_operand_zero_page_x_wrapping() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Test wrapping behavior in zero page
    cpu.pc = 0x0100;
    cpu.x = 0xFF;
    cpu.bus.borrow_mut().write(0x0100, 0x80, false);

    // 0x80 + 0xFF = 0x17F, but should wrap to 0x7F
    let op = opcode::lookup(0xB5);
    assert_eq!(
        cpu.get_operand(*op),
        0x7F,
        "Zero Page,X mode should wrap within zero page"
    );
}

#[test]
fn test_get_operand_zero_page_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Set up: Write ZP base address at PC, set Y register
    cpu.pc = 0x0100;
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x0100, 0x20, false);

    // Test LDX $20,Y (Zero Page,Y)
    let op = opcode::lookup(0xB6);
    assert_eq!(
        cpu.get_operand(*op),
        0x30,
        "Zero Page,Y mode should return (ZP + Y) & 0xFF"
    );
}

#[test]
fn test_get_operand_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Set up: Write 16-bit address at PC (little-endian)
    cpu.pc = 0x0100;
    cpu.bus.borrow_mut().write(0x0100, 0x34, false); // Low byte
    cpu.bus.borrow_mut().write(0x0101, 0x12, false); // High byte

    // Test LDA $1234 (Absolute)
    let op = opcode::lookup(0xAD);
    assert_eq!(
        cpu.get_operand(*op),
        0x1234,
        "Absolute mode should return 16-bit address"
    );
}

#[test]
fn test_get_operand_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Set up: Write 16-bit base address at PC, set X register
    cpu.pc = 0x0100;
    cpu.x = 0x10;
    cpu.bus.borrow_mut().write(0x0100, 0x00, false); // Low byte
    cpu.bus.borrow_mut().write(0x0101, 0x20, false); // High byte

    // Test LDA $2000,X (Absolute,X)
    let op = opcode::lookup(0xBD);
    assert_eq!(
        cpu.get_operand(*op),
        0x2010,
        "Absolute,X mode should return base + X"
    );
}

#[test]
fn test_get_operand_absolute_x_page_crossing() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Test page crossing
    cpu.pc = 0x0100;
    cpu.x = 0xFF;
    cpu.bus.borrow_mut().write(0x0100, 0x80, false); // Low byte
    cpu.bus.borrow_mut().write(0x0101, 0x20, false); // High byte

    // 0x2080 + 0xFF = 0x217F (page crossing)
    let op = opcode::lookup(0xBD);
    assert_eq!(
        cpu.get_operand(*op),
        0x217F,
        "Absolute,X mode should handle page crossing"
    );
}

#[test]
fn test_get_operand_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Set up: Write 16-bit base address at PC, set Y register
    cpu.pc = 0x0100;
    cpu.y = 0x05;
    cpu.bus.borrow_mut().write(0x0100, 0x00, false); // Low byte
    cpu.bus.borrow_mut().write(0x0101, 0x30, false); // High byte

    // Test LDA $3000,Y (Absolute,Y)
    let op = opcode::lookup(0xB9);
    assert_eq!(
        cpu.get_operand(*op),
        0x3005,
        "Absolute,Y mode should return base + Y"
    );
}

#[test]
fn test_get_operand_relative_positive() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Set up: Write positive offset at PC
    cpu.pc = 0x0100;
    cpu.bus.borrow_mut().write(0x0100, 0x10, false); // +16

    // Test BNE (Relative) - should return the raw offset byte
    let op = opcode::lookup(0xD0);
    assert_eq!(
        cpu.get_operand(*op),
        0x10,
        "Relative mode should return the immediate offset byte"
    );
    // PC should have advanced by 1
    assert_eq!(cpu.pc, 0x0101, "PC should advance after reading offset");
}

#[test]
fn test_get_operand_relative_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Set up: Write negative offset at PC
    cpu.pc = 0x0100;
    cpu.bus.borrow_mut().write(0x0100, 0xF0, false); // -16 (as signed byte)

    // Test BEQ (Relative) - should return the raw offset byte
    let op = opcode::lookup(0xF0);
    assert_eq!(
        cpu.get_operand(*op),
        0xF0,
        "Relative mode should return the immediate offset byte"
    );
    // PC should have advanced by 1
    assert_eq!(cpu.pc, 0x0101, "PC should advance after reading offset");
}

#[test]
fn test_get_operand_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Set up: JMP ($1234)
    // Write pointer address at PC
    cpu.pc = 0x0100;
    cpu.bus.borrow_mut().write(0x0100, 0x34, false); // Pointer low
    cpu.bus.borrow_mut().write(0x0101, 0x12, false); // Pointer high

    // Write target address at pointer location
    cpu.bus.borrow_mut().write(0x1234, 0x00, false); // Target low
    cpu.bus.borrow_mut().write(0x1235, 0x80, false); // Target high

    // Test JMP ($1234) (Indirect)
    let op = opcode::lookup(0x6C);
    assert_eq!(
        cpu.get_operand(*op),
        0x8000,
        "Indirect mode should return address at pointer"
    );
}

#[test]
fn test_get_operand_indirect_page_boundary_bug() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Test the famous 6502 JMP indirect bug
    // When pointer is at page boundary (e.g., $02FF), high byte wraps to $0200
    cpu.pc = 0x0100;
    cpu.bus.borrow_mut().write(0x0100, 0xFF, false); // Pointer low
    cpu.bus.borrow_mut().write(0x0101, 0x02, false); // Pointer high

    // Write target bytes
    cpu.bus.borrow_mut().write(0x02FF, 0x34, false); // Target low
    cpu.bus.borrow_mut().write(0x0200, 0x12, false); // Target high (wraps!)
    cpu.bus.borrow_mut().write(0x0300, 0x56, false); // What it should be if no bug

    // The bug causes high byte to be read from $0200 instead of $0300
    let op = opcode::lookup(0x6C);
    assert_eq!(
        cpu.get_operand(*op),
        0x1234,
        "Indirect mode should exhibit page boundary bug"
    );
}

#[test]
fn test_get_operand_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Set up: LDA ($20,X)
    cpu.pc = 0x0100;
    cpu.x = 0x04;
    cpu.bus.borrow_mut().write(0x0100, 0x20, false); // ZP base

    // Write pointer at ZP location ($20 + $04 = $24)
    cpu.bus.borrow_mut().write(0x0024, 0x00, false); // Pointer low
    cpu.bus.borrow_mut().write(0x0025, 0x30, false); // Pointer high

    // Test LDA ($20,X) (Indexed Indirect)
    let op = opcode::lookup(0xA1);
    assert_eq!(
        cpu.get_operand(*op),
        0x3000,
        "Indexed Indirect mode should return address from (ZP+X)"
    );
}

#[test]
fn test_get_operand_indexed_indirect_wrapping() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Test zero page wrapping in indexed indirect
    cpu.pc = 0x0100;
    cpu.x = 0xFF;
    cpu.bus.borrow_mut().write(0x0100, 0x80, false); // ZP base

    // $80 + $FF = $17F, wraps to $7F in zero page
    cpu.bus.borrow_mut().write(0x007F, 0x34, false); // Pointer low
    cpu.bus.borrow_mut().write(0x0080, 0x12, false); // Pointer high (wraps in ZP)

    let op = opcode::lookup(0xA1);
    assert_eq!(
        cpu.get_operand(*op),
        0x1234,
        "Indexed Indirect should wrap pointer address in zero page"
    );
}

#[test]
fn test_get_operand_indirect_indexed() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Set up: LDA ($20),Y
    cpu.pc = 0x0100;
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x0100, 0x20, false); // ZP pointer

    // Write base address at ZP pointer
    cpu.bus.borrow_mut().write(0x0020, 0x00, false); // Base low
    cpu.bus.borrow_mut().write(0x0021, 0x30, false); // Base high

    // Test LDA ($20),Y (Indirect Indexed)
    // Should return ($3000) + Y = $3000 + $10 = $3010
    let op = opcode::lookup(0xB1);
    assert_eq!(
        cpu.get_operand(*op),
        0x3010,
        "Indirect Indexed mode should return (ZP)+Y"
    );
}

#[test]
fn test_get_operand_indirect_indexed_page_crossing() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Test page crossing in indirect indexed
    cpu.pc = 0x0100;
    cpu.y = 0xFF;
    cpu.bus.borrow_mut().write(0x0100, 0x20, false); // ZP pointer

    // Write base address at ZP pointer
    cpu.bus.borrow_mut().write(0x0020, 0x80, false); // Base low
    cpu.bus.borrow_mut().write(0x0021, 0x20, false); // Base high

    // $2080 + $FF = $217F (page crossing)
    let op = opcode::lookup(0xB1);
    assert_eq!(
        cpu.get_operand(*op),
        0x217F,
        "Indirect Indexed mode should handle page crossing"
    );
}

#[test]
fn test_get_operand_indirect_indexed_zp_wrapping() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Test zero page pointer wrapping
    cpu.pc = 0x0100;
    cpu.y = 0x05;
    cpu.bus.borrow_mut().write(0x0100, 0xFF, false); // ZP pointer at $FF

    // Pointer spans $FF and $00 (wraps in zero page)
    cpu.bus.borrow_mut().write(0x00FF, 0x00, false); // Base low
    cpu.bus.borrow_mut().write(0x0000, 0x40, false); // Base high (wrapped)

    // $4000 + $05 = $4005
    let op = opcode::lookup(0xB1);
    assert_eq!(
        cpu.get_operand(*op),
        0x4005,
        "Indirect Indexed should wrap pointer in zero page"
    );
}

#[test]
fn test_get_operand_invalid_opcode() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // All 256 opcodes are defined (including undocumented ones).
    let op = opcode::lookup(0xFF); // 0xFF is SBC
    let result = cpu.get_operand(*op);

    // The function should not panic
    // Result depends on whether 0xFF is in the opcode table
    let _ = result; // Just verify it doesn't panic
}

// Tests for execute() method

#[test]
fn test_execute_brk() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Create a cartridge with BRK and set IRQ vector
    let mut prg_rom = vec![0; 0x4000]; // 16KB PRG ROM

    // Place BRK instruction at beginning
    prg_rom[0] = BRK;
    prg_rom[1] = 0x00; // Padding byte

    // Set reset vector to point to 0x8000
    prg_rom[0x3FFC] = 0x00; // Low byte of 0x8000
    prg_rom[0x3FFD] = 0x80; // High byte of 0x8000

    // Set IRQ vector to point to 0x8000 (IRQ vector is at 0xFFFE-0xFFFF)
    // For 16KB ROM: (0xFFFE - 0x8000) % 0x4000 = 0x7FFE % 0x4000 = 0x3FFE
    prg_rom[0x3FFE] = 0x00; // Low byte of 0x8000
    prg_rom[0x3FFF] = 0x80; // High byte of 0x8000

    let chr_rom = vec![0; 0x2000];
    let cartridge = Cartridge::from_parts(prg_rom, chr_rom, NametableLayout::Horizontal);
    cpu.bus.borrow_mut().map_cartridge(cartridge);
    cpu.reset(true);

    cpu.sp = 0xFF;
    cpu.p = 0x00;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    // Verify BRK behavior
    assert_eq!(cpu.pc, 0x8000, "PC should point to IRQ vector address");
    assert_eq!(
        cpu.p & FLAG_INTERRUPT,
        FLAG_INTERRUPT,
        "I flag should be set"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 7,
        "BRK should take 7 cycles"
    );
}

#[test]
fn test_execute_ora_immediate() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load ORA #$0F
    let program = vec![ORA_IMM, 0x0F];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xF0;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    assert_eq!(cpu.a, 0xFF, "A should be 0xF0 OR 0x0F = 0xFF");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "ORA immediate should take 2 cycles"
    );
}

#[test]
fn test_execute_ora_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load ORA $42
    let program = vec![ORA_ZP, 0x42];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0042, 0x0F, false); // Value at zero page $42
    cpu.a = 0xF0;

    cpu.execute();

    assert_eq!(cpu.a, 0xFF, "A should be 0xF0 OR 0x0F = 0xFF");
}

#[test]
fn test_execute_ora_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load ORA $40,X
    let program = vec![ORA_ZPX, 0x40];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0042, 0x0F, false); // Value at zero page $42 (base + X)
    cpu.a = 0xF0;
    cpu.x = 0x02;

    cpu.execute();

    assert_eq!(cpu.a, 0xFF, "A should be 0xF0 OR 0x0F = 0xFF");
}

#[test]
fn test_execute_ora_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load ORA $1234
    let program = vec![ORA_ABS, 0x34, 0x12]; // Low byte, High byte
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x1234, 0x0F, false); // Value at $1234
    cpu.a = 0xF0;

    cpu.execute();

    assert_eq!(cpu.a, 0xFF, "A should be 0xF0 OR 0x0F = 0xFF");
}

#[test]
fn test_execute_ora_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load ORA $1234,X
    let program = vec![ORA_ABSX, 0x34, 0x12]; // Low byte, High byte
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x1236, 0x0F, false); // Value at $1236 (base + X)
    cpu.a = 0xF0;
    cpu.x = 0x02;

    cpu.execute();

    assert_eq!(cpu.a, 0xFF, "A should be 0xF0 OR 0x0F = 0xFF");
}

#[test]
fn test_execute_ora_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load ORA $1234,Y
    let program = vec![ORA_ABSY, 0x34, 0x12]; // Low byte, High byte
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x1237, 0x0F, false); // Value at $1237 (base + Y)
    cpu.a = 0xF0;
    cpu.y = 0x03;

    cpu.execute();

    assert_eq!(cpu.a, 0xFF, "A should be 0xF0 OR 0x0F = 0xFF");
}

#[test]
fn test_execute_ora_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load ORA ($40,X)
    let program = vec![ORA_INDX, 0x40];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Set up indirect address at zero page $42 (base + X)
    cpu.bus.borrow_mut().write(0x0042, 0x34, false); // Low byte of target address
    cpu.bus.borrow_mut().write(0x0043, 0x12, false); // High byte of target address
    cpu.bus.borrow_mut().write(0x1234, 0x0F, false); // Value at target address
    cpu.a = 0xF0;
    cpu.x = 0x02;

    cpu.execute();

    assert_eq!(cpu.a, 0xFF, "A should be 0xF0 OR 0x0F = 0xFF");
}

#[test]
fn test_execute_ora_indirect_indexed() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load ORA ($40),Y
    let program = vec![ORA_INDY, 0x40];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Set up indirect address at zero page $40
    cpu.bus.borrow_mut().write(0x0040, 0x34, false); // Low byte of base address
    cpu.bus.borrow_mut().write(0x0041, 0x12, false); // High byte of base address
    cpu.bus.borrow_mut().write(0x1237, 0x0F, false); // Value at base + Y
    cpu.a = 0xF0;
    cpu.y = 0x03;

    cpu.execute();

    assert_eq!(cpu.a, 0xFF, "A should be 0xF0 OR 0x0F = 0xFF");
}

#[test]
fn test_execute_ora_zero_flag() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load ORA #$00
    let program = vec![ORA_IMM, 0x00];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x00;

    cpu.execute();

    assert_eq!(cpu.a, 0x00, "A should remain 0");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
}

// Tests for SLO instruction - performs ASL (shift left) on memory, then OR with accumulator

#[test]
fn test_execute_slo_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load SLO ($40,X)
    let program = vec![SLO_INDX, 0x40];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Set up indirect address at zero page $42 (base + X)
    cpu.bus.borrow_mut().write(0x0042, 0x34, false); // Low byte of target address
    cpu.bus.borrow_mut().write(0x0043, 0x12, false); // High byte of target address
    cpu.bus.borrow_mut().write(0x1234, 0b00000101, false); // Value at target (5)

    cpu.a = 0b11110000; // 0xF0
    cpu.x = 0x02;

    cpu.execute();

    // 0b00000101 << 1 = 0b00001010 (10)
    // 0b11110000 | 0b00001010 = 0b11111010 (250)
    assert_eq!(cpu.a, 0b11111010, "A should be 0xF0 | (5 << 1)");
    assert_eq!(
        cpu.bus.borrow_mut().read(0x1234, false),
        0b00001010,
        "Memory should contain shifted value"
    );
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry flag should not be set");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

#[test]
fn test_execute_slo_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load SLO $42
    let program = vec![SLO_ZP, 0x42];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0042, 0b01000000, false); // Value at ZP (64)
    cpu.a = 0b00001111; // 15

    cpu.execute();

    // 0b01000000 << 1 = 0b10000000 (128)
    // 0b00001111 | 0b10000000 = 0b10001111 (143)
    assert_eq!(cpu.a, 0b10001111, "A should be 15 | (64 << 1)");
    assert_eq!(
        cpu.bus.borrow_mut().read(0x0042, false),
        0b10000000,
        "Memory should contain shifted value"
    );
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry flag should not be set");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

#[test]
fn test_execute_slo_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load SLO $1234
    let program = vec![SLO_ABS, 0x34, 0x12]; // Low byte, High byte
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x1234, 0b10000001, false); // Value (129)
    cpu.a = 0b00001111; // 15

    cpu.execute();

    // 0b10000001 << 1 = 0b00000010 (2), carry set
    // 0b00001111 | 0b00000010 = 0b00001111 (15)
    assert_eq!(cpu.a, 0b00001111, "A should be 15 | (129 << 1)");
    assert_eq!(
        cpu.bus.borrow_mut().read(0x1234, false),
        0b00000010,
        "Memory should contain shifted value"
    );
    assert_eq!(
        cpu.p & FLAG_CARRY,
        FLAG_CARRY,
        "Carry flag should be set (bit 7 was 1)"
    );
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
}

#[test]
fn test_execute_slo_indirect_indexed() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load SLO ($40),Y
    let program = vec![SLO_INDYW, 0x40];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Set up indirect address at zero page $40
    cpu.bus.borrow_mut().write(0x0040, 0x34, false); // Low byte of base address
    cpu.bus.borrow_mut().write(0x0041, 0x12, false); // High byte of base address
    cpu.bus.borrow_mut().write(0x1237, 0b00000011, false); // Value at base + Y (3)

    cpu.a = 0b11000000; // 192
    cpu.y = 0x03;

    cpu.execute();

    // 0b00000011 << 1 = 0b00000110 (6)
    // 0b11000000 | 0b00000110 = 0b11000110 (198)
    assert_eq!(cpu.a, 0b11000110, "A should be 192 | (3 << 1)");
    assert_eq!(
        cpu.bus.borrow_mut().read(0x1237, false),
        0b00000110,
        "Memory should contain shifted value"
    );
}

#[test]
fn test_execute_slo_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load SLO $40,X
    let program = vec![SLO_ZPX, 0x40];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0042, 0b00010000, false); // Value at ZP $42 (16)
    cpu.a = 0b00000001; // 1
    cpu.x = 0x02;

    cpu.execute();

    // 0b00010000 << 1 = 0b00100000 (32)
    // 0b00000001 | 0b00100000 = 0b00100001 (33)
    assert_eq!(cpu.a, 0b00100001, "A should be 1 | (16 << 1)");
    assert_eq!(
        cpu.bus.borrow_mut().read(0x0042, false),
        0b00100000,
        "Memory should contain shifted value"
    );
}

#[test]
fn test_execute_slo_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load SLO $1234,Y
    let program = vec![SLO_ABSYW, 0x34, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x1237, 0b00000010, false); // Value at $1237 (2)
    cpu.a = 0b01010101; // 85
    cpu.y = 0x03;

    cpu.execute();

    // 0b00000010 << 1 = 0b00000100 (4)
    // 0b01010101 | 0b00000100 = 0b01010101 (85)
    assert_eq!(cpu.a, 0b01010101, "A should be 85 | (2 << 1)");
    assert_eq!(
        cpu.bus.borrow_mut().read(0x1237, false),
        0b00000100,
        "Memory should contain shifted value"
    );
}

#[test]
fn test_execute_slo_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load SLO $1234,X
    let program = vec![SLO_ABSXW, 0x34, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x1236, 0b00001000, false); // Value at $1236 (8)
    cpu.a = 0b00000000; // 0
    cpu.x = 0x02;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    // 0b00001000 << 1 = 0b00010000 (16)
    // 0b00000000 | 0b00010000 = 0b00010000 (16)
    assert_eq!(cpu.a, 0b00010000, "A should be 0 | (8 << 1)");
    assert_eq!(
        cpu.bus.borrow_mut().read(0x1236, false),
        0b00010000,
        "Memory should contain shifted value"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 7,
        "SLO absolute,X should take 7 cycles"
    );
}

#[test]
fn test_execute_slo_zero_flag() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load SLO $42
    let program = vec![SLO_ZP, 0x42];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0042, 0b00000000, false); // Zero value
    cpu.a = 0b00000000; // 0
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    // 0 << 1 = 0
    // 0 | 0 = 0
    assert_eq!(cpu.a, 0, "A should be 0");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry flag should not be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "SLO zero page should take 5 cycles"
    );
}

#[test]
fn test_execute_nop() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load NOP (official)
    let program = vec![NOP];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Set up initial state
    cpu.a = 0x42;
    cpu.x = 0x13;
    cpu.y = 0x37;
    cpu.p = 0b11001010;
    let initial_sp = cpu.sp;
    let initial_pc = cpu.pc;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    // NOP should not change any state except PC
    assert_eq!(cpu.a, 0x42, "A should remain unchanged");
    assert_eq!(cpu.x, 0x13, "X should remain unchanged");
    assert_eq!(cpu.y, 0x37, "Y should remain unchanged");
    assert_eq!(cpu.p, 0b11001010, "Status flags should remain unchanged");
    assert_eq!(cpu.sp, initial_sp, "Stack pointer should remain unchanged");
    assert_eq!(cpu.pc, initial_pc + 1, "PC should advance by 1 byte");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "NOP should take 2 cycles"
    );
}

#[test]
fn test_execute_nop_undocumented_0x1a() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    let program = vec![NOP_IMP];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x55;
    cpu.x = 0xAA;
    cpu.y = 0xFF;
    cpu.p = 0b10101010;
    let initial_sp = cpu.sp;
    let initial_pc = cpu.pc;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    assert_eq!(cpu.a, 0x55, "A should remain unchanged");
    assert_eq!(cpu.x, 0xAA, "X should remain unchanged");
    assert_eq!(cpu.y, 0xFF, "Y should remain unchanged");
    assert_eq!(cpu.p, 0b10101010, "Status flags should remain unchanged");
    assert_eq!(cpu.sp, initial_sp, "Stack pointer should remain unchanged");
    assert_eq!(cpu.pc, initial_pc + 1, "PC should advance by 1 byte");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "*NOP (0x1A) should take 2 cycles"
    );
}

#[test]
fn test_execute_nop_undocumented_0x3a() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    let program = vec![NOP_IMP2];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x11;
    cpu.x = 0x22;
    cpu.y = 0x33;
    cpu.p = 0b00110011;
    let initial_sp = cpu.sp;
    let initial_pc = cpu.pc;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    assert_eq!(cpu.a, 0x11, "A should remain unchanged");
    assert_eq!(cpu.x, 0x22, "X should remain unchanged");
    assert_eq!(cpu.y, 0x33, "Y should remain unchanged");
    assert_eq!(cpu.p, 0b00110011, "Status flags should remain unchanged");
    assert_eq!(cpu.sp, initial_sp, "Stack pointer should remain unchanged");
    assert_eq!(cpu.pc, initial_pc + 1, "PC should advance by 1 byte");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "*NOP (0x3A) should take 2 cycles"
    );
}

#[test]
fn test_execute_nop_undocumented_0x5a() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    let program = vec![NOP_IMP3];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xCC;
    cpu.x = 0xDD;
    cpu.y = 0xEE;
    cpu.p = 0b11110000;
    let initial_sp = cpu.sp;
    let initial_pc = cpu.pc;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    assert_eq!(cpu.a, 0xCC, "A should remain unchanged");
    assert_eq!(cpu.x, 0xDD, "X should remain unchanged");
    assert_eq!(cpu.y, 0xEE, "Y should remain unchanged");
    assert_eq!(cpu.p, 0b11110000, "Status flags should remain unchanged");
    assert_eq!(cpu.sp, initial_sp, "Stack pointer should remain unchanged");
    assert_eq!(cpu.pc, initial_pc + 1, "PC should advance by 1 byte");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "*NOP (0x5A) should take 2 cycles"
    );
}

#[test]
fn test_execute_nop_undocumented_0x7a() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    let program = vec![NOP_IMP4];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x01;
    cpu.x = 0x02;
    cpu.y = 0x03;
    cpu.p = 0b00001111;
    let initial_sp = cpu.sp;
    let initial_pc = cpu.pc;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    assert_eq!(cpu.a, 0x01, "A should remain unchanged");
    assert_eq!(cpu.x, 0x02, "X should remain unchanged");
    assert_eq!(cpu.y, 0x03, "Y should remain unchanged");
    assert_eq!(cpu.p, 0b00001111, "Status flags should remain unchanged");
    assert_eq!(cpu.sp, initial_sp, "Stack pointer should remain unchanged");
    assert_eq!(cpu.pc, initial_pc + 1, "PC should advance by 1 byte");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "*NOP (0x7A) should take 2 cycles"
    );
}

#[test]
fn test_execute_nop_undocumented_0xda() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    let program = vec![NOP_IMP5];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x7F;
    cpu.x = 0x80;
    cpu.y = 0x81;
    cpu.p = 0b01010101;
    let initial_sp = cpu.sp;
    let initial_pc = cpu.pc;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    assert_eq!(cpu.a, 0x7F, "A should remain unchanged");
    assert_eq!(cpu.x, 0x80, "X should remain unchanged");
    assert_eq!(cpu.y, 0x81, "Y should remain unchanged");
    assert_eq!(cpu.p, 0b01010101, "Status flags should remain unchanged");
    assert_eq!(cpu.sp, initial_sp, "Stack pointer should remain unchanged");
    assert_eq!(cpu.pc, initial_pc + 1, "PC should advance by 1 byte");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "*NOP (0xDA) should take 2 cycles"
    );
}

#[test]
fn test_execute_nop_undocumented_0xfa() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    let program = vec![NOP_IMP6];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xFE;
    cpu.x = 0xFD;
    cpu.y = 0xFC;
    cpu.p = 0b10011001;
    let initial_sp = cpu.sp;
    let initial_pc = cpu.pc;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    assert_eq!(cpu.a, 0xFE, "A should remain unchanged");
    assert_eq!(cpu.x, 0xFD, "X should remain unchanged");
    assert_eq!(cpu.y, 0xFC, "Y should remain unchanged");
    assert_eq!(cpu.p, 0b10011001, "Status flags should remain unchanged");
    assert_eq!(cpu.sp, initial_sp, "Stack pointer should remain unchanged");
    assert_eq!(cpu.pc, initial_pc + 1, "PC should advance by 1 byte");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "*NOP (0xFA) should take 2 cycles"
    );
}

// Tests for ASL instruction

#[test]
fn test_execute_asl_accumulator() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load ASL A
    let program = vec![ASL_A];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0b01010101; // 85 decimal
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    assert_eq!(
        cpu.a, 0b10101010,
        "A should be shifted left: 0b01010101 << 1 = 0b10101010"
    );
    assert_eq!(
        cpu.p & FLAG_CARRY,
        0,
        "Carry flag should not be set (bit 7 was 0)"
    );
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set (bit 7 is 1)"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "ASL accumulator should take 2 cycles"
    );
}

#[test]
fn test_execute_asl_accumulator_with_carry() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load ASL A
    let program = vec![ASL_A];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0b10000001; // bit 7 is set

    cpu.execute();

    assert_eq!(
        cpu.a, 0b00000010,
        "A should be shifted left with bit 7 moved to carry"
    );
    assert_eq!(
        cpu.p & FLAG_CARRY,
        FLAG_CARRY,
        "Carry flag should be set (bit 7 was 1)"
    );
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
}

#[test]
fn test_execute_asl_accumulator_zero_result() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load ASL A
    let program = vec![ASL_A];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0b00000000;

    cpu.execute();

    assert_eq!(cpu.a, 0, "A should be 0");
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry flag should not be set");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
}

#[test]
fn test_execute_asl_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load ASL $42
    let program = vec![ASL_ZP, 0x42];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0042, 0b01010101, false);
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0042, false),
        0b10101010,
        "Memory should be shifted left"
    );
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry flag should not be set");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "ASL zero page should take 5 cycles"
    );
}

#[test]
fn test_execute_asl_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load ASL $40,X
    let program = vec![ASL_ZPX, 0x40];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x0045, 0b11000000, false);
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0045, false),
        0b10000000,
        "Memory at $45 should be shifted left"
    );
    assert_eq!(
        cpu.p & FLAG_CARRY,
        FLAG_CARRY,
        "Carry flag should be set (bit 7 was 1)"
    );
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "ASL zero page,X should take 6 cycles"
    );
}

#[test]
fn test_execute_asl_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load ASL $1234
    let program = vec![ASL_ABS, 0x34, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x1234, 0b00100000, false);
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x1234, false),
        0b01000000,
        "Memory at $1234 should be shifted left"
    );
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry flag should not be set");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "ASL absolute should take 6 cycles"
    );
}

#[test]
fn test_execute_asl_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load ASL $1230,X
    let program = vec![ASL_ABSXW, 0x30, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x04;
    cpu.bus.borrow_mut().write(0x1234, 0b10000000, false);
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x1234, false),
        0b00000000,
        "Memory at $1234 should be shifted left to 0"
    );
    assert_eq!(
        cpu.p & FLAG_CARRY,
        FLAG_CARRY,
        "Carry flag should be set (bit 7 was 1)"
    );
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 7,
        "ASL absolute,X should take 7 cycles"
    );
}

// Tests for PHP instruction

#[test]
fn test_execute_php() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load PHP
    let program = vec![PHP];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Set some flags
    cpu.p = 0b10110101; // N=1, V=0, B=1(ignored), D=1, I=0, Z=1, C=1
    cpu.sp = 0xFF; // Start at top of stack
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    // PHP should push P with bits 4 and 5 set (BREAK and UNUSED flags)
    let pushed_value = cpu.bus.borrow_mut().read(0x01FF, false);
    assert_eq!(
        pushed_value,
        0b10110101 | FLAG_BREAK | FLAG_UNUSED,
        "PHP should push P with BREAK and UNUSED flags set"
    );
    assert_eq!(cpu.sp, 0xFE, "Stack pointer should be decremented by 1");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "PHP should take 3 cycles"
    );
}

#[test]
fn test_execute_php_preserves_flags() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load PHP
    let program = vec![PHP];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p = 0b11001010;
    let initial_p = cpu.p;

    cpu.execute();

    assert_eq!(cpu.p, initial_p, "PHP should not modify the P register");
}

// Tests for AAC (undocumented instruction)

#[test]
fn test_execute_aac_immediate_0x0b() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load AAC #$F0
    let program = vec![AAC_IMM, 0xF0];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0b11110000;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    // AAC does AND then copies bit 7 to carry
    assert_eq!(cpu.a, 0b11110000, "A should be ANDed with 0xF0");
    assert_eq!(
        cpu.p & FLAG_CARRY,
        FLAG_CARRY,
        "Carry flag should be set (bit 7 is 1)"
    );
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "AAC should take 2 cycles"
    );
}

#[test]
fn test_execute_aac_immediate_0x2b() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load AAC #$7F (alternate opcode)
    let program = vec![AAC_IMM2, 0x7F];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0b11110000;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    // AAC does AND then copies bit 7 to carry
    assert_eq!(cpu.a, 0b01110000, "A should be ANDed with 0x7F");
    assert_eq!(
        cpu.p & FLAG_CARRY,
        0,
        "Carry flag should not be set (bit 7 is 0)"
    );
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        0,
        "Negative flag should not be set (bit 7 is 0)"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "AAC should take 2 cycles"
    );
}

#[test]
fn test_execute_aac_zero_result() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load AAC #$00
    let program = vec![AAC_IMM, 0x00];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xFF;

    cpu.execute();

    assert_eq!(cpu.a, 0x00, "A should be 0");
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry flag should not be set");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
}

// Tests for BPL instruction

#[test]
fn test_execute_bpl_branch_taken_positive() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load BPL +5
    let program = vec![BPL, 0x05];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p &= !FLAG_NEGATIVE; // Clear negative flag
    let initial_pc = cpu.pc;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    // Branch taken: PC should be initial + 2 (instruction length) + 5 (offset)
    assert_eq!(cpu.pc, initial_pc + 2 + 5, "PC should branch forward by 5");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "BPL with branch taken (no page cross) should take 3 cycles"
    );
}

#[test]
fn test_execute_bpl_branch_not_taken_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load BPL +5
    let program = vec![BPL, 0x05];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p |= FLAG_NEGATIVE; // Set negative flag
    let initial_pc = cpu.pc;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    // Branch not taken: PC should just advance past the instruction
    assert_eq!(
        cpu.pc,
        initial_pc + 2,
        "PC should advance by 2 (instruction length)"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "BPL with branch not taken should take 2 cycles"
    );
}

#[test]
fn test_execute_bpl_branch_backward() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load BPL -10 (0xF6 in two's complement)
    let program = vec![BPL, 0xF6];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p &= !FLAG_NEGATIVE; // Clear negative flag
    let initial_pc = cpu.pc;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    // Branch taken backward: PC + 2 - 10 = PC - 8
    // This crosses a page boundary (0x8000 -> 0x7FF8), so should take 4 cycles
    assert_eq!(
        cpu.pc,
        initial_pc.wrapping_add(2).wrapping_sub(10),
        "PC should branch backward by 10"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "BPL with branch taken crossing page should take 4 cycles"
    );
}

#[test]
fn test_execute_bpl_branch_page_crossing() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Create program with BPL near end of page to cause page crossing
    // We want the branch TARGET to cross a page, not just the instruction location
    // Place instruction so that PC after instruction + offset crosses page
    // If we put BPL at 0x80FE, after reading it PC will be 0x8100
    // Then branching with offset 0x10 gives us 0x8110 (no page cross)
    // So we need offset to make PC go from 0x80xx to 0x81xx (or higher)
    // Let's use offset 0x70 (112 bytes forward) from position 0x8090
    // PC after instruction: 0x8092, target: 0x8092 + 0x70 = 0x8102 (page cross from 0x80 to 0x81)

    let mut program = vec![0xEA; 0x90]; // 144 NOPs to position us at 0x8090
    program.push(BPL); // At offset 0x90
    program.push(0x70); // +112 bytes

    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Execute NOPs to get to the BPL instruction
    for _ in 0..0x90 {
        cpu.execute();
    }

    cpu.p &= !FLAG_NEGATIVE; // Clear negative flag
    let initial_pc = cpu.pc;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    // Branch crosses page: PC goes from 0x8092 to 0x8102
    assert_eq!(
        cpu.pc,
        initial_pc + 2 + 0x70,
        "PC should branch forward crossing page"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "BPL with branch taken and page cross should take 4 cycles"
    );
}

// Tests for CLC instruction

#[test]
fn test_execute_clc() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load CLC
    let program = vec![CLC];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p = 0xFF; // Set all flags including carry
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry flag should be cleared");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Other flags should remain unchanged"
    );
    assert_eq!(
        cpu.p & FLAG_ZERO,
        FLAG_ZERO,
        "Other flags should remain unchanged"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "CLC should take 2 cycles"
    );
}

#[test]
fn test_execute_clc_already_clear() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    let program = vec![CLC];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p &= !FLAG_CARRY; // Clear carry flag

    cpu.execute();

    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry flag should remain cleared");
}

// Tests for AND instruction

#[test]
fn test_execute_and_immediate() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load AND #$0F
    let program = vec![AND_IMM, 0x0F];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xFF;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    assert_eq!(cpu.a, 0x0F, "A should be ANDed with 0x0F");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "AND immediate should take 2 cycles"
    );
}

#[test]
fn test_execute_and_zero_result() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load AND #$00
    let program = vec![AND_IMM, 0x00];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xFF;

    cpu.execute();

    assert_eq!(cpu.a, 0x00, "A should be 0");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
}

#[test]
fn test_execute_and_negative_result() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load AND #$80
    let program = vec![AND_IMM, 0x80];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xFF;

    cpu.execute();

    assert_eq!(cpu.a, 0x80, "A should be 0x80");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

#[test]
fn test_execute_and_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load AND $42
    let program = vec![AND_ZP, 0x42];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0042, 0xF0, false);
    cpu.a = 0x55;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    assert_eq!(cpu.a, 0x50, "A should be ANDed with memory value");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "AND zero page should take 3 cycles"
    );
}

#[test]
fn test_execute_and_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load AND $40,X
    let program = vec![AND_ZPX, 0x40];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x0045, 0x0F, false);
    cpu.a = 0xFF;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    assert_eq!(cpu.a, 0x0F, "A should be ANDed with memory at $45");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "AND zero page,X should take 4 cycles"
    );
}

#[test]
fn test_execute_and_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load AND $1234
    let program = vec![AND_ABS, 0x34, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x1234, 0xAA, false);
    cpu.a = 0x55;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    assert_eq!(cpu.a, 0x00, "A should be 0 (0x55 & 0xAA)");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "AND absolute should take 4 cycles"
    );
}

#[test]
fn test_execute_and_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load AND $1230,X
    let program = vec![AND_ABSX, 0x30, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x04;
    cpu.bus.borrow_mut().write(0x1234, 0xCC, false);
    cpu.a = 0xFF;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    assert_eq!(cpu.a, 0xCC, "A should be ANDed with memory at $1234");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "AND absolute,X should take 4 cycles (no page cross)"
    );
}

#[test]
fn test_execute_and_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load AND $1230,Y
    let program = vec![AND_ABSY, 0x30, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x04;
    cpu.bus.borrow_mut().write(0x1234, 0x3C, false);
    cpu.a = 0xFF;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    assert_eq!(cpu.a, 0x3C, "A should be ANDed with memory at $1234");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "AND absolute,Y should take 4 cycles (no page cross)"
    );
}

#[test]
fn test_execute_and_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load AND ($40,X)
    let program = vec![AND_INDX, 0x40];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x05;
    // Set up pointer at $45 pointing to $1234
    cpu.bus.borrow_mut().write(0x0045, 0x34, false);
    cpu.bus.borrow_mut().write(0x0046, 0x12, false);
    cpu.bus.borrow_mut().write(0x1234, 0x77, false);
    cpu.a = 0xFF;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    assert_eq!(cpu.a, 0x77, "A should be ANDed with memory at ($45)");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "AND indexed indirect should take 6 cycles"
    );
}

#[test]
fn test_execute_and_indirect_indexed() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load AND ($40),Y
    let program = vec![AND_INDY, 0x40];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x05;
    // Set up pointer at $40 pointing to $1230
    cpu.bus.borrow_mut().write(0x0040, 0x30, false);
    cpu.bus.borrow_mut().write(0x0041, 0x12, false);
    cpu.bus.borrow_mut().write(0x1235, 0x88, false);
    cpu.a = 0xFF;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    assert_eq!(cpu.a, 0x88, "A should be ANDed with memory at ($40),Y");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "AND indirect indexed should take 5 cycles (no page cross)"
    );
}

// Tests for RLA (undocumented instruction)

#[test]
fn test_execute_rla_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load RLA $42
    let program = vec![RLA_ZP, 0x42];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0042, 0b01010101, false);
    cpu.a = 0xFF;
    cpu.p &= !FLAG_CARRY; // Clear carry
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    // RLA: ROL memory then AND with A
    // 0b01010101 ROL with carry=0 -> 0b10101010
    // 0xFF AND 0b10101010 -> 0b10101010
    assert_eq!(
        cpu.bus.borrow_mut().read(0x0042, false),
        0b10101010,
        "Memory should be rotated left"
    );
    assert_eq!(cpu.a, 0b10101010, "A should be ANDed with rotated value");
    assert_eq!(
        cpu.p & FLAG_CARRY,
        0,
        "Carry should not be set (bit 7 was 0)"
    );
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "RLA zero page should take 5 cycles"
    );
}

#[test]
fn test_execute_rla_zero_page_with_carry() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load RLA $42
    let program = vec![RLA_ZP, 0x42];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0042, 0b10000001, false);
    cpu.a = 0xFF;
    cpu.p |= FLAG_CARRY; // Set carry

    cpu.execute();

    // RLA: ROL memory then AND with A
    // 0b10000001 ROL with carry=1 -> 0b00000011 (carry out = 1)
    // 0xFF AND 0b00000011 -> 0b00000011
    assert_eq!(
        cpu.bus.borrow_mut().read(0x0042, false),
        0b00000011,
        "Memory should be rotated left with carry in"
    );
    assert_eq!(cpu.a, 0b00000011, "A should be ANDed with rotated value");
    assert_eq!(
        cpu.p & FLAG_CARRY,
        FLAG_CARRY,
        "Carry should be set (bit 7 was 1)"
    );
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
}

#[test]
fn test_execute_rla_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load RLA $40,X
    let program = vec![RLA_ZPX, 0x40];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x0045, 0b00110011, false);
    cpu.a = 0b11110000;
    cpu.p &= !FLAG_CARRY;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    // 0b00110011 ROL with carry=0 -> 0b01100110
    // 0b11110000 AND 0b01100110 -> 0b01100000
    assert_eq!(cpu.a, 0b01100000, "A should be ANDed with rotated value");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "RLA zero page,X should take 6 cycles"
    );
}

#[test]
fn test_execute_rla_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load RLA $1234
    let program = vec![RLA_ABS, 0x34, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x1234, 0b01000000, false);
    cpu.a = 0b11111111;
    cpu.p &= !FLAG_CARRY;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    // 0b01000000 ROL -> 0b10000000
    // 0xFF AND 0x80 -> 0x80
    assert_eq!(cpu.a, 0b10000000, "A should be ANDed with rotated value");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "RLA absolute should take 6 cycles"
    );
}

#[test]
fn test_execute_rla_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load RLA $1230,X
    let program = vec![RLA_ABSXW, 0x30, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x04;
    cpu.bus.borrow_mut().write(0x1234, 0b00000001, false);
    cpu.a = 0b11111111;
    cpu.p &= !FLAG_CARRY;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    // 0b00000001 ROL -> 0b00000010
    // 0xFF AND 0x02 -> 0x02
    assert_eq!(cpu.a, 0b00000010, "A should be ANDed with rotated value");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 7,
        "RLA absolute,X should take 7 cycles"
    );
}

#[test]
fn test_execute_rla_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load RLA $1230,Y
    let program = vec![RLA_ABSYW, 0x30, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x04;
    cpu.bus.borrow_mut().write(0x1234, 0b11000000, false);
    cpu.a = 0b11111111;
    cpu.p &= !FLAG_CARRY;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    // 0b11000000 ROL -> 0b10000000 (carry out = 1)
    // 0xFF AND 0x80 -> 0x80
    assert_eq!(cpu.a, 0b10000000, "A should be ANDed with rotated value");
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 7,
        "RLA absolute,Y should take 7 cycles"
    );
}

#[test]
fn test_execute_rla_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load RLA ($40,X)
    let program = vec![RLA_INDX, 0x40];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x05;
    // Set up pointer at $45 pointing to $1234
    cpu.bus.borrow_mut().write(0x0045, 0x34, false);
    cpu.bus.borrow_mut().write(0x0046, 0x12, false);
    cpu.bus.borrow_mut().write(0x1234, 0b00001111, false);
    cpu.a = 0b11110000;
    cpu.p &= !FLAG_CARRY;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    // 0b00001111 ROL -> 0b00011110
    // 0b11110000 AND 0b00011110 -> 0b00010000
    assert_eq!(cpu.a, 0b00010000, "A should be ANDed with rotated value");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 8,
        "RLA indexed indirect should take 8 cycles"
    );
}

#[test]
fn test_execute_rla_indirect_indexed() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory.clone(), ppu.clone(), apu.clone());
    // Load RLA ($40),Y
    let program = vec![RLA_INDYW, 0x40];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x05;
    // Set up pointer at $40 pointing to $1230
    cpu.bus.borrow_mut().write(0x0040, 0x30, false);
    cpu.bus.borrow_mut().write(0x0041, 0x12, false);
    cpu.bus.borrow_mut().write(0x1235, 0b10101010, false);
    cpu.a = 0b11111111;
    cpu.p &= !FLAG_CARRY;
    let initial_cycles = cpu.total_cycles;

    cpu.execute();

    // 0b10101010 ROL -> 0b01010100 (carry out = 1)
    // 0xFF AND 0b01010100 -> 0b01010100
    assert_eq!(cpu.a, 0b01010100, "A should be ANDed with rotated value");
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 8,
        "RLA indirect indexed should take 8 cycles"
    );
}

#[test]
fn test_execute_jsr_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // JSR $1234 (opcode 0x20)
    let program = vec![JSR, 0x34, 0x12]; // JSR $1234
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.sp = 0xFF; // Full stack

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // JSR pushes return address (PC - 1) to stack
    // PC after reading all operands = 0x8003
    // Return address = 0x8003 - 1 = 0x8002
    assert_eq!(cpu.pc, 0x1234, "PC should be set to target address");
    assert_eq!(cpu.sp, 0xFD, "SP should have decremented by 2");

    // Check stack contents (return address high byte first, then low byte)
    assert_eq!(
        cpu.bus.borrow_mut().read(0x01FF, false),
        0x80,
        "High byte of return address on stack"
    );
    assert_eq!(
        cpu.bus.borrow_mut().read(0x01FE, false),
        0x02,
        "Low byte of return address on stack"
    );

    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "JSR should take 6 cycles"
    );
}

#[test]
fn test_execute_jsr_stack_wrapping() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // JSR $5678
    let program = vec![JSR, 0x78, 0x56]; // JSR $5678
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.sp = 0x01; // Stack pointer near bottom

    cpu.execute();

    // Check stack wrapping
    assert_eq!(cpu.sp, 0xFF, "SP should wrap around");
    assert_eq!(
        cpu.bus.borrow_mut().read(0x0101, false),
        0x80,
        "High byte pushed at correct location"
    );
    assert_eq!(
        cpu.bus.borrow_mut().read(0x0100, false),
        0x02,
        "Low byte pushed at correct location"
    );
    assert_eq!(cpu.pc, 0x5678, "PC set to target");
}

// BIT tests
#[test]
fn test_execute_bit_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BIT_ZP, 0x42];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0042, 0b11000000, false); // Bit 7 and 6 set
    cpu.a = 0b00001111; // Only lower bits set

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // Result of A & M = 0b00001111 & 0b11000000 = 0, so Zero flag set
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    // Bit 7 of M copied to Negative flag
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
    // Bit 6 of M copied to Overflow flag
    assert_eq!(
        cpu.p & FLAG_OVERFLOW,
        FLAG_OVERFLOW,
        "Overflow flag should be set"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "BIT ZP takes 3 cycles"
    );
}

#[test]
fn test_execute_bit_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BIT_ABS, 0x34, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x1234, 0b01110000, false); // Bit 6, 5 and 4 set
    cpu.a = 0b00110000; // Bit 5 and 4 set

    cpu.execute();

    // Result of A & M = 0b00110000, not zero
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    // Bit 7 of M (0) copied to Negative flag
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    // Bit 6 of M (1) copied to Overflow flag
    assert_eq!(
        cpu.p & FLAG_OVERFLOW,
        FLAG_OVERFLOW,
        "Overflow flag should be set"
    );
}

// ROL tests
#[test]
fn test_execute_rol_accumulator() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROL_ACC];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0b10000001;
    cpu.p |= FLAG_CARRY; // Carry in = 1

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // 0b10000001 << 1 | 1 = 0b00000011
    assert_eq!(cpu.a, 0b00000011, "A should be rotated left with carry in");
    assert_eq!(
        cpu.p & FLAG_CARRY,
        FLAG_CARRY,
        "Carry should be set from bit 7"
    );
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "ROL ACC takes 2 cycles"
    );
}

#[test]
fn test_execute_rol_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROL_ZP, 0x42];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0042, 0b01000000, false);
    cpu.p &= !FLAG_CARRY; // Carry in = 0

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // 0b01000000 << 1 | 0 = 0b10000000
    assert_eq!(
        cpu.bus.borrow_mut().read(0x0042, false),
        0b10000000,
        "Memory should be rotated left"
    );
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry should not be set");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "ROL ZP takes 5 cycles"
    );
}

#[test]
fn test_execute_rol_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROL_ZPX, 0x40];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x02;
    cpu.bus.borrow_mut().write(0x0042, 0b11111111, false);
    cpu.p |= FLAG_CARRY;

    cpu.execute();

    // 0b11111111 << 1 | 1 = 0b11111111 (with carry out)
    assert_eq!(
        cpu.bus.borrow_mut().read(0x0042, false),
        0b11111111,
        "Memory at ZP+X should be rotated"
    );
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry should be set");
}

#[test]
fn test_execute_rol_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROL_ABS, 0x34, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x1234, 0b00000001, false);
    cpu.p &= !FLAG_CARRY;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x1234, false),
        0b00000010,
        "Memory should be rotated left"
    );
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry should not be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "ROL ABS takes 6 cycles"
    );
}

#[test]
fn test_execute_rol_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROL_ABSXW, 0x00, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x34;
    cpu.bus.borrow_mut().write(0x1234, 0b10000000, false);
    cpu.p &= !FLAG_CARRY;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // 0b10000000 << 1 | 0 = 0b00000000
    assert_eq!(
        cpu.bus.borrow_mut().read(0x1234, false),
        0b00000000,
        "Memory at ABS+X should be rotated"
    );
    assert_eq!(
        cpu.p & FLAG_CARRY,
        FLAG_CARRY,
        "Carry should be set from bit 7"
    );
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 7,
        "ROL ABSXW takes 7 cycles"
    );
}

// PLP tests
#[test]
fn test_execute_plp() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![PLP];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Push a status value onto stack
    cpu.sp = 0xFF;
    cpu.bus.borrow_mut().write(0x01FE, 0b11001010, false); // Some flags
    cpu.sp = 0xFD; // Simulate already pushed

    cpu.p = 0b00000000; // Clear all flags

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // PLP should restore flags, ignoring BREAK and UNUSED bits
    // Expected: 0b11001010 with BREAK cleared and UNUSED set
    // The 6502 always has bit 5 (UNUSED) set, and ignores bit 4 (BREAK) on PLP
    assert_eq!(cpu.sp, 0xFE, "SP should increment");
    // Check that flags were restored (BREAK is cleared, UNUSED is set)
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be restored"
    );
    assert_eq!(
        cpu.p & FLAG_OVERFLOW,
        FLAG_OVERFLOW,
        "Overflow flag should be restored"
    );
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be restored");
    assert_eq!(
        cpu.p & FLAG_INTERRUPT,
        0,
        "Interrupt flag should be restored"
    );
    assert_eq!(
        cpu.p & FLAG_BREAK,
        0,
        "BREAK flag should be ignored/cleared"
    );
    assert_eq!(
        cpu.p & FLAG_UNUSED,
        FLAG_UNUSED,
        "UNUSED flag should be set"
    );
    assert_eq!(cpu.total_cycles, initial_cycles + 4, "PLP takes 4 cycles");
}

#[test]
fn test_execute_plp_preserves_break_behavior() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![PLP];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Push status with BREAK flag set
    cpu.bus.borrow_mut().write(0x01FF, 0b00010000, false); // Only BREAK set
    cpu.sp = 0xFE;

    cpu.execute();

    // BREAK flag should be ignored on PLP
    assert_eq!(cpu.p & FLAG_BREAK, 0, "BREAK flag should be cleared");
    assert_eq!(
        cpu.p & FLAG_UNUSED,
        FLAG_UNUSED,
        "UNUSED should always be set"
    );
}

// BMI tests (Branch if Minus/Negative)
#[test]
fn test_execute_bmi_branch_taken() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BMI, 0x10]; // Branch forward +16
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p |= FLAG_NEGATIVE; // Set negative flag

    let initial_cycles = cpu.total_cycles;
    let initial_pc = cpu.pc;
    cpu.execute();

    // Branch should be taken: PC = 0x8002 + 0x10 = 0x8012
    assert_eq!(cpu.pc, initial_pc + 2 + 0x10, "PC should branch forward");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "BMI taken without page cross takes 3 cycles"
    );
}

#[test]
fn test_execute_bmi_branch_not_taken() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BMI, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p &= !FLAG_NEGATIVE; // Clear negative flag

    let initial_cycles = cpu.total_cycles;
    let initial_pc = cpu.pc;
    cpu.execute();

    // Branch not taken, PC advances past instruction
    assert_eq!(cpu.pc, initial_pc + 2, "PC should not branch");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "BMI not taken takes 2 cycles"
    );
}

#[test]
fn test_execute_bmi_branch_backward() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BMI, 0xFE]; // Branch backward -2
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p |= FLAG_NEGATIVE;

    let initial_pc = cpu.pc;
    cpu.execute();

    // Backward branch: PC = 0x8002 + (-2) = 0x8000
    assert_eq!(
        cpu.pc,
        initial_pc.wrapping_add(2u16.wrapping_add(0xFEu8 as i8 as u16)),
        "PC should branch backward"
    );
}

#[test]
fn test_execute_bmi_page_crossing() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // Position BMI so branch crosses page boundary
    let program = vec![
        0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, // NOPs to position
        0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA,
        0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA,
        0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA,
        0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA,
        0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA,
        0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA,
        0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA,
        0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA, 0xEA,
        BMI, 0x7F, // At 0x8080, branch to cause page cross
    ];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Execute NOPs to position PC
    for _ in 0..128 {
        cpu.execute();
    }

    cpu.p |= FLAG_NEGATIVE;
    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "BMI with page crossing takes 4 cycles"
    );
}

// SEC tests (Set Carry Flag)
#[test]
fn test_execute_sec() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SEC];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p &= !FLAG_CARRY; // Clear carry

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry flag should be set");
    assert_eq!(cpu.total_cycles, initial_cycles + 2, "SEC takes 2 cycles");
}

#[test]
fn test_execute_sec_already_set() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SEC];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p |= FLAG_CARRY; // Already set

    cpu.execute();

    assert_eq!(
        cpu.p & FLAG_CARRY,
        FLAG_CARRY,
        "Carry flag should remain set"
    );
}

// RTI tests (Return from Interrupt)
#[test]
fn test_execute_rti() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![RTI];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Set up stack as if returning from interrupt
    cpu.sp = 0xFC;
    cpu.bus.borrow_mut().write(0x01FD, 0b11001010, false); // Status byte
    cpu.bus.borrow_mut().write(0x01FE, 0x34, false); // PCL
    cpu.bus.borrow_mut().write(0x01FF, 0x12, false); // PCH

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // Check PC restored
    assert_eq!(cpu.pc, 0x1234, "PC should be restored from stack");
    // Check SP restored
    assert_eq!(cpu.sp, 0xFF, "SP should be incremented by 3");
    // Check flags restored
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be restored"
    );
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be restored");
    assert_eq!(cpu.total_cycles, initial_cycles + 6, "RTI takes 6 cycles");
}

#[test]
fn test_execute_rti_clears_delayed_i_flag() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![RTI];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Set up delayed I flag (simulating CLI or SEI executed)
    cpu.delayed_i_flag = Some(true);

    // Set up stack
    cpu.sp = 0xFC;
    cpu.bus.borrow_mut().write(0x01FD, 0b00000000, false); // Status with I cleared
    cpu.bus.borrow_mut().write(0x01FE, 0x00, false);
    cpu.bus.borrow_mut().write(0x01FF, 0x80, false);

    cpu.execute();

    // RTI should clear the delayed I flag immediately
    assert_eq!(cpu.delayed_i_flag, None, "RTI should clear delayed I flag");
}

#[test]
fn test_execute_rti_restores_break_and_unused() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![RTI];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Set up stack with BREAK set (should be ignored like PLP)
    cpu.sp = 0xFC;
    cpu.bus.borrow_mut().write(0x01FD, 0b00110000, false); // BREAK and UNUSED set
    cpu.bus.borrow_mut().write(0x01FE, 0x00, false);
    cpu.bus.borrow_mut().write(0x01FF, 0x80, false);

    cpu.execute();

    // BREAK should be ignored, UNUSED should be set
    assert_eq!(cpu.p & FLAG_BREAK, 0, "BREAK flag should be ignored");
    assert_eq!(cpu.p & FLAG_UNUSED, FLAG_UNUSED, "UNUSED should be set");
}

// EOR tests (Exclusive OR)
#[test]
fn test_execute_eor_immediate() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![EOR_IMM, 0b11110000];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0b10101010;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // 0b10101010 XOR 0b11110000 = 0b01011010
    assert_eq!(cpu.a, 0b01011010, "A should be XORed");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "EOR IMM takes 2 cycles"
    );
}

#[test]
fn test_execute_eor_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![EOR_ZP, 0x42];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0042, 0xFF, false);
    cpu.a = 0xFF;

    cpu.execute();

    // 0xFF XOR 0xFF = 0x00
    assert_eq!(cpu.a, 0x00, "A should be 0");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
}

#[test]
fn test_execute_eor_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![EOR_ZPX, 0x40];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x02;
    cpu.bus.borrow_mut().write(0x0042, 0b10000000, false);
    cpu.a = 0b00000000;

    cpu.execute();

    assert_eq!(cpu.a, 0b10000000, "A should have bit 7 set");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

#[test]
fn test_execute_eor_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![EOR_ABS, 0x34, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x1234, 0x0F, false);
    cpu.a = 0xF0;

    cpu.execute();

    // 0xF0 XOR 0x0F = 0xFF
    assert_eq!(cpu.a, 0xFF, "A should be 0xFF");
}

#[test]
fn test_execute_eor_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![EOR_ABSX, 0x00, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x34;
    cpu.bus.borrow_mut().write(0x1234, 0xAA, false);
    cpu.a = 0x55;

    cpu.execute();

    // 0x55 XOR 0xAA = 0xFF
    assert_eq!(cpu.a, 0xFF, "A should be 0xFF");
}

#[test]
fn test_execute_eor_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![EOR_ABSY, 0x00, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x34;
    cpu.bus.borrow_mut().write(0x1234, 0b00110011, false);
    cpu.a = 0b11001100;

    cpu.execute();

    // 0b11001100 XOR 0b00110011 = 0b11111111
    assert_eq!(cpu.a, 0xFF, "A should be 0xFF");
}

#[test]
fn test_execute_eor_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![EOR_INDX, 0x40];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x02;
    cpu.bus.borrow_mut().write(0x0042, 0x34, false); // Low byte
    cpu.bus.borrow_mut().write(0x0043, 0x12, false); // High byte
    cpu.bus.borrow_mut().write(0x1234, 0x01, false);
    cpu.a = 0x00;

    cpu.execute();

    assert_eq!(cpu.a, 0x01, "A should be 0x01");
}

#[test]
fn test_execute_eor_indirect_indexed() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![EOR_INDY, 0x40];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x03;
    cpu.bus.borrow_mut().write(0x0040, 0x31, false); // Low byte
    cpu.bus.borrow_mut().write(0x0041, 0x12, false); // High byte
    cpu.bus.borrow_mut().write(0x1234, 0xFF, false);
    cpu.a = 0xAA;

    cpu.execute();

    // 0xAA XOR 0xFF = 0x55
    assert_eq!(cpu.a, 0x55, "A should be 0x55");
}

// LSR tests (Logical Shift Right)
#[test]
fn test_execute_lsr_accumulator() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LSR_ACC];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0b10000001;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // 0b10000001 >> 1 = 0b01000000
    assert_eq!(cpu.a, 0b01000000, "A should be shifted right");
    assert_eq!(
        cpu.p & FLAG_CARRY,
        FLAG_CARRY,
        "Carry should be set from bit 0"
    );
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "LSR ACC takes 2 cycles"
    );
}

#[test]
fn test_execute_lsr_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LSR_ZP, 0x42];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0042, 0b00000010, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // 0b00000010 >> 1 = 0b00000001
    assert_eq!(
        cpu.bus.borrow_mut().read(0x0042, false),
        0b00000001,
        "Memory should be shifted right"
    );
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry should not be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "LSR ZP takes 5 cycles"
    );
}

#[test]
fn test_execute_lsr_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LSR_ZPX, 0x40];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x02;
    cpu.bus.borrow_mut().write(0x0042, 0b00000001, false);

    cpu.execute();

    // 0b00000001 >> 1 = 0b00000000
    assert_eq!(
        cpu.bus.borrow_mut().read(0x0042, false),
        0b00000000,
        "Memory should be 0"
    );
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry should be set");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
}

#[test]
fn test_execute_lsr_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LSR_ABS, 0x34, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x1234, 0xFF, false);

    cpu.execute();

    // 0xFF >> 1 = 0x7F
    assert_eq!(
        cpu.bus.borrow_mut().read(0x1234, false),
        0x7F,
        "Memory should be 0x7F"
    );
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry should be set");
}

#[test]
fn test_execute_lsr_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LSR_ABSXW, 0x00, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x34;
    cpu.bus.borrow_mut().write(0x1234, 0b10101010, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // 0b10101010 >> 1 = 0b01010101
    assert_eq!(
        cpu.bus.borrow_mut().read(0x1234, false),
        0b01010101,
        "Memory should be shifted"
    );
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry should not be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 7,
        "LSR ABSXW takes 7 cycles"
    );
}

// SRE tests (Shift Right then EOR - undocumented)
#[test]
fn test_execute_sre_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SRE_ZP, 0x42];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0042, 0b00001111, false);
    cpu.a = 0b11110000;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // 0b00001111 >> 1 = 0b00000111
    // 0b11110000 XOR 0b00000111 = 0b11110111
    assert_eq!(cpu.a, 0b11110111, "A should be XORed with shifted value");
    assert_eq!(
        cpu.bus.borrow_mut().read(0x0042, false),
        0b00000111,
        "Memory should be shifted"
    );
    assert_eq!(
        cpu.p & FLAG_CARRY,
        FLAG_CARRY,
        "Carry should be set from bit 0"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "SRE ZP takes 5 cycles"
    );
}

#[test]
fn test_execute_sre_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SRE_ZPX, 0x40];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x02;
    cpu.bus.borrow_mut().write(0x0042, 0b11111110, false);
    cpu.a = 0xFF;

    cpu.execute();

    // 0b11111110 >> 1 = 0b01111111
    // 0xFF XOR 0b01111111 = 0b10000000
    assert_eq!(cpu.a, 0b10000000, "A should be XORed with shifted value");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

#[test]
fn test_execute_sre_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SRE_ABS, 0x34, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x1234, 0b10101010, false);
    cpu.a = 0b01010101;

    cpu.execute();

    // 0b10101010 >> 1 = 0b01010101
    // 0b01010101 XOR 0b01010101 = 0b00000000
    assert_eq!(cpu.a, 0b00000000, "A should be 0");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
}

#[test]
fn test_execute_sre_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SRE_ABSXW, 0x00, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x34;
    cpu.bus.borrow_mut().write(0x1234, 0xFF, false);
    cpu.a = 0x00;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // 0xFF >> 1 = 0x7F
    // 0x00 XOR 0x7F = 0x7F
    assert_eq!(cpu.a, 0x7F, "A should be 0x7F");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 7,
        "SRE ABSXW takes 7 cycles"
    );
}

#[test]
fn test_execute_sre_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SRE_ABSYW, 0x00, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x34;
    cpu.bus.borrow_mut().write(0x1234, 0b00000010, false);
    cpu.a = 0xFF;

    cpu.execute();

    // 0b00000010 >> 1 = 0b00000001
    // 0xFF XOR 0b00000001 = 0b11111110
    assert_eq!(cpu.a, 0b11111110, "A should be XORed with shifted value");
}

#[test]
fn test_execute_sre_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SRE_INDX, 0x40];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x02;
    cpu.bus.borrow_mut().write(0x0042, 0x34, false);
    cpu.bus.borrow_mut().write(0x0043, 0x12, false);
    cpu.bus.borrow_mut().write(0x1234, 0b11000000, false);
    cpu.a = 0b01100000;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // 0b11000000 >> 1 = 0b01100000
    // 0b01100000 XOR 0b01100000 = 0b00000000
    assert_eq!(cpu.a, 0b00000000, "A should be 0");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 8,
        "SRE INDX takes 8 cycles"
    );
}

#[test]
fn test_execute_sre_indirect_indexed() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SRE_INDYW, 0x40];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x03;
    cpu.bus.borrow_mut().write(0x0040, 0x31, false);
    cpu.bus.borrow_mut().write(0x0041, 0x12, false);
    cpu.bus.borrow_mut().write(0x1234, 0b10101010, false);
    cpu.a = 0xFF;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // 0b10101010 >> 1 = 0b01010101
    // 0xFF XOR 0b01010101 = 0b10101010
    assert_eq!(cpu.a, 0b10101010, "A should be XORed with shifted value");
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry should not be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 8,
        "SRE INDYW takes 8 cycles"
    );
}

// PHA tests (Push Accumulator)
#[test]
fn test_execute_pha() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![PHA];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x42;
    cpu.sp = 0xFF; // Full stack

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.sp, 0xFE, "SP should decrement");
    assert_eq!(
        cpu.bus.borrow_mut().read(0x01FF, false),
        0x42,
        "A should be pushed to stack"
    );
    assert_eq!(cpu.total_cycles, initial_cycles + 3, "PHA takes 3 cycles");
}

#[test]
fn test_execute_pha_stack_wrapping() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![PHA];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xAB;
    cpu.sp = 0x00; // At bottom of stack

    cpu.execute();

    assert_eq!(cpu.sp, 0xFF, "SP should wrap around");
    assert_eq!(
        cpu.bus.borrow_mut().read(0x0100, false),
        0xAB,
        "A should be pushed to correct location"
    );
}

// ASR tests (AND with immediate, then LSR - undocumented)
#[test]
fn test_execute_asr_immediate() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ASR_IMM, 0b11110000];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0b11111111;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // 0b11111111 AND 0b11110000 = 0b11110000
    // 0b11110000 >> 1 = 0b01111000
    assert_eq!(cpu.a, 0b01111000, "A should be ANDed then shifted");
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry should not be set");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(cpu.total_cycles, initial_cycles + 2, "ASR takes 2 cycles");
}

#[test]
fn test_execute_asr_with_carry() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ASR_IMM, 0b00001111];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0b10101111;

    cpu.execute();

    // 0b10101111 AND 0b00001111 = 0b00001111
    // 0b00001111 >> 1 = 0b00000111 (carry = 1)
    assert_eq!(cpu.a, 0b00000111, "A should be ANDed then shifted");
    assert_eq!(
        cpu.p & FLAG_CARRY,
        FLAG_CARRY,
        "Carry should be set from bit 0"
    );
}

#[test]
fn test_execute_asr_zero_result() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ASR_IMM, 0x00];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xFF;

    cpu.execute();

    // 0xFF AND 0x00 = 0x00
    // 0x00 >> 1 = 0x00
    assert_eq!(cpu.a, 0x00, "A should be 0");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry should not be set");
}

// JMP tests
#[test]
fn test_execute_jmp_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![JMP_ABS, 0x34, 0x12]; // JMP $1234
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.pc, 0x1234, "PC should jump to target address");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "JMP ABS takes 3 cycles"
    );
}

#[test]
fn test_execute_jmp_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![JMP_IND, 0x00, 0x12]; // JMP ($1200)
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Set up indirect address
    cpu.bus.borrow_mut().write(0x1200, 0x34, false); // Low byte
    cpu.bus.borrow_mut().write(0x1201, 0x56, false); // High byte

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.pc, 0x5634, "PC should jump to indirect address");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "JMP IND takes 5 cycles"
    );
}

#[test]
fn test_execute_jmp_indirect_page_boundary_bug() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![JMP_IND, 0xFF, 0x12]; // JMP ($12FF)
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Set up addresses at page boundary
    cpu.bus.borrow_mut().write(0x12FF, 0x34, false); // Low byte
    cpu.bus.borrow_mut().write(0x1200, 0x56, false); // High byte (wraps to same page)
    cpu.bus.borrow_mut().write(0x1300, 0x78, false); // This should NOT be read

    cpu.execute();

    // Due to 6502 bug, high byte read from 0x1200 instead of 0x1300
    assert_eq!(cpu.pc, 0x5634, "PC should use page boundary bug behavior");
}

// BVC Tests
#[test]
fn test_execute_bvc_not_taken_overflow_set() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BVC, 0x10]; // BVC +16
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    let start_pc = cpu.pc;
    cpu.p = FLAG_OVERFLOW; // Set overflow flag

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.pc, start_pc + 2, "Branch not taken, PC += 2");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "Branch not taken takes 2 cycles"
    );
}

#[test]
fn test_execute_bvc_taken_same_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BVC, 0x10]; // BVC +16
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    let start_pc = cpu.pc;
    cpu.p = 0; // Clear overflow flag

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.pc,
        start_pc.wrapping_add(2).wrapping_add(0x10),
        "Branch taken, PC += 2 + offset"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "Branch taken same page takes 3 cycles"
    );
}

#[test]
fn test_execute_bvc_taken_cross_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // Position BVC so that the branch target crosses a page boundary
    // Place BVC at 0x8090, after execution PC will be at 0x8092
    // Branch offset 0x70 (+112) will make PC = 0x8102 (crosses from 0x80 to 0x81)
    let mut program = vec![0xEA; 0x90]; // 144 NOPs
    program.push(BVC); // At offset 0x90
    program.push(0x70); // +112 bytes

    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Execute NOPs to get to the BVC instruction
    for _ in 0..0x90 {
        cpu.execute();
    }

    cpu.p = 0; // Clear overflow flag

    let start_pc = cpu.pc;
    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    let target_pc = start_pc.wrapping_add(2).wrapping_add(0x70);
    assert_eq!(cpu.pc, target_pc, "Branch to new page");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "Branch taken cross page takes 4 cycles"
    );
}

#[test]
fn test_execute_bvc_backward_branch() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BVC, 0xFE]; // BVC -2
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    let start_pc = cpu.pc;
    cpu.p = 0; // Clear overflow flag

    cpu.execute();

    let offset = -2_i16; // Negative offset
    let target_pc = start_pc.wrapping_add(2).wrapping_add_signed(offset);
    assert_eq!(cpu.pc, target_pc, "Branch backward");
}

// CLI Tests
#[test]
fn test_execute_cli_clears_interrupt_flag() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CLI];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p = FLAG_INTERRUPT; // Set interrupt flag

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.p & FLAG_INTERRUPT,
        0,
        "Interrupt flag should be cleared"
    );
    assert_eq!(cpu.total_cycles, initial_cycles + 2, "CLI takes 2 cycles");
}

#[test]
fn test_execute_cli_doesnt_affect_other_flags() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CLI];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p = FLAG_CARRY | FLAG_ZERO | FLAG_NEGATIVE | FLAG_INTERRUPT;

    cpu.execute();

    assert_eq!(
        cpu.p & FLAG_CARRY,
        FLAG_CARRY,
        "Carry flag should be preserved"
    );
    assert_eq!(
        cpu.p & FLAG_ZERO,
        FLAG_ZERO,
        "Zero flag should be preserved"
    );
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be preserved"
    );
    assert_eq!(
        cpu.p & FLAG_INTERRUPT,
        0,
        "Interrupt flag should be cleared"
    );
}

#[test]
fn test_execute_cli_delays_irq_for_one_instruction() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    // Minimal cartridge:
    // - reset vector -> $8000
    // - IRQ vector -> $9000
    // Program at $8000: CLI, NOP
    let mut prg_rom = vec![0; 0x4000];
    prg_rom[0x3FFC] = 0x00;
    prg_rom[0x3FFD] = 0x80;
    prg_rom[0x3FFE] = 0x00;
    prg_rom[0x3FFF] = 0x90;
    prg_rom[0x0000] = CLI;
    prg_rom[0x0001] = NOP;
    let chr_rom = vec![0; 0x2000];
    let cartridge = Cartridge::from_parts(prg_rom, chr_rom, NametableLayout::Horizontal);
    cpu.bus.borrow_mut().map_cartridge(cartridge);

    cpu.reset(true);

    // Start with interrupts disabled.
    cpu.p |= FLAG_INTERRUPT;
    cpu.set_irq_pending(true);

    // After CLI, the I flag is cleared, but IRQ must still be inhibited for exactly
    // one following instruction.
    cpu.execute();
    assert_eq!(cpu.p & FLAG_INTERRUPT, 0, "CLI should clear I");
    assert_ne!(
        cpu.pc, 0x9000,
        "IRQ must not be taken immediately after CLI"
    );

    // Execute one more instruction: IRQ should now be taken.
    cpu.execute();
    assert_eq!(cpu.pc, 0x9000, "IRQ should be taken after one instruction");
}

#[test]
fn test_execute_cli_irq_taken_after_one_following_instruction() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    // Minimal cartridge:
    // - reset vector -> $8000
    // - IRQ vector -> $9000
    // Program at $8000: CLI, NOP, NOP
    let mut prg_rom = vec![0; 0x4000];
    prg_rom[0x3FFC] = 0x00;
    prg_rom[0x3FFD] = 0x80;
    prg_rom[0x3FFE] = 0x00;
    prg_rom[0x3FFF] = 0x90;
    prg_rom[0x0000] = CLI;
    prg_rom[0x0001] = NOP;
    prg_rom[0x0002] = NOP;
    let chr_rom = vec![0; 0x2000];
    let cartridge = Cartridge::from_parts(prg_rom, chr_rom, NametableLayout::Horizontal);
    cpu.bus.borrow_mut().map_cartridge(cartridge);

    cpu.reset(true);

    // Start with interrupts disabled, and an asserted IRQ line.
    cpu.p |= FLAG_INTERRUPT;
    cpu.set_irq_pending(true);

    // Execute CLI: clears I, but IRQ must not be taken immediately.
    cpu.execute();
    assert_eq!(cpu.p & FLAG_INTERRUPT, 0, "CLI should clear I");
    assert_ne!(
        cpu.pc, 0x9000,
        "IRQ must not be taken immediately after CLI"
    );

    // Execute one instruction: IRQ should now be taken.
    cpu.execute();
    assert_eq!(
        cpu.pc, 0x9000,
        "IRQ should be taken after one following instruction"
    );
}

// RTS Tests
#[test]
fn test_execute_rts_basic() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![RTS];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Simulate JSR having pushed return address - 1
    let return_address = 0x1234_u16;
    cpu.push_word(return_address - 1);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.pc, return_address,
        "PC should be set to return address (popped value + 1)"
    );
    assert_eq!(cpu.total_cycles, initial_cycles + 6, "RTS takes 6 cycles");
}

#[test]
fn test_execute_rts_stack_pointer() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![RTS];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    let initial_sp = cpu.sp;
    cpu.push_word(0x5678);

    cpu.execute();

    assert_eq!(
        cpu.sp, initial_sp,
        "Stack pointer should be restored after RTS"
    );
}

#[test]
fn test_execute_rts_doesnt_affect_flags() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![RTS];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p = FLAG_CARRY | FLAG_ZERO | FLAG_NEGATIVE;
    cpu.push_word(0x1234);

    cpu.execute();

    assert_eq!(
        cpu.p & FLAG_CARRY,
        FLAG_CARRY,
        "Carry flag should be preserved"
    );
    assert_eq!(
        cpu.p & FLAG_ZERO,
        FLAG_ZERO,
        "Zero flag should be preserved"
    );
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be preserved"
    );
}

// ADC Tests
#[test]
fn test_execute_adc_immediate_no_carry() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_IMM, 0x10]; // ADC #$10
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x05;
    cpu.p = 0; // Clear all flags including carry

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x15, "A should be 0x05 + 0x10");
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry flag should be clear");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should be clear");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should be clear");
    assert_eq!(cpu.p & FLAG_OVERFLOW, 0, "Overflow flag should be clear");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "ADC IMM takes 2 cycles"
    );
}

#[test]
fn test_execute_adc_with_carry_in() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_IMM, 0x10]; // ADC #$10
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x05;
    cpu.p = FLAG_CARRY; // Set carry in

    cpu.execute();

    assert_eq!(cpu.a, 0x16, "A should be 0x05 + 0x10 + 1");
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry flag should be clear");
}

#[test]
fn test_execute_adc_with_carry_out() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_IMM, 0xFF]; // ADC #$FF
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x02;
    cpu.p = 0;

    cpu.execute();

    assert_eq!(cpu.a, 0x01, "A should wrap to 0x01");
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry flag should be set");
}

#[test]
fn test_execute_adc_zero_result() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_IMM, 0xFF]; // ADC #$FF
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x01;
    cpu.p = 0;

    cpu.execute();

    assert_eq!(cpu.a, 0x00, "A should be 0");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry flag should be set");
}

#[test]
fn test_execute_adc_negative_result() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_IMM, 0x80]; // ADC #$80
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x00;
    cpu.p = 0;

    cpu.execute();

    assert_eq!(cpu.a, 0x80, "A should be 0x80");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

#[test]
fn test_execute_adc_overflow_positive() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_IMM, 0x7F]; // ADC #$7F (127)
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x01; // 1
    cpu.p = 0;

    cpu.execute();

    // 1 + 127 = 128 = 0x80 (appears negative, overflow)
    assert_eq!(cpu.a, 0x80, "A should be 0x80");
    assert_eq!(
        cpu.p & FLAG_OVERFLOW,
        FLAG_OVERFLOW,
        "Overflow flag should be set"
    );
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

#[test]
fn test_execute_adc_overflow_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_IMM, 0x80]; // ADC #$80 (-128 in signed)
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x80; // -128 in signed
    cpu.p = 0;

    cpu.execute();

    // -128 + -128 = -256, but wraps to 0 with overflow
    assert_eq!(cpu.a, 0x00, "A should wrap to 0");
    assert_eq!(
        cpu.p & FLAG_OVERFLOW,
        FLAG_OVERFLOW,
        "Overflow flag should be set"
    );
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry flag should be set");
}

#[test]
fn test_execute_adc_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_ZP, 0x42]; // ADC $42
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0042, 0x33, false);
    cpu.a = 0x10;
    cpu.p = 0;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x43, "A should be 0x10 + 0x33");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "ADC ZP takes 3 cycles"
    );
}

#[test]
fn test_execute_adc_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_ZPX, 0x40]; // ADC $40,X
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x0045, 0x25, false);
    cpu.a = 0x10;
    cpu.p = 0;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x35, "A should be 0x10 + 0x25");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "ADC ZPX takes 4 cycles"
    );
}

#[test]
fn test_execute_adc_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_ABS, 0x00, 0x12]; // ADC $1200
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x1200, 0x44, false);
    cpu.a = 0x11;
    cpu.p = 0;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x55, "A should be 0x11 + 0x44");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "ADC ABS takes 4 cycles"
    );
}

#[test]
fn test_execute_adc_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_ABSX, 0x00, 0x12]; // ADC $1200,X
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x08;
    cpu.bus.borrow_mut().write(0x1208, 0x22, false);
    cpu.a = 0x10;
    cpu.p = 0;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x32, "A should be 0x10 + 0x22");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "ADC ABSX (no page cross) takes 4 cycles"
    );
}

#[test]
fn test_execute_adc_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_ABSY, 0x00, 0x12]; // ADC $1200,Y
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x03;
    cpu.bus.borrow_mut().write(0x1203, 0x15, false);
    cpu.a = 0x20;
    cpu.p = 0;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x35, "A should be 0x20 + 0x15");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "ADC ABSY (no page cross) takes 4 cycles"
    );
}

#[test]
fn test_execute_adc_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_INDX, 0x40]; // ADC ($40,X)
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x05;
    // Zero page address 0x45 contains pointer to 0x1234
    cpu.bus.borrow_mut().write(0x0045, 0x34, false);
    cpu.bus.borrow_mut().write(0x0046, 0x12, false);
    cpu.bus.borrow_mut().write(0x1234, 0x50, false);
    cpu.a = 0x10;
    cpu.p = 0;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x60, "A should be 0x10 + 0x50");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "ADC INDX takes 6 cycles"
    );
}

#[test]
fn test_execute_adc_indirect_indexed() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ADC_INDY, 0x40]; // ADC ($40),Y
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x08;
    // Zero page address 0x40 contains base address 0x1200
    cpu.bus.borrow_mut().write(0x0040, 0x00, false);
    cpu.bus.borrow_mut().write(0x0041, 0x12, false);
    cpu.bus.borrow_mut().write(0x1208, 0x33, false);
    cpu.a = 0x11;
    cpu.p = 0;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x44, "A should be 0x11 + 0x33");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "ADC INDY (no page cross) takes 5 cycles"
    );
}

// ROR Tests
#[test]
fn test_execute_ror_accumulator_no_carry() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROR_ACC]; // ROR A
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0b10110110;
    cpu.p = 0; // Clear carry

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0b01011011, "A should be rotated right");
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry flag should be clear");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should be clear");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "ROR ACC takes 2 cycles"
    );
}

#[test]
fn test_execute_ror_accumulator_with_carry() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROR_ACC]; // ROR A
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0b00110110;
    cpu.p = FLAG_CARRY; // Set carry

    cpu.execute();

    assert_eq!(cpu.a, 0b10011011, "A should be rotated right with carry");
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry flag should be clear");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

#[test]
fn test_execute_ror_accumulator_sets_carry() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROR_ACC]; // ROR A
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0b00110111; // Bit 0 is set
    cpu.p = 0;

    cpu.execute();

    assert_eq!(cpu.a, 0b00011011, "A should be rotated right");
    assert_eq!(
        cpu.p & FLAG_CARRY,
        FLAG_CARRY,
        "Carry flag should be set from bit 0"
    );
}

#[test]
fn test_execute_ror_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROR_ZP, 0x42]; // ROR $42
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0042, 0b11001100, false);
    cpu.p = 0;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0042, false),
        0b01100110,
        "Memory should be rotated right"
    );
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry flag should be clear");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "ROR ZP takes 5 cycles"
    );
}

#[test]
fn test_execute_ror_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROR_ZPX, 0x40]; // ROR $40,X
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x0045, 0b10101010, false);
    cpu.p = FLAG_CARRY;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0045, false),
        0b11010101,
        "Memory should be rotated right with carry in"
    );
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry flag should be clear");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "ROR ZPX takes 6 cycles"
    );
}

#[test]
fn test_execute_ror_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROR_ABS, 0x00, 0x12]; // ROR $1200
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x1200, 0b00110011, false);
    cpu.p = 0;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x1200, false),
        0b00011001,
        "Memory should be rotated right"
    );
    assert_eq!(
        cpu.p & FLAG_CARRY,
        FLAG_CARRY,
        "Carry flag should be set from bit 0"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "ROR ABS takes 6 cycles"
    );
}

#[test]
fn test_execute_ror_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ROR_ABSXW, 0x00, 0x12]; // ROR $1200,X
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x08;
    cpu.bus.borrow_mut().write(0x1208, 0b11110000, false);
    cpu.p = 0;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x1208, false),
        0b01111000,
        "Memory should be rotated right"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 7,
        "ROR ABSXW takes 7 cycles"
    );
}

// RRA Tests (undocumented - ROR then ADC)
#[test]
fn test_execute_rra_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![RRA_ZP, 0x42]; // RRA $42
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0042, 0b10000000, false);
    cpu.a = 0x10;
    cpu.p = 0;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // Memory: 0b10000000 rotated right = 0b01000000 (0x40)
    // A = 0x10 + 0x40 = 0x50
    assert_eq!(
        cpu.bus.borrow_mut().read(0x0042, false),
        0x40,
        "Memory should be rotated right"
    );
    assert_eq!(cpu.a, 0x50, "A should be updated with ADC");
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry flag should be clear");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "RRA ZP takes 5 cycles"
    );
}

#[test]
fn test_execute_rra_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![RRA_ZPX, 0x40]; // RRA $40,X
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x0045, 0b00000011, false); // Bit 0 is set
    cpu.a = 0x05;
    cpu.p = 0;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // Memory: 0b00000011 rotated right = 0b00000001, carry set
    // A = 0x05 + 0x01 + 1(carry from rotation) = 0x07
    assert_eq!(
        cpu.bus.borrow_mut().read(0x0045, false),
        0x01,
        "Memory should be rotated right"
    );
    assert_eq!(cpu.a, 0x07, "A should be updated with ADC including carry");
    assert_eq!(
        cpu.p & FLAG_CARRY,
        0,
        "Carry flag should be clear after ADC"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "RRA ZPX takes 6 cycles"
    );
}

#[test]
fn test_execute_rra_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![RRA_ABS, 0x00, 0x12]; // RRA $1200
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x1200, 0x02, false);
    cpu.a = 0xFF;
    cpu.p = 0;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // Memory: 0x02 rotated right = 0x01
    // A = 0xFF + 0x01 = 0x00 (with carry)
    assert_eq!(
        cpu.bus.borrow_mut().read(0x1200, false),
        0x01,
        "Memory should be rotated right"
    );
    assert_eq!(cpu.a, 0x00, "A should wrap with carry");
    assert_eq!(
        cpu.p & FLAG_CARRY,
        FLAG_CARRY,
        "Carry flag should be set from ADC"
    );
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "RRA ABS takes 6 cycles"
    );
}

#[test]
fn test_execute_rra_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![RRA_ABSXW, 0x00, 0x12]; // RRA $1200,X
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x10;
    cpu.bus.borrow_mut().write(0x1210, 0x20, false);
    cpu.a = 0x05;
    cpu.p = 0;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // Memory: 0x20 rotated right = 0x10
    // A = 0x05 + 0x10 = 0x15
    assert_eq!(
        cpu.bus.borrow_mut().read(0x1210, false),
        0x10,
        "Memory should be rotated right"
    );
    assert_eq!(cpu.a, 0x15, "A should be updated with ADC");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 7,
        "RRA ABSXW takes 7 cycles"
    );
}

#[test]
fn test_execute_rra_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![RRA_ABSYW, 0x00, 0x12]; // RRA $1200,Y
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x08;
    cpu.bus.borrow_mut().write(0x1208, 0x04, false);
    cpu.a = 0x01;
    cpu.p = 0;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // Memory: 0x04 rotated right = 0x02
    // A = 0x01 + 0x02 = 0x03
    assert_eq!(
        cpu.bus.borrow_mut().read(0x1208, false),
        0x02,
        "Memory should be rotated right"
    );
    assert_eq!(cpu.a, 0x03, "A should be updated with ADC");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 7,
        "RRA ABSYW takes 7 cycles"
    );
}

#[test]
fn test_execute_rra_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![RRA_INDX, 0x40]; // RRA ($40,X)
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x0045, 0x00, false);
    cpu.bus.borrow_mut().write(0x0046, 0x12, false);
    cpu.bus.borrow_mut().write(0x1200, 0x08, false);
    cpu.a = 0x01;
    cpu.p = 0;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // Memory: 0x08 rotated right = 0x04
    // A = 0x01 + 0x04 = 0x05
    assert_eq!(
        cpu.bus.borrow_mut().read(0x1200, false),
        0x04,
        "Memory should be rotated right"
    );
    assert_eq!(cpu.a, 0x05, "A should be updated with ADC");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 8,
        "RRA INDX takes 8 cycles"
    );
}

#[test]
fn test_execute_rra_indirect_indexed() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![RRA_INDYW, 0x40]; // RRA ($40),Y
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x08;
    cpu.bus.borrow_mut().write(0x0040, 0x00, false);
    cpu.bus.borrow_mut().write(0x0041, 0x12, false);
    cpu.bus.borrow_mut().write(0x1208, 0x10, false);
    cpu.a = 0x0F;
    cpu.p = 0;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // Memory: 0x10 rotated right = 0x08
    // A = 0x0F + 0x08 = 0x17
    assert_eq!(
        cpu.bus.borrow_mut().read(0x1208, false),
        0x08,
        "Memory should be rotated right"
    );
    assert_eq!(cpu.a, 0x17, "A should be updated with ADC");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 8,
        "RRA INDYW takes 8 cycles"
    );
}

// PLA Tests
#[test]
fn test_execute_pla_basic() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![PLA]; // PLA
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.push_byte(0x42);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x42, "A should be pulled from stack");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should be clear");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should be clear");
    assert_eq!(cpu.total_cycles, initial_cycles + 4, "PLA takes 4 cycles");
}

#[test]
fn test_execute_pla_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![PLA]; // PLA
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.push_byte(0x00);

    cpu.execute();

    assert_eq!(cpu.a, 0x00, "A should be 0");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should be clear");
}

#[test]
fn test_execute_pla_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![PLA]; // PLA
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.push_byte(0x80);

    cpu.execute();

    assert_eq!(cpu.a, 0x80, "A should be 0x80");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should be clear");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

#[test]
fn test_execute_pla_stack_pointer() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![PLA]; // PLA
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    let initial_sp = cpu.sp;
    cpu.push_byte(0x55);

    cpu.execute();

    assert_eq!(cpu.sp, initial_sp, "Stack pointer should be restored");
}

// ARR Tests (undocumented - AND then ROR)
#[test]
fn test_execute_arr_basic() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ARR_IMM, 0b11001100]; // ARR #$CC
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0b11110000;
    cpu.p = 0; // Clear carry

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // A = 0b11110000 AND 0b11001100 = 0b11000000
    // Then rotate right: 0b11000000 >> 1 = 0b01100000
    // ARR flags (2A03):
    // - C = bit 6 of result
    // - V = bit 6 XOR bit 5 of result
    assert_eq!(cpu.a, 0b01100000, "A should be ANDed then rotated");
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry should be bit 6");
    assert_eq!(cpu.p & FLAG_OVERFLOW, 0, "Overflow should be bit6^bit5");
    assert_eq!(cpu.total_cycles, initial_cycles + 2, "ARR takes 2 cycles");
}

#[test]
fn test_execute_arr_with_carry_in() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ARR_IMM, 0b11001100]; // ARR #$CC
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0b11110000;
    cpu.p = FLAG_CARRY; // Set carry

    cpu.execute();

    // A = 0b11110000 AND 0b11001100 = 0b11000000
    // Then rotate right with carry: 0b11000000 >> 1 | 0b10000000 = 0b11100000
    assert_eq!(
        cpu.a, 0b11100000,
        "A should be ANDed then rotated with carry"
    );
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry should be bit 6");
    assert_eq!(cpu.p & FLAG_OVERFLOW, 0, "Overflow should be bit6^bit5");
}

#[test]
fn test_execute_arr_sets_carry_and_overflow_from_bits_6_and_5() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ARR_IMM, 0xFF]; // ARR #$FF
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0b10000000;
    cpu.p = 0; // old carry = 0

    cpu.execute();

    // A = 0b10000000
    // ROR with carry=0 => 0b01000000
    // C = bit 6 => 1
    // V = bit6 ^ bit5 => 1 ^ 0 = 1
    assert_eq!(cpu.a, 0b01000000, "A should be rotated");
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry should be bit 6");
    assert_eq!(
        cpu.p & FLAG_OVERFLOW,
        FLAG_OVERFLOW,
        "Overflow should be bit6^bit5"
    );
}

// BVS Tests
#[test]
fn test_execute_bvs_not_taken_overflow_clear() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BVS, 0x10]; // BVS +16
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    let start_pc = cpu.pc;
    cpu.p = 0; // Clear overflow flag

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.pc, start_pc + 2, "Branch not taken, PC += 2");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "Branch not taken takes 2 cycles"
    );
}

#[test]
fn test_execute_bvs_taken_same_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BVS, 0x10]; // BVS +16
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    let start_pc = cpu.pc;
    cpu.p = FLAG_OVERFLOW; // Set overflow flag

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.pc,
        start_pc.wrapping_add(2).wrapping_add(0x10),
        "Branch taken, PC += 2 + offset"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "Branch taken same page takes 3 cycles"
    );
}

#[test]
fn test_execute_bvs_taken_cross_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // Position BVS so that the branch target crosses a page boundary
    let mut program = vec![0xEA; 0x90]; // 144 NOPs
    program.push(BVS);
    program.push(0x70); // +112 bytes

    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Execute NOPs to get to the BVS instruction
    for _ in 0..0x90 {
        cpu.execute();
    }

    cpu.p = FLAG_OVERFLOW; // Set overflow flag

    let start_pc = cpu.pc;
    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    let target_pc = start_pc.wrapping_add(2).wrapping_add(0x70);
    assert_eq!(cpu.pc, target_pc, "Branch to new page");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "Branch taken cross page takes 4 cycles"
    );
}

#[test]
fn test_execute_bvs_backward_branch() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BVS, 0xFE]; // BVS -2
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    let start_pc = cpu.pc;
    cpu.p = FLAG_OVERFLOW; // Set overflow flag

    cpu.execute();

    let offset = -2_i16;
    let target_pc = start_pc.wrapping_add(2).wrapping_add_signed(offset);
    assert_eq!(cpu.pc, target_pc, "Branch backward");
}

// SEI Tests
#[test]
fn test_execute_sei_sets_interrupt_flag() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SEI];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p = 0; // Clear interrupt flag

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.p & FLAG_INTERRUPT,
        FLAG_INTERRUPT,
        "Interrupt flag should be set"
    );
    assert_eq!(cpu.total_cycles, initial_cycles + 2, "SEI takes 2 cycles");
}

#[test]
fn test_execute_sei_doesnt_affect_other_flags() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SEI];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p = FLAG_CARRY | FLAG_ZERO | FLAG_NEGATIVE;

    cpu.execute();

    assert_eq!(
        cpu.p & FLAG_CARRY,
        FLAG_CARRY,
        "Carry flag should be preserved"
    );
    assert_eq!(
        cpu.p & FLAG_ZERO,
        FLAG_ZERO,
        "Zero flag should be preserved"
    );
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be preserved"
    );
    assert_eq!(
        cpu.p & FLAG_INTERRUPT,
        FLAG_INTERRUPT,
        "Interrupt flag should be set"
    );
}

// STA Tests
#[test]
fn test_execute_sta_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STA_ZP, 0x42]; // STA $42
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x55;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0042, false),
        0x55,
        "Memory should contain A"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "STA ZP takes 3 cycles"
    );
}

#[test]
fn test_execute_sta_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STA_ZPX, 0x40]; // STA $40,X
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xAA;
    cpu.x = 0x05;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0045, false),
        0xAA,
        "Memory should contain A"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "STA ZPX takes 4 cycles"
    );
}

#[test]
fn test_execute_sta_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STA_ABS, 0x00, 0x12]; // STA $1200
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x77;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x1200, false),
        0x77,
        "Memory should contain A"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "STA ABS takes 4 cycles"
    );
}

#[test]
fn test_execute_sta_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STA_ABSXW, 0x00, 0x12]; // STA $1200,X
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x88;
    cpu.x = 0x10;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x1210, false),
        0x88,
        "Memory should contain A"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "STA ABSXW takes 5 cycles"
    );
}

#[test]
fn test_execute_sta_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STA_ABSYW, 0x00, 0x12]; // STA $1200,Y
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x99;
    cpu.y = 0x08;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x1208, false),
        0x99,
        "Memory should contain A"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "STA ABSYW takes 5 cycles"
    );
}

#[test]
fn test_execute_sta_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STA_INDX, 0x40]; // STA ($40,X)
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xCC;
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x0045, 0x00, false);
    cpu.bus.borrow_mut().write(0x0046, 0x12, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x1200, false),
        0xCC,
        "Memory should contain A"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "STA INDX takes 6 cycles"
    );
}

#[test]
fn test_execute_sta_indirect_indexed() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STA_INDYW, 0x40]; // STA ($40),Y
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xDD;
    cpu.y = 0x08;
    cpu.bus.borrow_mut().write(0x0040, 0x00, false);
    cpu.bus.borrow_mut().write(0x0041, 0x12, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x1208, false),
        0xDD,
        "Memory should contain A"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "STA INDYW takes 6 cycles"
    );
}

// SAX Tests (undocumented - Store A AND X)
#[test]
fn test_execute_sax_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SAX_ZP, 0x42]; // SAX $42
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0b11110000;
    cpu.x = 0b11001100;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0042, false),
        0b11000000,
        "Memory should contain A AND X"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "SAX ZP takes 3 cycles"
    );
}

#[test]
fn test_execute_sax_zero_page_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SAX_ZPY, 0x40]; // SAX $40,Y
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xFF;
    cpu.x = 0x55;
    cpu.y = 0x05;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0045, false),
        0x55,
        "Memory should contain A AND X"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "SAX ZPY takes 4 cycles"
    );
}

#[test]
fn test_execute_sax_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SAX_ABS, 0x00, 0x12]; // SAX $1200
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0b10101010;
    cpu.x = 0b01010101;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x1200, false),
        0x00,
        "Memory should contain A AND X (0x00)"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "SAX ABS takes 4 cycles"
    );
}

#[test]
fn test_execute_sax_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SAX_INDX, 0x40]; // SAX ($40,X)
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xFF;
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x0045, 0x00, false);
    cpu.bus.borrow_mut().write(0x0046, 0x12, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x1200, false),
        0x05,
        "Memory should contain A AND X"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "SAX INDX takes 6 cycles"
    );
}

// STY Tests
#[test]
fn test_execute_sty_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STY_ZP, 0x42]; // STY $42
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x66;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0042, false),
        0x66,
        "Memory should contain Y"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "STY ZP takes 3 cycles"
    );
}

#[test]
fn test_execute_sty_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STY_ZPX, 0x40]; // STY $40,X
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x77;
    cpu.x = 0x05;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0045, false),
        0x77,
        "Memory should contain Y"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "STY ZPX takes 4 cycles"
    );
}

#[test]
fn test_execute_sty_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STY_ABS, 0x00, 0x12]; // STY $1200
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x88;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x1200, false),
        0x88,
        "Memory should contain Y"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "STY ABS takes 4 cycles"
    );
}

// STX Tests
#[test]
fn test_execute_stx_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STX_ZP, 0x42]; // STX $42
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x99;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0042, false),
        0x99,
        "Memory should contain X"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "STX ZP takes 3 cycles"
    );
}

#[test]
fn test_execute_stx_zero_page_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STX_ZPY, 0x40]; // STX $40,Y
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0xAA;
    cpu.y = 0x05;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0045, false),
        0xAA,
        "Memory should contain X"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "STX ZPY takes 4 cycles"
    );
}

#[test]
fn test_execute_stx_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STX_ABS, 0x00, 0x12]; // STX $1200
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0xBB;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x1200, false),
        0xBB,
        "Memory should contain X"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "STX ABS takes 4 cycles"
    );
}

// DEY Tests
#[test]
fn test_execute_dey_basic() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DEY]; // DEY
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x05;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.y, 0x04, "Y should be decremented");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should be clear");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should be clear");
    assert_eq!(cpu.total_cycles, initial_cycles + 2, "DEY takes 2 cycles");
}

#[test]
fn test_execute_dey_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DEY]; // DEY
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x01;

    cpu.execute();

    assert_eq!(cpu.y, 0x00, "Y should be 0");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should be clear");
}

#[test]
fn test_execute_dey_wrap() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DEY]; // DEY
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x00;

    cpu.execute();

    assert_eq!(cpu.y, 0xFF, "Y should wrap to 0xFF");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should be clear");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

// TXA Tests
#[test]
fn test_execute_txa_basic() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TXA]; // TXA
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x42;
    cpu.a = 0x00;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x42, "A should be set to X");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should be clear");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should be clear");
    assert_eq!(cpu.total_cycles, initial_cycles + 2, "TXA takes 2 cycles");
}

#[test]
fn test_execute_txa_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TXA]; // TXA
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x00;
    cpu.a = 0xFF;

    cpu.execute();

    assert_eq!(cpu.a, 0x00, "A should be 0");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
}

#[test]
fn test_execute_txa_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TXA]; // TXA
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x80;
    cpu.a = 0x00;

    cpu.execute();

    assert_eq!(cpu.a, 0x80, "A should be 0x80");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

// XAA/ANE Tests (undocumented - TXA then AND with immediate)
#[test]
fn test_execute_xaa_basic() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![XAA_IMM, 0b11110000]; // XAA #$F0
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0b11001100;
    cpu.a = 0xFF; // Should be overwritten

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // XAA: A = X AND immediate
    assert_eq!(cpu.a, 0b11000000, "A should be X AND immediate");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should be clear");
    assert_eq!(cpu.total_cycles, initial_cycles + 2, "XAA takes 2 cycles");
}

#[test]
fn test_execute_xaa_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![XAA_IMM, 0x00]; // XAA #$00
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0xFF;

    cpu.execute();

    assert_eq!(cpu.a, 0x00, "A should be 0");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
}

#[test]
fn test_execute_xaa_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![XAA_IMM, 0xFF]; // XAA #$FF
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x80;

    cpu.execute();

    assert_eq!(cpu.a, 0x80, "A should be 0x80");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

// BCC Tests
#[test]
fn test_execute_bcc_not_taken_carry_set() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BCC, 0x10]; // BCC +16
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    let start_pc = cpu.pc;
    cpu.p = FLAG_CARRY; // Set carry flag

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.pc, start_pc + 2, "Branch not taken, PC += 2");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "Branch not taken takes 2 cycles"
    );
}

#[test]
fn test_execute_bcc_taken_same_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BCC, 0x10]; // BCC +16
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    let start_pc = cpu.pc;
    cpu.p = 0; // Clear carry flag

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.pc,
        start_pc.wrapping_add(2).wrapping_add(0x10),
        "Branch taken, PC += 2 + offset"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "Branch taken same page takes 3 cycles"
    );
}

#[test]
fn test_execute_bcc_taken_cross_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // Position BCC so that the branch target crosses a page boundary
    let mut program = vec![0xEA; 0x90]; // 144 NOPs
    program.push(BCC);
    program.push(0x70); // +112 bytes

    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Execute NOPs to get to the BCC instruction
    for _ in 0..0x90 {
        cpu.execute();
    }

    cpu.p = 0; // Clear carry flag

    let start_pc = cpu.pc;
    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    let target_pc = start_pc.wrapping_add(2).wrapping_add(0x70);
    assert_eq!(cpu.pc, target_pc, "Branch to new page");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "Branch taken cross page takes 4 cycles"
    );
}

#[test]
fn test_execute_bcc_backward_branch() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BCC, 0xFE]; // BCC -2
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    let start_pc = cpu.pc;
    cpu.p = 0; // Clear carry flag

    cpu.execute();

    let offset = -2_i16;
    let target_pc = start_pc.wrapping_add(2).wrapping_add_signed(offset);
    assert_eq!(cpu.pc, target_pc, "Branch backward");
}

// XAS/SHAZ Tests (undocumented - Store A AND X AND (high byte of address + 1))
#[test]
fn test_execute_xas_basic() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![XAS_ABSY, 0x00, 0x12]; // XAS $1200,Y
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xFF;
    cpu.x = 0xFF;
    cpu.y = 0x00;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // XAS stores (A & X & (high_byte + 1))
    // High byte of address = 0x12, so (0x12 + 1) = 0x13
    // Result: 0xFF & 0xFF & 0x13 = 0x13
    assert_eq!(
        cpu.bus.borrow_mut().read(0x1200, false),
        0x13,
        "Memory should contain A & X & (H+1)"
    );
    assert_eq!(cpu.total_cycles, initial_cycles + 5, "XAS takes 5 cycles");
}

#[test]
fn test_execute_xas_with_offset() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![XAS_ABSY, 0xF0, 0x11]; // XAS $11F0,Y
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xFF;
    cpu.x = 0xFF;
    cpu.y = 0x08;

    cpu.execute();

    // XAS stores (A & X & (high_byte + 1))
    // Effective address = 0x11F0 + 0x08 = 0x11F8
    // High byte = 0x11, so (0x11 + 1) = 0x12
    // Result: 0xFF & 0xFF & 0x12 = 0x12
    assert_eq!(
        cpu.bus.borrow_mut().read(0x11F8, false),
        0x12,
        "Memory should contain A & X & (H+1)"
    );
}

#[test]
fn test_execute_xas_masking() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![XAS_ABSY, 0x00, 0x02]; // XAS $0200,Y (write to RAM at $0200)
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0b11110000;
    cpu.x = 0b11001100;
    cpu.y = 0x00;

    cpu.execute();

    // High byte = 0x02, so (0x02 + 1) = 0x03
    // Result: 0b11110000 & 0b11001100 & 0x03 = 0b11000000 & 0x03 = 0x00
    assert_eq!(
        cpu.bus.borrow_mut().read(0x0200, false),
        0x00,
        "Memory should contain masked value"
    );
}

// TYA Tests (Transfer Y to A)
#[test]
fn test_execute_tya_basic() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TYA]; // TYA
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x42;
    cpu.a = 0xFF; // Should be overwritten

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x42, "A should be set to Y");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should be clear");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should be clear");
    assert_eq!(cpu.total_cycles, initial_cycles + 2, "TYA takes 2 cycles");
}

#[test]
fn test_execute_tya_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TYA]; // TYA
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x00;

    cpu.execute();

    assert_eq!(cpu.a, 0x00, "A should be 0");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
}

#[test]
fn test_execute_tya_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TYA]; // TYA
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x80;

    cpu.execute();

    assert_eq!(cpu.a, 0x80, "A should be 0x80");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

// TXS Tests (Transfer X to S)
#[test]
fn test_execute_txs_basic() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TXS]; // TXS
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x42;
    cpu.sp = 0xFF; // Should be overwritten

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.sp, 0x42, "SP should be set to X");
    // TXS does not affect flags
    assert_eq!(cpu.total_cycles, initial_cycles + 2, "TXS takes 2 cycles");
}

#[test]
fn test_execute_txs_no_flags() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TXS]; // TXS
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x00;
    cpu.p = 0xFF; // Set all flags

    let initial_flags = cpu.p;
    cpu.execute();

    assert_eq!(cpu.sp, 0x00, "SP should be 0");
    assert_eq!(cpu.p, initial_flags, "Flags should not change");
}

// SHY/*SYA Tests (undocumented - Store Y AND (high byte of address + 1))
#[test]
fn test_execute_sya_basic() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SYA_ABSX, 0x00, 0x12]; // SYA $1200,X
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0xFF;
    cpu.x = 0x00;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // SYA stores Y & (high_byte + 1)
    // High byte of address = 0x12, so (0x12 + 1) = 0x13
    // Result: 0xFF & 0x13 = 0x13
    assert_eq!(
        cpu.bus.borrow_mut().read(0x1200, false),
        0x13,
        "Memory should contain Y & (H+1)"
    );
    assert_eq!(cpu.total_cycles, initial_cycles + 5, "SYA takes 5 cycles");
}

#[test]
fn test_execute_sya_masking() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SYA_ABSX, 0x00, 0x03]; // SYA $0300,X
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0b11110000;
    cpu.x = 0x00;

    cpu.execute();

    // High byte = 0x03, so (0x03 + 1) = 0x04
    // Result: 0b11110000 & 0x04 = 0x00
    assert_eq!(
        cpu.bus.borrow_mut().read(0x0300, false),
        0x00,
        "Memory should contain masked value"
    );
}

#[test]
fn test_execute_sya_page_crossing_uses_base_high_plus1_and_modifies_high_byte() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // Base $12FF, X=1 => effective $1300 (page crossed)
    let program = vec![SYA_ABSX, 0xFF, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x01;
    cpu.y = 0x0F;

    cpu.execute();

    // Value uses base high byte (0x12) + 1 => 0x13
    // Value = Y & 0x13 = 0x0F & 0x13 = 0x03
    // On page crossing, the high byte of the *target address* is ANDed with Y:
    // high(0x1300) & 0x0F = 0x13 & 0x0F = 0x03 => final addr 0x0300
    assert_eq!(
        cpu.bus.borrow_mut().read(0x0300, false),
        0x03,
        "SYA should use base high byte and apply page-crossing high-byte quirk"
    );
}

// SHX/*SXA Tests (undocumented - Store X AND (high byte of address + 1))
#[test]
fn test_execute_sxa_basic() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SXA_ABSY, 0x00, 0x12]; // SXA $1200,Y
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0xFF;
    cpu.y = 0x00;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // SXA stores X & (high_byte + 1)
    // High byte of address = 0x12, so (0x12 + 1) = 0x13
    // Result: 0xFF & 0x13 = 0x13
    assert_eq!(
        cpu.bus.borrow_mut().read(0x1200, false),
        0x13,
        "Memory should contain X & (H+1)"
    );
    assert_eq!(cpu.total_cycles, initial_cycles + 5, "SXA takes 5 cycles");
}

#[test]
fn test_execute_sxa_masking() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SXA_ABSY, 0x00, 0x03]; // SXA $0300,Y
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0b11110000;
    cpu.y = 0x00;

    cpu.execute();

    // High byte = 0x03, so (0x03 + 1) = 0x04
    // Result: 0b11110000 & 0x04 = 0x00
    assert_eq!(
        cpu.bus.borrow_mut().read(0x0300, false),
        0x00,
        "Memory should contain masked value"
    );
}

#[test]
fn test_execute_sxa_page_crossing_uses_base_high_plus1_and_modifies_high_byte() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // Base $12FF, Y=1 => effective $1300 (page crossed)
    let program = vec![SXA_ABSY, 0xFF, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x01;
    cpu.x = 0x0F;

    cpu.execute();

    // Value uses base high byte (0x12) + 1 => 0x13
    // Value = X & 0x13 = 0x0F & 0x13 = 0x03
    // On page crossing, the high byte of the *target address* is ANDed with X:
    // high(0x1300) & 0x0F = 0x13 & 0x0F = 0x03 => final addr 0x0300
    assert_eq!(
        cpu.bus.borrow_mut().read(0x0300, false),
        0x03,
        "SXA should use base high byte and apply page-crossing high-byte quirk"
    );
}

// SHAA/*AXA Tests (undocumented - Store A AND X AND (high byte + 1))
#[test]
fn test_execute_axa_indirect_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // Set up zero page pointer at 0x40 -> 0x1200
    let program = vec![AXA_INDY, 0x40]; // AXA ($40),Y
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0040, 0x00, false);
    cpu.bus.borrow_mut().write(0x0041, 0x12, false);

    cpu.a = 0xFF;
    cpu.x = 0xFF;
    cpu.y = 0x00;

    cpu.execute();

    // AXA stores A & X & (high_byte + 1)
    // High byte of address = 0x12, so (0x12 + 1) = 0x13
    // Result: 0xFF & 0xFF & 0x13 = 0x13
    assert_eq!(
        cpu.bus.borrow_mut().read(0x1200, false),
        0x13,
        "Memory should contain A & X & (H+1)"
    );
}

#[test]
fn test_execute_axa_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![AXA_ABSY, 0x00, 0x12]; // AXA $1200,Y
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xFF;
    cpu.x = 0xFF;
    cpu.y = 0x00;

    cpu.execute();

    // AXA stores A & X & (high_byte + 1)
    // High byte of address = 0x12, so (0x12 + 1) = 0x13
    // Result: 0xFF & 0xFF & 0x13 = 0x13
    assert_eq!(
        cpu.bus.borrow_mut().read(0x1200, false),
        0x13,
        "Memory should contain A & X & (H+1)"
    );
}

// LDY Tests (Load Y Register)
#[test]
fn test_execute_ldy_immediate() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDY_IMM, 0x42]; // LDY #$42
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.y, 0x42, "Y should be 0x42");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should be clear");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should be clear");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "LDY IMM takes 2 cycles"
    );
}

#[test]
fn test_execute_ldy_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDY_ZP, 0x42]; // LDY $42
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0042, 0x99, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.y, 0x99, "Y should be 0x99");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "LDY ZP takes 3 cycles"
    );
}

#[test]
fn test_execute_ldy_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDY_ZPX, 0x40]; // LDY $40,X
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x0045, 0xAA, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.y, 0xAA, "Y should be 0xAA");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "LDY ZPX takes 4 cycles"
    );
}

#[test]
fn test_execute_ldy_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDY_ABS, 0x00, 0x12]; // LDY $1200
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x1200, 0xBB, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.y, 0xBB, "Y should be 0xBB");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "LDY ABS takes 4 cycles"
    );
}

#[test]
fn test_execute_ldy_absolute_x_no_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDY_ABSX, 0x00, 0x12]; // LDY $1200,X
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x1205, 0xCC, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.y, 0xCC, "Y should be 0xCC");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "LDY ABSX no page cross takes 4 cycles"
    );
}

#[test]
fn test_execute_ldy_absolute_x_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDY_ABSX, 0xFF, 0x11]; // LDY $11FF,X
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x02; // Crosses page boundary
    cpu.bus.borrow_mut().write(0x1201, 0xDD, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.y, 0xDD, "Y should be 0xDD");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "LDY ABSX with page cross takes 5 cycles"
    );
}

#[test]
fn test_execute_ldy_zero_flag() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDY_IMM, 0x00]; // LDY #$00
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.execute();

    assert_eq!(cpu.y, 0x00, "Y should be 0");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
}

#[test]
fn test_execute_ldy_negative_flag() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDY_IMM, 0x80]; // LDY #$80
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.execute();

    assert_eq!(cpu.y, 0x80, "Y should be 0x80");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

// TAY tests
#[test]
fn test_execute_tay_positive() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TAY]; // TAY
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x42;
    cpu.y = 0x00;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.y, 0x42, "Y should equal A");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(cpu.total_cycles, initial_cycles + 2, "TAY takes 2 cycles");
}

#[test]
fn test_execute_tay_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TAY]; // TAY
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x00;
    cpu.y = 0xFF;

    cpu.execute();

    assert_eq!(cpu.y, 0x00, "Y should be 0");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
}

#[test]
fn test_execute_tay_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TAY]; // TAY
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x80;
    cpu.y = 0x00;

    cpu.execute();

    assert_eq!(cpu.y, 0x80, "Y should be 0x80");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

// TAX tests
#[test]
fn test_execute_tax_positive() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TAX]; // TAX
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x42;
    cpu.x = 0x00;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.x, 0x42, "X should equal A");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(cpu.total_cycles, initial_cycles + 2, "TAX takes 2 cycles");
}

#[test]
fn test_execute_tax_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TAX]; // TAX
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x00;
    cpu.x = 0xFF;

    cpu.execute();

    assert_eq!(cpu.x, 0x00, "X should be 0");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
}

#[test]
fn test_execute_tax_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TAX]; // TAX
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x80;
    cpu.x = 0x00;

    cpu.execute();

    assert_eq!(cpu.x, 0x80, "X should be 0x80");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

// LDA tests - all addressing modes
#[test]
fn test_execute_lda_immediate() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_IMM, 0x42];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x42, "A should be 0x42");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "LDA IMM takes 2 cycles"
    );
}

#[test]
fn test_execute_lda_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_ZP, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0010, 0x55, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x55, "A should be 0x55");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "LDA ZP takes 3 cycles"
    );
}

#[test]
fn test_execute_lda_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_ZPX, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x0015, 0x66, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x66, "A should be 0x66");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "LDA ZPX takes 4 cycles"
    );
}

#[test]
fn test_execute_lda_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_ABS, 0x00, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x1200, 0x77, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x77, "A should be 0x77");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "LDA ABS takes 4 cycles"
    );
}

#[test]
fn test_execute_lda_absolute_x_no_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_ABSX, 0x00, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x1205, 0x88, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x88, "A should be 0x88");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "LDA ABSX no page cross takes 4 cycles"
    );
}

#[test]
fn test_execute_lda_absolute_x_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_ABSX, 0xFF, 0x11];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x1204, 0x99, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x99, "A should be 0x99");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "LDA ABSX page cross takes 5 cycles"
    );
}

#[test]
fn test_execute_lda_absolute_y_no_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_ABSY, 0x00, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x05;
    cpu.bus.borrow_mut().write(0x1205, 0xAA, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0xAA, "A should be 0xAA");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "LDA ABSY no page cross takes 4 cycles"
    );
}

#[test]
fn test_execute_lda_absolute_y_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_ABSY, 0xFF, 0x11];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x05;
    cpu.bus.borrow_mut().write(0x1204, 0xBB, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0xBB, "A should be 0xBB");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "LDA ABSY page cross takes 5 cycles"
    );
}

#[test]
fn test_execute_lda_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_INDX, 0x20];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x04;
    cpu.bus.borrow_mut().write(0x0024, 0x00, false); // Low byte
    cpu.bus.borrow_mut().write(0x0025, 0x13, false); // High byte
    cpu.bus.borrow_mut().write(0x1300, 0xCC, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0xCC, "A should be 0xCC");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "LDA INDX takes 6 cycles"
    );
}

#[test]
fn test_execute_lda_indirect_indexed_no_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_INDY, 0x20];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x04;
    cpu.bus.borrow_mut().write(0x0020, 0x00, false); // Low byte
    cpu.bus.borrow_mut().write(0x0021, 0x13, false); // High byte
    cpu.bus.borrow_mut().write(0x1304, 0xDD, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0xDD, "A should be 0xDD");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "LDA INDY no page cross takes 5 cycles"
    );
}

#[test]
fn test_execute_lda_indirect_indexed_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_INDY, 0x20];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0xFF;
    cpu.bus.borrow_mut().write(0x0020, 0x10, false); // Low byte
    cpu.bus.borrow_mut().write(0x0021, 0x12, false); // High byte
    cpu.bus.borrow_mut().write(0x130F, 0xEE, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0xEE, "A should be 0xEE");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "LDA INDY page cross takes 6 cycles"
    );
}

#[test]
fn test_execute_lda_zero_flag() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_IMM, 0x00];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.execute();

    assert_eq!(cpu.a, 0x00, "A should be 0");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
}

#[test]
fn test_execute_lda_negative_flag() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDA_IMM, 0x80];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.execute();

    assert_eq!(cpu.a, 0x80, "A should be 0x80");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

// LDX tests - all addressing modes
#[test]
fn test_execute_ldx_immediate() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDX_IMM, 0x42];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.x, 0x42, "X should be 0x42");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "LDX IMM takes 2 cycles"
    );
}

#[test]
fn test_execute_ldx_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDX_ZP, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0010, 0x55, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.x, 0x55, "X should be 0x55");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "LDX ZP takes 3 cycles"
    );
}

#[test]
fn test_execute_ldx_zero_page_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDX_ZPY, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x05;
    cpu.bus.borrow_mut().write(0x0015, 0x66, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.x, 0x66, "X should be 0x66");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "LDX ZPY takes 4 cycles"
    );
}

#[test]
fn test_execute_ldx_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDX_ABS, 0x00, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x1200, 0x77, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.x, 0x77, "X should be 0x77");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "LDX ABS takes 4 cycles"
    );
}

#[test]
fn test_execute_ldx_absolute_y_no_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDX_ABSY, 0x00, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x05;
    cpu.bus.borrow_mut().write(0x1205, 0x88, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.x, 0x88, "X should be 0x88");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "LDX ABSY no page cross takes 4 cycles"
    );
}

#[test]
fn test_execute_ldx_absolute_y_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDX_ABSY, 0xFF, 0x11];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x05;
    cpu.bus.borrow_mut().write(0x1204, 0x99, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.x, 0x99, "X should be 0x99");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "LDX ABSY page cross takes 5 cycles"
    );
}

#[test]
fn test_execute_ldx_zero_flag() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDX_IMM, 0x00];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.execute();

    assert_eq!(cpu.x, 0x00, "X should be 0");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
}

#[test]
fn test_execute_ldx_negative_flag() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LDX_IMM, 0x80];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.execute();

    assert_eq!(cpu.x, 0x80, "X should be 0x80");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

// LAX tests - undocumented instruction, all addressing modes
#[test]
fn test_execute_lax_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LAX_ZP, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0010, 0x42, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x42, "A should be 0x42");
    assert_eq!(cpu.x, 0x42, "X should be 0x42");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "LAX ZP takes 3 cycles"
    );
}

#[test]
fn test_execute_lax_zero_page_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LAX_ZPY, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x05;
    cpu.bus.borrow_mut().write(0x0015, 0x55, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x55, "A should be 0x55");
    assert_eq!(cpu.x, 0x55, "X should be 0x55");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "LAX ZPY takes 4 cycles"
    );
}

#[test]
fn test_execute_lax_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LAX_ABS, 0x00, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x1200, 0x66, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x66, "A should be 0x66");
    assert_eq!(cpu.x, 0x66, "X should be 0x66");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "LAX ABS takes 4 cycles"
    );
}

#[test]
fn test_execute_lax_absolute_y_no_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LAX_ABSY, 0x00, 0x12];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x05;
    cpu.bus.borrow_mut().write(0x1205, 0x77, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x77, "A should be 0x77");
    assert_eq!(cpu.x, 0x77, "X should be 0x77");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "LAX ABSY no page cross takes 4 cycles"
    );
}

#[test]
fn test_execute_lax_absolute_y_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LAX_ABSY, 0xFF, 0x11];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x05;
    cpu.bus.borrow_mut().write(0x1204, 0x88, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x88, "A should be 0x88");
    assert_eq!(cpu.x, 0x88, "X should be 0x88");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "LAX ABSY page cross takes 5 cycles"
    );
}

#[test]
fn test_execute_lax_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LAX_INDX, 0x20];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x04;
    cpu.bus.borrow_mut().write(0x0024, 0x00, false); // Low byte
    cpu.bus.borrow_mut().write(0x0025, 0x13, false); // High byte
    cpu.bus.borrow_mut().write(0x1300, 0x99, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x99, "A should be 0x99");
    assert_eq!(cpu.x, 0x99, "X should be 0x99");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "LAX INDX takes 6 cycles"
    );
}

#[test]
fn test_execute_lax_indirect_indexed_no_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LAX_INDY, 0x20];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x04;
    cpu.bus.borrow_mut().write(0x0020, 0x00, false); // Low byte
    cpu.bus.borrow_mut().write(0x0021, 0x13, false); // High byte
    cpu.bus.borrow_mut().write(0x1304, 0xAA, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0xAA, "A should be 0xAA");
    assert_eq!(cpu.x, 0xAA, "X should be 0xAA");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "LAX INDY no page cross takes 5 cycles"
    );
}

#[test]
fn test_execute_lax_flags_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LAX_ZP, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0010, 0x00, false);

    cpu.execute();

    assert_eq!(cpu.a, 0x00, "A should be 0");
    assert_eq!(cpu.x, 0x00, "X should be 0");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
}

#[test]
fn test_execute_lax_flags_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LAX_ZP, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0010, 0x80, false);

    cpu.execute();

    assert_eq!(cpu.a, 0x80, "A should be 0x80");
    assert_eq!(cpu.x, 0x80, "X should be 0x80");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

// CLV tests
#[test]
fn test_execute_clv() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CLV];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p |= FLAG_OVERFLOW; // Set overflow flag

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.p & FLAG_OVERFLOW, 0, "Overflow flag should be cleared");
    assert_eq!(cpu.total_cycles, initial_cycles + 2, "CLV takes 2 cycles");
}

#[test]
fn test_execute_clv_already_clear() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CLV];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p &= !FLAG_OVERFLOW; // Clear overflow flag

    cpu.execute();

    assert_eq!(
        cpu.p & FLAG_OVERFLOW,
        0,
        "Overflow flag should remain clear"
    );
}

// TSX tests
#[test]
fn test_execute_tsx_positive() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TSX];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.sp = 0x42;
    cpu.x = 0x00;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.x, 0x42, "X should equal SP");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(cpu.total_cycles, initial_cycles + 2, "TSX takes 2 cycles");
}

#[test]
fn test_execute_tsx_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TSX];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.sp = 0x00;
    cpu.x = 0xFF;

    cpu.execute();

    assert_eq!(cpu.x, 0x00, "X should be 0");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
}

#[test]
fn test_execute_tsx_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![TSX];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.sp = 0x80;
    cpu.x = 0x00;

    cpu.execute();

    assert_eq!(cpu.x, 0x80, "X should be 0x80");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

// BCS tests
#[test]
fn test_execute_bcs_taken_no_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BCS, 0x05]; // Branch forward 5 bytes
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p |= FLAG_CARRY; // Set carry flag

    let pc_before = cpu.pc;
    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // PC advances 2 bytes (instruction), then branches forward 5 bytes
    assert_eq!(
        cpu.pc,
        pc_before + 2 + 0x05,
        "PC should advance past instruction then branch forward 5 bytes"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "BCS taken no page cross takes 3 cycles"
    );
}

#[test]
fn test_execute_bcs_taken_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // Position BCS so branch crosses page boundary
    // Place 128 NOPs, then BCS at 0x8080
    // After reading BCS + offset, PC = 0x8082
    // Branch forward by 0x7F: 0x8082 + 0x7F = 0x8101 (page cross from 0x80 to 0x81)
    let mut program = vec![0xEA; 128]; // 128 NOPs to position at 0x8080
    program.push(BCS); // At offset 0x80
    program.push(0x7F); // Branch forward 127 bytes

    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Execute NOPs to position PC at BCS instruction
    for _ in 0..128 {
        cpu.execute();
    }

    cpu.p |= FLAG_CARRY; // Set carry flag

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // Branch taken with page cross: 4 cycles
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "BCS taken with page cross takes 4 cycles"
    );
}

#[test]
fn test_execute_bcs_not_taken() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BCS, 0x05];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p &= !FLAG_CARRY; // Clear carry flag

    let pc_before = cpu.pc;
    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // PC advances 2 bytes past the instruction (branch not taken)
    assert_eq!(cpu.pc, pc_before + 2, "PC should advance past instruction");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "BCS not taken takes 2 cycles"
    );
}

#[test]
fn test_execute_bcs_backward() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BCS, 0xFE]; // Branch backward -2 bytes (signed)
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p |= FLAG_CARRY; // Set carry flag

    let pc_before = cpu.pc;
    cpu.execute();

    // PC advances 2 bytes past instruction, then adds signed offset (-2)
    // Result: pc_before + 2 + (-2) = pc_before
    assert_eq!(
        cpu.pc,
        pc_before.wrapping_add(2).wrapping_add((-2i8) as u16),
        "PC should branch backward 2 bytes"
    );
}

// ATX tests (*ATX undocumented instruction)
#[test]
fn test_execute_atx_positive() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ATX_IMM, 0x42];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xFF; // Set A to known value
    cpu.x = 0x00;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x42, "A should be 0x42");
    assert_eq!(cpu.x, 0x42, "X should be 0x42");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "ATX IMM takes 2 cycles"
    );
}

#[test]
fn test_execute_atx_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ATX_IMM, 0x00];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xFF;
    cpu.x = 0xFF;

    cpu.execute();

    assert_eq!(cpu.a, 0x00, "A should be 0");
    assert_eq!(cpu.x, 0x00, "X should be 0");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
}

#[test]
fn test_execute_atx_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ATX_IMM, 0x80];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xFF;
    cpu.x = 0x00;

    cpu.execute();

    assert_eq!(cpu.a, 0x80, "A should be 0x80");
    assert_eq!(cpu.x, 0x80, "X should be 0x80");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

// INY tests
#[test]
fn test_execute_iny_normal() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![INY];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x42;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.y, 0x43, "Y should be incremented to 0x43");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(cpu.total_cycles, initial_cycles + 2, "INY takes 2 cycles");
}

#[test]
fn test_execute_iny_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![INY];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0xFF;

    cpu.execute();

    assert_eq!(cpu.y, 0x00, "Y should wrap to 0x00");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
}

#[test]
fn test_execute_iny_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![INY];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x7F;

    cpu.execute();

    assert_eq!(cpu.y, 0x80, "Y should be 0x80");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

// CPY tests
#[test]
fn test_execute_cpy_immediate_equal() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CPY_IMM, 0x42];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x42;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry flag should be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "CPY IMM takes 2 cycles"
    );
}

#[test]
fn test_execute_cpy_immediate_greater() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CPY_IMM, 0x30];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x42;

    cpu.execute();

    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry flag should be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
}

#[test]
fn test_execute_cpy_immediate_less() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CPY_IMM, 0x50];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x42;

    cpu.execute();

    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry flag should not be set");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

#[test]
fn test_execute_cpy_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CPY_ZP, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x42;
    cpu.bus.borrow_mut().write(0x0010, 0x42, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "CPY ZP takes 3 cycles"
    );
}

#[test]
fn test_execute_cpy_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CPY_ABS, 0x00, 0x20];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.y = 0x42;
    cpu.bus.borrow_mut().write(0x2000, 0x42, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "CPY ABS takes 4 cycles"
    );
}

// CMP tests
#[test]
fn test_execute_cmp_immediate() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CMP_IMM, 0x42];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x42;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "CMP IMM takes 2 cycles"
    );
}

#[test]
fn test_execute_cmp_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CMP_ZP, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.bus.borrow_mut().write(0x0010, 0x40, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "CMP ZP takes 3 cycles"
    );
}

#[test]
fn test_execute_cmp_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CMP_ZPX, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x0015, 0x40, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "CMP ZPX takes 4 cycles"
    );
}

#[test]
fn test_execute_cmp_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CMP_ABS, 0x00, 0x20];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.bus.borrow_mut().write(0x2000, 0x40, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "CMP ABS takes 4 cycles"
    );
}

#[test]
fn test_execute_cmp_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CMP_ABSX, 0x00, 0x20];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.x = 0x10;
    cpu.bus.borrow_mut().write(0x2010, 0x40, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "CMP ABSX no page cross takes 4 cycles"
    );
}

#[test]
fn test_execute_cmp_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CMP_ABSY, 0x00, 0x20];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x2010, 0x40, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "CMP ABSY no page cross takes 4 cycles"
    );
}

#[test]
fn test_execute_cmp_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CMP_INDX, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x0015, 0x00, false);
    cpu.bus.borrow_mut().write(0x0016, 0x30, false);
    cpu.bus.borrow_mut().write(0x3000, 0x40, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "CMP INDX takes 6 cycles"
    );
}

#[test]
fn test_execute_cmp_indirect_indexed() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CMP_INDY, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x0010, 0x00, false);
    cpu.bus.borrow_mut().write(0x0011, 0x30, false);
    cpu.bus.borrow_mut().write(0x3010, 0x40, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "CMP INDY no page cross takes 5 cycles"
    );
}

// DCP tests (undocumented - decrement memory then compare with A)
#[test]
fn test_execute_dcp_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DCP_ZP, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x42;
    cpu.bus.borrow_mut().write(0x0010, 0x43, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0010, false),
        0x42,
        "Memory should be decremented to 0x42"
    );
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "DCP ZP takes 5 cycles"
    );
}

#[test]
fn test_execute_dcp_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DCP_ZPX, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x42;
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x0015, 0x43, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0015, false),
        0x42,
        "Memory should be decremented to 0x42"
    );
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "DCP ZPX takes 6 cycles"
    );
}

#[test]
fn test_execute_dcp_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DCP_ABS, 0x00, 0x20];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x42;
    cpu.bus.borrow_mut().write(0x2000, 0x43, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x2000, false),
        0x42,
        "Memory should be decremented to 0x42"
    );
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "DCP ABS takes 6 cycles"
    );
}

#[test]
fn test_execute_dcp_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DCP_ABSXW, 0x00, 0x20];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x42;
    cpu.x = 0x10;
    cpu.bus.borrow_mut().write(0x2010, 0x43, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x2010, false),
        0x42,
        "Memory should be decremented to 0x42"
    );
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 7,
        "DCP ABSXW takes 7 cycles"
    );
}

#[test]
fn test_execute_dcp_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DCP_ABSYW, 0x00, 0x20];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x42;
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x2010, 0x43, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x2010, false),
        0x42,
        "Memory should be decremented to 0x42"
    );
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 7,
        "DCP ABSYW takes 7 cycles"
    );
}

#[test]
fn test_execute_dcp_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DCP_INDX, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x42;
    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x0015, 0x00, false);
    cpu.bus.borrow_mut().write(0x0016, 0x30, false);
    cpu.bus.borrow_mut().write(0x3000, 0x43, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x3000, false),
        0x42,
        "Memory should be decremented to 0x42"
    );
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 8,
        "DCP INDX takes 8 cycles"
    );
}

#[test]
fn test_execute_dcp_indirect_indexed() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DCP_INDYW, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x42;
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x0010, 0x00, false);
    cpu.bus.borrow_mut().write(0x0011, 0x30, false);
    cpu.bus.borrow_mut().write(0x3010, 0x43, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x3010, false),
        0x42,
        "Memory should be decremented to 0x42"
    );
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 8,
        "DCP INDYW takes 8 cycles"
    );
}

// LAR tests (undocumented - AND memory with SP, transfer to A, X, and SP)
#[test]
fn test_execute_lar_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LAR_ABSY, 0x00, 0x20];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.sp = 0xFF;
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x2010, 0x42, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    let expected = 0x42;
    assert_eq!(cpu.a, expected, "A should be SP & memory");
    assert_eq!(cpu.x, expected, "X should be SP & memory");
    assert_eq!(cpu.sp, expected, "SP should be SP & memory");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "LAR ABSY no page cross takes 4 cycles"
    );
}

#[test]
fn test_execute_lar_absolute_y_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LAR_ABSY, 0x00, 0x20];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.sp = 0xFF;
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x2010, 0x00, false);

    cpu.execute();

    assert_eq!(cpu.a, 0x00, "A should be 0");
    assert_eq!(cpu.x, 0x00, "X should be 0");
    assert_eq!(cpu.sp, 0x00, "SP should be 0");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
}

#[test]
fn test_execute_lar_absolute_y_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![LAR_ABSY, 0x00, 0x20];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.sp = 0xFF;
    cpu.y = 0x10;
    cpu.bus.borrow_mut().write(0x2010, 0x80, false);

    cpu.execute();

    assert_eq!(cpu.a, 0x80, "A should be 0x80");
    assert_eq!(cpu.x, 0x80, "X should be 0x80");
    assert_eq!(cpu.sp, 0x80, "SP should be 0x80");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

// DEX tests
#[test]
fn test_execute_dex_normal() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DEX];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x42;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.x, 0x41, "X should be decremented to 0x41");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(cpu.total_cycles, initial_cycles + 2, "DEX takes 2 cycles");
}

#[test]
fn test_execute_dex_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DEX];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x01;

    cpu.execute();

    assert_eq!(cpu.x, 0x00, "X should be 0x00");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
}

#[test]
fn test_execute_dex_wrap() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DEX];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x00;

    cpu.execute();

    assert_eq!(cpu.x, 0xFF, "X should wrap to 0xFF");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

// CPX tests
#[test]
fn test_execute_cpx_immediate_equal() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CPX_IMM, 0x42];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x42;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry flag should be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "CPX IMM takes 2 cycles"
    );
}

#[test]
fn test_execute_cpx_immediate_greater() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CPX_IMM, 0x30];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x42;

    cpu.execute();

    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry flag should be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
}

#[test]
fn test_execute_cpx_immediate_less() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CPX_IMM, 0x50];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x42;

    cpu.execute();

    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry flag should not be set");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

#[test]
fn test_execute_cpx_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CPX_ZP, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x42;
    cpu.bus.borrow_mut().write(0x0010, 0x42, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "CPX ZP takes 3 cycles"
    );
}

#[test]
fn test_execute_cpx_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CPX_ABS, 0x00, 0x20];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x42;
    cpu.bus.borrow_mut().write(0x2000, 0x42, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "CPX ABS takes 4 cycles"
    );
}

// CLD tests
#[test]
fn test_execute_cld() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CLD];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p |= FLAG_DECIMAL;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.p & FLAG_DECIMAL, 0, "Decimal flag should be cleared");
    assert_eq!(cpu.total_cycles, initial_cycles + 2, "CLD takes 2 cycles");
}

#[test]
fn test_execute_cld_already_clear() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![CLD];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p &= !FLAG_DECIMAL;

    cpu.execute();

    assert_eq!(
        cpu.p & FLAG_DECIMAL,
        0,
        "Decimal flag should remain cleared"
    );
}

// BNE tests
#[test]
fn test_execute_bne_taken_no_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BNE, 0x05]; // Branch forward 5 bytes
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p &= !FLAG_ZERO; // Clear zero flag

    let pc_before = cpu.pc;
    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // PC advances 2 bytes (instruction), then branches forward 5 bytes
    assert_eq!(
        cpu.pc,
        pc_before + 2 + 0x05,
        "PC should advance past instruction then branch forward 5 bytes"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "BNE taken no page cross takes 3 cycles"
    );
}

#[test]
fn test_execute_bne_taken_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // Position BNE so branch crosses page boundary
    let mut program = vec![0xEA; 128]; // 128 NOPs to position at 0x8080
    program.push(BNE); // At offset 0x80
    program.push(0x7F); // Branch forward 127 bytes

    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Execute NOPs to position PC at BNE instruction
    for _ in 0..128 {
        cpu.execute();
    }

    cpu.p &= !FLAG_ZERO; // Clear zero flag

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // Branch taken with page cross: 4 cycles
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "BNE taken with page cross takes 4 cycles"
    );
}

#[test]
fn test_execute_bne_not_taken() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BNE, 0x05];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p |= FLAG_ZERO; // Set zero flag

    let pc_before = cpu.pc;
    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // PC advances 2 bytes past the instruction (branch not taken)
    assert_eq!(cpu.pc, pc_before + 2, "PC should advance past instruction");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "BNE not taken takes 2 cycles"
    );
}

#[test]
fn test_execute_bne_backward() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BNE, 0xFE]; // Branch backward -2 bytes (signed)
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p &= !FLAG_ZERO; // Clear zero flag

    let pc_before = cpu.pc;
    cpu.execute();

    // PC advances 2 bytes past instruction, then adds signed offset (-2)
    // Result: pc_before + 2 + (-2) = pc_before
    assert_eq!(
        cpu.pc,
        pc_before.wrapping_add(2).wrapping_add((-2i8) as u16),
        "PC should branch backward 2 bytes"
    );
}

// AXS tests (undocumented - AND X with A, then subtract immediate from result)
#[test]
fn test_execute_axs_immediate() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![AXS_IMM, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xFF;
    cpu.x = 0x50;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // AXS: X = (A & X) - immediate = (0xFF & 0x50) - 0x10 = 0x50 - 0x10 = 0x40
    assert_eq!(cpu.x, 0x40, "X should be 0x40");
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry flag should be set");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "AXS IMM takes 2 cycles"
    );
}

#[test]
fn test_execute_axs_immediate_borrow() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![AXS_IMM, 0x50];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xFF;
    cpu.x = 0x30;

    cpu.execute();

    // AXS: X = (A & X) - immediate = (0xFF & 0x30) - 0x50 = 0x30 - 0x50 = -0x20 = 0xE0
    assert_eq!(cpu.x, 0xE0, "X should wrap to 0xE0");
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry flag should be clear (borrow)");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

#[test]
fn test_execute_axs_immediate_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![AXS_IMM, 0x42];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0xFF;
    cpu.x = 0x42;

    cpu.execute();

    // AXS: X = (A & X) - immediate = (0xFF & 0x42) - 0x42 = 0x42 - 0x42 = 0x00
    assert_eq!(cpu.x, 0x00, "X should be 0x00");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry flag should be set");
}

// INX tests
#[test]
fn test_execute_inx_normal() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![INX];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x42;

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.x, 0x43, "X should be incremented to 0x43");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(cpu.total_cycles, initial_cycles + 2, "INX takes 2 cycles");
}

#[test]
fn test_execute_inx_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![INX];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0xFF;

    cpu.execute();

    assert_eq!(cpu.x, 0x00, "X should wrap to 0x00");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
}

#[test]
fn test_execute_inx_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![INX];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x7F;

    cpu.execute();

    assert_eq!(cpu.x, 0x80, "X should be 0x80");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

// BEQ tests
#[test]
fn test_execute_beq_taken_no_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BEQ, 0x05]; // Branch forward 5 bytes
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p |= FLAG_ZERO; // Set zero flag

    let pc_before = cpu.pc;
    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // PC advances 2 bytes (instruction), then branches forward 5 bytes
    assert_eq!(
        cpu.pc,
        pc_before + 2 + 0x05,
        "PC should advance past instruction then branch forward 5 bytes"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "BEQ taken no page cross takes 3 cycles"
    );
}

#[test]
fn test_execute_beq_taken_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // Position BEQ so branch crosses page boundary
    let mut program = vec![0xEA; 128]; // 128 NOPs to position at 0x8080
    program.push(BEQ); // At offset 0x80
    program.push(0x7F); // Branch forward 127 bytes

    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    // Execute NOPs to position PC at BEQ instruction
    for _ in 0..128 {
        cpu.execute();
    }

    cpu.p |= FLAG_ZERO; // Set zero flag

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // Branch taken with page cross: 4 cycles
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "BEQ taken with page cross takes 4 cycles"
    );
}

#[test]
fn test_execute_beq_not_taken() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BEQ, 0x05];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p &= !FLAG_ZERO; // Clear zero flag

    let pc_before = cpu.pc;
    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // PC advances 2 bytes past the instruction (branch not taken)
    assert_eq!(cpu.pc, pc_before + 2, "PC should advance past instruction");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "BEQ not taken takes 2 cycles"
    );
}

#[test]
fn test_execute_beq_backward() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![BEQ, 0xFE]; // Branch backward -2 bytes (signed)
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p |= FLAG_ZERO; // Set zero flag

    let pc_before = cpu.pc;
    cpu.execute();

    // PC advances 2 bytes past instruction, then adds signed offset (-2)
    assert_eq!(
        cpu.pc,
        pc_before.wrapping_add(2).wrapping_add((-2i8) as u16),
        "PC should branch backward 2 bytes"
    );
}

// SBC tests
#[test]
fn test_execute_sbc_immediate_no_borrow() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_IMM, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.p |= FLAG_CARRY; // Set carry (no borrow)

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // 0x50 - 0x10 - 0 = 0x40
    assert_eq!(cpu.a, 0x40, "A should be 0x40");
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry flag should be set");
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(cpu.p & FLAG_OVERFLOW, 0, "Overflow flag should not be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 2,
        "SBC IMM takes 2 cycles"
    );
}

#[test]
fn test_execute_sbc_immediate_with_borrow() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_IMM, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.p &= !FLAG_CARRY; // Clear carry (borrow)

    cpu.execute();

    // 0x50 - 0x10 - 1 = 0x3F
    assert_eq!(cpu.a, 0x3F, "A should be 0x3F");
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry flag should be set");
}

#[test]
fn test_execute_sbc_immediate_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_IMM, 0x42];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x42;
    cpu.p |= FLAG_CARRY; // Set carry (no borrow)

    cpu.execute();

    assert_eq!(cpu.a, 0x00, "A should be 0x00");
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_CARRY, FLAG_CARRY, "Carry flag should be set");
}

#[test]
fn test_execute_sbc_immediate_underflow() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_IMM, 0x50];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x30;
    cpu.p |= FLAG_CARRY; // Set carry (no borrow)

    cpu.execute();

    // 0x30 - 0x50 = 0xE0 (wraps)
    assert_eq!(cpu.a, 0xE0, "A should wrap to 0xE0");
    assert_eq!(cpu.p & FLAG_CARRY, 0, "Carry flag should be clear");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

#[test]
fn test_execute_sbc_immediate_overflow_positive() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_IMM, 0x80];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.p |= FLAG_CARRY; // Set carry (no borrow)

    cpu.execute();

    // 0x50 - 0x80 = 0xD0 (overflow: positive - negative = negative)
    assert_eq!(cpu.a, 0xD0, "A should be 0xD0");
    assert_eq!(
        cpu.p & FLAG_OVERFLOW,
        FLAG_OVERFLOW,
        "Overflow flag should be set"
    );
}

#[test]
fn test_execute_sbc_immediate_overflow_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_IMM, 0x01];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x80;
    cpu.p |= FLAG_CARRY; // Set carry (no borrow)

    cpu.execute();

    // 0x80 - 0x01 = 0x7F (overflow: negative - positive = positive)
    assert_eq!(cpu.a, 0x7F, "A should be 0x7F");
    assert_eq!(
        cpu.p & FLAG_OVERFLOW,
        FLAG_OVERFLOW,
        "Overflow flag should be set"
    );
}

#[test]
fn test_execute_sbc_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_ZP, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x0010, 0x10, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x40, "A should be 0x40");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 3,
        "SBC ZP takes 3 cycles"
    );
}

#[test]
fn test_execute_sbc_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_ZPX, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.x = 0x05;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x0015, 0x10, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x40, "A should be 0x40");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "SBC ZPX takes 4 cycles"
    );
}

#[test]
fn test_execute_sbc_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_ABS, 0x00, 0x20];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x2000, 0x10, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x40, "A should be 0x40");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "SBC ABS takes 4 cycles"
    );
}

#[test]
fn test_execute_sbc_absolute_x_no_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_ABSX, 0x00, 0x20];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.x = 0x05;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x2005, 0x10, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x40, "A should be 0x40");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "SBC ABSX no page cross takes 4 cycles"
    );
}

#[test]
fn test_execute_sbc_absolute_x_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_ABSX, 0xFF, 0x01];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.x = 0x05;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x0204, 0x10, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x40, "A should be 0x40");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "SBC ABSX with page cross takes 5 cycles"
    );
}

#[test]
fn test_execute_sbc_absolute_y_no_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_ABSY, 0x00, 0x20];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.y = 0x05;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x2005, 0x10, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x40, "A should be 0x40");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 4,
        "SBC ABSY no page cross takes 4 cycles"
    );
}

#[test]
fn test_execute_sbc_absolute_y_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_ABSY, 0xFF, 0x01];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.y = 0x05;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x0204, 0x10, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x40, "A should be 0x40");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "SBC ABSY with page cross takes 5 cycles"
    );
}

#[test]
fn test_execute_sbc_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_INDX, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.x = 0x05;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x0015, 0x00, false);
    cpu.bus.borrow_mut().write(0x0016, 0x20, false);
    cpu.bus.borrow_mut().write(0x2000, 0x10, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x40, "A should be 0x40");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "SBC INDX takes 6 cycles"
    );
}

#[test]
fn test_execute_sbc_indirect_indexed_no_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_INDY, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.y = 0x05;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x0010, 0x00, false);
    cpu.bus.borrow_mut().write(0x0011, 0x20, false);
    cpu.bus.borrow_mut().write(0x2005, 0x10, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x40, "A should be 0x40");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "SBC INDY no page cross takes 5 cycles"
    );
}

#[test]
fn test_execute_sbc_indirect_indexed_page_cross() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SBC_INDY, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.y = 0x05;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x0010, 0xFF, false);
    cpu.bus.borrow_mut().write(0x0011, 0x01, false);
    cpu.bus.borrow_mut().write(0x0204, 0x10, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(cpu.a, 0x40, "A should be 0x40");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "SBC INDY with page cross takes 6 cycles"
    );
}

// ISB tests (undocumented: INC then SBC)
#[test]
fn test_execute_isb_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ISB_ZP, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x0010, 0x0F, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    // Memory increments from 0x0F to 0x10, then 0x50 - 0x10 = 0x40
    assert_eq!(
        cpu.bus.borrow_mut().read(0x0010, false),
        0x10,
        "Memory should be incremented to 0x10"
    );
    assert_eq!(cpu.a, 0x40, "A should be 0x40");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "ISB ZP takes 5 cycles"
    );
}

#[test]
fn test_execute_isb_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ISB_ZPX, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.x = 0x05;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x0015, 0x0F, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0015, false),
        0x10,
        "Memory should be incremented to 0x10"
    );
    assert_eq!(cpu.a, 0x40, "A should be 0x40");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "ISB ZPX takes 6 cycles"
    );
}

#[test]
fn test_execute_isb_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ISB_ABS, 0x00, 0x20];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x2000, 0x0F, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x2000, false),
        0x10,
        "Memory should be incremented to 0x10"
    );
    assert_eq!(cpu.a, 0x40, "A should be 0x40");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "ISB ABS takes 6 cycles"
    );
}

#[test]
fn test_execute_isb_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ISB_ABSXW, 0x00, 0x20];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.x = 0x05;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x2005, 0x0F, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x2005, false),
        0x10,
        "Memory should be incremented to 0x10"
    );
    assert_eq!(cpu.a, 0x40, "A should be 0x40");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 7,
        "ISB ABSXW takes 7 cycles"
    );
}

#[test]
fn test_execute_isb_absolute_y() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ISB_ABSYW, 0x00, 0x20];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.y = 0x05;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x2005, 0x0F, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x2005, false),
        0x10,
        "Memory should be incremented to 0x10"
    );
    assert_eq!(cpu.a, 0x40, "A should be 0x40");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 7,
        "ISB ABSYW takes 7 cycles"
    );
}

#[test]
fn test_execute_isb_indexed_indirect() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ISB_INDX, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.x = 0x05;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x0015, 0x00, false);
    cpu.bus.borrow_mut().write(0x0016, 0x20, false);
    cpu.bus.borrow_mut().write(0x2000, 0x0F, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x2000, false),
        0x10,
        "Memory should be incremented to 0x10"
    );
    assert_eq!(cpu.a, 0x40, "A should be 0x40");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 8,
        "ISB INDX takes 8 cycles"
    );
}

#[test]
fn test_execute_isb_indirect_indexed() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![ISB_INDYW, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.a = 0x50;
    cpu.y = 0x05;
    cpu.p |= FLAG_CARRY;
    cpu.bus.borrow_mut().write(0x0010, 0x00, false);
    cpu.bus.borrow_mut().write(0x0011, 0x20, false);
    cpu.bus.borrow_mut().write(0x2005, 0x0F, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x2005, false),
        0x10,
        "Memory should be incremented to 0x10"
    );
    assert_eq!(cpu.a, 0x40, "A should be 0x40");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 8,
        "ISB INDYW takes 8 cycles"
    );
}

// SED tests
#[test]
fn test_execute_sed() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SED];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p &= !FLAG_DECIMAL; // Clear decimal flag

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.p & FLAG_DECIMAL,
        FLAG_DECIMAL,
        "Decimal flag should be set"
    );
    assert_eq!(cpu.total_cycles, initial_cycles + 2, "SED takes 2 cycles");
}

#[test]
fn test_execute_sed_already_set() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![SED];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.p |= FLAG_DECIMAL; // Set decimal flag

    cpu.execute();

    assert_eq!(
        cpu.p & FLAG_DECIMAL,
        FLAG_DECIMAL,
        "Decimal flag should remain set"
    );
}

// INC tests
#[test]
fn test_execute_inc_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![INC_ZP, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0010, 0x42, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0010, false),
        0x43,
        "Memory should be incremented to 0x43"
    );
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "INC ZP takes 5 cycles"
    );
}

#[test]
fn test_execute_inc_zero_page_wrap() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![INC_ZP, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0010, 0xFF, false);

    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0010, false),
        0x00,
        "Memory should wrap to 0x00"
    );
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
}

#[test]
fn test_execute_inc_zero_page_negative() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![INC_ZP, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0010, 0x7F, false);

    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0010, false),
        0x80,
        "Memory should be 0x80"
    );
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

#[test]
fn test_execute_inc_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![INC_ZPX, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x0015, 0x42, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0015, false),
        0x43,
        "Memory should be incremented to 0x43"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "INC ZPX takes 6 cycles"
    );
}

#[test]
fn test_execute_inc_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![INC_ABS, 0x00, 0x02];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0200, 0x42, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0200, false),
        0x43,
        "Memory should be incremented to 0x43"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "INC ABS takes 6 cycles"
    );
}

#[test]
fn test_execute_inc_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![INC_ABSXW, 0x00, 0x02];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x0205, 0x42, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0205, false),
        0x43,
        "Memory should be incremented to 0x43"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 7,
        "INC ABSXW takes 7 cycles"
    );
}

// DEC tests
#[test]
fn test_execute_dec_zero_page() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DEC_ZP, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0010, 0x42, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0010, false),
        0x41,
        "Memory should be decremented to 0x41"
    );
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 5,
        "DEC ZP takes 5 cycles"
    );
}

#[test]
fn test_execute_dec_zero_page_zero() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DEC_ZP, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0010, 0x01, false);

    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0010, false),
        0x00,
        "Memory should be 0x00"
    );
    assert_eq!(cpu.p & FLAG_ZERO, FLAG_ZERO, "Zero flag should be set");
    assert_eq!(cpu.p & FLAG_NEGATIVE, 0, "Negative flag should not be set");
}

#[test]
fn test_execute_dec_zero_page_wrap() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DEC_ZP, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0010, 0x00, false);

    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0010, false),
        0xFF,
        "Memory should wrap to 0xFF"
    );
    assert_eq!(cpu.p & FLAG_ZERO, 0, "Zero flag should not be set");
    assert_eq!(
        cpu.p & FLAG_NEGATIVE,
        FLAG_NEGATIVE,
        "Negative flag should be set"
    );
}

#[test]
fn test_execute_dec_zero_page_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DEC_ZPX, 0x10];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x0015, 0x42, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0015, false),
        0x41,
        "Memory should be decremented to 0x41"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "DEC ZPX takes 6 cycles"
    );
}

#[test]
fn test_execute_dec_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DEC_ABS, 0x00, 0x02];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.bus.borrow_mut().write(0x0200, 0x42, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0200, false),
        0x41,
        "Memory should be decremented to 0x41"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 6,
        "DEC ABS takes 6 cycles"
    );
}

#[test]
fn test_execute_dec_absolute_x() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![DEC_ABSXW, 0x00, 0x02];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.x = 0x05;
    cpu.bus.borrow_mut().write(0x0205, 0x42, false);

    let initial_cycles = cpu.total_cycles;
    cpu.execute();

    assert_eq!(
        cpu.bus.borrow_mut().read(0x0205, false),
        0x41,
        "Memory should be decremented to 0x41"
    );
    assert_eq!(
        cpu.total_cycles,
        initial_cycles + 7,
        "DEC ABSXW takes 7 cycles"
    );
}

// --- CPU write-address tracking ---

#[test]
fn test_last_cpu_write_addr_is_none_initially() {
    let (ppu, apu, memory) = create_test_memory();
    let cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    assert_eq!(cpu.last_cpu_write_addr(), None);
}

#[test]
fn test_last_cpu_write_addr_is_set_after_sta_absolute() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![STA_ABS, 0x34, 0x12]; // STA $1234
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0xAB;

    cpu.execute();

    assert_eq!(cpu.last_cpu_write_addr(), Some(0x1234));
}

#[test]
fn test_last_cpu_write_addr_is_none_after_lda_immediate() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    let program = vec![opcode::LDA_IMM, 0x42]; // LDA #$42 (read only)
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);

    cpu.execute();

    assert_eq!(cpu.last_cpu_write_addr(), None);
}

#[test]
fn test_last_cpu_write_addr_is_cleared_before_next_instruction() {
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(TimingMode::Ntsc, memory, ppu, apu);
    // STA $1234 followed by LDA #$42 (read only)
    let program = vec![STA_ABS, 0x34, 0x12, opcode::LDA_IMM, 0x42];
    fake_cartridge(&mut cpu, &program);
    cpu.reset(true);
    cpu.a = 0xAB;

    cpu.execute(); // STA — sets last_cpu_write_addr to $1234
    assert_eq!(cpu.last_cpu_write_addr(), Some(0x1234));

    cpu.execute(); // LDA — read only, should clear last_cpu_write_addr
    assert_eq!(cpu.last_cpu_write_addr(), None);
}

// --- Mapper capability flag caching tests (issue #2108 hot-path optimization) ---

fn make_ines_rom_for_mapper(mapper_id: u8) -> Vec<u8> {
    // Build a minimal valid iNES 1.0 header for the given mapper.
    // 2 × 16 KiB PRG banks, 1 × 8 KiB CHR bank.
    let flags6 = (mapper_id & 0x0F) << 4;
    let flags7 = mapper_id & 0xF0;
    let mut rom = vec![
        b'N', b'E', b'S', 0x1A, 2, 1, flags6, flags7, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    rom.extend(vec![0u8; 2 * 16 * 1024]); // 2 PRG banks
    rom.extend(vec![0u8; 8 * 1024]); // 1 CHR bank
    rom
}

#[test]
fn mapper_capability_flags_expansion_audio_false_for_nrom() {
    // NROM (mapper 0) has no expansion audio — flag must be false after insert.
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    let prg_rom = vec![0u8; 0x4000];
    let chr_rom = vec![0u8; 0x2000];
    let cart = Cartridge::from_parts(prg_rom, chr_rom, NametableLayout::Horizontal);
    memory.borrow_mut().map_cartridge(cart);
    cpu.update_mapper_capability_flags();

    assert!(
        !cpu.test_mapper_has_expansion_audio(),
        "NROM must not report expansion audio"
    );
}

#[test]
fn mapper_capability_flags_expansion_audio_true_for_vrc6() {
    // VRC6 (mapper 24) has expansion audio — flag must be true after insert.
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    let rom_data = make_ines_rom_for_mapper(24);
    let cart = Cartridge::load_from_file(&rom_data, "vrc6-test.nes", None).expect("Load VRC6 cart");
    memory.borrow_mut().map_cartridge(cart);
    cpu.update_mapper_capability_flags();

    assert!(
        cpu.test_mapper_has_expansion_audio(),
        "VRC6 must report expansion audio"
    );
}

#[test]
fn mapper_capability_flags_irq_false_for_nrom() {
    // NROM (mapper 0) has no IRQ — flag must be false after insert.
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    let prg_rom = vec![0u8; 0x4000];
    let chr_rom = vec![0u8; 0x2000];
    let cart = Cartridge::from_parts(prg_rom, chr_rom, NametableLayout::Horizontal);
    memory.borrow_mut().map_cartridge(cart);
    cpu.update_mapper_capability_flags();

    assert!(
        !cpu.test_mapper_has_irq(),
        "NROM must not report IRQ capability"
    );
}

#[test]
fn mapper_capability_flags_irq_true_for_mmc3() {
    // MMC3 (mapper 4) has IRQ — flag must be true after insert.
    let (ppu, apu, memory) = create_test_memory();
    let mut cpu = Cpu::new(
        TimingMode::Ntsc,
        Rc::clone(&memory),
        Rc::clone(&ppu),
        Rc::clone(&apu),
    );

    let rom_data = make_ines_rom_for_mapper(4);
    let cart = Cartridge::load_from_file(&rom_data, "mmc3-test.nes", None).expect("Load MMC3 cart");
    memory.borrow_mut().map_cartridge(cart);
    cpu.update_mapper_capability_flags();

    assert!(cpu.test_mapper_has_irq(), "MMC3 must report IRQ capability");
}
