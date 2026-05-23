//! Shared egui font and theme setup for native UI surfaces.

const NUNITO_REGULAR: &str = "nunito_regular";
const NUNITO_BOLD: &str = "nunito_bold";

pub(crate) fn native_font_definitions() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        NUNITO_REGULAR.to_owned(),
        egui::FontData::from_static(include_bytes!(
            "../../../assets/fonts/NunitoSans-Regular.ttf"
        ))
        .into(),
    );
    fonts.font_data.insert(
        NUNITO_BOLD.to_owned(),
        egui::FontData::from_static(include_bytes!("../../../assets/fonts/NunitoSans-Bold.ttf"))
            .into(),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, NUNITO_REGULAR.to_owned());
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, NUNITO_BOLD.to_owned());
    fonts
}

pub(crate) fn native_dark_visuals() -> egui::Visuals {
    egui::Visuals {
        dark_mode: true,
        window_fill: egui::Color32::from_rgb(15, 15, 20),
        panel_fill: egui::Color32::from_rgb(15, 15, 20),
        extreme_bg_color: egui::Color32::from_rgb(10, 10, 15),
        faint_bg_color: egui::Color32::from_rgb(20, 20, 30),
        ..egui::Visuals::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_font_definitions_register_nunito_faces() {
        // Given shared native egui font definitions.
        let fonts = native_font_definitions();

        // Then both bundled Nunito faces are available by stable names.
        assert!(fonts.font_data.contains_key(NUNITO_REGULAR));
        assert!(fonts.font_data.contains_key(NUNITO_BOLD));
    }

    #[test]
    fn native_font_definitions_prefer_nunito_families() {
        // Given shared native egui font definitions.
        let fonts = native_font_definitions();

        // Then proportional body text uses Nunito Regular first.
        let proportional = fonts
            .families
            .get(&egui::FontFamily::Proportional)
            .expect("proportional family should exist");
        assert_eq!(
            proportional.first().map(String::as_str),
            Some(NUNITO_REGULAR)
        );

        // And monospace-style headings use Nunito Bold first.
        let monospace = fonts
            .families
            .get(&egui::FontFamily::Monospace)
            .expect("monospace family should exist");
        assert_eq!(monospace.first().map(String::as_str), Some(NUNITO_BOLD));
    }

    #[test]
    fn native_dark_visuals_use_browser_background_palette() {
        // Given shared native egui dark visuals.
        let visuals = native_dark_visuals();

        // Then the base colors match the existing ROM browser palette.
        assert!(visuals.dark_mode);
        assert_eq!(visuals.window_fill, egui::Color32::from_rgb(15, 15, 20));
        assert_eq!(visuals.panel_fill, egui::Color32::from_rgb(15, 15, 20));
        assert_eq!(
            visuals.extreme_bg_color,
            egui::Color32::from_rgb(10, 10, 15)
        );
        assert_eq!(visuals.faint_bg_color, egui::Color32::from_rgb(20, 20, 30));
    }
}
