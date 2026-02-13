# Rust Refactoring Do/Don't Examples

## Do/Don't (Mixed)

- Do: Consolidate duplicated parsing logic into a shared helper. Don't: Keep copy-pasted parse loops in each module.
- Do: Use consistent names for the same concept. Don't: Mix `mapper_id` and `mapper_number` across modules.

```rust
// Do: Prefer borrowing and iterators when possible.
fn sum(values: &[u32]) -> u32 {
    values.iter().copied().sum()
}

// Don't: Clone just to satisfy the loop shape.
fn sum(values: &[u32]) -> u32 {
    values.to_vec().into_iter().sum()
}
```

```rust
// Do: Model intent with enums for clarity.
enum Mirroring {
    Horizontal,
    Vertical,
}

// Don't: Use bool flags with unclear meaning.
fn set_mirroring(is_horizontal: bool) {
    if is_horizontal { /* ... */ }
}
```

```rust
// Do: Keep error types domain-specific in libraries.
#[derive(thiserror::Error, Debug)]
enum ParseError {
    #[error("invalid header")]
    InvalidHeader,
}

// Don't: Use `String` errors in library code.
fn parse() -> Result<(), String> {
    Err("invalid header".to_string())
}
```

```rust
// Do: Express ownership explicitly.
struct Bus {
    cpu: Cpu,
}

// Don't: Hide ownership behind `Rc<RefCell<T>>` without need.
struct Bus {
    cpu: std::rc::Rc<std::cell::RefCell<Cpu>>,
}
```

## Trait Implementations

```rust
// Do: All trait implementations use the same constructor pattern.
pub trait Device {
    fn new(data: Vec<u8>) -> Self;
}

struct Mapper1 { ... }
impl Device for Mapper1 {
    pub fn new(data: Vec<u8>) -> Self { ... }  // Public
}

struct Mapper2 { ... }
impl Device for Mapper2 {
    pub fn new(data: Vec<u8>) -> Self { ... }  // Public
}

// Don't: Inconsistent visibility across implementations.
impl Device for Mapper1 {
    #[cfg(test)]
    pub fn new(data: Vec<u8>) -> Self { ... }  // Test-only!
}

impl Device for Mapper2 {
    pub fn new(data: Vec<u8>) -> Self { ... }  // Always available
}
```

```rust
// Do: Extract repeated patterns in trait implementations to helpers.
// BAD: Each mapper duplicates bank offset calculation
impl Mapper for Mapper1 {
    fn read_chr(&self, addr: u16) -> u8 {
        let num_banks = (self.chr_rom.len() / CHR_BANK_SIZE).max(1);
        let bank = (self.chr_bank_select as usize) % num_banks;
        let offset = bank * CHR_BANK_SIZE;
        self.chr_rom[offset + (addr & 0x1FFF) as usize]
    }
}

impl Mapper for Mapper2 {
    fn read_chr(&self, addr: u16) -> u8 {
        let num_banks = (self.chr_rom.len() / CHR_BANK_SIZE).max(1);
        let bank = (self.chr_bank_select as usize) % num_banks;
        let offset = bank * CHR_BANK_SIZE;
        self.chr_rom[offset + (addr & 0x1FFF) as usize]
    }
}

// GOOD: Extract to helper struct
struct BankedRom {
    data: Vec<u8>,
    bank_size: usize,
}

impl BankedRom {
    fn read(&self, bank: usize, offset: usize) -> u8 {
        let num_banks = (self.data.len() / self.bank_size).max(1);
        let bank = bank % num_banks;
        self.data[(bank * self.bank_size) + offset]
    }
}

impl Mapper for Mapper1 {
    fn read_chr(&self, addr: u16) -> u8 {
        self.chr_rom.read(self.chr_bank_select as usize, (addr & 0x1FFF) as usize)
    }
}
```

```rust
// Do: Document trait contract clearly, especially for default methods.
pub trait Snapshot {
    /// Capture current state for persistence.
    /// 
    /// **MUST override** if internal state exceeds 8KB default window.
    /// See: https://docs.example.com/snapshot-contract
    fn snapshot(&self) -> Vec<u8> {
        // Default only handles standard 8KB WRAM
        let mut data = Vec::new();
        for i in 0..0x2000 {
            data.push(self.read_wram(0x6000 + i));
        }
        data
    }
}

// Don't: Leave it ambiguous which implementers must override.
pub trait Snapshot {
    fn snapshot(&self) -> Vec<u8> { ... }  // Unclear when override is needed
}
```

```rust
// Do: Use consistent naming for the same concept across trait implementations.
pub trait MemoryDevice {
    fn read(&self, addr: u16) -> u8;
}

struct Mapper1 {
    chr_memory: ChrMemory,  // Consistent name
}

struct Mapper2 {
    chr_memory: ChrMemory,  // Consistent name
}

// Don't: Mix naming conventions.
struct Mapper1 {
    chr_rom: Vec<u8>,  // Sometimes called chr_rom
}

struct Mapper2 {
    chr_memory: ChrMemory,  // Sometimes called chr_memory
}
```
