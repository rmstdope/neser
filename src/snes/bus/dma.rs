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
}

impl DmaController {
    pub fn new() -> Self {
        Self {
            regs: [0; DMA_REG_BYTES],
            bbus_ports: [0; B_BUS_PORT_BYTES],
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
}

impl Default for DmaController {
    fn default() -> Self {
        Self::new()
    }
}
