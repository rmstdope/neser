#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeTexture {
    pub egui_id: egui::TextureId,
    pub width: u32,
    pub height: u32,
}

impl NativeTexture {
    pub(crate) fn size(self) -> [u32; 2] {
        [self.width, self.height]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_texture_size_returns_width_and_height() {
        // Given a registered egui native texture with dimensions.
        let texture = NativeTexture {
            egui_id: egui::TextureId::User(7),
            width: 256,
            height: 128,
        };

        // When reading its size.
        let size = texture.size();

        // Then it returns width and height without exposing renderer-specific fields.
        assert_eq!(size, [256, 128]);
    }
}
