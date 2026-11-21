//! Items drawn through the resolved item model pipeline
//! (`horizon_client::item_render`): GUI icons (`GuiGraphics.renderItem`),
//! dropped items (`ItemEntityRenderer`) and items posed in the world (hands,
//! frames, armour stands). This binds the resolver's host services
//! (`ItemEnv`) and the special renderers' registries to the game.
use super::{Dim, Game, entity_culling::EntityCullDecision};
use horizon_assets::item_model::{
    Components, ComponentsView, DisplayContext, GlobalPos, ItemEnv, LevelState,
    OwnerState, StackView,
};
use horizon_client::{
    block_entity_render::{diffuse_lights, vertex_light},
    cit::{Hand as CitHand, Kind as CitKind},
    graphics::EntityCullingMode, hud::{HudBatch, Rect},
    item_render::{self, ItemRenderState, SpecialHost, WorldSink},
    items::ItemStack, mesher::Vertex,
};
use horizon_core::geometry::{Aabb, Vec3};
use serde_json::Value;
use std::{cell::RefCell, collections::HashMap};
/// The game's painter for the HUD kit's icons (`hud_kit::icon`): items by their models, the
/// player's face from `face`, their skin's texture.
pub(super) struct IconPainter<'g, 'a> {
    pub(super) game: &'g Game<'a>,
    pub(super) face: Option<&'g str>,
}
impl horizon_client::hud_kit::icon::IconPainter for IconPainter<'_, '_> {
    fn item(&self, batch: &mut HudBatch, id: &str, [x, y, size]: [f32; 3]) -> bool {
        let start = batch.vertices.len();
        self.game.draw_item_model(batch, &ItemStack::new(id, 1), x, y);
        let scale = size / horizon_client::hud_kit::icon::ITEM_SIZE;
        batch.scale_since(start, x, y, scale, scale);
        batch.vertices.len() > start
    }
    fn face(&self, batch: &mut HudBatch, [x, y, size]: [f32; 3]) -> bool {
        let Some(skin) = self.face else {
            return false;
        };
        let mut draws = Vec::new();
        horizon_client::tab_list::face(
            &mut draws,
            skin,
            x as i32,
            y as i32,
            size as i32,
            true,
            false,
            u32::MAX,
        );
        horizon_client::hud_layout::paint(batch, &self.game.gui.atlas, &draws);
        true
    }
}
/// The game's answers to what the resolver cannot read from the stack.
pub(super) struct Env<'a, 'g> {
    game: &'a Game<'g>,
}
impl ItemEnv for Env<'_, '_> {
    fn item_model_property(&self, property: &str, stack: &StackView) -> bool {
        if horizon_client::mod_api::mods().is_empty() {
            return false;
        }
        let mut value = false;
        self.game.native_item_model_property(property, stack, &mut value);
        value
    }
    fn enchantment_in_tag(&self, enchantment: &str, tag: &str) -> bool {
        self.game.stations().enchantments.in_tag(tag, enchantment)
    }
    fn potion_effect_color(&self, contents: &Value) -> Option<i32> {
        let id = contents["potion"].as_str();
        let effects = id
            .and_then(horizon_client::brewing::potion)
            .is_some_and(|p| !p.effects.is_empty());
        effects.then(|| (0xFF00_0000 | horizon_client::brewing::potion_color(id)) as i32)
    }
    fn grass_color(&self, temperature: f64, downfall: f64) -> i32 {
        self.game.pack.item_renderer.grass_color(temperature, downfall)
    }
    fn bundle_fullness(&self, _stack: &StackView, components: &Components) -> f32 {
        let Some(items) = components
            .get("minecraft:bundle_contents")
            .and_then(Value::as_array) else {
            return 0.0;
        };
        let prototypes = &self.game.pack.item_renderer.models.prototypes;
        let mut total = 0.0f64;
        for item in items {
            let id = item["id"].as_str().unwrap_or("");
            let count = item["count"].as_i64().unwrap_or(1) as f64;
            let max = prototypes
                .get(id)
                .and_then(|c| c.get("minecraft:max_stack_size"))
                .and_then(Value::as_i64)
                .unwrap_or(1) as f64;
            total += count / max;
        }
        total as f32
    }
    fn has_shift_down(&self) -> bool {
        self.game.has_shift_down()
    }
    fn local_time(
        &self,
        pattern: &str,
        _locale: &str,
        _time_zone: Option<&str>,
    ) -> String {
        local_time(pattern)
    }
    fn random_float(&self) -> f32 {
        if self.game.clock.still() {
            let mixed = (self.game.visual_ticks() ^ 0x9e37_79b9)
                .wrapping_mul(0x9e37_79b9_7f4a_7c15);
            return (mixed >> 40) as f32 / (1u64 << 24) as f32;
        }
        let nanos = self.game.started.elapsed().subsec_nanos();
        (nanos % 1_000_000) as f32 / 1_000_000.0
    }
}
/// The days since 1970-01-01 as a civil (year, month, day) date.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}
/// `DateTimeFormatter.ofPattern(pattern).format(now)` for the month, day,
/// year and time letters item models use, in the system time zone
/// (`LocalTime` without a `time_zone`).
pub(super) fn local_time(pattern: &str) -> String {
    let now = chrono::Local::now();
    let secs = now.timestamp() + i64::from(now.offset().local_minus_utc());
    let (y, mo, d) = civil(secs.div_euclid(86_400));
    let s = secs.rem_euclid(86_400);
    let (h, mi, se) = (s / 3600, s / 60 % 60, s % 60);
    let mut out = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let mut n = 1;
        while i + n < chars.len() && chars[i + n] == c {
            n += 1;
        }
        let num = |v: i64| format!("{v:0n$}");
        match c {
            'M' => out += &num(i64::from(mo)),
            'd' => out += &num(i64::from(d)),
            'y' | 'u' => out += &if n == 2 { num(y % 100) } else { num(y) },
            'H' => out += &num(h),
            'm' => out += &num(mi),
            's' => out += &num(se),
            '\'' => {
                let end = chars[i + 1..].iter().position(|&x| x == '\'');
                if let Some(e) = end {
                    out.extend(&chars[i + 1..i + 1 + e]);
                    i += e + 2;
                    continue;
                }
                n = 1;
            }
            other => out.extend(std::iter::repeat_n(other, n)),
        }
        i += n;
    }
    out
}
/// Banner and shield pattern textures from the banner pattern registry.
pub(super) struct Host<'a> {
    banners: &'a horizon_client::banner::Banners,
    sprites: &'a HashMap<String, Rect>,
    skins: &'a RefCell<horizon_client::skins::Downloader>,
    profiles: &'a RefCell<horizon_client::skins::Resolver>,
}
impl SpecialHost for Host<'_> {
    fn banner_texture(&self, pattern: &str) -> Option<String> {
        self.banners.pattern(pattern).map(|_| self.banners.banner_texture(pattern))
    }
    fn shield_texture(&self, pattern: &str) -> Option<String> {
        self.banners.pattern(pattern).map(|_| self.banners.shield_texture(pattern))
    }
    fn player_skin(&self, profile: &Value) -> Option<String> {
        Some(
            horizon_client::skins::head_skin(
                profile,
                &mut self.profiles.borrow_mut(),
                &mut self.skins.borrow_mut(),
                |location| self.sprites.contains_key(location),
            ),
        )
    }
}
/// `ItemOwner` of an entity holding or showing an item.
pub(super) fn owner_at(position: [f64; 3], yaw: f32, entity_type: &str) -> OwnerState {
    OwnerState {
        position,
        visual_rotation_y_degrees: yaw,
        entity_type: entity_type.to_owned(),
        ..Default::default()
    }
}
/// `UseDuration` of a stack (`Item.getUseDuration`): 72000 for drawn items
/// and for `blocks_attacks` items (shields), the consume time for food.
fn use_duration(components: ComponentsView<'_>) -> i32 {
    if let Some(c) = components.get("minecraft:consumable") {
        let seconds = c["consume_seconds"].as_f64().unwrap_or(1.6);
        return (seconds * 20.0) as i32;
    }
    if components.get("minecraft:blocks_attacks").is_some()
        || components.get("minecraft:kinetic_weapon").is_some()
    {
        return 72_000;
    }
    let item_model = components
        .get("minecraft:item_model")
        .and_then(Value::as_str)
        .unwrap_or("");
    match item_model {
        "minecraft:bow" | "minecraft:crossbow" | "minecraft:trident"
        | "minecraft:spyglass" | "minecraft:goat_horn" => 72_000,
        "minecraft:brush" => 200,
        _ => 0,
    }
}
impl Game<'_> {
    pub(super) fn item_env(&self) -> Env<'_, '_> {
        Env { game: self }
    }
    pub(super) fn item_host(&self) -> Host<'_> {
        Host {
            banners: &self.stations().banners,
            sprites: &self.gui.atlas.sprites,
            skins: &self.skin_downloads,
            profiles: &self.skin_profiles,
        }
    }
    /// The client level as item models read it: worked out once a frame (a
    /// screen of items read the same level, its sky sampled once).
    pub(super) fn item_level(&self) -> LevelState {
        let dimension = match self.dim {
            Dim::Overworld => "minecraft:overworld",
            Dim::Nether => "minecraft:the_nether",
            Dim::End => "minecraft:the_end",
        };
        let key = (self.frames, self.world.game_time, self.world.day_time, dimension);
        let mut cached = self.item_level_cache.borrow_mut();
        if let Some((at, level)) = cached.as_ref() && *at == key {
            return level.clone();
        }
        let level = self.level_state_now(dimension);
        *cached = Some((key, level.clone()));
        level
    }
    fn level_state_now(&self, dimension: &'static str) -> LevelState {
        let sky = self
            .dim_environment()
            .sample(
                &horizon_client::environment::Sample {
                    day_time: self.world.day_time,
                    ..Default::default()
                },
            );
        LevelState {
            dimension: dimension.to_owned(),
            game_time: self.world.game_time,
            sun_angle: sky.sun_angle,
            moon_phase: (self.world.day_time.div_euclid(24_000)).rem_euclid(8) as i32,
            respawn: self
                .spawn
                .map(|s| GlobalPos {
                    dimension: "minecraft:overworld".to_owned(),
                    pos: [s.x.floor() as i32, s.y.floor() as i32, s.z.floor() as i32],
                }),
            runs_normally: true,
        }
    }
    /// The local player as `ItemOwner`; `stack` is the stack drawn, to tell
    /// whether it is the one in use.
    pub(super) fn player_owner(&self, stack: Option<&ItemStack>) -> OwnerState {
        let position = self
            .player
            .as_ref()
            .map_or([0.0; 3], |p| [p.position.x, p.position.y, p.position.z]);
        let mut owner = owner_at(position, self.yaw, "minecraft:player");
        owner.main_arm_left = self.settings.main_hand_left;
        owner.is_local_player = true;
        owner.is_view_entity = true;
        owner.last_death_location = self
            .level
            .last_death
            .clone()
            .map(|(dimension, pos)| GlobalPos { dimension, pos });
        if let (Some((slot, ticks)), Some(stack)) = (self.using, stack)
            && self.stack_in(slot) == Some(stack)
        {
            let view = item_render::stack_view(stack);
            let mut duration = use_duration(
                self.pack.item_renderer.models.components_view(&view),
            );
            self.native_use_duration(stack, &mut duration);
            owner.using_this_item = true;
            owner.use_duration = duration;
            owner.use_item_remaining_ticks = self.use_duration - ticks as i32;
            owner.crossbow_charge_duration = Self::charge_ticks(stack).max(1) as i32;
        }
        owner.fishing_cast_with_this_item = stack
            .zip(self.cast_rod())
            .is_some_and(|(s, rod)| std::ptr::eq(s, rod));
        owner.is_selected = stack.is_some()
            && self.inventory.get(self.selected).and_then(Option::as_ref) == stack;
        owner.is_carried = owner.is_selected;
        owner
    }
    /// `ItemModelResolver.updateForTopItem`.
    pub(super) fn resolve_item(
        &self,
        stack: &ItemStack,
        display: DisplayContext,
        owner: Option<&OwnerState>,
        seed: i32,
    ) -> ItemRenderState {
        self.resolve_item_in_hand(stack, display, owner, seed, CitHand::Main)
    }
    /// `ItemStack.hasFoil`, with the item's default glint.
    fn has_foil(&self, stack: &ItemStack) -> bool {
        let item_glint = self
            .res
            .items()
            .enchantments
            .props
            .get(&*stack.item)
            .is_some_and(|p| p.glint);
        stack.has_foil(item_glint)
    }
    /// [`Self::resolve_item`] of a stack held in `hand`, which OptiFine CIT rules can test
    /// (GUI items count as the main hand).
    pub(super) fn resolve_item_in_hand(
        &self,
        stack: &ItemStack,
        display: DisplayContext,
        owner: Option<&OwnerState>,
        seed: i32,
        hand: CitHand,
    ) -> ItemRenderState {
        let level = self.item_level();
        let renderer = &self.pack.item_renderer;
        let (rule, glint) = renderer
            .cit
            .as_ref()
            .map_or(
                (None, None),
                |cit| {
                    let item_max = self
                        .res
                        .items()
                        .get(&stack.item)
                        .map_or(0, |i| i.max_damage);
                    let max_damage = item_render::cit::stack_max_damage(stack, item_max);
                    (
                        cit.rules.select(CitKind::Item, stack, hand, max_damage),
                        cit.rules.glint(stack, hand, max_damage, self.has_foil(stack)),
                    )
                },
            );
        let mut state = renderer
            .resolve_with_rule(
                &item_render::stack_view(stack),
                display,
                owner,
                Some(&level),
                seed,
                &self.item_env(),
                rule,
            );
        if let Some(glint) = glint {
            let gui = display == DisplayContext::Gui;
            let pack_keeps = renderer
                .cit
                .as_ref()
                .is_none_or(|c| c.rules.global.use_glint);
            if !glint.vanilla && (gui || !pack_keeps) {
                for layer in &mut state.layers {
                    layer.foil = horizon_assets::item_model::FoilType::None;
                }
            }
            if gui && !glint.is_empty() {
                state.cit_glint = Some(glint);
            }
        }
        state.hdr_tier = self.luminance().hdr_tier(&stack.item);
        if state.hdr_tier > 0 && self.renderer.as_ref().is_some_and(|r| r.hdr_on()) {
            state.hdr_light = self.luminance().of(&stack.item);
        }
        state
    }
    /// `GuiGraphics.renderItem(player, level, stack, x, y, 0)`.
    pub(super) fn draw_item_model(
        &self,
        batch: &mut HudBatch,
        stack: &ItemStack,
        x: f32,
        y: f32,
    ) {
        if let ::core::ops::ControlFlow::Break(__horizon_mixins_return) = {
            fn sunset_item_durability(
                game: &crate::game::Game<'_>,
                batch: &mut HudBatch,
                stack: &ItemStack,
                x: f32,
                y: f32,
            ) -> std::ops::ControlFlow<()> {
                if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() {
                    return std::ops::ControlFlow::Continue(());
                }
                crate::game::sunset_hud_bars::item_model(game, batch, stack, x, y)
            }
            sunset_item_durability(&*self, &mut *batch, stack, x, y)
        } {
            return __horizon_mixins_return;
        }
        let owner = self.player_owner(Some(stack));
        let state = self.resolve_item(stack, DisplayContext::Gui, Some(&owner), 0);
        item_render::gui::draw(
            &state,
            &self.pack.item_renderer,
            &self.gui.atlas.sprites,
            &self.item_host(),
            batch,
            x,
            y,
            self.glint_frame.millis / 1000.0,
            self.glint_frame.speed,
            self.glint_frame.alpha,
        );
    }
    /// The glint clock, options and sprite for this frame: read once per
    /// frame, not per item (`GameRenderer.extractOptions`).
    pub(super) fn capture_glint_frame(&self) -> item_render::GlintFrame {
        item_render::GlintFrame {
            millis: if self.clock.still() {
                self.visual_millis() as f32
            } else {
                self.started.elapsed().as_secs_f32() * 1000.0
            },
            speed: self.settings.file.double("glintSpeed") as f32,
            alpha: self.settings.file.double("glintStrength") as f32,
            sprite: self.gui.atlas.sprites.contains_key("misc/enchanted_glint_item"),
        }
    }
    /// The `ENTITY_GLINT_TEXTURING` inputs of this frame.
    pub(super) fn glint(&self) -> Option<item_render::Glint> {
        self.glint_frame.glint(0.5)
    }
    /// Submit a resolved item under `pose` (camera-relative) at `light`
    /// (packed lightmap coordinates).
    pub(super) fn submit_item_world(
        &self,
        state: &ItemRenderState,
        pose: glam::Mat4,
        light: u32,
        out: &mut Vec<Vertex>,
    ) {
        if let ::core::ops::ControlFlow::Break(__horizon_mixins_return) = {
            fn sunset_hand_item(
                this: &Game<'_>,
                state: &ItemRenderState,
                pose: glam::Mat4,
                light: u32,
                out: &mut Vec<Vertex>,
            ) -> std::ops::ControlFlow<()> {
                if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() {
                    return std::ops::ControlFlow::Continue(());
                }
                use horizon_client::first_person::sunset_hand_view as view;
                let left = match state.display {
                    DisplayContext::FirstPersonLeftHand => true,
                    DisplayContext::FirstPersonRightHand => false,
                    _ => return std::ops::ControlFlow::Continue(()),
                };
                let hand = if left == this.settings.main_hand_left {
                    horizon_client::first_person::Hand::Main
                } else {
                    horizon_client::first_person::Hand::Off
                };
                let stack = if hand == horizon_client::first_person::Hand::Main {
                    this.held()
                } else {
                    this.offhand.as_ref()
                };
                let plain = this.using.is_none()
                    && !this
                        .player
                        .as_ref()
                        .is_some_and(|p| p.auto_spin_attack_ticks > 0)
                    && !this.swing.current.is_some_and(|c| c.animation.kind == 2)
                    && stack
                        .is_none_or(|s| {
                            &*s.item != "minecraft:crossbow"
                                && horizon_client::spear::Kinetic::of(&s.item).is_none()
                        });
                let attack = if this.swing.attack_hand() == hand {
                    this.swing.animation(this.clock.camera_fraction() as f32)
                } else {
                    0.0
                };
                let sword = stack
                    .is_some_and(|s| {
                        this
                            .res
                            .items()
                            .item_tags
                            .get("minecraft:swords")
                            .is_some_and(|t| t.contains(&*s.item))
                    });
                let game_pose = pose;
                let main = hand == horizon_client::first_person::Hand::Main;
                let flat = !state.layers.is_empty()
                    && state
                        .layers
                        .iter()
                        .all(|l| horizon_client::first_person::is_modern_flat_display(
                            &l.matrix,
                            left,
                        ));
                let native = this.settings.extras.pvp.combat.legacy && flat;
                let native_block = native && main && this.right
                    && this.screen == super::Screen::Playing
                    && stack.is_some_and(|s| s.item.ends_with("_sword"));
                let block = main && sword
                    && (view::SWORD_BLOCK.load(std::sync::atomic::Ordering::Relaxed)
                        || native_block);
                let time = (this.ticks as f64 + this.clock.camera_fraction()) as f32
                    / 20.0;
                let s = if main && this.swing.attack_hand() == hand {
                    view::swing_clock(
                        this.swings_started as u64,
                        this.swing.current.map_or(6, |c| c.duration_ticks),
                        time,
                    )
                } else {
                    attack
                };
                let how = if native && !left {
                    view::Drawn::Legacy {
                        block: native_block,
                    }
                } else {
                    view::Drawn::Modern
                };
                let shape = |block: bool| -> glam::Mat4 {
                    let legacy = (plain && main && !left && flat)
                        .then(|| view::legacy_pose(
                            pose,
                            attack,
                            s,
                            block,
                            time,
                            how,
                            view::view_of(left),
                        ))
                        .flatten();
                    if let Some(legacy) = legacy {
                        return legacy;
                    }
                    let swapped = match view::swing_swap(left, attack, s, sword, block)
                        .filter(|_| plain && !native)
                    {
                        Some(swap) => pose * swap,
                        None => pose,
                    };
                    view::posed(left, swapped)
                };
                let posed = shape(block);
                let lit = view::hand_light(light);
                if posed == game_pose && lit == light {
                    return std::ops::ControlFlow::Continue(());
                }
                this.submit_item_lit(
                    state,
                    posed,
                    lit,
                    diffuse_lights(this.dim == Dim::Nether),
                    out,
                );
                std::ops::ControlFlow::Break(())
            }
            sunset_hand_item(&*self, state, pose, light, &mut *out)
        } {
            return __horizon_mixins_return;
        }
        let lights = diffuse_lights(self.nether_lighting());
        self.submit_item_lit(state, pose, light, lights, out);
    }
    /// [`Self::submit_item_world`] shaded by other diffuse lights (a
    /// `Lighting.Entry` other than the level's), in the pose's output space.
    pub(super) fn submit_item_lit(
        &self,
        state: &ItemRenderState,
        pose: glam::Mat4,
        light: u32,
        lights: (glam::Vec3, glam::Vec3),
        out: &mut Vec<Vertex>,
    ) {
        let mut sink = WorldSink {
            out,
            light: item_hdr_light(state, light),
            lights: (state.hdr_light == 0).then_some(lights),
            glint: self.glint(),
            tier: state.hdr_tier,
        };
        state
            .submit(
                &self.pack.item_renderer,
                &self.gui.atlas.sprites,
                &self.item_host(),
                pose,
                &mut sink,
            );
    }
    /// [`Self::submit_item_world`] without the level's diffuse shading.
    pub(super) fn submit_item_unlit(
        &self,
        state: &ItemRenderState,
        pose: glam::Mat4,
        light: u32,
        out: &mut Vec<Vertex>,
    ) {
        let mut sink = WorldSink {
            out,
            light: item_hdr_light(state, light),
            lights: None,
            glint: self.glint(),
            tier: state.hdr_tier,
        };
        state
            .submit(
                &self.pack.item_renderer,
                &self.gui.atlas.sprites,
                &self.item_host(),
                pose,
                &mut sink,
            );
    }
    /// [`Self::submit_item_lit`] of `state` under each of `poses` on the GPU path: the mark
    /// before its draws and their bounds, or `None` (nothing drawn) when the state cannot
    /// take it.
    pub(super) fn submit_item_skinned(
        &self,
        state: &ItemRenderState,
        poses: &[glam::Mat4],
        light: u32,
        lights: (glam::Vec3, glam::Vec3),
    ) -> Option<((usize, usize, u32), Option<(glam::Vec3, glam::Vec3)>)> {
        let mut models = self.skin_models.borrow_mut();
        let mut frame = self.skin_frame.borrow_mut();
        let mark = frame.mark();
        let mut skin = horizon_client::mobs::skin::Skin::new(&mut models, &mut frame);
        for &pose in poses {
            if !self.skin_item(state, pose, light, lights, &mut skin) {
                return None;
            }
        }
        Some((mark, skin.bounds))
    }
    /// One pose of [`Self::submit_item_skinned`] into an entity's open
    /// `skin` (its bounds grow by the item's): false, with nothing drawn,
    /// when the item cannot take the GPU path.
    pub(super) fn skin_item(
        &self,
        state: &ItemRenderState,
        pose: glam::Mat4,
        light: u32,
        lights: (glam::Vec3, glam::Vec3),
        skin: &mut horizon_client::mobs::skin::Skin,
    ) -> bool {
        let sink = (
            item_hdr_light(state, light),
            (state.hdr_light == 0).then_some(lights),
            state.hdr_tier,
        );
        let renderer = &self.pack.item_renderer;
        state.submit_skinned(renderer, &self.gui.atlas.sprites, pose, sink, skin)
    }
    /// [`Self::submit_item_world`] of `state` under `pose` on the GPU path, its draws counted
    /// with an entity's (`skinned`: its mark and bounds, `cull_skinned`'s); false, with
    /// nothing drawn, when the state cannot take it.
    pub(super) fn skin_item_into(
        &self,
        state: &ItemRenderState,
        pose: glam::Mat4,
        light: u32,
        skinned: &mut super::creatures::Skinned,
    ) -> bool {
        let lights = diffuse_lights(self.nether_lighting());
        let Some((mark, bounds)) = self
            .submit_item_skinned(state, &[pose], light, lights) else {
            return false;
        };
        *skinned = Some(
            match skinned.take() {
                Some((first, before)) => {
                    (
                        first,
                        match (before, bounds) {
                            (Some((a, b)), Some((c, d))) => Some((a.min(c), b.max(d))),
                            (one, other) => one.or(other),
                        },
                    )
                }
                None => (mark, bounds),
            },
        );
        true
    }
    /// `ItemEntityRenderer.submit` for every dropped item.
    pub(super) fn dropped_item_geometry(
        &self,
        camera: glam::DVec3,
        alpha: f64,
        out: &mut Vec<Vertex>,
    ) {
        let live = self
            .dropped
            .iter()
            .map(|item| {
                let (p, q) = (item.previous, item.position);
                let at = [
                    p.x + (q.x - p.x) * alpha,
                    p.y + (q.y - p.y) * alpha,
                    p.z + (q.z - p.z) * alpha,
                ];
                (item, at, alpha as f32, true)
            });
        let taken = self
            .pickups
            .live
            .iter()
            .filter_map(|p| match &p.taken {
                super::pickups::Taken::Item(item) => {
                    Some((&**item, super::pickups::position(p, alpha), 1.0, false))
                }
                _ => None,
            });
        let range = super::entity_culling::default_range(
            glam::DVec3::splat(0.25),
            self.view_scale(),
        );
        let skinning = self.skinning();
        let lights = diffuse_lights(self.nether_lighting());
        for (item, at, partial, entity) in live.chain(taken) {
            let entity_start = out.len();
            self.mark_motion(entity_start, (2 << 32) | u64::from(item.id));
            let q = item.position;
            let pos = glam::DVec3::from_array(at) - camera;
            let far = if entity {
                (glam::DVec3::new(q.x, q.y, q.z) - camera).length_squared()
                    >= range * range
            } else {
                pos.length_squared() > 64.0 * 64.0
            };
            if far {
                continue;
            }
            if entity && let Some(view) = &self.view_frustum {
                let (lo, hi) = dropped_item_cull_box(
                    glam::DVec3::new(item.previous.x, item.previous.y, item.previous.z),
                    glam::DVec3::new(q.x, q.y, q.z),
                    alpha,
                    camera,
                );
                if !view.intersects(lo, hi) {
                    continue;
                }
            }
            let owner = owner_at([q.x, q.y, q.z], 0.0, "minecraft:item");
            let state = self
                .resolve_item(
                    &item.stack,
                    DisplayContext::Ground,
                    Some(&owner),
                    item.id as i32,
                );
            if state.is_empty() {
                continue;
            }
            let age = item.age as f32 + partial;
            let (lo, hi) = state.bounding_box();
            let min_offset_y = -lo.y + 0.0625;
            let bob = (age / 10.0 + item.bob_offset).sin() * 0.1 + 0.1;
            let spin = item_render::item_spin(age, item.bob_offset);
            let pose = glam::Mat4::from_translation(pos.as_vec3())
                * glam::Mat4::from_translation(
                    glam::Vec3::new(0.0, bob + min_offset_y, 0.0),
                ) * glam::Mat4::from_rotation_y(spin);
            let packed = if entity {
                let probe = item.light_probe(alpha);
                item.render_light(
                    self
                        .world
                        .light(
                            probe[0].floor() as i32,
                            probe[1].floor() as i32,
                            probe[2].floor() as i32,
                        ),
                )
            } else {
                vertex_light(
                    self
                        .world
                        .light(
                            q.x.floor() as i32,
                            (q.y + 0.1).floor() as i32,
                            q.z.floor() as i32,
                        ),
                )
            };
            let seed = i64::from(self.pack.item_renderer.item_id(&item.stack.item))
                + i64::from(item.stack.damage);
            let copies = item_render::cluster_poses_with_depth(
                pose,
                item_render::rendered_amount(item.stack.count),
                seed,
                hi.z - lo.z,
            );
            let culling = entity
                .then(|| {
                    if self.settings.horizon.entity_culling == EntityCullingMode::Off {
                        return EntityCullDecision::Bypass;
                    }
                    state
                        .posed_bounds(&copies)
                        .map(|(lo, hi)| {
                            self.entity_cull_decision_in_bounds(
                                item,
                                Aabb::new(
                                    Vec3::new(
                                        camera.x + f64::from(lo.x),
                                        camera.y + f64::from(lo.y),
                                        camera.z + f64::from(lo.z),
                                    ),
                                    Vec3::new(
                                        camera.x + f64::from(hi.x),
                                        camera.y + f64::from(hi.y),
                                        camera.z + f64::from(hi.z),
                                    ),
                                ),
                            )
                        })
                        .unwrap_or_else(|| {
                            self.entity_cull_decision(item, f64::from(partial))
                        })
                });
            if culling.is_some_and(|decision| decision.hides_entity()) {
                continue;
            }
            let outline = self.thing_outline(item.id, "minecraft:item");
            let skinned = skinning
                .then(|| self.submit_item_skinned(&state, &copies, packed, lights))
                .flatten();
            if skinned.is_none() {
                for &copy in &copies {
                    self.submit_item_world(&state, copy, packed, out);
                }
            }
            if let Some(rgb) = outline {
                self.mark_outline(&out[entity_start..], rgb);
                if let Some(((first, _, _), _)) = skinned {
                    let color = horizon_client::mobs::render::abgr(rgb);
                    self.skin_frame.borrow_mut().outline_since(first, color);
                }
            } else if let Some(culling) = culling {
                self.finish_entity_occlusion(culling, item, out, entity_start);
            } else {
                self.cull_skinned(out, entity_start, skinned);
            }
        }
    }
}
/// `EntityRenderer.shouldRender`'s box for a dropped item, in camera space:
/// `EntityRenderer.getBoundingBoxForCulling(entity, partialTick)` is
/// `Entity.getInterpolatedBoundingBox(partialTick)` - the item's 0.25 box at
/// the position the frame draws, `ItemEntity` overriding neither - grown by 0.5
/// all round before the frustum test.
fn dropped_item_cull_box(
    previous: glam::DVec3,
    position: glam::DVec3,
    alpha: f64,
    camera: glam::DVec3,
) -> (glam::Vec3, glam::Vec3) {
    let feet = (previous.lerp(position, alpha) - camera).as_vec3();
    let half = glam::Vec3::new(0.125 + 0.5, 0.5, 0.125 + 0.5);
    (feet - half, feet + glam::Vec3::new(half.x, 0.25 + 0.5, half.z))
}
/// The packed light of an item (sky low, block above it: `vertex_light`, entity.wgsl), its block
/// light raised to what it glows with in HDR output.
fn item_hdr_light(state: &ItemRenderState, light: u32) -> u32 {
    let block = ((light >> 8) & 0xff).max(u32::from(state.hdr_light) * 16);
    (light & !0xff00) | block << 8
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    /// `EntityRenderer.shouldRender` culls with `getBoundingBoxForCulling(entity,
    /// partialTick)`, which is `Entity.getInterpolatedBoundingBox(partialTick)`:
    /// an item's 0.25 box at the position the frame draws, grown by 0.5. The box
    /// was built from the tick's own position, so an item crossing the frustum
    /// edge mid-tick disappeared for the frames before it got there.
    #[test]
    fn a_dropped_item_is_culled_at_the_frame_s_position() {
        let camera = glam::DVec3::ZERO;
        let previous = glam::DVec3::new(0.0, 64.0, 0.0);
        let position = glam::DVec3::new(1.0, 64.0, 0.0);
        let bounds = |alpha: f64| {
            let (lo, hi) = dropped_item_cull_box(previous, position, alpha, camera);
            (lo.to_array(), hi.to_array())
        };
        assert_eq!(bounds(0.0), ([- 0.625, 63.5, - 0.625], [0.625, 64.75, 0.625]));
        assert_eq!(bounds(0.5), ([- 0.125, 63.5, - 0.625], [1.125, 64.75, 0.625]));
        assert_eq!(bounds(1.0), ([0.375, 63.5, - 0.625], [1.625, 64.75, 0.625]));
        let (lo, hi) = dropped_item_cull_box(
            glam::DVec3::new(0.0, 66.0, 0.0),
            glam::DVec3::new(0.0, 65.0, 0.0),
            0.5,
            camera,
        );
        assert!((lo.y - 65.0).abs() < 1e-6);
        assert!((hi.y - 66.25).abs() < 1e-6);
    }
    #[test]
    fn civil_dates_follow_the_gregorian_calendar() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(19_716), (2023, 12, 25));
        assert_eq!(civil(11_016), (2000, 2, 29));
    }
    #[test]
    fn a_shield_is_used_for_the_vanilla_s_time() {
        let mut shield = Components::new();
        shield.insert("minecraft:blocks_attacks".into(), json!({}));
        assert_eq!(use_duration(ComponentsView::of(& shield)), 72_000);
        let mut food = Components::new();
        food.insert("minecraft:consumable".into(), json!({ "consume_seconds" : 1.6 }));
        assert_eq!(use_duration(ComponentsView::of(& food)), 32);
        let mut bow = Components::new();
        bow.insert("minecraft:item_model".into(), json!("minecraft:bow"));
        assert_eq!(use_duration(ComponentsView::of(& bow)), 72_000);
        assert_eq!(use_duration(ComponentsView::of(& Components::new())), 0);
    }
}
