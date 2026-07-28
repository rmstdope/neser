use super::*;

impl ApplicationHandler for RomBrowserApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gl.is_some() {
            return;
        }

        match BrowserGl::new(
            event_loop,
            self.default_width,
            self.default_height,
            self.fullscreen,
        ) {
            Ok(gl) => {
                self.gl = Some(gl);
                // Spawn background thread for catalog loading + enrichment.
                if matches!(self.catalog_state, CatalogState::Idle) {
                    let (
                        search_paths,
                        rebuild,
                        metadata_db_path,
                        image_cache_path,
                        include_unofficial,
                    ) = {
                        let ctx = self.app_context.borrow();
                        let config = ctx.config();
                        (
                            config.frontend.cartridge_search_paths.clone(),
                            config.frontend.rebuild_cartridge_catalog,
                            config.frontend.resolved_metadata_db_path(),
                            config.frontend.resolved_image_cache_path(),
                            config.frontend.include_unofficial_roms,
                        )
                    };
                    let (tx, rx) = mpsc::channel();
                    std::thread::spawn(move || {
                        match crate::platform::catalog::load_catalog(
                            &search_paths,
                            rebuild,
                            include_unofficial,
                        ) {
                            Ok(mut catalog) => {
                                let tx2 = tx.clone();
                                crate::platform::catalog::enrich_catalog(
                                    &mut catalog,
                                    &metadata_db_path,
                                    &image_cache_path,
                                    rebuild,
                                    move |progress| {
                                        let _ = tx2.send(CatalogMessage::Progress(progress));
                                    },
                                );
                                catalog.sort_by(|a, b| {
                                    a.display_name
                                        .to_lowercase()
                                        .cmp(&b.display_name.to_lowercase())
                                });
                                let _ = tx.send(CatalogMessage::Done(catalog));
                            }
                            Err(e) => {
                                eprintln!("Failed to load ROM catalog: {e}");
                                let _ = tx.send(CatalogMessage::Done(Vec::new()));
                            }
                        }
                    });
                    self.catalog_state = CatalogState::Loading {
                        receiver: rx,
                        progress: None,
                    };
                }
                event_loop.set_control_flow(ControlFlow::Poll);
            }
            Err(e) => {
                eprintln!("Failed to create browser GL context: {e}");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                self.result = BrowserResult::Closed;
                event_loop.exit();
            }

            WindowEvent::Resized(physical_size) => {
                if let Some(ref mut gl) = self.gl {
                    gl.notify_resize(physical_size.width, physical_size.height);
                }
            }

            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                use winit::keyboard::{Key, NamedKey};

                // Ctrl+Q always quits regardless of active overlay.
                let ctrl = self
                    .modifiers
                    .contains(winit::keyboard::ModifiersState::CONTROL)
                    || self
                        .modifiers
                        .contains(winit::keyboard::ModifiersState::SUPER);
                if let Key::Character(ref ch) = event.logical_key
                    && (ch.as_str() == "q" || ch.as_str() == "Q")
                    && ctrl
                {
                    self.result = BrowserResult::Closed;
                    event_loop.exit();
                    return;
                }

                // Ctrl+F always toggles fullscreen regardless of active overlay.
                if let Key::Character(ref ch) = event.logical_key
                    && (ch.as_str() == "f" || ch.as_str() == "F")
                    && ctrl
                {
                    if let Some(ref gl) = self.gl {
                        let window = gl.window();
                        if window.fullscreen().is_some() {
                            window.set_fullscreen(None);
                        } else {
                            window
                                .set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
                        }
                    }
                    return;
                }

                // Tab toggles search from any screen.
                if matches!(event.logical_key, Key::Named(NamedKey::Tab)) {
                    if self.search_active {
                        self.close_search_panel();
                    } else {
                        // Close any other overlay first.
                        self.filter_panel_active = false;
                        self.detail_view_active = false;
                        self.open_search_panel();
                    }
                    return;
                }

                if self.search_active {
                    // Search mode input handling.
                    // Physical keyboard typing works directly; arrows move
                    // the on-screen keyboard cursor for gamepad users.
                    match event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            self.close_search_panel();
                        }
                        Key::Named(NamedKey::Backspace) => {
                            self.search_query.pop();
                            self.rebuild_filtered();
                        }
                        Key::Named(NamedKey::Enter) => {
                            self.search_kb_confirm();
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            self.search_kb_move_up();
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            self.search_kb_move_down();
                        }
                        Key::Named(NamedKey::ArrowLeft) => {
                            self.search_kb_move_left();
                        }
                        Key::Named(NamedKey::ArrowRight) => {
                            self.search_kb_move_right();
                        }
                        Key::Character(ref ch) => {
                            self.search_query.push_str(ch.as_str());
                            self.rebuild_filtered();
                        }
                        _ => {}
                    }
                } else if self.filter_panel_active {
                    match event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            self.close_filter_panel();
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            self.filter_panel_move_cursor_up();
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            self.filter_panel_move_cursor_down();
                        }
                        Key::Named(NamedKey::ArrowLeft) => {
                            self.filter_panel_move_left();
                        }
                        Key::Named(NamedKey::ArrowRight) => {
                            self.filter_panel_move_right();
                        }
                        Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                            self.filter_panel_confirm();
                        }
                        _ => {}
                    }
                } else if self.detail_view_active {
                    // Detail view mode.
                    match event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            self.detail_view_active = false;
                        }
                        Key::Named(NamedKey::Enter) => {
                            if let Some(entry) = self.selected_entry() {
                                self.result = BrowserResult::RomSelected(entry.path.clone());
                                event_loop.exit();
                            }
                        }
                        Key::Named(NamedKey::Space) => {
                            self.toggle_favorite();
                        }
                        Key::Character(ref ch) if ch.as_str() == " " => {
                            self.toggle_favorite();
                        }
                        Key::Named(NamedKey::ArrowUp)
                        | Key::Named(NamedKey::ArrowLeft)
                        | Key::Named(NamedKey::ArrowDown)
                        | Key::Named(NamedKey::ArrowRight) => {
                            // Screenshots auto-scroll; arrow keys are ignored.
                        }
                        _ => {}
                    }
                } else {
                    // Normal browsing mode.
                    match event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            self.open_filter_panel();
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            self.navigate_up();
                        }
                        Key::Named(NamedKey::ArrowDown) => {
                            self.navigate_down();
                        }
                        Key::Named(NamedKey::ArrowLeft) => {
                            if self.selected_index > 0 {
                                self.selected_index -= 1;
                                self.ensure_selected_visible();
                            }
                        }
                        Key::Named(NamedKey::ArrowRight) => {
                            if self.selected_index + 1 < self.filtered_indices.len() {
                                self.selected_index += 1;
                                self.ensure_selected_visible();
                            }
                        }
                        Key::Named(NamedKey::Enter) => {
                            // Open detail view; launch is done from the detail view.
                            self.open_detail_view();
                        }
                        Key::Named(NamedKey::Space) => {
                            self.toggle_favorite();
                        }
                        Key::Character(ref ch) if ch.as_str() == " " => {
                            self.toggle_favorite();
                        }
                        _ => {}
                    }
                }
            }

            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
                if let Some(ref mut gl) = self.gl {
                    let _ = gl.on_window_event(&event);
                }
            }

            WindowEvent::Focused(focused) => {
                self.window_focused = focused;
                // Discard gamepad events queued while unfocused: redraws
                // (which normally drain the queue) may pause when the window
                // is hidden or minimized, and the backlog would otherwise
                // apply all at once on refocus.
                if focused && let Some(ref mut gilrs) = self.gilrs {
                    while gilrs.next_event().is_some() {}
                }
                if let Some(ref mut gl) = self.gl {
                    let _ = gl.on_window_event(&event);
                }
            }

            WindowEvent::CursorMoved { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. }
            | WindowEvent::Touch { .. }
            | WindowEvent::ScaleFactorChanged { .. }
            | WindowEvent::Ime(_) => {
                if let Some(ref mut gl) = self.gl {
                    let _ = gl.on_window_event(&event);
                }
            }

            WindowEvent::RedrawRequested => {
                // Poll gamepad events; apply them only while focused (gilrs
                // sees the pad globally, so polling always drains the queue).
                let actions =
                    Self::filter_gamepad_actions(self.poll_gamepad(), self.window_focused);
                for action in actions {
                    self.apply_action(action, event_loop);
                }

                self.render_frame();
                if let Some(ref gl) = self.gl {
                    gl.window().request_redraw();
                }
            }
            _ => {}
        }
    }
}
