// Rendering alignment tests for background and sprite rendering

#[cfg(test)]
mod tests {
    use crate::console::{Nes, TimingMode};
    use crate::ppu::ppu::Ppu;
    use crate::ppu::test_utils::InesRomBuilder;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn test_background_rendering_alignment() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Create CHR ROM with known tiles
        let mut chr_rom = vec![0u8; 0x2000];

        // Tile 0 (at $0000): Empty tile (all transparent)
        // Pattern low and high bytes are all 0

        // Tile 1 (at $0010): Solid tile with pattern value 3 (color 3 in palette)
        // Each byte represents one row of 8 pixels
        // Pattern low = 0xFF (all bits set)
        // Pattern high = 0xFF (all bits set)
        // This gives pattern value 3 (both bits set) for all pixels
        for row in 0..8 {
            chr_rom[0x10 + row] = 0xFF; // Pattern low
            chr_rom[0x18 + row] = 0xFF; // Pattern high
        }

        // Tile 2 (at $0020): Tile with pattern value 1 (only low bit set)
        for row in 0..8 {
            chr_rom[0x20 + row] = 0xFF; // Pattern low
            chr_rom[0x28 + row] = 0x00; // Pattern high
        }

        // Tile 3 (at $0030): Tile with pattern value 2 (only high bit set)
        for row in 0..8 {
            chr_rom[0x30 + row] = 0x00; // Pattern low
            chr_rom[0x38 + row] = 0xFF; // Pattern high
        }

        // Build ROM using the builder
        let cartridge = InesRomBuilder::new()
            .prg_rom_size(2) // 2 * 16KB = 32KB
            .chr_rom_size(1) // 1 * 8KB
            .chr_rom_data(chr_rom)
            .build_cartridge();

        ppu.set_cartridge(Rc::new(RefCell::new(cartridge)));

        // Set up palette - use distinct colors for each palette entry
        // Palette 0 will be: backdrop (black), red, green, blue
        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x0F); // Universal backdrop (black)
        ppu.write_address(0x3F, false);
        ppu.write_address(0x01, false);
        ppu.write_data(0x16); // Palette 0, color 1 (red)
        ppu.write_address(0x3F, false);
        ppu.write_address(0x02, false);
        ppu.write_data(0x2A); // Palette 0, color 2 (green)
        ppu.write_address(0x3F, false);
        ppu.write_address(0x03, false);
        ppu.write_data(0x12); // Palette 0, color 3 (blue)

        // Set up nametable - create a known pattern
        // Place tile 1 (solid, pattern 3) at position (0,0) - top-left corner
        ppu.write_address(0x20, false);
        ppu.write_address(0x00, false);
        ppu.write_data(1); // Tile 1 at (0,0)

        // Place tile 2 (pattern 1) at position (1,0) - second tile in first row
        ppu.write_data(2); // Tile 2 at (1,0)

        // Place tile 3 (pattern 2) at position (2,0) - third tile in first row
        ppu.write_data(3); // Tile 3 at (2,0)

        // Fill rest of first row with tile 0 (empty/transparent)
        for _ in 3..32 {
            ppu.write_data(0); // Empty tiles
        }

        // Set up attribute table - palette 0 for all tiles
        ppu.write_address(0x23, false);
        ppu.write_address(0xC0, false);
        for _ in 0..64 {
            ppu.write_data(0x00); // Palette 0 for all
        }

        // Set scroll position to 0,0
        // This ensures t register is properly initialized
        ppu.write_scroll(0, false); // X scroll = 0
        ppu.write_scroll(0, false); // Y scroll = 0

        // Enable rendering
        ppu.write_control(0b0000_0000); // BG pattern table at $0000, no NMI, nametable $2000
        ppu.write_mask(0b0000_1010); // Enable background rendering, no clipping

        // Run PPU to render two complete frames
        // NTSC: 262 scanlines * 341 dots/scanline
        // First frame: renders with empty shift registers (will show offset)
        // Second frame: pre-render scanline 261 of first frame loads shift registers,
        //               so second frame renders correctly with tiles at positions 0-7, 8-15, etc.
        ppu.run_ppu_cycles(2 * 262 * 341);

        // Debug: Check if palette was actually written
        // Use direct memory access to check
        ppu.write_address(0x3F, false);
        ppu.write_address(0x03, false);
        let _pal3 = ppu.read_data();

        // Now check the screen buffer for expected colors
        let screen_buffer = ppu.screen_buffer();

        // Get the system palette colors for our palette entries
        let (red_r, red_g, red_b) = Nes::lookup_system_palette(0x16);
        let (green_r, green_g, green_b) = Nes::lookup_system_palette(0x2A);
        let (blue_r, blue_g, blue_b) = Nes::lookup_system_palette(0x12);
        let (black_r, black_g, black_b) = Nes::lookup_system_palette(0x0F);

        // Verify all pixels in the topmost 16 rows
        // After running two complete frames, the pre-render scanline has properly loaded
        // the shift registers, so tiles should appear at their correct pixel positions
        for row in 0..16 {
            for x in 0..256 {
                let (r, g, b) = screen_buffer.get_pixel(x, row);
                let expected_color = match row {
                    0..=7 => {
                        // First tile row - should show tiles at their correct positions:
                        // Nametable position 0: tile 1 (blue) at pixels 0-7
                        // Nametable position 1: tile 2 (red) at pixels 8-15
                        // Nametable position 2: tile 3 (green) at pixels 16-23
                        // Rest: tile 0 (black/empty)
                        if x <= 7 {
                            (blue_r, blue_g, blue_b) // Tile 1 from nametable position 0
                        } else if (8..=15).contains(&x) {
                            (red_r, red_g, red_b) // Tile 2 from nametable position 1
                        } else if (16..=23).contains(&x) {
                            (green_r, green_g, green_b) // Tile 3 from nametable position 2
                        } else {
                            (black_r, black_g, black_b) // Empty tiles (positions 3+)
                        }
                    }
                    _ => (black_r, black_g, black_b), // Second tile row (nametable row 1), all empty
                };

                assert_eq!(
                    (r, g, b),
                    expected_color,
                    "Pixel ({}, {}) has wrong color",
                    x,
                    row
                );
            }
        }
    }

    #[test]
    fn test_sprite_rendering_alignment() {
        let mut ppu = Ppu::new_for_testing(TimingMode::Ntsc);

        // Create CHR ROM with known sprite tiles
        let mut chr_rom = vec![0u8; 0x2000];

        // Tile 0 (at $0000): Empty tile (all transparent)
        // Pattern low and high bytes are all 0

        // Tile 1 (at $0010): Solid tile with pattern value 3 (color 3 in palette)
        for row in 0..8 {
            chr_rom[0x10 + row] = 0xFF; // Pattern low
            chr_rom[0x18 + row] = 0xFF; // Pattern high
        }

        // Tile 2 (at $0020): Tile with pattern value 1 (only low bit set)
        for row in 0..8 {
            chr_rom[0x20 + row] = 0xFF; // Pattern low
            chr_rom[0x28 + row] = 0x00; // Pattern high
        }

        // Tile 3 (at $0030): Tile with pattern value 2 (only high bit set)
        for row in 0..8 {
            chr_rom[0x30 + row] = 0x00; // Pattern low
            chr_rom[0x38 + row] = 0xFF; // Pattern high
        }

        // Build ROM using the builder
        let cartridge = InesRomBuilder::new()
            .prg_rom_size(2) // 2 * 16KB = 32KB
            .chr_rom_size(1) // 1 * 8KB
            .chr_rom_data(chr_rom)
            .build_cartridge();

        ppu.set_cartridge(Rc::new(RefCell::new(cartridge)));

        // Set up sprite palette - use distinct colors
        // Palette 0 will be: backdrop (black), yellow, cyan, magenta
        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        ppu.write_data(0x0F); // Universal backdrop (black)
        ppu.write_address(0x3F, false);
        ppu.write_address(0x11, false);
        ppu.write_data(0x28); // Sprite palette 0, color 1 (yellow)
        ppu.write_address(0x3F, false);
        ppu.write_address(0x12, false);
        ppu.write_data(0x2C); // Sprite palette 0, color 2 (cyan)
        ppu.write_address(0x3F, false);
        ppu.write_address(0x13, false);
        ppu.write_data(0x14); // Sprite palette 0, color 3 (magenta)

        // Set up sprites in OAM
        // Sprite 0: tile 1 (pattern 3 = magenta) at position (16, 16)
        ppu.write_oam_address(0x00);
        ppu.write_oam_data(16); // Y position
        ppu.write_oam_data(1); // Tile index 1
        ppu.write_oam_data(0x00); // Attributes: palette 0, no flip
        ppu.write_oam_data(16); // X position

        // Sprite 1: tile 2 (pattern 1 = yellow) at position (32, 16)
        ppu.write_oam_data(16); // Y position
        ppu.write_oam_data(2); // Tile index 2
        ppu.write_oam_data(0x00); // Attributes: palette 0, no flip
        ppu.write_oam_data(32); // X position

        // Sprite 2: tile 3 (pattern 2 = cyan) at position (48, 16)
        ppu.write_oam_data(16); // Y position
        ppu.write_oam_data(3); // Tile index 3
        ppu.write_oam_data(0x00); // Attributes: palette 0, no flip
        ppu.write_oam_data(48); // X position

        // Fill rest of OAM with off-screen sprites (Y = 0xFF)
        for _ in 3..64 {
            ppu.write_oam_data(0xFF); // Y position (off-screen)
            ppu.write_oam_data(0); // Tile index
            ppu.write_oam_data(0); // Attributes
            ppu.write_oam_data(0); // X position
        }

        // Set scroll position to 0,0
        ppu.write_scroll(0, false);
        ppu.write_scroll(0, false);

        // Enable rendering - sprites only, use sprite pattern table at $0000
        ppu.write_control(0b0000_0000); // Sprite pattern table at $0000, no NMI
        ppu.write_mask(0b0001_0100); // Enable sprite rendering, no clipping

        // Run PPU to render two complete frames
        ppu.run_ppu_cycles(2 * 262 * 341);

        let screen_buffer = ppu.screen_buffer();

        // Get the system palette colors for our sprite palette entries
        let (yellow_r, yellow_g, yellow_b) = Nes::lookup_system_palette(0x28);
        let (cyan_r, cyan_g, cyan_b) = Nes::lookup_system_palette(0x2C);
        let (magenta_r, magenta_g, magenta_b) = Nes::lookup_system_palette(0x14);
        let (black_r, black_g, black_b) = Nes::lookup_system_palette(0x0F);

        // Verify sprite rendering according to NES hardware specification:
        // - X coordinate: Direct mapping, screen_x = OAM.X (no offset)
        // - Y coordinate: +1 offset, screen_y = OAM.Y + 1
        //
        // Sprites with Y=N are rendered on scanlines N+1 to N+8
        //
        // Expected correct behavior:
        // Sprite 0 (magenta) at OAM (X=16, Y=16) should render at pixels (16-23, 17-24)
        // Sprite 1 (yellow) at OAM (X=32, Y=16) should render at pixels (32-39, 17-24)
        // Sprite 2 (cyan) at OAM (X=48, Y=16) should render at pixels (48-55, 17-24)

        for y in 0..240 {
            for x in 0..256 {
                let (r, g, b) = screen_buffer.get_pixel(x, y);
                let expected_color = if (17..=24).contains(&y) {
                    // Scanlines where sprites are visible (Y position 16 + 1, for 8 rows)
                    // Using CORRECT X coordinates per hardware specification
                    if (16..=23).contains(&x) {
                        (magenta_r, magenta_g, magenta_b) // Sprite 0 (correct position)
                    } else if (32..=39).contains(&x) {
                        (yellow_r, yellow_g, yellow_b) // Sprite 1 (correct position)
                    } else if (48..=55).contains(&x) {
                        (cyan_r, cyan_g, cyan_b) // Sprite 2 (correct position)
                    } else {
                        (black_r, black_g, black_b) // Backdrop
                    }
                } else {
                    (black_r, black_g, black_b) // Backdrop
                };

                assert_eq!(
                    (r, g, b),
                    expected_color,
                    "Sprite pixel ({}, {}) has wrong color",
                    x,
                    y
                );
            }
        }
    }
}
