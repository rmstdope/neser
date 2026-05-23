use std::num::NonZeroU32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeTexture {
    pub egui_id: egui::TextureId,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeTextureName(NonZeroU32);

impl NativeTextureName {
    pub(crate) fn from_gl_id(gl_id: u32) -> Option<Self> {
        NonZeroU32::new(gl_id).map(Self)
    }

    pub(crate) fn egui_glow_texture(self) -> egui_glow::glow::Texture {
        egui_glow::glow::NativeTexture(self.0)
    }
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

    #[test]
    fn native_texture_name_rejects_zero_gl_id() {
        assert_eq!(NativeTextureName::from_gl_id(0), None);
    }

    #[test]
    fn native_texture_name_wraps_egui_glow_texture() {
        let texture_name = NativeTextureName::from_gl_id(42).expect("non-zero texture id");

        assert_eq!(
            texture_name.egui_glow_texture(),
            egui_glow::glow::NativeTexture(NonZeroU32::new(42).expect("non-zero texture id"))
        );
    }
}
