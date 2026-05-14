#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ObjFetchModel {
    Dmg,
    CgbDmgCompat,
}

impl ObjFetchModel {
    pub(super) fn for_dmg_render_path(cgb_mode: bool, dmg_compat: bool) -> Option<Self> {
        if !cgb_mode {
            Some(Self::Dmg)
        } else if dmg_compat {
            Some(Self::CgbDmgCompat)
        } else {
            None
        }
    }

    pub(super) fn ignores_lcdc_obj_enable(self) -> bool {
        matches!(self, Self::CgbDmgCompat)
    }
}
