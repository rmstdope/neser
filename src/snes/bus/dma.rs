use crate::snes::console::save_state::SnesDmaState;

const DMA_REG_BYTES: usize = 0x80;
const B_BUS_PORT_BYTES: usize = 0x100;

pub trait DmaABus {
    fn dma_read_a_bus(&mut self, addr: u32, open_bus: u8) -> u8;
    fn dma_write_a_bus(&mut self, addr: u32, value: u8);
}

#[derive(Clone)]
pub struct DmaController {
    regs: [u8; DMA_REG_BYTES],
    bbus_ports: [u8; B_BUS_PORT_BYTES],
    hdma_active_mask: u8,
    hdma_do_transfer: [bool; 8],
    hdma_repeat_mode: [bool; 8],
    hdma_lines_left: [u16; 8],
}

impl DmaController {
    pub fn new() -> Self {
        Self {
            regs: [0; DMA_REG_BYTES],
            bbus_ports: [0; B_BUS_PORT_BYTES],
            hdma_active_mask: 0,
            hdma_do_transfer: [false; 8],
            hdma_repeat_mode: [false; 8],
            hdma_lines_left: [0; 8],
        }
    }

    pub fn read_register(&self, offset: u16) -> Option<u8> {
        if !(0x4300..=0x437F).contains(&offset) {
            return None;
        }
        Some(self.regs[(offset - 0x4300) as usize])
    }

    pub fn write_register(&mut self, offset: u16, value: u8) -> bool {
        if !(0x4300..=0x437F).contains(&offset) {
            return false;
        }
        self.regs[(offset - 0x4300) as usize] = value;
        true
    }

    pub(crate) fn capture_state(&self) -> SnesDmaState {
        SnesDmaState {
            regs: self.regs.to_vec(),
            bbus_ports: self.bbus_ports.to_vec(),
            hdma_active_mask: self.hdma_active_mask,
            hdma_do_transfer: self.hdma_do_transfer.to_vec(),
            hdma_repeat_mode: self.hdma_repeat_mode.to_vec(),
            hdma_lines_left: self.hdma_lines_left.to_vec(),
        }
    }

    pub(crate) fn restore_state(&mut self, state: &SnesDmaState) -> Result<(), String> {
        if state.regs.len() != DMA_REG_BYTES {
            return Err(format!(
                "DMA register state size mismatch (expected {DMA_REG_BYTES}, found {})",
                state.regs.len()
            ));
        }
        if state.bbus_ports.len() != B_BUS_PORT_BYTES {
            return Err(format!(
                "DMA B-bus state size mismatch (expected {B_BUS_PORT_BYTES}, found {})",
                state.bbus_ports.len()
            ));
        }
        if state.hdma_do_transfer.len() != 8
            || state.hdma_repeat_mode.len() != 8
            || state.hdma_lines_left.len() != 8
        {
            return Err("DMA HDMA state size mismatch".to_string());
        }

        self.regs.copy_from_slice(&state.regs);
        self.bbus_ports.copy_from_slice(&state.bbus_ports);
        self.hdma_active_mask = state.hdma_active_mask;
        self.hdma_do_transfer
            .copy_from_slice(&state.hdma_do_transfer);
        self.hdma_repeat_mode
            .copy_from_slice(&state.hdma_repeat_mode);
        self.hdma_lines_left.copy_from_slice(&state.hdma_lines_left);
        Ok(())
    }

    pub fn start_dma<B: DmaABus>(
        &mut self,
        mdmaen: u8,
        abus: &mut B,
        seed_open_bus: u8,
    ) -> (u64, u8) {
        if mdmaen == 0 {
            return (0, seed_open_bus);
        }

        let mut ticks = 16u64;
        let mut open_bus = seed_open_bus;

        for channel in 0u8..8 {
            if (mdmaen & (1 << channel)) == 0 {
                continue;
            }

            ticks += 8;
            ticks += self.run_channel(channel, abus, &mut open_bus);
        }

        (ticks, open_bus)
    }

    pub fn hdma_init<B: DmaABus>(
        &mut self,
        hdmaen: u8,
        abus: &mut B,
        seed_open_bus: u8,
    ) -> (u64, u8) {
        self.hdma_active_mask = 0;
        self.hdma_do_transfer = [false; 8];
        self.hdma_repeat_mode = [false; 8];
        self.hdma_lines_left = [0; 8];

        if hdmaen == 0 {
            return (0, seed_open_bus);
        }

        let mut ticks = 18u64;
        let mut open_bus = seed_open_bus;

        for channel in 0u8..8 {
            if (hdmaen & (1 << channel)) == 0 {
                continue;
            }

            let dmap = self.get_reg(channel, 0x0);
            let indirect = (dmap & 0x40) != 0;
            ticks += 8;

            let a1t_low = self.get_reg(channel, 0x2);
            let a1t_high = self.get_reg(channel, 0x3);
            self.set_reg(channel, 0x8, a1t_low);
            self.set_reg(channel, 0x9, a1t_high);

            let descriptor = self.read_hdma_table_byte(channel, abus, &mut open_bus);
            self.set_reg(channel, 0xA, descriptor);
            if descriptor == 0 {
                continue;
            }
            let (repeat_mode, lines_left) = Self::decode_hdma_descriptor(descriptor);
            self.hdma_repeat_mode[channel as usize] = repeat_mode;
            self.hdma_lines_left[channel as usize] = lines_left;

            if indirect {
                ticks += 16;
                let indirect_low = self.read_hdma_table_byte(channel, abus, &mut open_bus);
                let indirect_high = self.read_hdma_table_byte(channel, abus, &mut open_bus);
                self.set_reg(channel, 0x5, indirect_low);
                self.set_reg(channel, 0x6, indirect_high);
            }

            self.hdma_active_mask |= 1 << channel;
            self.hdma_do_transfer[channel as usize] = true;
        }

        (ticks, open_bus)
    }

    pub fn hdma_do_line<B: DmaABus>(&mut self, abus: &mut B, seed_open_bus: u8) -> (u64, u8) {
        if self.hdma_active_mask == 0 {
            return (0, seed_open_bus);
        }

        let mut ticks = 18u64;
        let mut open_bus = seed_open_bus;

        for channel in 0u8..8 {
            if (self.hdma_active_mask & (1 << channel)) == 0 {
                continue;
            }

            ticks += 8;
            if self.hdma_do_transfer[channel as usize] {
                ticks += self.run_hdma_transfer_unit(channel, abus, &mut open_bus);
            }

            let idx = channel as usize;
            self.hdma_lines_left[idx] = self.hdma_lines_left[idx].saturating_sub(1);
            self.set_reg(
                channel,
                0xA,
                Self::encode_hdma_counter(self.hdma_repeat_mode[idx], self.hdma_lines_left[idx]),
            );
            self.hdma_do_transfer[idx] = self.hdma_repeat_mode[idx];

            if self.hdma_lines_left[idx] == 0 {
                if !self.reload_hdma_entry(channel, abus, &mut open_bus, &mut ticks) {
                    self.hdma_active_mask &= !(1 << channel);
                    self.hdma_do_transfer[idx] = false;
                } else {
                    self.hdma_do_transfer[idx] = true;
                }
            }
        }

        (ticks, open_bus)
    }

    fn run_channel<B: DmaABus>(&mut self, channel: u8, abus: &mut B, open_bus: &mut u8) -> u64 {
        let dmap = self.get_reg(channel, 0x0);
        let bbad = self.get_reg(channel, 0x1);
        let mut a_low = self.get_reg(channel, 0x2);
        let mut a_high = self.get_reg(channel, 0x3);
        let bank = self.get_reg(channel, 0x4);
        let das_low = self.get_reg(channel, 0x5);
        let das_high = self.get_reg(channel, 0x6);

        let mode = Self::canonical_mode(dmap & 0x07);
        let pattern = Self::transfer_offsets(mode);
        let direction_b_to_a = (dmap & 0x80) != 0;
        let step = (dmap >> 3) & 0x03;

        let mut count = u16::from_le_bytes([das_low, das_high]);
        let transfer_bytes: usize = if count == 0 { 0x1_0000 } else { count as usize };
        let mut ticks = 0u64;

        for i in 0..transfer_bytes {
            let a_addr = ((bank as u32) << 16) | ((a_high as u32) << 8) | (a_low as u32);
            let b_addr = bbad.wrapping_add(pattern[i % pattern.len()]);

            if direction_b_to_a {
                let value = self.read_b_bus(b_addr);
                *open_bus = value;
                abus.dma_write_a_bus(a_addr, value);
            } else {
                let value = abus.dma_read_a_bus(a_addr, *open_bus);
                *open_bus = value;
                self.write_b_bus(b_addr, value);
            }

            ticks += 8;
            count = count.wrapping_sub(1);
            (a_low, a_high) = Self::advance_a_address(a_low, a_high, step);
        }

        self.set_reg(channel, 0x2, a_low);
        self.set_reg(channel, 0x3, a_high);
        self.set_reg(channel, 0x5, (count & 0x00FF) as u8);
        self.set_reg(channel, 0x6, (count >> 8) as u8);
        ticks
    }

    fn canonical_mode(mode: u8) -> u8 {
        match mode {
            0x5 => 0x1,
            0x6 => 0x2,
            0x7 => 0x3,
            _ => mode,
        }
    }

    fn transfer_offsets(mode: u8) -> &'static [u8] {
        match mode {
            0x0 => &[0],
            0x1 => &[0, 1],
            0x2 => &[0, 0],
            0x3 => &[0, 0, 1, 1],
            0x4 => &[0, 1, 2, 3],
            _ => &[0],
        }
    }

    fn advance_a_address(low: u8, high: u8, step: u8) -> (u8, u8) {
        let mut addr = u16::from_le_bytes([low, high]);
        match step {
            0x0 => addr = addr.wrapping_add(1),
            0x2 => addr = addr.wrapping_sub(1),
            _ => {}
        }
        let [next_low, next_high] = addr.to_le_bytes();
        (next_low, next_high)
    }

    fn read_b_bus(&self, addr: u8) -> u8 {
        self.bbus_ports[addr as usize]
    }

    fn write_b_bus(&mut self, addr: u8, value: u8) {
        self.bbus_ports[addr as usize] = value;
    }

    fn get_reg(&self, channel: u8, reg: usize) -> u8 {
        self.regs[(channel as usize) * 0x10 + reg]
    }

    fn set_reg(&mut self, channel: u8, reg: usize, value: u8) {
        self.regs[(channel as usize) * 0x10 + reg] = value;
    }

    fn decode_hdma_descriptor(descriptor: u8) -> (bool, u16) {
        if descriptor == 0 {
            return (false, 0);
        }
        if descriptor <= 0x80 {
            if descriptor == 0x80 {
                (false, 128)
            } else {
                (false, descriptor as u16)
            }
        } else {
            (true, (descriptor - 0x80) as u16)
        }
    }

    fn encode_hdma_counter(repeat_mode: bool, lines_left: u16) -> u8 {
        if lines_left == 0 {
            0
        } else if repeat_mode {
            0x80 | (lines_left as u8)
        } else if lines_left == 128 {
            0x80
        } else {
            lines_left as u8
        }
    }

    fn hdma_table_address(&self, channel: u8) -> u32 {
        let bank = self.get_reg(channel, 0x4);
        let low = self.get_reg(channel, 0x8);
        let high = self.get_reg(channel, 0x9);
        ((bank as u32) << 16) | u16::from_le_bytes([low, high]) as u32
    }

    fn read_hdma_table_byte<B: DmaABus>(
        &mut self,
        channel: u8,
        abus: &mut B,
        open_bus: &mut u8,
    ) -> u8 {
        let table_addr = self.hdma_table_address(channel);
        let value = abus.dma_read_a_bus(table_addr, *open_bus);
        *open_bus = value;

        let next = u16::from_le_bytes([self.get_reg(channel, 0x8), self.get_reg(channel, 0x9)])
            .wrapping_add(1);
        let [next_low, next_high] = next.to_le_bytes();
        self.set_reg(channel, 0x8, next_low);
        self.set_reg(channel, 0x9, next_high);
        value
    }

    fn run_hdma_transfer_unit<B: DmaABus>(
        &mut self,
        channel: u8,
        abus: &mut B,
        open_bus: &mut u8,
    ) -> u64 {
        let dmap = self.get_reg(channel, 0x0);
        let bbad = self.get_reg(channel, 0x1);
        let mode = Self::canonical_mode(dmap & 0x07);
        let pattern = Self::transfer_offsets(mode);
        let direction_b_to_a = (dmap & 0x80) != 0;
        let indirect = (dmap & 0x40) != 0;

        let bank = if indirect {
            self.get_reg(channel, 0x7)
        } else {
            self.get_reg(channel, 0x4)
        };
        let mut addr = if indirect {
            u16::from_le_bytes([self.get_reg(channel, 0x5), self.get_reg(channel, 0x6)])
        } else {
            u16::from_le_bytes([self.get_reg(channel, 0x8), self.get_reg(channel, 0x9)])
        };

        for offset in pattern {
            let a_addr = ((bank as u32) << 16) | addr as u32;
            let b_addr = bbad.wrapping_add(*offset);
            if direction_b_to_a {
                let value = self.read_b_bus(b_addr);
                *open_bus = value;
                abus.dma_write_a_bus(a_addr, value);
            } else {
                let value = abus.dma_read_a_bus(a_addr, *open_bus);
                *open_bus = value;
                self.write_b_bus(b_addr, value);
            }
            addr = addr.wrapping_add(1);
        }

        let [low, high] = addr.to_le_bytes();
        if indirect {
            self.set_reg(channel, 0x5, low);
            self.set_reg(channel, 0x6, high);
        } else {
            self.set_reg(channel, 0x8, low);
            self.set_reg(channel, 0x9, high);
        }

        (pattern.len() as u64) * 8
    }

    fn reload_hdma_entry<B: DmaABus>(
        &mut self,
        channel: u8,
        abus: &mut B,
        open_bus: &mut u8,
        ticks: &mut u64,
    ) -> bool {
        let descriptor = self.read_hdma_table_byte(channel, abus, open_bus);
        self.set_reg(channel, 0xA, descriptor);
        if descriptor == 0 {
            self.hdma_repeat_mode[channel as usize] = false;
            self.hdma_lines_left[channel as usize] = 0;
            return false;
        }
        let (repeat_mode, lines_left) = Self::decode_hdma_descriptor(descriptor);
        self.hdma_repeat_mode[channel as usize] = repeat_mode;
        self.hdma_lines_left[channel as usize] = lines_left;

        if (self.get_reg(channel, 0x0) & 0x40) != 0 {
            let low = self.read_hdma_table_byte(channel, abus, open_bus);
            let high = self.read_hdma_table_byte(channel, abus, open_bus);
            self.set_reg(channel, 0x5, low);
            self.set_reg(channel, 0x6, high);
            *ticks += 16;
        }

        true
    }
}

impl Default for DmaController {
    fn default() -> Self {
        Self::new()
    }
}
