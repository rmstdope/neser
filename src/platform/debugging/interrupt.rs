//! Unified interrupt types for debugger operations.
//!
//! This module provides interrupt type definitions used by the debugger
//! for run-to-interrupt and step-out operations. Each platform has its own
//! interrupt variants.

use std::fmt;

/// NES interrupt types used for debugger run-to-interrupt operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NesInterruptKind {
    /// Non-maskable interrupt (triggered by PPU at start of VBlank).
    Nmi,
    /// Maskable interrupt (triggered by mapper or APU).
    Irq,
    /// Software interrupt (BRK instruction).
    Brk,
    /// Reset interrupt.
    Reset,
}

impl fmt::Display for NesInterruptKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NesInterruptKind::Nmi => write!(f, "NMI"),
            NesInterruptKind::Irq => write!(f, "IRQ"),
            NesInterruptKind::Brk => write!(f, "BRK"),
            NesInterruptKind::Reset => write!(f, "Reset"),
        }
    }
}

impl NesInterruptKind {
    /// Parse interrupt kind from string.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "nmi" => Some(NesInterruptKind::Nmi),
            "irq" => Some(NesInterruptKind::Irq),
            "brk" => Some(NesInterruptKind::Brk),
            "reset" => Some(NesInterruptKind::Reset),
            _ => None,
        }
    }

    /// Get the interrupt vector address.
    pub fn vector_address(self) -> u16 {
        match self {
            NesInterruptKind::Nmi => 0xFFFA,
            NesInterruptKind::Reset => 0xFFFC,
            NesInterruptKind::Irq | NesInterruptKind::Brk => 0xFFFE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nes_interrupt_parse() {
        assert_eq!(NesInterruptKind::parse("nmi"), Some(NesInterruptKind::Nmi));
        assert_eq!(NesInterruptKind::parse("NMI"), Some(NesInterruptKind::Nmi));
        assert_eq!(NesInterruptKind::parse("irq"), Some(NesInterruptKind::Irq));
        assert_eq!(NesInterruptKind::parse("brk"), Some(NesInterruptKind::Brk));
        assert_eq!(
            NesInterruptKind::parse("reset"),
            Some(NesInterruptKind::Reset)
        );
        assert_eq!(NesInterruptKind::parse("invalid"), None);
    }

    #[test]
    fn test_nes_interrupt_vector_address() {
        assert_eq!(NesInterruptKind::Nmi.vector_address(), 0xFFFA);
        assert_eq!(NesInterruptKind::Reset.vector_address(), 0xFFFC);
        assert_eq!(NesInterruptKind::Irq.vector_address(), 0xFFFE);
        assert_eq!(NesInterruptKind::Brk.vector_address(), 0xFFFE);
    }
}
