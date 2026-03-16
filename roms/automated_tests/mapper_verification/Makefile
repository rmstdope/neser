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
	$(SINGLES_DIR)/m004.1_irq.nes \
	$(SINGLES_DIR)/m007.0_prg_banking.nes \
	$(SINGLES_DIR)/m007.0_nametable.nes \
	$(SINGLES_DIR)/m007.1_prg_banking.nes \
	$(SINGLES_DIR)/m007.1_nametable.nes \
	$(SINGLES_DIR)/m005.0_prg_banking.nes \
	$(SINGLES_DIR)/m005.0_chr_banking.nes \
	$(SINGLES_DIR)/m005.0_nametable.nes \
	$(SINGLES_DIR)/m005.0_irq.nes \
	$(SINGLES_DIR)/m005.0_prg_ram.nes \
	$(SINGLES_DIR)/m005.0_multiplier.nes \
	$(SINGLES_DIR)/m006.0_prg_banking.nes \
	$(SINGLES_DIR)/m006.0_chr_banking.nes \
	$(SINGLES_DIR)/m006.0_nametable.nes \
	$(SINGLES_DIR)/m006.0_irq.nes \
	$(SINGLES_DIR)/m006.0_prg_ram.nes \
	$(SINGLES_DIR)/m011.0_prg_banking.nes \
	$(SINGLES_DIR)/m011.0_chr_banking.nes \
	$(SINGLES_DIR)/m013.0_chr_ram_banking.nes \
	$(SINGLES_DIR)/m009.0_prg_banking.nes \
	$(SINGLES_DIR)/m009.0_chr_latch.nes \
	$(SINGLES_DIR)/m009.0_nametable.nes \
	$(SINGLES_DIR)/m010.0_prg_banking.nes \
	$(SINGLES_DIR)/m010.0_chr_latch.nes \
	$(SINGLES_DIR)/m010.0_nametable.nes \
	$(SINGLES_DIR)/m010.0_prg_ram.nes \
	$(SINGLES_DIR)/m008.0_prg_banking.nes \
	$(SINGLES_DIR)/m008.0_chr_banking.nes \
	$(SINGLES_DIR)/m008.0_nametable.nes \
	$(SINGLES_DIR)/m008.0_irq.nes \
	$(SINGLES_DIR)/m008.0_prg_ram.nes \
	$(SINGLES_DIR)/m015.0_prg_banking.nes \
	$(SINGLES_DIR)/m015.0_nametable.nes

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
	$(BIN_DIR)/m007.0.nes \
	$(BIN_DIR)/m007.1.nes \
	$(BIN_DIR)/m005.0.nes \
	$(BIN_DIR)/m006.0.nes \
	$(BIN_DIR)/m011.0.nes \
	$(BIN_DIR)/m013.0.nes \
	$(BIN_DIR)/m009.0.nes \
	$(BIN_DIR)/m010.0.nes \
	$(BIN_DIR)/m008.0.nes \
	$(BIN_DIR)/m015.0.nes

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

# Mapper 4, Submapper 1 (NEC IRQ)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m004.1_irq.nes,irq,m004.1))

# Mapper 7, Submapper 0 (Bus Conflicts)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m007.0_prg_banking.nes,prg_banking,m007.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m007.0_nametable.nes,nametable,m007.0))

# Mapper 7, Submapper 1 (No Bus Conflicts)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m007.1_prg_banking.nes,prg_banking,m007.1))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m007.1_nametable.nes,nametable,m007.1))

# Mapper 5, Submapper 0 (MMC5)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m005.0_prg_banking.nes,prg_banking,m005.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m005.0_chr_banking.nes,chr_banking,m005.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m005.0_nametable.nes,nametable,m005.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m005.0_irq.nes,irq,m005.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m005.0_prg_ram.nes,prg_ram,m005.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m005.0_multiplier.nes,multiplier,m005.0))

# Mapper 6, Submapper 0 (Front Fareast Magic Card, mode 1 UN1ROM+CHRSW)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m006.0_prg_banking.nes,prg_banking,m006.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m006.0_chr_banking.nes,chr_banking,m006.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m006.0_nametable.nes,nametable,m006.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m006.0_irq.nes,irq,m006.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m006.0_prg_ram.nes,prg_ram,m006.0))

# Mapper 11, Submapper 0 (Color Dreams)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m011.0_prg_banking.nes,prg_banking,m011.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m011.0_chr_banking.nes,chr_banking,m011.0))

# Mapper 13, Submapper 0 (CPROM)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m013.0_chr_ram_banking.nes,chr_ram_banking,m013.0))

# Mapper 9, Submapper 0 (MMC2)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m009.0_prg_banking.nes,prg_banking,m009.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m009.0_chr_latch.nes,chr_latch,m009.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m009.0_nametable.nes,nametable,m009.0))

# Mapper 10, Submapper 0 (MMC4)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m010.0_prg_banking.nes,prg_banking,m010.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m010.0_chr_latch.nes,chr_latch,m010.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m010.0_nametable.nes,nametable,m010.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m010.0_prg_ram.nes,prg_ram,m010.0))

# Mapper 8, Submapper 0 (SMC GNROM mode 4)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m008.0_prg_banking.nes,prg_banking,m008.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m008.0_chr_banking.nes,chr_banking,m008.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m008.0_nametable.nes,nametable,m008.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m008.0_irq.nes,irq,m008.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m008.0_prg_ram.nes,prg_ram,m008.0))

# Mapper 15, Submapper 0 (K-1029 multicart, mode 1 UNROM)
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m015.0_prg_banking.nes,prg_banking,m015.0))
$(eval $(call ROM_RULE,$(SINGLES_DIR)/m015.0_nametable.nes,nametable,m015.0))

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
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m007.0.nes,m007.0,prg_banking nametable))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m007.1.nes,m007.1,prg_banking nametable))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m005.0.nes,m005.0,prg_banking chr_banking nametable irq prg_ram multiplier))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m006.0.nes,m006.0,prg_banking chr_banking nametable irq prg_ram))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m011.0.nes,m011.0,prg_banking chr_banking))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m013.0.nes,m013.0,chr_ram_banking))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m009.0.nes,m009.0,prg_banking chr_latch nametable))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m010.0.nes,m010.0,prg_banking chr_latch nametable prg_ram))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m008.0.nes,m008.0,prg_banking chr_banking nametable irq prg_ram))
$(eval $(call COMBINED_RULE,$(BIN_DIR)/m015.0.nes,m015.0,prg_banking nametable))
