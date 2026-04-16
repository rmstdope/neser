/// Game Boy hardware model variant.
///
/// Distinguishes between the first-generation DMG-0 and the
/// production DMG-A/B/C models.  The two variants differ in
/// boot ROM content, post-boot CPU register values, and the
/// DIV counter phase at boot exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DmgModel {
    /// Production DMG hardware: DMG-A, DMG-B, DMG-C.
    ///
    /// Post-boot CPU registers: A=$01 F=$B0 B=$00 C=$13 D=$00 E=$D8 H=$01 L=$4D SP=$FFFE.
    /// DIV=$AD at cartridge entry.
    #[default]
    DmgAbc,

    /// First-generation DMG-0 hardware.
    ///
    /// Post-boot CPU registers: A=$01 F=$00 B=$FF C=$13 D=$00 E=$C1 H=$84 L=$03 SP=$FFFE.
    /// DIV=$19 at cartridge entry (shorter boot ROM).
    Dmg0,
}
