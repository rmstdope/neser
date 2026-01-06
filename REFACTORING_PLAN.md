# CPU Instruction Table Refactoring Plan

## Goal
Refactor the CPU's large match statement into a function pointer table, similar to Mesen's `NesCpu::_opTable`.

## Approach
1. Create instruction function type signature
2. Build opcode table with 256 function pointers
3. Refactor existing match arms into individual methods
4. Maintain all existing behavior (1165 tests must pass)
5. Follow Mesen's implementation patterns

## Status
- [ ] Phase 1: Infrastructure setup
  - [ ] Define instruction function type
  - [ ] Create opcode table structure
  - [ ] Add table initialization
  
- [ ] Phase 2: Implement instructions by category
  - [ ] Load/Store instructions (LDA, LDX, LDY, STA, STX, STY)
  - [ ] Arithmetic instructions (ADC, SBC, INC, DEC, INX, DEX, INY, DEY)
  - [ ] Logic instructions (AND, ORA, EOR, BIT)
  - [ ] Shift/Rotate instructions (ASL, LSR, ROL, ROR)
  - [ ] Branch instructions (BCC, BCS, BEQ, BNE, BMI, BPL, BVC, BVS)
  - [ ] Compare instructions (CMP, CPX, CPY)
  - [ ] Flag instructions (CLC, CLD, CLI, CLV, SEC, SED, SEI)
  - [ ] Transfer instructions (TAX, TAY, TSX, TXA, TXS, TYA)
  - [ ] Stack instructions (PHA, PHP, PLA, PLP)
  - [ ] Jump/Call instructions (JMP, JSR, RTS, RTI)
  - [ ] System instructions (BRK, NOP, KIL)
  - [ ] Illegal/Undocumented instructions

- [ ] Phase 3: Replace match statement with table lookup

- [ ] Phase 4: Verify all tests pass
