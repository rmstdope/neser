use crate::cartridge::Cartridge;
use crate::cpu;
use crate::memory;
use std::cell::RefCell;
use std::rc::Rc;

pub struct Nes {
    pub memory: Rc<RefCell<memory::Memory>>,
    pub cpu: cpu::Cpu,
}

impl Nes {
    pub fn new() -> Self {
        let memory = Rc::new(RefCell::new(memory::Memory::new()));
        let cpu = cpu::Cpu::new(memory.clone());
        Self { memory, cpu }
    }

    /// Insert a cartridge and map it into memory
    pub fn insert_cartridge(&mut self, cartridge: Cartridge) {
        self.memory.borrow_mut().map_cartridge(cartridge);
    }
}
