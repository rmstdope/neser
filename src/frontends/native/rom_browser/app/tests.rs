use super::*;
use crate::platform::app_context::IntoSharedAppContext;
use crate::platform::catalog::Platform;

#[test]
fn browser_result_default_is_closed() {
    let result = BrowserResult::Closed;
    assert!(matches!(result, BrowserResult::Closed));
}

#[test]
fn browser_result_rom_selected_holds_path() {
    let path = PathBuf::from("/roms/game.nes");
    let result = BrowserResult::RomSelected(path.clone());
    match result {
        BrowserResult::RomSelected(p) => assert_eq!(p, path),
        BrowserResult::Closed => panic!("expected RomSelected"),
    }
}

fn make_entry(name: &str) -> RomEntry {
    RomEntry {
        path: PathBuf::from(format!("{name}.nes")),
        display_name: name.to_string(),
        search_key: name.to_lowercase(),
        mapper_label: "-".to_string(),
        mapper: None,
        hardware: None,
        crc: None,
        recording_duration: None,
        metadata_game_id: None,
        genres: Vec::new(),
        overview: None,
        release_date: None,
        players: None,
        rating: None,
        boxart_path: None,
        screenshot_paths: Vec::new(),
        is_favorite: false,
        platform: Platform::Nes,
    }
}

/// Create a minimal RomBrowserApp for testing (without GL or AppContext).
fn test_browser(entries: Vec<RomEntry>) -> RomBrowserApp {
    let dir = tempfile::TempDir::new().unwrap();
    let fav_path = dir.path().join("favorites.json");
    let mut app = RomBrowserApp {
        app_context: crate::platform::app_context::AppContext::new().into_shared(),
        gl: None,
        result: BrowserResult::Closed,
        default_width: 1280,
        default_height: 720,
        fullscreen: false,
        last_render_instant: Instant::now(),
        catalog: Vec::new(),
        filtered_indices: Vec::new(),
        selected_index: 0,
        scroll_offset: 0.0,
        scroll_target: 0.0,
        search_active: false,
        search_query: String::new(),
        search_anim: 0.0,
        search_kb_row: 1,
        search_kb_col: 0,
        available_genres: Vec::new(),
        active_genres: Vec::new(),
        detail_view_active: false,
        detail_screenshot_index: 0,
        detail_scroll_last_advance: Instant::now(),
        detail_scroll_forward: true,
        filter_panel_active: false,
        filter_panel_anim: 0.0,
        filter_panel_cursor: 0,
        filter_panel_column: 0,
        active_platform: None,
        min_players_filter: None,
        favorites: Favorites::load(&fav_path),
        show_favorites_only: false,
        catalog_state: CatalogState::Ready,
        modifiers: winit::keyboard::ModifiersState::empty(),
        gilrs: None, // No gamepad in tests
        gamepad_axis: GamepadAxisState::default(),
        gamepad_repeat: GamepadRepeatState::default(),
        texture_request_tx: {
            let (tx, _rx) = mpsc::channel();
            tx
        },
        texture_result_rx: {
            let (_tx, rx) = mpsc::channel();
            rx
        },
        texture_pending: Vec::new(),
        boxart_by_game_id: std::collections::HashMap::new(),
        no_roms_hint: RomBrowserApp::no_roms_hint(&[]),
    };
    app.set_catalog(entries);
    app
}

#[test]
fn rebuild_filtered_shows_all_when_no_query() {
    let app = test_browser(vec![
        make_entry("Super Mario Bros"),
        make_entry("Zelda"),
        make_entry("Metroid"),
    ]);
    assert_eq!(app.filtered_indices.len(), 3);
    assert_eq!(app.filtered_indices, vec![0, 1, 2]);
}

#[test]
fn rebuild_filtered_narrows_by_search() {
    let mut app = test_browser(vec![
        make_entry("Super Mario Bros"),
        make_entry("Zelda"),
        make_entry("Super Metroid"),
    ]);
    app.search_query = "super".to_string();
    app.rebuild_filtered();
    assert_eq!(app.filtered_indices.len(), 2);
    assert_eq!(app.filtered_indices, vec![0, 2]);
}

#[test]
fn rebuild_filtered_empty_result() {
    let mut app = test_browser(vec![make_entry("Mario"), make_entry("Zelda")]);
    app.search_query = "castlevania".to_string();
    app.rebuild_filtered();
    assert!(app.filtered_indices.is_empty());
}

#[test]
fn no_roms_hint_defaults_to_home_roms_dir_when_no_search_paths() {
    let hint = RomBrowserApp::no_roms_hint(&[]);
    assert_eq!(hint, "No ROMs found. Add ROM files to ~/.neser/roms/");
}

#[test]
fn no_roms_hint_lists_single_configured_search_path() {
    let paths = vec!["/mnt/roms".to_string()];
    let hint = RomBrowserApp::no_roms_hint(&paths);
    assert_eq!(hint, "No ROMs found. Add ROM files to /mnt/roms");
}

#[test]
fn no_roms_hint_lists_multiple_configured_search_paths() {
    let paths = vec!["/mnt/roms".to_string(), "/home/user/games".to_string()];
    let hint = RomBrowserApp::no_roms_hint(&paths);
    assert_eq!(
        hint,
        "No ROMs found. Add ROM files to /mnt/roms, /home/user/games"
    );
}

#[test]
fn selected_entry_returns_correct_rom() {
    let app = test_browser(vec![
        make_entry("Game A"),
        make_entry("Game B"),
        make_entry("Game C"),
    ]);
    assert_eq!(app.selected_entry().unwrap().display_name, "Game A");
}

#[test]
fn selected_entry_after_filter_returns_filtered_item() {
    let mut app = test_browser(vec![
        make_entry("Alpha"),
        make_entry("Beta"),
        make_entry("Gamma"),
    ]);
    app.search_query = "beta".to_string();
    app.rebuild_filtered();
    // selected_index 0 in filtered = "Beta" (catalog index 1).
    assert_eq!(app.selected_entry().unwrap().display_name, "Beta");
}

#[test]
fn selection_clamps_after_filter_reduces_results() {
    let mut app = test_browser(vec![make_entry("A"), make_entry("B"), make_entry("C")]);
    app.selected_index = 2; // last item
    app.search_query = "a".to_string();
    app.rebuild_filtered();
    // Only 1 match, selection should clamp to 0.
    assert_eq!(app.selected_index, 0);
    assert_eq!(app.selected_entry().unwrap().display_name, "A");
}

fn make_entry_with_genres(name: &str, genres: Vec<&str>) -> RomEntry {
    let mut entry = make_entry(name);
    entry.genres = genres.into_iter().map(String::from).collect();
    entry
}

#[test]
fn set_catalog_collects_available_genres() {
    let app = test_browser(vec![
        make_entry_with_genres("Mario", vec!["Platform", "Action"]),
        make_entry_with_genres("Zelda", vec!["Adventure", "Action"]),
        make_entry_with_genres("Tetris", vec!["Puzzle"]),
    ]);
    assert_eq!(
        app.available_genres,
        vec!["Action", "Adventure", "Platform", "Puzzle"]
    );
}

#[test]
fn genre_filter_narrows_results() {
    let mut app = test_browser(vec![
        make_entry_with_genres("Mario", vec!["Platform"]),
        make_entry_with_genres("Zelda", vec!["Adventure"]),
        make_entry_with_genres("Contra", vec!["Platform", "Shooter"]),
    ]);
    app.active_genres = vec!["Platform".to_string()];
    app.rebuild_filtered();
    assert_eq!(app.filtered_indices.len(), 2);
    assert_eq!(app.filtered_indices, vec![0, 2]);
}

#[test]
fn genre_and_search_combined() {
    let mut app = test_browser(vec![
        make_entry_with_genres("Super Mario", vec!["Platform"]),
        make_entry_with_genres("Super Contra", vec!["Platform", "Shooter"]),
        make_entry_with_genres("Zelda", vec!["Adventure"]),
    ]);
    app.active_genres = vec!["Platform".to_string()];
    app.search_query = "super".to_string();
    app.rebuild_filtered();
    assert_eq!(app.filtered_indices.len(), 2);
    assert_eq!(app.filtered_indices, vec![0, 1]);
}

#[test]
fn detail_view_opens_when_entry_selected() {
    let mut app = test_browser(vec![make_entry("Castlevania")]);
    assert!(!app.detail_view_active);
    app.open_detail_view();
    assert!(app.detail_view_active);
    assert_eq!(app.detail_screenshot_index, 0);
}

#[test]
fn detail_view_does_not_open_when_catalog_empty() {
    let mut app = test_browser(vec![]);
    assert!(!app.detail_view_active);
    app.open_detail_view();
    assert!(!app.detail_view_active);
}

#[test]
fn detail_screenshot_auto_scroll() {
    let mut entry = make_entry("Zelda");
    entry.screenshot_paths = vec![
        PathBuf::from("s1.jpg"),
        PathBuf::from("s2.jpg"),
        PathBuf::from("s3.jpg"),
    ];
    let mut app = test_browser(vec![entry]);
    app.open_detail_view();
    assert!(app.detail_view_active);
    assert_eq!(app.detail_screenshot_index, 0);
    assert!(app.detail_scroll_forward);

    // Not enough time yet — stays at 0.
    app.advance_screenshot_auto_scroll();
    assert_eq!(app.detail_screenshot_index, 0);

    // Simulate endpoint pause (2.0s) by backdating the last advance.
    app.detail_scroll_last_advance = Instant::now() - std::time::Duration::from_secs_f64(2.1);
    app.advance_screenshot_auto_scroll();
    assert_eq!(app.detail_screenshot_index, 1);

    // Simulate mid-point pause (1.5s).
    app.detail_scroll_last_advance = Instant::now() - std::time::Duration::from_secs_f64(1.6);
    app.advance_screenshot_auto_scroll();
    assert_eq!(app.detail_screenshot_index, 2);

    // At last screenshot, reverses direction.
    app.detail_scroll_last_advance = Instant::now() - std::time::Duration::from_secs_f64(2.1);
    app.advance_screenshot_auto_scroll();
    assert_eq!(app.detail_screenshot_index, 1);
    assert!(!app.detail_scroll_forward);

    // Continue backward.
    app.detail_scroll_last_advance = Instant::now() - std::time::Duration::from_secs_f64(1.6);
    app.advance_screenshot_auto_scroll();
    assert_eq!(app.detail_screenshot_index, 0);

    // At first screenshot, reverses to forward.
    app.detail_scroll_last_advance = Instant::now() - std::time::Duration::from_secs_f64(2.1);
    app.advance_screenshot_auto_scroll();
    assert_eq!(app.detail_screenshot_index, 1);
    assert!(app.detail_scroll_forward);
}

#[test]
fn detail_screenshot_resets_on_reopen() {
    let mut entry = make_entry("Zelda");
    entry.screenshot_paths = vec![PathBuf::from("s1.jpg"), PathBuf::from("s2.jpg")];
    let mut app = test_browser(vec![entry]);
    app.open_detail_view();
    app.detail_screenshot_index = 1;
    app.detail_scroll_forward = false;

    // Close and reopen — index and direction should reset.
    app.detail_view_active = false;
    app.open_detail_view();
    assert_eq!(app.detail_screenshot_index, 0);
    assert!(app.detail_scroll_forward);
}

#[test]
fn toggle_favorite_marks_entry() {
    let mut app = test_browser(vec![make_entry("Zelda"), make_entry("Mario")]);
    assert!(!app.catalog[0].is_favorite);
    app.toggle_favorite();
    assert!(app.catalog[0].is_favorite);
    // Toggle off.
    app.toggle_favorite();
    assert!(!app.catalog[0].is_favorite);
}

#[test]
fn show_favorites_only_filters_non_favorites() {
    let mut app = test_browser(vec![
        make_entry("Zelda"),
        make_entry("Mario"),
        make_entry("Contra"),
    ]);
    // Favorite only Mario (index 1).
    app.selected_index = 1;
    app.toggle_favorite();
    // Enable favorites filter.
    app.show_favorites_only = true;
    app.rebuild_filtered();
    assert_eq!(app.filtered_indices.len(), 1);
    assert_eq!(app.catalog[app.filtered_indices[0]].display_name, "Mario");
}

#[test]
fn toggle_favorite_rebuilds_filter_when_showing_favorites() {
    let mut app = test_browser(vec![make_entry("Zelda"), make_entry("Mario")]);
    // Favorite both.
    app.toggle_favorite();
    app.selected_index = 1;
    app.toggle_favorite();
    // Enable favorites filter.
    app.show_favorites_only = true;
    app.rebuild_filtered();
    assert_eq!(app.filtered_indices.len(), 2);
    // Unfavorite Zelda (selected = 0).
    app.selected_index = 0;
    app.toggle_favorite();
    // Should auto-rebuild and now only Mario remains.
    assert_eq!(app.filtered_indices.len(), 1);
}

#[test]
fn poll_catalog_loading_receives_catalog() {
    let mut app = test_browser(vec![]);
    assert!(app.catalog.is_empty());

    let (tx, rx) = mpsc::channel();
    app.catalog_state = CatalogState::Loading {
        receiver: rx,
        progress: None,
    };

    // Before send: poll does nothing.
    app.poll_catalog_loading();
    assert!(app.catalog.is_empty());

    // Send catalog from "background thread".
    tx.send(CatalogMessage::Done(vec![
        make_entry("Zelda"),
        make_entry("Mario"),
    ]))
    .unwrap();
    app.poll_catalog_loading();

    assert_eq!(app.catalog.len(), 2);
    assert!(matches!(app.catalog_state, CatalogState::Ready));
}

#[test]
fn poll_catalog_loading_tracks_progress() {
    let mut app = test_browser(vec![]);

    let (tx, rx) = mpsc::channel();
    app.catalog_state = CatalogState::Loading {
        receiver: rx,
        progress: None,
    };

    tx.send(CatalogMessage::Progress(EnrichmentProgress {
        current: 3,
        total: 10,
        game_title: "Zelda".to_string(),
        phase: EnrichmentPhase::MatchingMetadata,
    }))
    .unwrap();
    app.poll_catalog_loading();

    if let CatalogState::Loading { ref progress, .. } = app.catalog_state {
        let p = progress.as_ref().unwrap();
        assert_eq!(p.current, 3);
        assert_eq!(p.total, 10);
        assert_eq!(p.game_title, "Zelda");
    } else {
        panic!("Expected CatalogState::Loading");
    }
}

#[test]
fn back_opens_filter_panel_instead_of_exiting() {
    let mut app = test_browser(vec![make_entry("Zelda")]);
    assert!(!app.filter_panel_active);

    app.open_filter_panel();
    assert!(app.filter_panel_active);
    assert_eq!(app.filter_panel_cursor, 0);
    assert_eq!(app.filter_panel_column, 0);
}

#[test]
fn back_closes_filter_panel() {
    let mut app = test_browser(vec![make_entry("Zelda")]);
    app.filter_panel_active = true;

    app.close_filter_panel();
    assert!(!app.filter_panel_active);
}

#[test]
fn platform_filter_narrows_results() {
    let mut nes = make_entry("Zelda");
    nes.platform = Platform::Nes;
    let mut gb = make_entry("Pokemon");
    gb.platform = Platform::Gb;

    let mut app = test_browser(vec![nes, gb]);
    assert_eq!(app.filtered_indices.len(), 2);

    app.active_platform = Some(Platform::Nes);
    app.rebuild_filtered();
    assert_eq!(app.filtered_indices.len(), 1);
    assert_eq!(app.catalog[app.filtered_indices[0]].display_name, "Zelda");

    app.active_platform = Some(Platform::Gb);
    app.rebuild_filtered();
    assert_eq!(app.filtered_indices.len(), 1);
    assert_eq!(app.catalog[app.filtered_indices[0]].display_name, "Pokemon");

    app.active_platform = None;
    app.rebuild_filtered();
    assert_eq!(app.filtered_indices.len(), 2);
}

#[test]
fn legend_shows_keyboard_keys_in_gallery_without_controller() {
    let items = RomBrowserApp::legend_items(false, false, false, false);
    assert_eq!(
        items,
        [
            ("Enter", "Details"),
            ("Esc", "Filter"),
            ("Space", "Favorite"),
            ("Tab", "Search"),
        ]
    );
}

#[test]
fn legend_shows_controller_buttons_in_gallery_with_controller() {
    // Search opens with the Start button on a controller, not Tab.
    let items = RomBrowserApp::legend_items(false, false, false, true);
    assert_eq!(
        items,
        [
            ("A", "Details"),
            ("B", "Filter"),
            ("Select", "Favorite"),
            ("Start", "Search"),
        ]
    );
}

#[test]
fn legend_shows_keyboard_keys_in_search_without_controller() {
    let items = RomBrowserApp::legend_items(true, false, false, false);
    assert_eq!(
        items,
        [("Tab", "Close"), ("Enter", "Select"), ("Type", "Search")]
    );
}

#[test]
fn legend_shows_controller_buttons_in_search_with_controller() {
    // Start opened the search view, so Start closes it — not B.
    let items = RomBrowserApp::legend_items(true, false, false, true);
    assert_eq!(items, [("A", "Select"), ("Start", "Close")]);
}

#[test]
fn search_action_closes_search_but_back_does_not() {
    let mut app = test_browser(vec![make_entry("Zelda")]);
    app.search_active = true;

    app.apply_search_action(BrowserAction::Back);
    assert!(app.search_active, "B must not close the search view");

    app.apply_search_action(BrowserAction::Search);
    assert!(!app.search_active, "Start must close the search view");
}

#[test]
fn button_pill_colors_follow_snes_controller_scheme() {
    use crate::frontends::native::rom_browser::theme;
    // SNES controller: A red, B yellow, X blue, Y green.
    let a = theme::BUTTON_COLOR_A;
    assert!(a.r() > a.g() && a.r() > a.b(), "A must be red");
    let b = theme::BUTTON_COLOR_B;
    assert!(b.r() > b.b() && b.g() > b.b(), "B must be yellow");
    let x = theme::BUTTON_COLOR_X;
    assert!(x.b() > x.r() && x.b() > x.g(), "X must be blue");
    let y = theme::BUTTON_COLOR_Y;
    assert!(y.g() > y.r() && y.g() > y.b(), "Y must be green");
}

#[test]
fn legend_shows_controller_buttons_in_filter_panel_with_controller() {
    let items = RomBrowserApp::legend_items(false, true, false, true);
    assert_eq!(items, [("↑↓", "Navigate"), ("A", "Toggle"), ("B", "Close")]);
}

#[test]
fn legend_shows_keyboard_keys_in_filter_panel_without_controller() {
    let items = RomBrowserApp::legend_items(false, true, false, false);
    assert_eq!(
        items,
        [("↑↓", "Navigate"), ("Enter", "Toggle"), ("Esc", "Close")]
    );
}

#[test]
fn legend_shows_keyboard_keys_in_detail_view_without_controller() {
    let items = RomBrowserApp::legend_items(false, false, true, false);
    assert_eq!(
        items,
        [("Enter", "Launch"), ("Space", "Fav"), ("Esc", "Back")]
    );
}

#[test]
fn legend_shows_controller_buttons_in_detail_view_with_controller() {
    let items = RomBrowserApp::legend_items(false, false, true, true);
    assert_eq!(items, [("A", "Launch"), ("Y", "Fav"), ("B", "Back")]);
}

#[test]
fn keyboard_key_pills_are_white_for_visibility() {
    // Keyboard keys have no gamepad button color and fell back to a dark
    // grey that was near-invisible on the grey legend background.
    for key in ["Enter", "Esc", "Space", "Tab", "F", "↑↓"] {
        assert_eq!(
            RomBrowserApp::button_pill_color(key),
            egui::Color32::WHITE,
            "pill for {key} must be white on the grey legend background"
        );
    }
    // Gamepad buttons keep their dedicated colors.
    assert_eq!(
        RomBrowserApp::button_pill_color("A"),
        crate::frontends::native::rom_browser::theme::BUTTON_COLOR_A
    );
}

#[test]
fn new_browser_defaults_to_no_platform_filter() {
    let app = RomBrowserApp::new(crate::platform::app_context::AppContext::new().into_shared());
    assert_eq!(
        app.active_platform, None,
        "browser must start unfiltered, showing all platforms"
    );
}

#[test]
fn platform_filter_gba_and_snes_narrow_results() {
    let mut nes = make_entry("Zelda");
    nes.platform = Platform::Nes;
    let mut gba = make_entry("Golden Sun");
    gba.platform = Platform::Gba;
    let mut snes = make_entry("Super Metroid");
    snes.platform = Platform::Snes;

    let mut app = test_browser(vec![nes, gba, snes]);
    assert_eq!(app.filtered_indices.len(), 3);

    app.active_platform = Some(Platform::Gba);
    app.rebuild_filtered();
    assert_eq!(app.filtered_indices.len(), 1);
    assert_eq!(
        app.catalog[app.filtered_indices[0]].display_name,
        "Golden Sun"
    );

    app.active_platform = Some(Platform::Snes);
    app.rebuild_filtered();
    assert_eq!(app.filtered_indices.len(), 1);
    assert_eq!(
        app.catalog[app.filtered_indices[0]].display_name,
        "Super Metroid"
    );
}

#[test]
fn filter_panel_offers_all_supported_platforms() {
    assert_eq!(
        RomBrowserApp::PLATFORMS,
        [
            Platform::Nes,
            Platform::Gb,
            Platform::Gbc,
            Platform::Gba,
            Platform::Snes
        ]
    );
}

#[test]
fn filter_panel_favorites_column_toggles_favorites_only() {
    let mut fav = make_entry("Zelda");
    fav.is_favorite = true;
    let mut app = test_browser(vec![make_entry("Mario"), make_entry("Metroid")]);
    app.catalog.push(fav);
    app.catalog[2].is_favorite = true;
    app.rebuild_filtered();
    assert_eq!(app.filtered_indices.len(), 3);

    app.filter_panel_active = true;
    app.filter_panel_column = 3;
    app.filter_panel_cursor = 0;
    app.filter_panel_confirm();
    assert!(app.show_favorites_only);
    assert_eq!(app.filtered_indices.len(), 1);

    app.filter_panel_confirm();
    assert!(!app.show_favorites_only);
    assert_eq!(app.filtered_indices.len(), 3);
}

#[test]
fn filter_panel_move_right_reaches_favorites_column() {
    let mut app = test_browser(vec![make_entry("Zelda")]);
    app.available_genres = vec!["Action".to_string()];
    app.filter_panel_active = true;
    app.filter_panel_column = 2;
    app.filter_panel_cursor = 0;

    app.filter_panel_move_right();
    assert_eq!(app.filter_panel_column, 3);
    assert_eq!(app.filter_panel_cursor, 0);

    // Favorites is the last column.
    app.filter_panel_move_right();
    assert_eq!(app.filter_panel_column, 3);
}

#[test]
fn filter_panel_cursor_bounded_within_column() {
    let mut app = test_browser(vec![make_entry("Zelda")]);
    app.available_genres = vec!["Action".to_string(), "RPG".to_string()];
    app.filter_panel_active = true;

    // Platform column has 5 items (NES, GB, GBC, GBA, SNES)
    app.filter_panel_column = 0;
    app.filter_panel_cursor = 0;
    for _ in 0..4 {
        app.filter_panel_move_cursor_down();
    }
    assert_eq!(app.filter_panel_cursor, 4); // last platform
    app.filter_panel_move_cursor_down();
    assert_eq!(app.filter_panel_cursor, 4); // bounded

    // Genre column has 2 items (now column 2)
    app.filter_panel_column = 2;
    app.filter_panel_cursor = 0;
    app.filter_panel_move_cursor_down();
    assert_eq!(app.filter_panel_cursor, 1);
    app.filter_panel_move_cursor_down();
    assert_eq!(app.filter_panel_cursor, 1); // bounded
}

#[test]
fn filter_panel_confirm_toggles_platform() {
    let mut nes = make_entry("Zelda");
    nes.platform = Platform::Nes;
    let mut gb = make_entry("Pokemon");
    gb.platform = Platform::Gb;

    let mut app = test_browser(vec![nes, gb]);
    app.filter_panel_active = true;
    app.filter_panel_column = 0;
    assert_eq!(app.active_platform, None);

    // Cursor 0 = NES platform
    app.filter_panel_cursor = 0;
    app.filter_panel_confirm();
    assert_eq!(app.active_platform, Some(Platform::Nes));
    assert_eq!(app.filtered_indices.len(), 1);

    // Confirm same platform again deselects it.
    app.filter_panel_confirm();
    assert_eq!(app.active_platform, None);
    assert_eq!(app.filtered_indices.len(), 2);

    // Cursor 1 = GB platform
    app.filter_panel_cursor = 1;
    app.filter_panel_confirm();
    assert_eq!(app.active_platform, Some(Platform::Gb));
    assert_eq!(app.filtered_indices.len(), 1);

    // Deselect GB
    app.filter_panel_confirm();
    assert_eq!(app.active_platform, None);

    // Cursor 2 = GBC platform
    app.filter_panel_cursor = 2;
    app.filter_panel_confirm();
    assert_eq!(app.active_platform, Some(Platform::Gbc));
    assert_eq!(app.filtered_indices.len(), 0);
}

#[test]
fn filter_panel_confirm_toggles_genre() {
    let mut entry = make_entry("Zelda");
    entry.genres = vec!["Action".to_string(), "RPG".to_string()];

    let mut app = test_browser(vec![entry, make_entry("Mario")]);
    app.available_genres = vec!["Action".to_string(), "RPG".to_string()];
    app.filter_panel_active = true;

    // Column 2 = Genre, cursor 0 = first genre ("Action")
    app.filter_panel_column = 2;
    app.filter_panel_cursor = 0;
    app.filter_panel_confirm();
    assert!(app.active_genres.contains(&"Action".to_string()));
    assert_eq!(app.filtered_indices.len(), 1);

    // Toggle off
    app.filter_panel_confirm();
    assert!(!app.active_genres.contains(&"Action".to_string()));
    assert_eq!(app.filtered_indices.len(), 2);
}

#[test]
fn filter_panel_move_cursor_up_down() {
    let mut app = test_browser(vec![make_entry("Zelda")]);
    app.available_genres = vec!["Action".to_string(), "RPG".to_string()];
    app.filter_panel_active = true;
    app.filter_panel_column = 0;
    app.filter_panel_cursor = 0;

    app.filter_panel_move_cursor_down();
    assert_eq!(app.filter_panel_cursor, 1);

    app.filter_panel_move_cursor_down();
    assert_eq!(app.filter_panel_cursor, 2);

    app.filter_panel_move_cursor_up();
    assert_eq!(app.filter_panel_cursor, 1);

    // Should not go below 0
    app.filter_panel_cursor = 0;
    app.filter_panel_move_cursor_up();
    assert_eq!(app.filter_panel_cursor, 0);

    // Should not go above max (4 for platforms: NES, GB, GBC, GBA, SNES)
    app.filter_panel_cursor = 4;
    app.filter_panel_move_cursor_down();
    assert_eq!(app.filter_panel_cursor, 4);
}

#[test]
fn filter_panel_left_right_switches_column() {
    let mut app = test_browser(vec![make_entry("Zelda")]);
    app.available_genres = vec!["Action".to_string(), "RPG".to_string()];
    app.filter_panel_active = true;
    app.filter_panel_column = 0;
    app.filter_panel_cursor = 1;

    // Move right to players column
    app.filter_panel_move_right();
    assert_eq!(app.filter_panel_column, 1);
    assert_eq!(app.filter_panel_cursor, 1); // preserved

    // Move left back to platform column
    app.filter_panel_move_left();
    assert_eq!(app.filter_panel_column, 0);
    assert_eq!(app.filter_panel_cursor, 1); // preserved

    // Can't go left from platform column
    app.filter_panel_move_left();
    assert_eq!(app.filter_panel_column, 0);

    // Can go right to genre column (column 2)
    app.filter_panel_column = 1;
    app.filter_panel_move_right();
    assert_eq!(app.filter_panel_column, 2);

    // Can go right to the favorites column (column 3), which is the last.
    app.filter_panel_move_right();
    assert_eq!(app.filter_panel_column, 3);
    app.filter_panel_move_right();
    assert_eq!(app.filter_panel_column, 3);
}

#[test]
fn filter_panel_player_filter_narrows_results() {
    let mut entry1 = make_entry("Single");
    entry1.players = Some(1);
    let mut entry2 = make_entry("TwoPlayer");
    entry2.players = Some(2);
    let mut entry4 = make_entry("FourPlayer");
    entry4.players = Some(4);

    let mut app = test_browser(vec![entry1, entry2, entry4]);
    assert_eq!(app.filtered_indices.len(), 3);

    // Filter to 2+ players
    app.min_players_filter = Some(2);
    app.rebuild_filtered();
    assert_eq!(app.filtered_indices.len(), 2);

    // Filter to 4+ players
    app.min_players_filter = Some(4);
    app.rebuild_filtered();
    assert_eq!(app.filtered_indices.len(), 1);
    assert_eq!(
        app.catalog[app.filtered_indices[0]].display_name,
        "FourPlayer"
    );

    // Clear filter
    app.min_players_filter = None;
    app.rebuild_filtered();
    assert_eq!(app.filtered_indices.len(), 3);
}

#[test]
fn filter_panel_confirm_toggles_player_filter() {
    let mut app = test_browser(vec![make_entry("Zelda")]);
    app.filter_panel_active = true;
    app.filter_panel_column = 1; // Players column
    app.filter_panel_cursor = 0;

    // Cursor 0 = "Any" — should clear filter
    app.filter_panel_confirm();
    assert_eq!(app.min_players_filter, None);

    // Cursor 1 = "2+"
    app.filter_panel_cursor = 1;
    app.filter_panel_confirm();
    assert_eq!(app.min_players_filter, Some(2));

    // Cursor 2 = "4+"
    app.filter_panel_cursor = 2;
    app.filter_panel_confirm();
    assert_eq!(app.min_players_filter, Some(4));

    // Re-select same deselects (back to Any)
    app.filter_panel_confirm();
    assert_eq!(app.min_players_filter, None);
}

#[test]
fn open_search_panel_sets_state() {
    let mut app = test_browser(vec![make_entry("A")]);
    assert!(!app.search_active);
    app.open_search_panel();
    assert!(app.search_active);
    assert_eq!(app.search_kb_row, 1);
    assert_eq!(app.search_kb_col, 0);
}

#[test]
fn close_search_panel_clears_active() {
    let mut app = test_browser(vec![make_entry("A")]);
    app.open_search_panel();
    app.close_search_panel();
    assert!(!app.search_active);
}

#[test]
fn search_kb_confirm_types_character() {
    let mut app = test_browser(vec![make_entry("A")]);
    app.open_search_panel();
    // Default cursor at row 1, col 0 = 'Q'
    app.search_kb_confirm();
    assert_eq!(app.search_query, "q");
}

#[test]
fn search_kb_confirm_backspace_deletes() {
    let mut app = test_browser(vec![make_entry("A")]);
    app.search_query = "hello".to_string();
    app.open_search_panel();
    // Backspace is at row 3, col 8
    app.search_kb_row = 3;
    app.search_kb_col = 8;
    app.search_kb_confirm();
    assert_eq!(app.search_query, "hell");
}

#[test]
fn search_kb_confirm_enter_closes_search() {
    let mut app = test_browser(vec![make_entry("A")]);
    app.open_search_panel();
    // Enter is at row 3, col 9
    app.search_kb_row = 3;
    app.search_kb_col = 9;
    app.search_kb_confirm();
    assert!(!app.search_active);
}

#[test]
fn search_kb_navigation_bounded() {
    let mut app = test_browser(vec![make_entry("A")]);
    app.open_search_panel();
    // Start at row 1, col 0. Moving up goes to row 0.
    app.search_kb_move_up();
    assert_eq!(app.search_kb_row, 0);
    // Moving up again stays at 0.
    app.search_kb_move_up();
    assert_eq!(app.search_kb_row, 0);
    // Move to bottom.
    app.search_kb_move_down();
    app.search_kb_move_down();
    app.search_kb_move_down();
    assert_eq!(app.search_kb_row, 3);
    // Can't go past last row.
    app.search_kb_move_down();
    assert_eq!(app.search_kb_row, 3);
}

#[test]
fn search_kb_left_right_bounded() {
    let mut app = test_browser(vec![make_entry("A")]);
    app.open_search_panel();
    // At col 0, can't go left.
    app.search_kb_move_left();
    assert_eq!(app.search_kb_col, 0);
    // Move right to col 9 (end of 10-char row).
    for _ in 0..20 {
        app.search_kb_move_right();
    }
    assert_eq!(app.search_kb_col, 9);
}

#[test]
fn search_kb_space_inserts_space() {
    let mut app = test_browser(vec![make_entry("A")]);
    app.open_search_panel();
    app.search_kb_row = 3;
    app.search_kb_col = 7; // space key
    app.search_kb_confirm();
    assert_eq!(app.search_query, " ");
}
