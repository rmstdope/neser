//! SNES integration tests.
//!
//! Tests will be added as subsystems are implemented.

mod blargg_apu_tests;
mod byuu_test_oam_tests;
mod ddribin_hdrv_tests;
mod dsp_audio_golden_tests;
mod fixture_rom;
mod gilyon_cpu_tests;
mod gilyon_spc_tests;
mod hblank_dma_vram_tests;
mod input_mouse_tests;
mod input_standard_controller_tests;
mod jonasquinn_math_tests;
mod kungfufurby_irq_tests;
mod kungfufurby_nmi_tests;
mod neser_color_math_tests;
mod neser_mode7_tests;
mod neser_obj_tests;
mod neser_opt_tests;
mod peterlemon_cpu_tests;
mod peterlemon_ppu_advanced_tests;
mod peterlemon_ppu_bg_tests;
mod peterlemon_spc_tests;
mod processor_tests_65816;
mod processor_tests_spc700;
mod rom_runner;
mod rom_screen_crc_helpers;
mod sa1_absindx_tests;
mod sa1_boot_tests;
mod sa1_bwram_tests;
mod sa1_iram_tests;
mod sa1_irq_tests;
mod sour_dma_irq_tests;
mod undisbeliever_ppu_bg_tests;
mod undisbeliever_ppu_mode7_tests;
mod undisbeliever_ppu_obj_tests;
mod undisbeliever_ppu_window_tests;
mod undisbeliever_tests;
