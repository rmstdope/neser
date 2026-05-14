#[cfg(test)]
const LCDC_OBJ_ENABLE: u8 = 0x02;

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

    #[cfg(test)]
    pub(super) fn object_fetch_allowed(self, lcdc: u8) -> bool {
        match self {
            Self::Dmg => lcdc & LCDC_OBJ_ENABLE != 0,
            Self::CgbDmgCompat => true,
        }
    }
}

#[cfg(test)]
#[derive(Debug, Default, Clone)]
pub(super) struct ObjFetcher {
    fetch_active: bool,
    fetch_canceled: bool,
}

#[cfg(test)]
impl ObjFetcher {
    pub(super) fn begin_fetch(&mut self) {
        self.fetch_active = true;
        self.fetch_canceled = false;
    }

    pub(super) fn record_lcdc_write(&mut self, model: ObjFetchModel, previous: u8, new: u8) {
        let obj_turning_off = previous & LCDC_OBJ_ENABLE != 0 && new & LCDC_OBJ_ENABLE == 0;
        if model == ObjFetchModel::Dmg && self.fetch_active && obj_turning_off {
            self.fetch_active = false;
            self.fetch_canceled = true;
        }
    }

    pub(super) fn is_fetch_canceled(&self) -> bool {
        self.fetch_canceled
    }
}

#[cfg(test)]
mod tests {
    use super::{ObjFetchModel, ObjFetcher};

    #[test]
    fn cgb_dmg_compat_object_fetch_still_starts_when_lcdc_obj_enable_is_clear() {
        let model = ObjFetchModel::CgbDmgCompat;

        assert!(model.object_fetch_allowed(0x81));
    }

    #[test]
    fn dmg_object_fetch_is_canceled_when_lcdc_obj_enable_turns_off_mid_fetch() {
        let mut fetcher = ObjFetcher::default();
        fetcher.begin_fetch();

        fetcher.record_lcdc_write(ObjFetchModel::Dmg, 0x83, 0x81);

        assert!(fetcher.is_fetch_canceled());
    }
}
