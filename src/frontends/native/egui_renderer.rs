#![allow(dead_code)] // Incremental egui backend seam; wired into the emulator renderer next.

use crate::frontends::native::input::EguiInputState;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EguiFrameInput {
    logical_size: [f32; 2],
    drawable_size: [u32; 2],
    pixels_per_point: f32,
    predicted_dt: f32,
}

impl EguiFrameInput {
    pub(crate) fn new(
        logical_size: [f32; 2],
        drawable_size: [u32; 2],
        pixels_per_point: f32,
        predicted_dt: f32,
    ) -> Self {
        Self {
            logical_size,
            drawable_size,
            pixels_per_point,
            predicted_dt,
        }
    }

    pub(crate) fn to_raw_input(self, input_state: &mut EguiInputState) -> egui::RawInput {
        let screen_rect = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(self.logical_size[0], self.logical_size[1]),
        );
        let mut raw_input = egui::RawInput {
            screen_rect: Some(screen_rect),
            predicted_dt: self.predicted_dt,
            events: input_state.take_events(),
            ..Default::default()
        };
        if let Some(viewport) = raw_input.viewports.get_mut(&raw_input.viewport_id) {
            viewport.native_pixels_per_point = Some(self.pixels_per_point);
            viewport.inner_rect = Some(screen_rect);
            viewport.focused = Some(raw_input.focused);
        }
        raw_input
    }

    pub(crate) fn drawable_size(self) -> [u32; 2] {
        self.drawable_size
    }

    pub(crate) fn pixels_per_point(self) -> f32 {
        self.pixels_per_point
    }
}

pub(crate) struct EguiPaintData {
    drawable_size: [u32; 2],
    pixels_per_point: f32,
    clipped_primitives: Vec<egui::ClippedPrimitive>,
    textures_delta: egui::TexturesDelta,
}

impl EguiPaintData {
    fn new(
        drawable_size: [u32; 2],
        pixels_per_point: f32,
        clipped_primitives: Vec<egui::ClippedPrimitive>,
        textures_delta: egui::TexturesDelta,
    ) -> Self {
        Self {
            drawable_size,
            pixels_per_point,
            clipped_primitives,
            textures_delta,
        }
    }

    pub(crate) fn drawable_size(&self) -> [u32; 2] {
        self.drawable_size
    }

    pub(crate) fn pixels_per_point(&self) -> f32 {
        self.pixels_per_point
    }

    pub(crate) fn clipped_primitives(&self) -> &[egui::ClippedPrimitive] {
        &self.clipped_primitives
    }
}

pub(crate) struct EguiFrameRunner {
    context: egui::Context,
}

impl EguiFrameRunner {
    pub(crate) fn new() -> Self {
        let context = egui::Context::default();
        context.set_fonts(crate::frontends::native::egui_theme::native_font_definitions());
        context.set_visuals(crate::frontends::native::egui_theme::native_dark_visuals());
        Self { context }
    }

    pub(crate) fn run(
        &self,
        frame_input: EguiFrameInput,
        input_state: &mut EguiInputState,
        run_ui: impl FnMut(&mut egui::Ui),
    ) -> EguiPaintData {
        let output = self
            .context
            .run_ui(frame_input.to_raw_input(input_state), run_ui);
        let pixels_per_point = output.pixels_per_point;
        let clipped_primitives = self.context.tessellate(output.shapes, pixels_per_point);
        EguiPaintData::new(
            frame_input.drawable_size(),
            pixels_per_point,
            clipped_primitives,
            output.textures_delta,
        )
    }

    pub(crate) fn context(&self) -> &egui::Context {
        &self.context
    }
}

pub(crate) struct NativeEguiRenderer {
    frame_runner: EguiFrameRunner,
    painter: egui_glow::Painter,
}

impl NativeEguiRenderer {
    pub(crate) fn new(glow_context: Arc<egui_glow::glow::Context>) -> Result<Self, String> {
        let painter = egui_glow::Painter::new(glow_context, "", None, true)
            .map_err(|err| format!("Failed to initialise egui glow painter: {err}"))?;
        Ok(Self {
            frame_runner: EguiFrameRunner::new(),
            painter,
        })
    }

    pub(crate) fn run(
        &self,
        frame_input: EguiFrameInput,
        input_state: &mut EguiInputState,
        run_ui: impl FnMut(&mut egui::Ui),
    ) -> EguiPaintData {
        self.frame_runner.run(frame_input, input_state, run_ui)
    }

    pub(crate) fn paint(&mut self, paint_data: EguiPaintData) {
        self.painter.paint_and_update_textures(
            paint_data.drawable_size,
            paint_data.pixels_per_point,
            &paint_data.clipped_primitives,
            &paint_data.textures_delta,
        );
    }

    pub(crate) fn context(&self) -> &egui::Context {
        self.frame_runner.context()
    }

    pub(crate) fn register_native_texture(
        &mut self,
        texture: egui_glow::glow::Texture,
    ) -> egui::TextureId {
        self.painter.register_native_texture(texture)
    }

    pub(crate) fn free_texture(&mut self, texture_id: egui::TextureId) {
        self.painter.free_texture(texture_id);
    }

    /// Releases egui GL resources. Must be called while the owning GL context is current.
    pub(crate) fn destroy(&mut self) {
        self.painter.destroy();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontends::native::input::{InputEvent, UiKey};

    #[test]
    fn raw_input_uses_logical_screen_rect() {
        let mut input_state = EguiInputState::default();
        let frame_input = EguiFrameInput::new([320.0, 240.0], [640, 480], 2.0, 1.0 / 60.0);

        let raw_input = frame_input.to_raw_input(&mut input_state);

        assert_eq!(
            raw_input.screen_rect,
            Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 240.0)
            ))
        );
    }

    #[test]
    fn raw_input_records_native_scale_on_root_viewport() {
        let mut input_state = EguiInputState::default();
        let frame_input = EguiFrameInput::new([320.0, 240.0], [640, 480], 2.0, 1.0 / 60.0);

        let raw_input = frame_input.to_raw_input(&mut input_state);

        assert_eq!(raw_input.viewport().native_pixels_per_point, Some(2.0));
        assert_eq!(raw_input.viewport().inner_rect, raw_input.screen_rect);
        assert_eq!(raw_input.viewport().focused, Some(true));
    }

    #[test]
    fn raw_input_drains_queued_events() {
        let mut input_state = EguiInputState::default();
        input_state.apply_input(&InputEvent::Key {
            key: UiKey::F5,
            down: true,
        });
        let frame_input = EguiFrameInput::new([320.0, 240.0], [320, 240], 1.0, 1.0 / 60.0);

        let raw_input = frame_input.to_raw_input(&mut input_state);

        assert_eq!(raw_input.events.len(), 1);
        assert!(input_state.take_events().is_empty());
    }

    #[test]
    fn frame_input_keeps_drawable_size_and_scale() {
        let frame_input = EguiFrameInput::new([320.0, 240.0], [640, 480], 2.0, 1.0 / 60.0);

        assert_eq!(frame_input.drawable_size(), [640, 480]);
        assert_eq!(frame_input.pixels_per_point(), 2.0);
    }

    #[test]
    fn frame_runner_runs_ui_and_preserves_paint_target() {
        let runner = EguiFrameRunner::new();
        let mut input_state = EguiInputState::default();
        let frame_input = EguiFrameInput::new([320.0, 240.0], [640, 480], 2.0, 1.0 / 60.0);
        let mut ui_ran = false;

        let paint_data = runner.run(frame_input, &mut input_state, |ui| {
            ui.label("Frame");
            ui_ran = true;
        });

        assert!(ui_ran);
        assert_eq!(paint_data.drawable_size(), [640, 480]);
        assert_eq!(paint_data.pixels_per_point(), 2.0);
        assert!(!paint_data.clipped_primitives().is_empty());
    }
}
