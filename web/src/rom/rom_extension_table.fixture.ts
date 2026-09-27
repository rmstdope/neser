/**
 * The table the wasm binding's `rom_extension_table()` returns, for tests that run without wasm.
 * Rust's `platform::rom_extensions::ROM_EXTENSIONS` is the source; `wasm_tests.rs` pins the binding
 * to exactly these pairs.
 */
export const ROM_EXTENSION_TABLE: readonly (readonly [string, string])[] = [
    ["nes", "nes"],
    ["gb", "gb"],
    ["gbc", "gb"],
    ["cgb", "gb"],
    ["gba", "gba"],
    ["sfc", "snes"],
    ["smc", "snes"],
];
