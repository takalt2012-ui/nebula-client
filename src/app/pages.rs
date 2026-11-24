use super::*;
use crate::ui::components::{
    account_chip, action_button, accent_pill, card_frame, choice_row, diagnostic_row,
    horizon_logo, integer_slider, labeled_combo, launch_progress, nav_item, page_header,
    panel_card, play_button, setting_row, toggle_switch, window_control, ButtonKind, Icon,
    WindowAction,
};
use crate::ui::theme::THEME;
use super::graphics::{CullingMode, FramePacing, GfxChange, GpuCulling, LeafCulling};
use super::platform::{settings_path, yes_no};
use crate::ui::theme::{medium, semibold};
use eframe::egui::{
    self, Align, Color32, Context, FontFamily, FontId, Frame, Layout, Margin, RichText, Stroke, Vec2,
};

impl NebulaClient {
    pub(super) fn windowbar(&mut self, ctx: &Context) {
        let account = self.selected_account().map(|account| account.name.as_str());
        let bar = egui::TopBottomPanel::top("windowbar")
            .exact_height(48.0)
            .frame(
                Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(10, 7, 24, 112))
                    .inner_margin(Margin::symmetric(16, 7))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 22))),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    horizon_logo(ui, &self.logo, 28.0);
                    ui.add_space(6.0);
                    ui.label(RichText::new("NEBULA").size(14.5).family(semibold()).color(THEME.text));
                    ui.label(RichText::new("CLIENT").size(12.0).family(semibold()).color(THEME.primary_bright));
                    let drag_width = (ui.available_width() - 330.0).max(24.0);
                    let (drag_rect, drag) = ui.allocate_exact_size(Vec2::new(drag_width, 34.0), egui::Sense::click_and_drag());
                    if drag.drag_started() { ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag); }
                    ui.painter().text(drag_rect.center(), egui::Align2::CENTER_CENTER, "Horizon  ·  Rust Edition", FontId::new(12.0, FontFamily::Proportional), Color32::from_rgba_unmultiplied(255, 255, 255, 140));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        window_control(ui, ctx, WindowAction::Close);
                        window_control(ui, ctx, WindowAction::Maximize);
                        window_control(ui, ctx, WindowAction::Minimize);
                        ui.add_space(8.0);
                        account_chip(ui, account.as_deref());
                    });
                });
            });
        let rect = bar.response.rect;
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("bar_accent")));
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.bottom() - 1.0),
                rect.right_bottom(),
            ),
            0.0,
            THEME.primary.gamma_multiply(0.68),
        );
    }

    pub(super) fn sidebar(&mut self, ctx: &Context) {
        // Collapse to an icon rail on narrow windows.
        let compact = ctx.screen_rect().width() < 1060.0;
        egui::SidePanel::left("sidebar")
            .exact_width(if compact { 78.0 } else { 196.0 })
            .frame(
                Frame::NONE
                    .fill(Color32::from_rgba_unmultiplied(10, 7, 24, 100))
                    .inner_margin(Margin::symmetric(12, 18))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 22))),
            )
            .show(ctx, |ui| {
                if !compact {
                    ui.label(RichText::new("MENU").size(12.0).family(semibold()).color(THEME.muted));
                }
                ui.spacing_mut().item_spacing.y = 6.0;
                for (page, label) in Page::ALL {
                    if nav_item(ui, page, label, self.page == page, compact).clicked() {
                        self.go_to(page);
                    }
                }
                ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                    ui.label(
                        RichText::new(if compact { "26.3" } else { "Horizon 26.3  ·  Rust" })
                            .size(11.0)
                            .color(THEME.muted),
                    );
                    if !compact {
                        ui.label(RichText::new("Ctrl + 1-6 switches pages").size(12.0).color(THEME.muted));
                    }
                });
            });
    }

    pub(super) fn play_page(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        let running = game_is_running();
        let starting = self.pending_game.is_some();

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                accent_pill(ui);
                ui.label(RichText::new("NEBULA  /  HORIZON 26.3").size(12.0).family(semibold()).color(THEME.primary_bright));
                ui.add_space(3.0);
                ui.label(
                    RichText::new("Your world, at full speed.")
                        .font(FontId::new(30.0, semibold()))
                        .color(THEME.text),
                );
                ui.add_space(6.0);
                ui.label(RichText::new("Native Minecraft client  ·  Rust edition").color(THEME.muted));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.vertical(|ui| {
                    let (state, color) = if running {
                        ("GAME RUNNING", THEME.success)
                    } else if starting {
                        ("STARTING", THEME.primary_bright)
                    } else {
                        ("READY TO PLAY", THEME.primary_bright)
                    };
                    ui.label(RichText::new(state).size(12.0).family(semibold()).color(color));
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(
                            self.selected_version()
                                .map(|v| v.label.as_str())
                                .unwrap_or("No build selected"),
                        )
                        .color(THEME.text),
                    );
                });
            });
        });
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(4.0);

        let performance_changed = panel_card(ui, card_frame(false), |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Performance profile").family(medium()).color(THEME.text));
                    ui.label(
                        RichText::new("Dedicated shader cache and optional game-scoped tweaks")
                            .size(12.5)
                            .color(THEME.muted),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(if self.settings.performance_mode { "ON" } else { "OFF" })
                            .size(12.5)
                            .family(medium())
                            .color(if self.settings.performance_mode { THEME.primary_bright } else { THEME.muted }),
                    );
                    toggle_switch(ui, &mut self.settings.performance_mode)
                })
                .inner
                .changed()
            })
            .inner
        });
        if performance_changed {
            self.persist_selection();
        }
        ui.add_space(9.0);

        let previous_version = self.selected_version;
        labeled_combo(
            ui,
            "Build",
            "Choose a locally prepared Horizon build",
            "version",
            &mut self.selected_version,
            self.versions
                .iter()
                .enumerate()
                .map(|(index, version)| (index, version.label.as_str())),
        );
        if previous_version != self.selected_version {
            self.selected_resource = self
                .selected_version()
                .and_then(|version| (!version.resources.is_empty()).then_some(0));
            self.persist_selection();
        }

        let resource_choices: Vec<(usize, String)> = self
            .selected_version()
            .into_iter()
            .flat_map(|version| version.resources.iter().enumerate())
            .map(|(index, resource)| (index, resource.label.as_str()))
            .collect();
        let previous_resource = self.selected_resource;
        labeled_combo(
            ui,
            "Resources",
            "Prepared game resources used by this build",
            "resources",
            &mut self.selected_resource,
            resource_choices
                .iter()
                .map(|(index, label)| (*index, *label)),
        );

        let previous_account = self.selected_account;
        if self.accounts.is_empty() {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Account").family(medium()).color(THEME.text));
                    ui.label(
                        RichText::new("A one-time sign-in session is created only when you play.")
                            .size(12.5)
                            .color(THEME.muted),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let can_load = self.official_launcher.is_some();
                    if ui
                        .add_enabled_ui(can_load, |ui| {
                            action_button(ui, ButtonKind::Secondary, Some(Icon::User), "Load accounts", 150.0)
                        })
                        .inner
                        .clicked()
                    {
                        self.load_accounts();
                    }
                });
            });
            ui.separator();
        } else {
            labeled_combo(
                ui,
                "Account",
                "A fresh one-time game session is requested just before launch.",
                "account",
                &mut self.selected_account,
                self.accounts
                    .iter()
                    .enumerate()
                    .map(|(index, account)| (index, account.name.as_str())),
            );
        }
        if previous_resource != self.selected_resource || previous_account != self.selected_account
        {
            self.persist_selection();
        }

        ui.add_space(8.0);
        // Same four requirements the launch path checks, surfaced before the click.
        let blocker = if self.official_launcher.is_none() {
            Some("Horizon's sign-in helper was not found.")
        } else if self.selected_version().is_none() {
            Some("Choose a Horizon build above.")
        } else if self.selected_resource().is_none() {
            Some("This build has no prepared resource set.")
        } else if self.selected_account_id().is_none() {
            Some("Load and choose an account to play.")
        } else {
            None
        };
        ui.horizontal(|ui| {
            if play_button(ui, blocker.is_none() && !starting, starting) {
                self.launch(ctx);
            }
            if action_button(ui, ButtonKind::Ghost, Some(Icon::Refresh), "Refresh builds", 148.0).clicked() {
                self.refresh();
                self.status = Some(Status {
                    message: "Build list refreshed.".into(),
                    kind: StatusKind::Success,
                });
            }
        });
        if starting {
            ui.add_space(6.0);
            launch_progress(ui, ctx.input(|input| input.time) as f32);
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        } else if let Some(reason) = blocker {
            ui.add_space(4.0);
            ui.label(RichText::new(reason).size(12.5).color(THEME.muted));
        }
        if running {
            ui.label(RichText::new("A Horizon client is already running.").color(THEME.primary_bright));
        }
        self.status_line(ui);
    }

    pub(super) fn graphics_page(&mut self, ui: &mut egui::Ui) {
        page_header(
            ui,
            "GRAPHICS",
            "Leaves & Smoothness",
            "Written to the selected instance's settings files. Close the game first: Horizon rewrites them when it exits.",
        );
        let Some(mut gfx) = self.gfx.take() else {
            ui.label(
                RichText::new(
                    "Graphics settings are unavailable. Select a build on Home or resolve the file-access error below.",
                )
                .color(THEME.muted),
            );
            self.status_line(ui);
            return;
        };
        let locked = game_is_running();
        if locked {
            ui.label(RichText::new("Horizon is running. Close it to change these settings.").color(THEME.primary_bright));
            ui.add_space(6.0);
        }

        let mut changes: Vec<GfxChange> = Vec::new();
        let culling_modes = [("off", "Off"), ("sync", "Synchronous"), ("async", "Asynchronous")];
        ui.add_enabled_ui(!locked, |ui| {
            panel_card(ui, card_frame(false), |ui| {
                ui.label(RichText::new("QUICK SETUPS").size(12.0).family(semibold()).color(THEME.primary_bright));
                ui.add_space(4.0);
                ui.label(
                    RichText::new(
                        "Smooth, same look turns on Horizon's culling options, which skip drawing what you cannot see. Plain rendering returns them to Horizon's defaults. Neither touches leaf transparency.",
                    )
                    .size(12.5)
                    .color(THEME.muted),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if action_button(ui, ButtonKind::Primary, Some(Icon::Bolt), "Smooth, same look", 190.0).clicked() {
                        changes.extend(gfx.apply_smooth_same_look());
                    }
                    if action_button(ui, ButtonKind::Secondary, Some(Icon::Reset), "Plain rendering", 170.0).clicked() {
                        changes.extend(gfx.apply_plain_rendering());
                    }
                });
            });
            ui.add_space(12.0);

            panel_card(ui, card_frame(false), |ui| {
                ui.label(RichText::new("LEAVES").size(12.0).family(semibold()).color(THEME.primary_bright));
                ui.add_space(4.0);
                if setting_row(
                    ui,
                    &mut gfx.cutout_leaves,
                    "Leaf transparency",
                    "See-through gaps in leaves instead of solid blocks. On is Horizon's default.",
                    true,
                ) {
                    changes.push((true, "cutoutLeaves", gfx.cutout_leaves.to_string()));
                }
                if setting_row(
                    ui,
                    &mut gfx.improved_transparency,
                    "Improved transparency",
                    "Cleaner blending for glass, water and other translucent blocks. Can cost GPU time.",
                    true,
                ) {
                    changes.push((true, "improvedTransparency", gfx.improved_transparency.to_string()));
                }
                if let Some(value) = choice_row(
                    ui,
                    "Leaf culling",
                    "Skips leaf faces you cannot see. Hidden faces keeps a tree crown looking the same from outside. Solid and Solid+ draw inner leaves opaque, which reduces see-through and makes canopies look chunkier.",
                    &[
                        ("off", "Off"),
                        ("hidden", "Hidden faces"),
                        ("hollow", "Hollow"),
                        ("solid", "Solid"),
                        ("solid_aggressive", "Solid+"),
                    ],
                    gfx.leaf_culling.key(),
                ) {
                    let next = LeafCulling::from_key(&value).unwrap_or(gfx.leaf_culling);
                    gfx.leaf_culling = next;
                    changes.push((false, "leafCulling", next.key().to_owned()));
                }
            });
            ui.add_space(12.0);

            panel_card(ui, card_frame(false), |ui| {
                ui.label(RichText::new("SMOOTHNESS").size(12.0).family(semibold()).color(THEME.primary_bright));
                ui.add_space(4.0);
                if setting_row(
                    ui,
                    &mut gfx.cave_culling,
                    "Cave culling",
                    "Skips terrain hidden behind rock. On is Horizon's default.",
                    true,
                ) {
                    changes.push((false, "caveCulling", gfx.cave_culling.to_string()));
                }
                if let Some(value) = choice_row(
                    ui,
                    "Entity culling",
                    "Hides mobs and items that are out of sight. Synchronous is Horizon's same-frame CPU test; Asynchronous only hides a mesh while its camera and occluder evidence still match.",
                    &culling_modes,
                    gfx.entity_culling.key(),
                ) {
                    let next = CullingMode::from_key(&value).unwrap_or(gfx.entity_culling);
                    gfx.entity_culling = next;
                    changes.push((false, "entityCulling", next.key().to_owned()));
                }
                if let Some(value) = choice_row(
                    ui,
                    "Particle culling",
                    "Hides particles that are out of sight, independent of entity culling.",
                    &culling_modes,
                    gfx.particle_culling.key(),
                ) {
                    let next = CullingMode::from_key(&value).unwrap_or(gfx.particle_culling);
                    gfx.particle_culling = next;
                    changes.push((false, "particleCulling", next.key().to_owned()));
                }
                if let Some(value) = choice_row(
                    ui,
                    "GPU culling",
                    "Auto turns it on only where the CPU limits your frame rate and it makes frames faster.",
                    &[("off", "Off"), ("on", "On"), ("auto", "Auto")],
                    gfx.gpu_culling.key(),
                ) {
                    let next = GpuCulling::from_key(&value).unwrap_or(gfx.gpu_culling);
                    gfx.gpu_culling = next;
                    changes.push((false, "gpuCulling", next.key().to_owned()));
                }
                if let Some(value) = choice_row(
                    ui,
                    "Frame pacing",
                    "Efficient sleeps until the next frame and spends no CPU spinning. Precise is vanilla's limiter: it sleeps, then spins to the deadline.",
                    &[("efficient", "Efficient"), ("precise", "Precise")],
                    gfx.frame_pacing.key(),
                ) {
                    let next = FramePacing::from_key(&value).unwrap_or(gfx.frame_pacing);
                    gfx.frame_pacing = next;
                    changes.push((false, "framePacing", next.key().to_owned()));
                }
            });
            ui.add_space(12.0);

            panel_card(ui, card_frame(false), |ui| {
                ui.label(RichText::new("FRAME-RATE SAFETY NET").size(12.0).family(semibold()).color(THEME.primary_bright));
                ui.add_space(4.0);
                if setting_row(
                    ui,
                    &mut gfx.dynamic_resolution,
                    "Dynamic resolution",
                    "Lowers the world's resolution only when frames run long. It trades sharpness for smoothness and does nothing while Resolution Scale is 100%. Not part of either quick setup.",
                    true,
                ) {
                    changes.push((false, "dynamicResolution", gfx.dynamic_resolution.to_string()));
                }
                ui.label(RichText::new("Target frame rate").family(medium()).color(THEME.text));
                // Written once, when the drag ends or a key press settles, not on every frame of a drag.
                if integer_slider(ui, &mut gfx.target_fps, 30..=240, " fps") {
                    changes.push((false, "targetFps", gfx.target_fps.to_string()));
                }
            });
        });

        let saved_any = !changes.is_empty();
        let mut failed = false;
        for (in_options, key, value) in changes {
            if !self.write_gfx(in_options, key, &value) {
                failed = true;
                break;
            }
        }
        if failed {
            self.reload_gfx();
        } else if saved_any {
            self.reload_gfx();
            self.status = Some(Status { message: "Graphics settings saved.".into(), kind: StatusKind::Success });
        } else {
            self.gfx = Some(gfx);
        }
        self.status_line(ui);
    }

    pub(super) fn performance_page(&mut self, ui: &mut egui::Ui) {
        page_header(
            ui,
            "PERFORMANCE",
            "Performance Mode",
            "Only supported, game-scoped settings are applied. No system-wide tuning is changed.",
        );
        panel_card(ui, card_frame(false), |ui| {
            ui.label(RichText::new("IN-GAME HUD  ·  BASIC HUD").size(12.0).family(semibold()).color(THEME.primary_bright));
            ui.label(RichText::new(if self.hud_config_path.is_some() { "These switches update the installed native mod." } else { "Basic HUD settings are not available for the selected instance." }).size(12.5).color(THEME.muted));
            ui.add_space(8.0);
            egui::Grid::new("nebula_hud_features").num_columns(2).spacing([18.0, 5.0]).show(ui, |ui| {
                for (index, (name, hint)) in HUD_FEATURE_LABELS.iter().enumerate() {
                    let mut active = self.hud_features[index];
                    let hud_available = self.hud_config_path.is_some();
                    let response = ui
                        .add_enabled_ui(hud_available, |ui| {
                            ui.horizontal(|ui| {
                                let switch = toggle_switch(ui, &mut active);
                                ui.label(RichText::new(*name).family(medium()).color(THEME.text));
                                switch
                            })
                            .inner
                        })
                        .inner;
                    ui.label(RichText::new(*hint).size(12.5).color(THEME.muted));
                    ui.end_row();
                    if response.changed() { self.save_hud_feature(index, active); }
                }
            });
        });
        ui.add_space(24.0);

        let changed = setting_row(
            ui,
            &mut self.settings.performance_mode,
            "Performance Mode",
            "Keeps a dedicated Mesa shader cache for faster warm starts.",
            true,
        );
        let gamemode_changed = setting_row(
            ui,
            &mut self.settings.gamemode,
            "Feral GameMode",
            if self.capabilities.gamemode {
                "Run the game through gamemoderun."
            } else {
                "gamemoderun is not installed; this option is unavailable."
            },
            self.capabilities.gamemode,
        );
        let mangohud_changed = setting_row(
            ui,
            &mut self.settings.mangohud,
            "MangoHud",
            if self.capabilities.mangohud {
                "Run the game through MangoHud."
            } else {
                "mangohud is not installed; this option is unavailable."
            },
            self.capabilities.mangohud,
        );
        let io_changed = setting_row(
            ui,
            &mut self.settings.io_priority,
            "Loading I/O priority",
            if self.capabilities.ionice {
                "Uses ionice best-effort priority 0 for the client process."
            } else {
                "ionice is not available on this system."
            },
            self.capabilities.ionice,
        );

        ui.add_space(16.0);
        ui.label(
            RichText::new("Advanced renderer switches")
                .family(medium())
                .color(THEME.text),
        );
        ui.label(
            RichText::new(
                "Leave these off unless you know the active renderer benefits from them.",
            )
            .size(12.5)
            .color(THEME.muted),
        );
        let gl_changed = setting_row(
            ui,
            &mut self.settings.opengl_threading,
            "Mesa OpenGL driver thread",
            "Sets MESA_GLTHREAD=true. It has no benefit for Vulkan and can regress some OpenGL paths.",
            true,
        );
        let nvidia_changed = setting_row(
            ui,
            &mut self.settings.nvidia_threading,
            "NVIDIA OpenGL driver thread",
            "Sets __GL_THREADED_OPTIMIZATIONS=1. It is NVIDIA/OpenGL-specific.",
            true,
        );

        if changed
            || gamemode_changed
            || mangohud_changed
            || io_changed
            || gl_changed
            || nvidia_changed
        {
            self.persist_selection();
        }
        self.status_line(ui);
    }

    pub(super) fn shaders_page(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        // Install packs dropped anywhere on this page.
        let dropped: Vec<PathBuf> = ctx
            .input(|input| {
                input.raw.dropped_files.iter()
                    .filter_map(|file| file.path.clone())
                    .collect()
            });
        for path in dropped {
            self.install_shader_pack(&path);
        }

        page_header(
            ui,
            "SHADERS",
            "Shader Packs",
            "Enable or disable a pack for the selected instance. Drop a .zip or .hshader anywhere on this page to install it.",
        );

        if self.shader_dir.is_none() {
            ui.label(
                RichText::new(
                    "No Horizon instance is selected, so shader packs cannot be loaded. Pick a build on Home, then refresh.",
                )
                .color(THEME.muted),
            );
            self.status_line(ui);
            return;
        }

        let running = game_is_running();
        let mut install_builtin = false;
        panel_card(ui, card_frame(false), |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("INCLUDED WITH NEBULA").size(11.5).family(semibold()).color(THEME.primary_bright));
                    ui.label(RichText::new("Golden Hour · Horizon-native shader").family(medium()).color(THEME.text));
                    ui.label(RichText::new("A lightweight warm-sky and subtle colour-grade pack, using Horizon shader API 0.1.").size(12.0).color(THEME.muted));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if action_button(ui, ButtonKind::Primary, Some(Icon::Check), "Install & enable", 148.0).clicked() {
                        install_builtin = true;
                    }
                });
            });
        });
        if install_builtin {
            self.install_builtin_shader_pack();
        }
        let picked = self.shader_picked.as_str();
        let packs = &self.shader_packs;
        let mut disable_request = false;

        // Current pick / off state.
        panel_card(ui, card_frame(false), |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("ACTIVE").size(12.0).family(semibold()).color(THEME.primary_bright));
                        if picked.is_empty() {
                            ui.label(RichText::new("Shaders off (vanilla rendering)").color(THEME.text));
                        } else {
                            let title = packs
                                .iter()
                                .find(|pack| pack.key == picked || pack.file == picked)
                                .map(|pack| pack.title.as_str())
                                .unwrap_or(picked);
                            ui.label(RichText::new(title).family(medium()).color(THEME.success));
                            ui.label(RichText::new(&picked).size(12.5).monospace().color(THEME.muted));
                        }
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if !picked.is_empty()
                            && ui
                                .add_enabled_ui(!running, |ui| {
                                    action_button(ui, ButtonKind::Secondary, Some(Icon::Power), "Disable", 120.0)
                                })
                                .inner
                                .clicked()
                        {
                            disable_request = true;
                        }
                    });
                });
            });

        ui.add_space(10.0);
        let hovering = ctx.input(|input| !input.raw.hovered_files.is_empty());
        panel_card(ui, card_frame(hovering), |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("Drop a shader pack here to install")
                            .family(medium())
                            .color(THEME.text),
                    );
                    ui.label(
                        RichText::new(".zip (shaders/ format) or .hshader (Horizon native)")
                            .size(12.5)
                            .color(THEME.muted),
                    );
                });
            });

        ui.add_space(16.0);
        ui.label(RichText::new("INSTALLED PACKS").size(12.0).family(semibold()).color(THEME.primary_bright));
        ui.add_space(6.0);

        if packs.is_empty() {
            ui.label(
                RichText::new("No shader packs found in this instance's shaderpacks folder.")
                    .color(THEME.muted),
            );
        }

        let mut enable_request: Option<String> = None;
        for pack in packs {
            let active = !picked.is_empty() && (picked == pack.key || picked == pack.file);
            panel_card(ui, card_frame(active), |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&pack.title).family(medium()).color(THEME.text));
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(if pack.native { "Horizon native" } else { "shaders/ format" })
                                        .size(12.5)
                                        .color(THEME.muted),
                                );
                                ui.label(
                                    RichText::new(pack.file.as_str())
                                        .size(12.5)
                                        .color(THEME.muted),
                                );
                            });
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if active {
                                ui.label(RichText::new("ENABLED").size(12.0).family(semibold()).color(THEME.success));
                            } else if ui
                                .add_enabled_ui(!running, |ui| {
                                    action_button(ui, ButtonKind::Primary, Some(Icon::Check), "Enable", 120.0)
                                })
                                .inner
                                .clicked()
                            {
                                enable_request = Some(pack.key.clone());
                            }
                        });
                    });
                });
            ui.add_space(8.0);
        }

        if let Some(key) = enable_request {
            self.set_picked_shader(&key);
        } else if disable_request {
            self.set_picked_shader("");
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if action_button(ui, ButtonKind::Ghost, Some(Icon::Refresh), "Refresh packs", 0.0).clicked() {
                self.load_shader_state();
                self.status = Some(Status {
                    message: format!("Found {} shader pack(s).", self.shader_packs.len()),
                    kind: StatusKind::Success,
                });
            }
            if running {
                ui.label(
                    RichText::new("A Horizon client is running; close it to change shader packs.")
                        .color(THEME.primary_bright),
                );
            }
        });
        self.status_line(ui);
    }

    pub(super) fn diagnostics_page(&mut self, ui: &mut egui::Ui) {
        page_header(ui, "DIAGNOSTICS", "Local installation", "");
        diagnostic_row(ui, "Horizon data", &horizon_data_root());
        diagnostic_row(ui, "Nebula cache", &cache_dir());
        diagnostic_row(ui, "Nebula settings", &settings_path());
        ui.add_space(18.0);
        ui.label(RichText::new(format!("Prepared builds: {}", self.versions.len())).color(THEME.text));
        ui.label(RichText::new(format!("Loaded accounts: {}", self.accounts.len())).color(THEME.text));
        ui.label(
            RichText::new(match &self.official_launcher {
                Some(path) => format!("Sign-in helper: {}", path.display()),
                None => "Sign-in helper: not found".into(),
            })
            .size(12.5)
            .color(THEME.muted),
        );
        ui.label(
            RichText::new(format!(
                "GameMode: {}  •  MangoHud: {}  •  ionice: {}",
                yes_no(self.capabilities.gamemode),
                yes_no(self.capabilities.mangohud),
                yes_no(self.capabilities.ionice)
            ))
            .color(THEME.muted),
        );
        ui.add_space(18.0);
        ui.label(
            RichText::new("Horizon's sign-in helper creates a one-time session at launch. Nebula never reads or stores your credentials.")
                .size(12.5)
                .color(THEME.muted),
        );
        self.status_line(ui);
    }

    pub(super) fn settings_page(&mut self, ui: &mut egui::Ui) {
        page_header(
            ui,
            "SETTINGS",
            "Nebula Client",
            "Minimal by design: no account service, updater, or telemetry loop.",
        );
        if setting_row(
            ui,
            &mut self.settings.close_after_launch,
            "Exit immediately after launch",
            "Closes Nebula after the native game process starts, leaving no launcher process in memory.",
            true,
        ) {
            self.persist_selection();
        }
        self.status_line(ui);
    }

    pub(super) fn status_line(&self, ui: &mut egui::Ui) {
        if let Some(status) = &self.status {
            ui.add_space(14.0);
            let tint = if status.kind == StatusKind::Error { THEME.danger } else { THEME.success };
            Frame::NONE
                .fill(tint.gamma_multiply(0.13))
                .stroke(Stroke::new(1.0_f32, tint.gamma_multiply(0.45)))
                .corner_radius(egui::CornerRadius::same(10))
                .inner_margin(Margin::symmetric(14, 10))
                .show(ui, |ui| {
                    if status.kind == StatusKind::Error {
                        ui.label(RichText::new(format!("Problem: {}", status.message)).color(THEME.text));
                    } else {
                        ui.label(RichText::new(status.message.as_str()).color(THEME.text));
                    }
                });
        }
    }

}
