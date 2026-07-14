//! SNES integration tests.
//!
//! Tests will be added as subsystems are implemented.

mod blargg_apu_tests;
mod byuu_test_oam_tests;
mod dsp_audio_golden_tests;
mod gilyon_cpu_tests;
mod gilyon_spc_tests;
mod hblank_dma_vram_tests;
mod neser_obj_tests;
mod peterlemon_cpu_tests;
mod peterlemon_ppu_bg_tests;
mod peterlemon_spc_tests;
mod processor_tests_65816;
mod processor_tests_spc700;
mod rom_runner;
mod sa1_absindx_tests;
mod sa1_boot_tests;
mod sa1_bwram_tests;
mod sa1_iram_tests;
mod sa1_irq_tests;
mod undisbeliever_ppu_bg_tests;
mod undisbeliever_ppu_obj_tests;
mod undisbeliever_tests;
