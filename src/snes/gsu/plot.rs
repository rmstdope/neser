//! The GSU bitmap unit: COLOR/GETC into COLR, CMODE into POR, PLOT and RPIX.

use super::Gsu;

impl Gsu {
    /// The value COLOR/GETC load into COLR for an incoming byte.
    pub(super) fn color_register_input(&self, value: u8) -> u8 {
        value
    }
}
