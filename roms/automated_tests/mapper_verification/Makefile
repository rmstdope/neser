# Makefile — Mapper Verification Test ROM Suite
#
# Builds combined ROMs (all tests per mapper) in bin/ and
# individual test ROMs in bin/rom_singles/.
# Requires ca65 and ld65 from the cc65 suite.
#
# Usage:
#   make          — Build all ROMs (combined + singles)
#   make clean    — Remove all built ROMs and object files

# Toolchain
CA65 := ca65
LD65 := ld65

# Directories
COMMON_DIR  := common
TESTS_DIR   := tests
CONFIGS_DIR := configs
DEFS_DIR    := defs
BIN_DIR     := bin
SINGLES_DIR := bin/rom_singles
OBJ_DIR     := bin/obj

# Common source files that all ROMs link against
COMMON_SRCS := $(COMMON_DIR)/shell.s $(COMMON_DIR)/nes_init.s $(COMMON_DIR)/console.s
COMMON_INCS := $(COMMON_DIR)/nes.inc $(COMMON_DIR)/nes20_header.inc $(COMMON_DIR)/test_macros.inc
COMMON_DATA := $(COMMON_DIR)/ascii.chr

# ============================================================
# Single ROM target list (individual test aspects)
# ============================================================

SINGLES := \
	$(SINGLES_DIR)/m000.0_prg_ram.nes \
	$(SINGLES_DIR)/m001.0_prg_banking.nes \
	$(SINGLES_DIR)/m001.0_chr_banking.nes \
	$(SINGLES_DIR)/m001.0_nametable.nes \
	$(SINGLES_DIR)/m001.0_prg_ram.nes \
	$(SINGLES_DIR)/m001.5_prg_banking.nes \
	$(SINGLES_DIR)/m002.0_prg_banking.nes \
	$(SINGLES_DIR)/m002.0_bus_conflicts.nes \
	$(SINGLES_DIR)/m002.2_prg_banking.nes \
	$(SINGLES_DIR)/m003.0_chr_banking.nes \
	$(SINGLES_DIR)/m003.0_bus_conflicts.nes \
	$(SINGLES_DIR)/m003.1_chr_banking.nes \
	$(SINGLES_DIR)/m004.0_prg_banking.nes \
	$(SINGLES_DIR)/m004.0_chr_banking.nes \
	$(SINGLES_DIR)/m004.0_nametable.nes \
	$(SINGLES_DIR)/m004.0_irq.nes \
	$(SINGLES_DIR)/m004.0_prg_ram.nes \
	$(SINGLES_DIR)/m004.0_write_protect.nes \
	$(SINGLES_DIR)/m004.1_irq.nes \
	$(SINGLES_DIR)/m004.1_write_protect.nes \
	$(SINGLES_DIR)/m005.0_prg_banking.nes \
	$(SINGLES_DIR)/m005.0_chr_banking.nes \
	$(SINGLES_DIR)/m005.0_nametable.nes \
	$(SINGLES_DIR)/m005.0_irq.nes \
	$(SINGLES_DIR)/m005.0_prg_ram.nes \
	$(SINGLES_DIR)/m005.0_multiplier.nes \
	$(SINGLES_DIR)/m005.0_write_protect.nes \
	$(SINGLES_DIR)/m005.0_exram.nes \
	$(SINGLES_DIR)/m006.0_prg_banking.nes \
	$(SINGLES_DIR)/m006.0_chr_banking.nes \
	$(SINGLES_DIR)/m006.0_nametable.nes \
	$(SINGLES_DIR)/m006.0_irq.nes \
	$(SINGLES_DIR)/m006.0_prg_ram.nes \
	$(BIN_DIR)/m006.0_modes.nes \
	$(SINGLES_DIR)/m007.0_prg_banking.nes \
	$(SINGLES_DIR)/m007.0_nametable.nes \
	$(SINGLES_DIR)/m007.1_prg_banking.nes \
	$(SINGLES_DIR)/m007.1_nametable.nes \
	$(SINGLES_DIR)/m008.0_prg_banking.nes \
	$(SINGLES_DIR)/m008.0_chr_banking.nes \
	$(SINGLES_DIR)/m008.0_nametable.nes \
	$(SINGLES_DIR)/m008.0_irq.nes \
	$(SINGLES_DIR)/m008.0_prg_ram.nes \
	$(SINGLES_DIR)/m009.0_prg_banking.nes \
	$(SINGLES_DIR)/m009.0_chr_latch.nes \
	$(SINGLES_DIR)/m009.0_nametable.nes \
	$(SINGLES_DIR)/m010.0_prg_banking.nes \
	$(SINGLES_DIR)/m010.0_chr_latch.nes \
	$(SINGLES_DIR)/m010.0_nametable.nes \
	$(SINGLES_DIR)/m010.0_prg_ram.nes \
	$(SINGLES_DIR)/m011.0_prg_banking.nes \
	$(SINGLES_DIR)/m011.0_chr_banking.nes \
	$(SINGLES_DIR)/m011.1_prg_banking.nes \
	$(SINGLES_DIR)/m011.1_chr_banking.nes \
	$(SINGLES_DIR)/m012.0_prg_banking.nes \
	$(SINGLES_DIR)/m012.0_chr_banking.nes \
	$(SINGLES_DIR)/m012.0_nametable.nes \
	$(SINGLES_DIR)/m012.0_irq.nes \
	$(SINGLES_DIR)/m012.0_prg_ram.nes \
	$(SINGLES_DIR)/m012.0_write_protect.nes \
	$(SINGLES_DIR)/m013.0_chr_ram_banking.nes \
	$(SINGLES_DIR)/m014.0_prg_banking.nes \
	$(SINGLES_DIR)/m014.0_chr_banking.nes \
	$(SINGLES_DIR)/m014.0_nametable.nes \
	$(SINGLES_DIR)/m014.0_irq.nes \
	$(SINGLES_DIR)/m014.0_prg_ram.nes \
	$(SINGLES_DIR)/m014.0_write_protect.nes \
	$(SINGLES_DIR)/m014.0_vrc2.nes \
	$(SINGLES_DIR)/m015.0_prg_banking.nes \
	$(SINGLES_DIR)/m015.0_nametable.nes \
	$(SINGLES_DIR)/m015.0_mode0.nes \
	$(SINGLES_DIR)/m015.0_mode2.nes \
	$(SINGLES_DIR)/m015.0_mode3.nes \
	$(SINGLES_DIR)/m019.0_prg_banking.nes \
	$(SINGLES_DIR)/m019.0_chr_banking.nes \
	$(SINGLES_DIR)/m019.0_nametable.nes \
	$(SINGLES_DIR)/m019.0_irq.nes \
	$(SINGLES_DIR)/m019.0_nt_from_chr.nes \
	$(SINGLES_DIR)/m019.2_prg_banking.nes \
	$(SINGLES_DIR)/m019.2_chr_banking.nes \
	$(SINGLES_DIR)/m019.2_nametable.nes \
	$(SINGLES_DIR)/m019.2_irq.nes \
	$(SINGLES_DIR)/m021.1_prg_banking.nes \
	$(SINGLES_DIR)/m021.1_chr_banking.nes \
	$(SINGLES_DIR)/m021.1_nametable.nes \
	$(SINGLES_DIR)/m021.1_irq.nes \
	$(SINGLES_DIR)/m021.2_prg_banking.nes \
	$(SINGLES_DIR)/m021.2_chr_banking.nes \
	$(SINGLES_DIR)/m021.2_nametable.nes \
	$(SINGLES_DIR)/m021.2_irq.nes \
	$(SINGLES_DIR)/m023.1_prg_banking.nes \
	$(SINGLES_DIR)/m023.1_chr_banking.nes \
	$(SINGLES_DIR)/m023.1_nametable.nes \
	$(SINGLES_DIR)/m023.1_irq.nes \
	$(SINGLES_DIR)/m023.2_prg_banking.nes \
	$(SINGLES_DIR)/m023.2_chr_banking.nes \
	$(SINGLES_DIR)/m023.2_nametable.nes \
	$(SINGLES_DIR)/m023.2_irq.nes \
	$(SINGLES_DIR)/m023.3_prg_banking.nes \
	$(SINGLES_DIR)/m023.3_chr_banking.nes \
	$(SINGLES_DIR)/m023.3_nametable.nes \
	$(SINGLES_DIR)/m025.1_prg_banking.nes \
	$(SINGLES_DIR)/m025.1_chr_banking.nes \
	$(SINGLES_DIR)/m025.1_nametable.nes \
	$(SINGLES_DIR)/m025.1_irq.nes \
	$(SINGLES_DIR)/m025.2_prg_banking.nes \
	$(SINGLES_DIR)/m025.2_chr_banking.nes \
	$(SINGLES_DIR)/m025.2_nametable.nes \
	$(SINGLES_DIR)/m025.2_irq.nes \
	$(SINGLES_DIR)/m025.3_prg_banking.nes \
	$(SINGLES_DIR)/m025.3_chr_banking.nes \
	$(SINGLES_DIR)/m025.3_nametable.nes \
	$(SINGLES_DIR)/m016.4_prg_banking.nes \
	$(SINGLES_DIR)/m016.4_chr_banking.nes \
	$(SINGLES_DIR)/m016.4_nametable.nes \
	$(SINGLES_DIR)/m016.4_irq.nes \
	$(SINGLES_DIR)/m016.5_prg_banking.nes \
	$(SINGLES_DIR)/m016.5_chr_banking.nes \
	$(SINGLES_DIR)/m016.5_nametable.nes \
	$(SINGLES_DIR)/m016.5_irq.nes \
	$(SINGLES_DIR)/m018.0_prg_banking.nes \
	$(SINGLES_DIR)/m018.0_chr_banking.nes \
	$(SINGLES_DIR)/m018.0_nametable.nes \
	$(SINGLES_DIR)/m018.0_irq.nes \
	$(SINGLES_DIR)/m018.0_prg_ram.nes \
	$(SINGLES_DIR)/m022.0_prg_banking.nes \
	$(SINGLES_DIR)/m022.0_chr_banking.nes \
	$(SINGLES_DIR)/m022.0_nametable.nes \
	$(SINGLES_DIR)/m024.0_prg_banking.nes \
	$(SINGLES_DIR)/m024.0_chr_banking.nes \
	$(SINGLES_DIR)/m024.0_nametable.nes \
	$(SINGLES_DIR)/m024.0_irq.nes \
	$(SINGLES_DIR)/m026.0_prg_banking.nes \
	$(SINGLES_DIR)/m026.0_chr_banking.nes \
	$(SINGLES_DIR)/m026.0_nametable.nes \
	$(SINGLES_DIR)/m026.0_irq.nes \
	$(SINGLES_DIR)/m028.0_prg_banking.nes \
	$(SINGLES_DIR)/m028.0_nametable.nes \
	$(SINGLES_DIR)/m029.0_prg_banking.nes \
	$(SINGLES_DIR)/m029.0_chr_ram_banking.nes \
	$(SINGLES_DIR)/m030.0_prg_banking.nes \
	$(SINGLES_DIR)/m030.0_chr_ram_banking.nes \
	$(SINGLES_DIR)/m030.2_prg_banking.nes \
	$(SINGLES_DIR)/m030.2_chr_ram_banking.nes \
	$(SINGLES_DIR)/m030.2_nametable.nes \
	$(SINGLES_DIR)/m031.0_prg_banking.nes \
	$(SINGLES_DIR)/m032.0_prg_banking.nes \
	$(SINGLES_DIR)/m032.0_chr_banking.nes \
	$(SINGLES_DIR)/m032.0_nametable.nes \
	$(SINGLES_DIR)/m032.0_prg_ram.nes \
	$(SINGLES_DIR)/m032.0_prg_mode.nes \
	$(SINGLES_DIR)/m032.1_prg_banking.nes \
	$(SINGLES_DIR)/m032.1_chr_banking.nes \
	$(SINGLES_DIR)/m032.1_nametable.nes \
	$(SINGLES_DIR)/m032.1_prg_ram.nes \
	$(SINGLES_DIR)/m033.0_prg_banking.nes \
	$(SINGLES_DIR)/m033.0_chr_banking.nes \
	$(SINGLES_DIR)/m033.0_nametable.nes \
	$(SINGLES_DIR)/m033.0_prg_ram.nes \
	$(SINGLES_DIR)/m034.0_prg_banking.nes \
	$(SINGLES_DIR)/m034.0_chr_banking.nes \
	$(SINGLES_DIR)/m034.0_prg_ram.nes \
	$(SINGLES_DIR)/m034.1_prg_banking.nes \
	$(SINGLES_DIR)/m034.1_chr_banking.nes \
	$(SINGLES_DIR)/m034.1_prg_ram.nes \
	$(SINGLES_DIR)/m034.2_prg_banking.nes \
	$(SINGLES_DIR)/m037.0_block_select.nes \
	$(SINGLES_DIR)/m038.0_prg_banking.nes \
	$(SINGLES_DIR)/m038.0_chr_banking.nes \
	$(SINGLES_DIR)/m038.0_prg_ram.nes \
	$(SINGLES_DIR)/m044.0_prg_banking.nes \
	$(SINGLES_DIR)/m044.0_chr_banking.nes \
	$(SINGLES_DIR)/m044.0_nametable.nes \
	$(SINGLES_DIR)/m044.0_irq.nes \
	$(SINGLES_DIR)/m044.0_block_select.nes \
	$(SINGLES_DIR)/m048.0_prg_banking.nes \
	$(SINGLES_DIR)/m048.0_chr_banking.nes \
	$(SINGLES_DIR)/m048.0_nametable.nes \
	$(SINGLES_DIR)/m048.0_irq.nes \
	$(SINGLES_DIR)/m048.0_prg_ram.nes \
	$(SINGLES_DIR)/m035.0_prg_banking.nes \
	$(SINGLES_DIR)/m035.0_chr_banking.nes \
	$(SINGLES_DIR)/m035.0_nametable.nes \
	$(SINGLES_DIR)/m035.0_prg_ram.nes \
	$(SINGLES_DIR)/m035.0_multiplier.nes \
	$(SINGLES_DIR)/m039.0_prg_banking.nes \
	$(SINGLES_DIR)/m040.0_prg_banking.nes \
	$(SINGLES_DIR)/m041.0_prg_banking.nes \
	$(SINGLES_DIR)/m042.0_prg_banking.nes \
	$(SINGLES_DIR)/m042.0_chr_banking.nes \
	$(SINGLES_DIR)/m042.0_nametable.nes \
	$(SINGLES_DIR)/m043.0_prg_banking.nes \
	$(SINGLES_DIR)/m045.0_block_select.nes \
	$(SINGLES_DIR)/m046.0_prg_banking.nes \
	$(SINGLES_DIR)/m046.0_chr_banking.nes \
	$(SINGLES_DIR)/m047.0_block_select.nes \
	$(SINGLES_DIR)/m064.0_prg_banking.nes \
	$(SINGLES_DIR)/m064.0_chr_banking.nes \
	$(SINGLES_DIR)/m064.0_nametable.nes \
	$(SINGLES_DIR)/m064.0_irq.nes \
	$(SINGLES_DIR)/m065.0_prg_banking.nes \
	$(SINGLES_DIR)/m065.0_chr_banking.nes \
	$(SINGLES_DIR)/m065.0_nametable.nes \
	$(SINGLES_DIR)/m065.0_irq.nes \
	$(SINGLES_DIR)/m067.0_prg_banking.nes \
	$(SINGLES_DIR)/m067.0_chr_banking.nes \
	$(SINGLES_DIR)/m067.0_nametable.nes \
	$(SINGLES_DIR)/m067.0_irq.nes

# ============================================================
# Combined ROM target list (all tests per mapper/submapper)
# ============================================================

COMBINED := \
	$(BIN_DIR)/m000.0.nes \
	$(BIN_DIR)/m001.0.nes \
	$(BIN_DIR)/m001.5.nes \
	$(BIN_DIR)/m002.0.nes \
	$(BIN_DIR)/m002.2.nes \
	$(BIN_DIR)/m003.0.nes \
	$(BIN_DIR)/m003.1.nes \
	$(BIN_DIR)/m004.0.nes \
	$(BIN_DIR)/m004.1.nes \
	$(BIN_DIR)/m005.0.nes \
	$(BIN_DIR)/m006.0.nes \
	$(BIN_DIR)/m007.0.nes \
	$(BIN_DIR)/m007.1.nes \
	$(BIN_DIR)/m008.0.nes \
	$(BIN_DIR)/m009.0.nes \
	$(BIN_DIR)/m010.0.nes \
	$(BIN_DIR)/m011.0.nes \
	$(BIN_DIR)/m011.1.nes \
	$(BIN_DIR)/m012.0.nes \
	$(BIN_DIR)/m013.0.nes \
	$(BIN_DIR)/m014.0.nes \
	$(BIN_DIR)/m015.0.nes \
	$(BIN_DIR)/m015.0_modes.nes \
	$(BIN_DIR)/m019.0.nes \
	$(BIN_DIR)/m019.2.nes \
	$(BIN_DIR)/m021.1.nes \
	$(BIN_DIR)/m021.2.nes \
	$(BIN_DIR)/m023.1.nes \
	$(BIN_DIR)/m023.2.nes \
	$(BIN_DIR)/m023.3.nes \
	$(BIN_DIR)/m025.1.nes \
	$(BIN_DIR)/m025.2.nes \
	$(BIN_DIR)/m025.3.nes \
	$(BIN_DIR)/m016.4.nes \
	$(BIN_DIR)/m016.5.nes \
	$(BIN_DIR)/m018.0.nes \
	$(BIN_DIR)/m022.0.nes \
	$(BIN_DIR)/m024.0.nes \
	$(BIN_DIR)/m026.0.nes \
	$(BIN_DIR)/m028.0.nes \
	$(BIN_DIR)/m029.0.nes \
	$(BIN_DIR)/m030.0.nes \
	$(BIN_DIR)/m030.2.nes \
	$(BIN_DIR)/m031.0.nes \
	$(BIN_DIR)/m032.0.nes \
	$(BIN_DIR)/m032.1.nes \
	$(BIN_DIR)/m033.0.nes \
	$(BIN_DIR)/m034.0.nes \
	$(BIN_DIR)/m034.1.nes \
	$(BIN_DIR)/m034.2.nes \
	$(BIN_DIR)/m037.0.nes \
	$(BIN_DIR)/m038.0.nes \
	$(BIN_DIR)/m040.0.nes \
	$(BIN_DIR)/m044.0.nes \
	$(BIN_DIR)/m048.0.nes \
	$(BIN_DIR)/m035.0.nes \
	$(BIN_DIR)/m039.0.nes \
	$(BIN_DIR)/m041.0.nes \
	$(BIN_DIR)/m042.0.nes \
	$(BIN_DIR)/m043.0.nes \
	$(BIN_DIR)/m045.0.nes \
	$(BIN_DIR)/m046.0.nes \
	$(BIN_DIR)/m047.0.nes \
	$(BIN_DIR)/m064.0.nes \
	$(BIN_DIR)/m065.0.nes \
	$(BIN_DIR)/m067.0.nes

.PHONY: all clean check-tools singles combined

all: check-tools $(COMBINED) $(SINGLES)
	@echo "Built $(words $(COMBINED)) combined + $(words $(SINGLES)) single test ROMs."

singles: check-tools $(SINGLES)
	@echo "Built $(words $(SINGLES)) single test ROMs."

combined: check-tools $(COMBINED)
	@echo "Built $(words $(COMBINED)) combined test ROMs."

clean:
	rm -rf $(BIN_DIR)/*.nes $(SINGLES_DIR) $(OBJ_DIR)

check-tools:
	@which $(CA65) > /dev/null 2>&1 || (echo "Error: ca65 not found. Install cc65: https://cc65.github.io/" && exit 1)
	@which $(LD65) > /dev/null 2>&1 || (echo "Error: ld65 not found. Install cc65: https://cc65.github.io/" && exit 1)

$(OBJ_DIR):
	@mkdir -p $(OBJ_DIR)

$(SINGLES_DIR):
	@mkdir -p $(SINGLES_DIR)

# ============================================================
# Single ROM build rule
# $(call ROM_RULE, target, aspect, mapper_id)
# ============================================================

define ROM_RULE
$(1): $(TESTS_DIR)/test_$(2).s $(CONFIGS_DIR)/$(3).cfg $(DEFS_DIR)/$(3)_defs.inc \
      $(COMMON_SRCS) $(COMMON_INCS) $(COMMON_DATA) | $(OBJ_DIR) $(SINGLES_DIR)
	@echo "  ASM  $(notdir $(1))"
	@echo '.include "$(3)_defs.inc"' > $(OBJ_DIR)/mapper_config.inc
	$(CA65) -o $(OBJ_DIR)/test_$(2)_$(3).o $(TESTS_DIR)/test_$(2).s \
		-I $(COMMON_DIR) -I $(DEFS_DIR) -I $(OBJ_DIR) --bin-include-dir $(COMMON_DIR)
	$(CA65) -o $(OBJ_DIR)/shell_$(3).o $(COMMON_DIR)/shell.s \
		-I $(COMMON_DIR) -I $(DEFS_DIR) -I $(OBJ_DIR)
	$(CA65) -o $(OBJ_DIR)/nes_init_$(3).o $(COMMON_DIR)/nes_init.s \
		-I $(COMMON_DIR)
	$(CA65) -o $(OBJ_DIR)/console_$(3).o $(COMMON_DIR)/console.s \
		-I $(COMMON_DIR) -I $(DEFS_DIR) -I $(OBJ_DIR) --bin-include-dir $(COMMON_DIR)
	$(LD65) -o $$@ \
		$(OBJ_DIR)/test_$(2)_$(3).o \
		$(OBJ_DIR)/shell_$(3).o \
		$(OBJ_DIR)/nes_init_$(3).o \
		$(OBJ_DIR)/console_$(3).o \
		-C $(CONFIGS_DIR)/$(3).cfg
	@echo "  OK   $(notdir $(1))"
endef

# ============================================================
# Combined ROM build rule
# $(call COMBINED_RULE, target, mapper_id, aspect1 aspect2 ...)
# ============================================================

# Convert aspect name to -D flag (e.g., prg_banking -> -DHAS_TEST_PRG_BANKING=1)
aspect_to_flag = $(patsubst %,-DHAS_TEST_$(shell echo $(1) | tr 'a-z' 'A-Z')=1,$(1))

define COMBINED_RULE
$(1): $(TESTS_DIR)/test_combined.s $(foreach a,$(3),$(TESTS_DIR)/test_$(a).s) \
      $(CONFIGS_DIR)/$(2).cfg $(DEFS_DIR)/$(2)_defs.inc \
      $(COMMON_SRCS) $(COMMON_INCS) $(COMMON_DATA) | $(OBJ_DIR)
	@echo "  ASM  $(notdir $(1)) [combined]"
	@echo '.include "$(2)_defs.inc"' > $(OBJ_DIR)/mapper_config.inc
	$(foreach a,$(3),$(CA65) -o $(OBJ_DIR)/test_$(a)_$(2)_c.o $(TESTS_DIR)/test_$(a).s \
		-I $(COMMON_DIR) -I $(DEFS_DIR) -I $(OBJ_DIR) --bin-include-dir $(COMMON_DIR) -DCOMBINED=1 && ) true
	$(CA65) -o $(OBJ_DIR)/test_combined_$(2).o $(TESTS_DIR)/test_combined.s \
		-I $(COMMON_DIR) -I $(DEFS_DIR) -I $(OBJ_DIR) --bin-include-dir $(COMMON_DIR) \
		$(foreach a,$(3),$(call aspect_to_flag,$(a)))
	$(CA65) -o $(OBJ_DIR)/shell_$(2).o $(COMMON_DIR)/shell.s \
		-I $(COMMON_DIR) -I $(DEFS_DIR) -I $(OBJ_DIR)
	$(CA65) -o $(OBJ_DIR)/nes_init_$(2).o $(COMMON_DIR)/nes_init.s \
		-I $(COMMON_DIR)
	$(CA65) -o $(OBJ_DIR)/console_$(2).o $(COMMON_DIR)/console.s \
		-I $(COMMON_DIR) -I $(DEFS_DIR) -I $(OBJ_DIR) --bin-include-dir $(COMMON_DIR)
	$(LD65) -o $$@ \
		$(OBJ_DIR)/test_combined_$(2).o \
		$(foreach a,$(3),$(OBJ_DIR)/test_$(a)_$(2)_c.o) \
		$(OBJ_DIR)/shell_$(2).o \
		$(OBJ_DIR)/nes_init_$(2).o \
		$(OBJ_DIR)/console_$(2).o \
		-C $(CONFIGS_DIR)/$(2).cfg
	@echo "  OK   $(notdir $(1))"
endef

# ============================================================
# Single ROM targets
# $(call ROM_RULE, target, aspect, mapper_id)
# ============================================================

# Mapper 0
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m000.0_prg_ram.nes,prg_ram,m000.0))

# Mapper 1, Submapper 0
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m001.0_prg_banking.nes,prg_banking,m001.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m001.0_chr_banking.nes,chr_banking,m001.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m001.0_nametable.nes,nametable,m001.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m001.0_prg_ram.nes,prg_ram,m001.0))

# Mapper 1, Submapper 5 (Fixed PRG)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m001.5_prg_banking.nes,prg_banking,m001.5))

# Mapper 2, Submapper 0 (Bus Conflicts)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m002.0_prg_banking.nes,prg_banking,m002.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m002.0_bus_conflicts.nes,bus_conflicts,m002.0))

# Mapper 2, Submapper 2 (No Bus Conflicts)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m002.2_prg_banking.nes,prg_banking,m002.2))

# Mapper 3, Submapper 0 (Bus Conflicts)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m003.0_chr_banking.nes,chr_banking,m003.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m003.0_bus_conflicts.nes,bus_conflicts,m003.0))

# Mapper 3, Submapper 1 (No Bus Conflicts)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m003.1_chr_banking.nes,chr_banking,m003.1))

# Mapper 4, Submapper 0 (Sharp IRQ)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m004.0_prg_banking.nes,prg_banking,m004.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m004.0_chr_banking.nes,chr_banking,m004.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m004.0_nametable.nes,nametable,m004.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m004.0_irq.nes,irq,m004.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m004.0_prg_ram.nes,prg_ram,m004.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m004.0_write_protect.nes,write_protect,m004.0))

# Mapper 4, Submapper 1 (NEC IRQ)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m004.1_irq.nes,irq,m004.1))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m004.1_write_protect.nes,write_protect,m004.1))

# Mapper 5, Submapper 0 (MMC5)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m005.0_prg_banking.nes,prg_banking,m005.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m005.0_chr_banking.nes,chr_banking,m005.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m005.0_nametable.nes,nametable,m005.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m005.0_irq.nes,irq,m005.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m005.0_prg_ram.nes,prg_ram,m005.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m005.0_multiplier.nes,multiplier,m005.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m005.0_write_protect.nes,write_protect,m005.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m005.0_exram.nes,exram,m005.0))

# Mapper 6, Submapper 0 (Front Fareast Magic Card, mode 1 UN1ROM+CHRSW)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m006.0_prg_banking.nes,prg_banking,m006.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m006.0_chr_banking.nes,chr_banking,m006.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m006.0_nametable.nes,nametable,m006.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m006.0_irq.nes,irq,m006.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m006.0_prg_ram.nes,prg_ram,m006.0))

# Mapper 6, Submapper 0 — Mode tests (GNROM, 2M PRG, write-enable, WRAM banking)
$(eval $(call ROM_RULE,$(BIN_DIR)/m006.0_modes.nes,modes,m006.0_modes))

# Mapper 7, Submapper 0 (Bus Conflicts)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m007.0_prg_banking.nes,prg_banking,m007.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m007.0_nametable.nes,nametable,m007.0))

# Mapper 7, Submapper 1 (No Bus Conflicts)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m007.1_prg_banking.nes,prg_banking,m007.1))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m007.1_nametable.nes,nametable,m007.1))

# Mapper 8, Submapper 0 (SMC GNROM mode 4)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m008.0_prg_banking.nes,prg_banking,m008.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m008.0_chr_banking.nes,chr_banking,m008.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m008.0_nametable.nes,nametable,m008.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m008.0_irq.nes,irq,m008.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m008.0_prg_ram.nes,prg_ram,m008.0))

# Mapper 9, Submapper 0 (MMC2)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m009.0_prg_banking.nes,prg_banking,m009.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m009.0_chr_latch.nes,chr_latch,m009.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m009.0_nametable.nes,nametable,m009.0))

# Mapper 10, Submapper 0 (MMC4)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m010.0_prg_banking.nes,prg_banking,m010.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m010.0_chr_latch.nes,chr_latch,m010.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m010.0_nametable.nes,nametable,m010.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m010.0_prg_ram.nes,prg_ram,m010.0))

# Mapper 11, Submapper 0 (Color Dreams)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m011.0_prg_banking.nes,prg_banking,m011.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m011.0_chr_banking.nes,chr_banking,m011.0))

# Mapper 11, Submapper 1 (Color Dreams, no bus conflicts)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m011.1_prg_banking.nes,prg_banking,m011.1))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m011.1_chr_banking.nes,chr_banking,m011.1))

# Mapper 12, Submapper 0 (SL-5020B, MMC3 + outer CHR)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m012.0_prg_banking.nes,prg_banking,m012.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m012.0_chr_banking.nes,chr_banking,m012.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m012.0_nametable.nes,nametable,m012.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m012.0_irq.nes,irq,m012.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m012.0_prg_ram.nes,prg_ram,m012.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m012.0_write_protect.nes,write_protect,m012.0))

# Mapper 13, Submapper 0 (CPROM)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m013.0_chr_ram_banking.nes,chr_ram_banking,m013.0))

# Mapper 14, Submapper 0 (SL-1632, MMC3/VRC2 hybrid)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m014.0_prg_banking.nes,prg_banking,m014.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m014.0_chr_banking.nes,chr_banking,m014.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m014.0_nametable.nes,nametable,m014.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m014.0_irq.nes,irq,m014.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m014.0_prg_ram.nes,prg_ram,m014.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m014.0_write_protect.nes,write_protect,m014.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m014.0_vrc2.nes,vrc2,m014.0))

# Mapper 15, Submapper 0 (K-1029 multicart, mode 1 UNROM)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m015.0_prg_banking.nes,prg_banking,m015.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m015.0_nametable.nes,nametable,m015.0))
# Mapper 15, Submapper 0 — 8KB mode tests (modes 0, 2, 3)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m015.0_mode0.nes,mode0,m015.0_8k))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m015.0_mode2.nes,mode2,m015.0_8k))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m015.0_mode3.nes,mode3,m015.0_8k))

# Mapper 19, Submapper 0 (Namco 129/163 with expansion audio)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m019.0_prg_banking.nes,prg_banking,m019.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m019.0_chr_banking.nes,chr_banking,m019.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m019.0_nametable.nes,nametable,m019.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m019.0_irq.nes,irq,m019.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m019.0_nt_from_chr.nes,nt_from_chr,m019.0))

# Mapper 19, Submapper 2 (Namco 163, no expansion audio)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m019.2_prg_banking.nes,prg_banking,m019.2))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m019.2_chr_banking.nes,chr_banking,m019.2))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m019.2_nametable.nes,nametable,m019.2))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m019.2_irq.nes,irq,m019.2))

# Mapper 21, Submapper 1 (VRC4a)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m021.1_prg_banking.nes,prg_banking,m021.1))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m021.1_chr_banking.nes,chr_banking,m021.1))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m021.1_nametable.nes,nametable,m021.1))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m021.1_irq.nes,irq,m021.1))

# Mapper 21, Submapper 2 (VRC4c)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m021.2_prg_banking.nes,prg_banking,m021.2))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m021.2_chr_banking.nes,chr_banking,m021.2))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m021.2_nametable.nes,nametable,m021.2))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m021.2_irq.nes,irq,m021.2))

# Mapper 23, Submapper 1 (VRC4f)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m023.1_prg_banking.nes,prg_banking,m023.1))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m023.1_chr_banking.nes,chr_banking,m023.1))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m023.1_nametable.nes,nametable,m023.1))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m023.1_irq.nes,irq,m023.1))

# Mapper 23, Submapper 2 (VRC4e)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m023.2_prg_banking.nes,prg_banking,m023.2))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m023.2_chr_banking.nes,chr_banking,m023.2))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m023.2_nametable.nes,nametable,m023.2))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m023.2_irq.nes,irq,m023.2))

# Mapper 23, Submapper 3 (VRC2b — no IRQ)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m023.3_prg_banking.nes,prg_banking,m023.3))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m023.3_chr_banking.nes,chr_banking,m023.3))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m023.3_nametable.nes,nametable,m023.3))

# Mapper 25, Submapper 1 (VRC4b)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m025.1_prg_banking.nes,prg_banking,m025.1))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m025.1_chr_banking.nes,chr_banking,m025.1))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m025.1_nametable.nes,nametable,m025.1))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m025.1_irq.nes,irq,m025.1))

# Mapper 25, Submapper 2 (VRC4d)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m025.2_prg_banking.nes,prg_banking,m025.2))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m025.2_chr_banking.nes,chr_banking,m025.2))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m025.2_nametable.nes,nametable,m025.2))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m025.2_irq.nes,irq,m025.2))

# Mapper 25, Submapper 3 (VRC2c — no IRQ)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m025.3_prg_banking.nes,prg_banking,m025.3))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m025.3_chr_banking.nes,chr_banking,m025.3))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m025.3_nametable.nes,nametable,m025.3))

# ============================================================
# Combined ROM targets
# $(call COMBINED_RULE, target, mapper_id, aspects...)
# ============================================================

$(eval $(call COMBINED_RULE,$(BIN_DIR)/m000.0.nes,m000.0,prg_ram))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m001.0.nes,m001.0,prg_banking chr_banking nametable prg_ram))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m001.5.nes,m001.5,prg_banking))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m002.0.nes,m002.0,prg_banking bus_conflicts))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m002.2.nes,m002.2,prg_banking))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m003.0.nes,m003.0,chr_banking bus_conflicts))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m003.1.nes,m003.1,chr_banking))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m004.0.nes,m004.0,prg_banking chr_banking nametable irq prg_ram))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m004.1.nes,m004.1,irq))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m005.0.nes,m005.0,prg_banking chr_banking nametable irq prg_ram multiplier exram))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m006.0.nes,m006.0,prg_banking chr_banking nametable irq prg_ram))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m007.0.nes,m007.0,prg_banking nametable))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m007.1.nes,m007.1,prg_banking nametable))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m008.0.nes,m008.0,prg_banking chr_banking nametable irq prg_ram))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m009.0.nes,m009.0,prg_banking chr_latch nametable))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m010.0.nes,m010.0,prg_banking chr_latch nametable prg_ram))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m011.0.nes,m011.0,prg_banking chr_banking))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m011.1.nes,m011.1,prg_banking chr_banking))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m012.0.nes,m012.0,prg_banking chr_banking nametable irq prg_ram))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m013.0.nes,m013.0,chr_ram_banking))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m014.0.nes,m014.0,prg_banking chr_banking nametable irq prg_ram))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m015.0.nes,m015.0,prg_banking nametable))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m015.0_modes.nes,m015.0_8k,mode0 mode2 mode3))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m019.0.nes,m019.0,prg_banking chr_banking nametable irq nt_from_chr))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m019.2.nes,m019.2,prg_banking chr_banking nametable irq))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m021.1.nes,m021.1,prg_banking chr_banking nametable irq))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m021.2.nes,m021.2,prg_banking chr_banking nametable irq))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m023.1.nes,m023.1,prg_banking chr_banking nametable irq))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m023.2.nes,m023.2,prg_banking chr_banking nametable irq))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m023.3.nes,m023.3,prg_banking chr_banking nametable))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m025.1.nes,m025.1,prg_banking chr_banking nametable irq))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m025.2.nes,m025.2,prg_banking chr_banking nametable irq))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m025.3.nes,m025.3,prg_banking chr_banking nametable))

# Mapper 16, Submapper 4 (Bandai FCG-1/2)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m016.4_prg_banking.nes,prg_banking,m016.4))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m016.4_chr_banking.nes,chr_banking,m016.4))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m016.4_nametable.nes,nametable,m016.4))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m016.4_irq.nes,irq,m016.4))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m016.4.nes,m016.4,prg_banking chr_banking nametable irq))

# Mapper 16, Submapper 5 (Bandai LZ93D50)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m016.5_prg_banking.nes,prg_banking,m016.5))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m016.5_chr_banking.nes,chr_banking,m016.5))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m016.5_nametable.nes,nametable,m016.5))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m016.5_irq.nes,irq,m016.5))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m016.5.nes,m016.5,prg_banking chr_banking nametable irq))

# Mapper 18, Submapper 0 (Jaleco SS 88006)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m018.0_prg_banking.nes,prg_banking,m018.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m018.0_chr_banking.nes,chr_banking,m018.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m018.0_nametable.nes,nametable,m018.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m018.0_irq.nes,irq,m018.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m018.0_prg_ram.nes,prg_ram,m018.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m018.0.nes,m018.0,prg_banking chr_banking nametable irq prg_ram))

# Mapper 22, Submapper 0 (VRC2a)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m022.0_prg_banking.nes,prg_banking,m022.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m022.0_chr_banking.nes,chr_banking,m022.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m022.0_nametable.nes,nametable,m022.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m022.0.nes,m022.0,prg_banking chr_banking nametable))

# Mapper 24, Submapper 0 (VRC6a)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m024.0_prg_banking.nes,prg_banking,m024.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m024.0_chr_banking.nes,chr_banking,m024.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m024.0_nametable.nes,nametable,m024.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m024.0_irq.nes,irq,m024.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m024.0.nes,m024.0,prg_banking chr_banking nametable irq))

# Mapper 26, Submapper 0 (VRC6b)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m026.0_prg_banking.nes,prg_banking,m026.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m026.0_chr_banking.nes,chr_banking,m026.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m026.0_nametable.nes,nametable,m026.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m026.0_irq.nes,irq,m026.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m026.0.nes,m026.0,prg_banking chr_banking nametable irq))

# Mapper 28, Submapper 0 (Action 53)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m028.0_prg_banking.nes,prg_banking,m028.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m028.0_nametable.nes,nametable,m028.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m028.0.nes,m028.0,prg_banking nametable))

# Mapper 29, Submapper 0 (Sealie Computing / RET-CUFROM revD)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m029.0_prg_banking.nes,prg_banking,m029.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m029.0_chr_ram_banking.nes,chr_ram_banking,m029.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m029.0.nes,m029.0,prg_banking chr_ram_banking))

# Mapper 30, Submapper 0 (UNROM 512, fixed mirroring)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m030.0_prg_banking.nes,prg_banking,m030.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m030.0_chr_ram_banking.nes,chr_ram_banking,m030.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m030.0.nes,m030.0,prg_banking chr_ram_banking))

# Mapper 30, Submapper 2 (UNROM 512, 1-screen mirroring)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m030.2_prg_banking.nes,prg_banking,m030.2))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m030.2_chr_ram_banking.nes,chr_ram_banking,m030.2))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m030.2_nametable.nes,nametable,m030.2))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m030.2.nes,m030.2,prg_banking chr_ram_banking nametable))

# Mapper 31, Submapper 0 (NSF subset mapper)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m031.0_prg_banking.nes,prg_banking,m031.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m031.0.nes,m031.0,prg_banking))

# Mapper 32, Submapper 0 (Irem G-101 — Standard)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m032.0_prg_banking.nes,prg_banking,m032.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m032.0_chr_banking.nes,chr_banking,m032.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m032.0_nametable.nes,nametable,m032.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m032.0_prg_ram.nes,prg_ram,m032.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m032.0_prg_mode.nes,prg_mode,m032.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m032.0.nes,m032.0,prg_banking chr_banking nametable prg_ram prg_mode))

# Mapper 32, Submapper 1 (Irem G-101 — Major League)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m032.1_prg_banking.nes,prg_banking,m032.1))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m032.1_chr_banking.nes,chr_banking,m032.1))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m032.1_nametable.nes,nametable,m032.1))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m032.1_prg_ram.nes,prg_ram,m032.1))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m032.1.nes,m032.1,prg_banking chr_banking nametable prg_ram))

# Mapper 33, Submapper 0 (Taito TC0190)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m033.0_prg_banking.nes,prg_banking,m033.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m033.0_chr_banking.nes,chr_banking,m033.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m033.0_nametable.nes,nametable,m033.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m033.0_prg_ram.nes,prg_ram,m033.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m033.0.nes,m033.0,prg_banking chr_banking nametable prg_ram))

# Mapper 34, Submapper 0 (BNROM/NINA-001 combined)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m034.0_prg_banking.nes,prg_banking,m034.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m034.0_chr_banking.nes,chr_banking,m034.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m034.0_prg_ram.nes,prg_ram,m034.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m034.0.nes,m034.0,prg_banking chr_banking prg_ram))

# Mapper 34, Submapper 1 (NINA-001)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m034.1_prg_banking.nes,prg_banking,m034.1))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m034.1_chr_banking.nes,chr_banking,m034.1))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m034.1_prg_ram.nes,prg_ram,m034.1))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m034.1.nes,m034.1,prg_banking chr_banking prg_ram))

# Mapper 34, Submapper 2 (BNROM)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m034.2_prg_banking.nes,prg_banking,m034.2))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m034.2.nes,m034.2,prg_banking))

# Mapper 38, Submapper 0 (Bit Corp. PCI556)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m038.0_prg_banking.nes,prg_banking,m038.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m038.0_chr_banking.nes,chr_banking,m038.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m038.0_prg_ram.nes,prg_ram,m038.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m038.0.nes,m038.0,prg_banking chr_banking prg_ram))

# Mapper 44, Submapper 0 (Super Big 7-in-1, MMC3 multicart)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m044.0_prg_banking.nes,prg_banking,m044.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m044.0_chr_banking.nes,chr_banking,m044.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m044.0_nametable.nes,nametable,m044.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m044.0_irq.nes,irq,m044.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m044.0_block_select.nes,block_select,m044.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m044.0.nes,m044.0,prg_banking chr_banking nametable irq block_select))

# Mapper 48, Submapper 0 (Taito TC0690)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m048.0_prg_banking.nes,prg_banking,m048.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m048.0_chr_banking.nes,chr_banking,m048.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m048.0_nametable.nes,nametable,m048.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m048.0_irq.nes,irq,m048.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m048.0_prg_ram.nes,prg_ram,m048.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m048.0.nes,m048.0,prg_banking chr_banking nametable irq prg_ram))

# Mapper 35, Submapper 0 (J.Y. Company ASIC)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m035.0_prg_banking.nes,prg_banking,m035.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m035.0_chr_banking.nes,chr_banking,m035.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m035.0_nametable.nes,nametable,m035.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m035.0_prg_ram.nes,prg_ram,m035.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m035.0_multiplier.nes,multiplier,m035.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m035.0.nes,m035.0,prg_banking chr_banking nametable prg_ram multiplier))

# Mapper 39, Submapper 0 (BNROM clone — Study & Game 32-in-1)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m039.0_prg_banking.nes,prg_banking,m039.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m039.0.nes,m039.0,prg_banking))

# Mapper 37, Submapper 0 (SMB + Tetris + Nintendo World Cup)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m037.0_block_select.nes,block_select,m037.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m037.0.nes,m037.0,block_select))

# Mapper 40, Submapper 0 (NTDEC 2722)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m040.0_prg_banking.nes,prg_banking,m040.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m040.0.nes,m040.0,prg_banking))

# Mapper 41, Submapper 0 (Caltron 6-in-1)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m041.0_prg_banking.nes,prg_banking,m041.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m041.0.nes,m041.0,prg_banking))

# Mapper 42, Submapper 0 (Mario Baby / Ai Senshi Nicol)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m042.0_prg_banking.nes,prg_banking,m042.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m042.0_chr_banking.nes,chr_banking,m042.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m042.0_nametable.nes,nametable,m042.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m042.0.nes,m042.0,prg_banking chr_banking nametable))

# Mapper 43, Submapper 0 (TONY-I / YS-612)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m043.0_prg_banking.nes,prg_banking,m043.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m043.0.nes,m043.0,prg_banking))

# Mapper 45, Submapper 0 (GA23C multicart)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m045.0_block_select.nes,block_select,m045.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m045.0.nes,m045.0,block_select))

# Mapper 46, Submapper 0 (Rumble Station)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m046.0_prg_banking.nes,prg_banking,m046.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m046.0_chr_banking.nes,chr_banking,m046.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m046.0.nes,m046.0,prg_banking chr_banking))

# Mapper 47, Submapper 0 (Super Spike V'Ball + Nintendo World Cup)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m047.0_block_select.nes,block_select,m047.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m047.0.nes,m047.0,block_select))

# Mapper 64, Submapper 0 (RAMBO-1)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m064.0_prg_banking.nes,prg_banking,m064.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m064.0_chr_banking.nes,chr_banking,m064.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m064.0_nametable.nes,nametable,m064.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m064.0_irq.nes,irq,m064.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m064.0.nes,m064.0,prg_banking chr_banking nametable irq))

# Mapper 65 (Irem H3001)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m065.0_prg_banking.nes,prg_banking,m065.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m065.0_chr_banking.nes,chr_banking,m065.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m065.0_nametable.nes,nametable,m065.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m065.0_irq.nes,irq,m065.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m065.0.nes,m065.0,prg_banking chr_banking nametable irq))

# Mapper 67 (Sunsoft 3)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m067.0_prg_banking.nes,prg_banking,m067.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m067.0_chr_banking.nes,chr_banking,m067.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m067.0_nametable.nes,nametable,m067.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m067.0_irq.nes,irq,m067.0))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m067.0.nes,m067.0,prg_banking chr_banking nametable irq))
