//! Sorting (`sort`, `sortKey`; #332, `docs/extras.md`), off by default: a small button by a
//! menu's storage slots and one by the inventory's (an optional key sorts the part under the
//! cursor). The clicks come from `inv_sort::plan` and go through the Mouse Tweaks' ordinary slot
//! clicks (`mouse_tweaks.rs`), so they work wherever vanilla's do.
use crate::game::Game;
use horizon_client::{
    hud::{HudBatch, rgba},
    inv_sort, items::ItemStack, keybinds, menu::Input, stations::hover_name,
};
use winit::{event::MouseButton, keyboard::KeyCode};
/// A sort button's side in GUI pixels.
const SORT_BUTTON: f32 = 9.0;
impl Game<'_> {
    /// The sort buttons of the open screen, with the extra on: their
    /// corner (from the screen's) and the slots each sorts. Any menu screen
    /// gets them, by its slots' roles (`native_slot_roles`): one by the top
    /// right of its storage slots (a chest's title line) when it has some,
    /// one by the main inventory's (the "Inventory" label's line).
    fn sort_buttons(&self) -> Vec<((f32, f32), Vec<usize>)> {
        use crate::game::NativeSlotRole as Role;
        if !self.settings.extras.sort {
            return Vec::new();
        }
        let roles = self.native_slot_roles();
        let of = |role| -> Vec<usize> {
            roles.iter().filter(|(_, r)| *r == role).map(|(s, _)| *s).collect()
        };
        let right = |slots: &[usize]| {
            slots.iter().map(|&s| self.slot_position(s).0 + 8.0).fold(f32::MIN, f32::max)
        };
        let mut buttons = Vec::new();
        let storage = of(Role::Storage);
        if !storage.is_empty() {
            let top = storage
                .iter()
                .map(|&s| self.slot_position(s).1 - 13.0)
                .fold(f32::MAX, f32::min);
            buttons.push(((right(&storage), top), storage));
        }
        let main = of(Role::Inventory);
        if !main.is_empty() {
            let y = self
                .native_screen_inventory_y()
                .map_or(self.screen_height() - 95.0, |y| y - 13.0);
            buttons.push(((right(&main), y), main));
        }
        buttons
    }
    /// The sort buttons: a bordered square with three bars, lighter under
    /// the cursor.
    pub(in crate::game) fn draw_sort_buttons(
        &self,
        batch: &mut HudBatch,
        ox: f32,
        oy: f32,
    ) {
        if let ::core::ops::ControlFlow::Break(__horizon_mixins_return) = {
            fn sunset_sort_draw(
                game: &crate::game::Game<'_>,
                batch: &mut horizon_client::hud::HudBatch,
                ox: f32,
                oy: f32,
            ) -> std::ops::ControlFlow<()> {
                if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() {
                    return std::ops::ControlFlow::Continue(());
                }
                crate::game::sunset_extras::note_hover(
                    game.slot_at(batch, game.cursor.0, game.cursor.1),
                );
                if crate::game::sunset_extras::on() {
                    crate::game::extras::sorting::sunset_sort_buttons::draw(
                        game,
                        batch,
                        ox,
                        oy,
                    );
                    return std::ops::ControlFlow::Break(());
                }
                std::ops::ControlFlow::Continue(())
            }
            sunset_sort_draw(&*self, &mut *batch, ox, oy)
        } {
            return __horizon_mixins_return;
        }
        let atlas = &self.gui.atlas;
        for ((x, y), _) in self.sort_buttons() {
            let (x, y) = (ox + x, oy + y);
            let (cx, cy) = self.cursor;
            let hovered = cx >= x && cx < x + SORT_BUTTON && cy >= y
                && cy < y + SORT_BUTTON;
            let face = if hovered {
                rgba(220, 220, 220, 255)
            } else {
                rgba(160, 160, 160, 255)
            };
            batch.fill(atlas, x, y, SORT_BUTTON, SORT_BUTTON, rgba(55, 55, 55, 255));
            batch
                .fill(
                    atlas,
                    x + 1.0,
                    y + 1.0,
                    SORT_BUTTON - 2.0,
                    SORT_BUTTON - 2.0,
                    face,
                );
            for (i, w) in [5.0, 4.0, 3.0].into_iter().enumerate() {
                let by = y + 2.0 + 2.0 * i as f32;
                batch.fill(atlas, x + 2.0, by, w, 1.0, rgba(55, 55, 55, 255));
            }
        }
    }
    /// A left press on a sort button sorts its slots (true).
    pub(in crate::game) fn sort_press(&mut self) -> bool {
        if let ::core::ops::ControlFlow::Break(__horizon_mixins_return) = {
            fn sunset_sort_press(
                game: &mut crate::game::Game<'_>,
            ) -> std::ops::ControlFlow<bool> {
                if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() {
                    return std::ops::ControlFlow::Continue(());
                }
                if crate::game::sunset_extras::on() {
                    return std::ops::ControlFlow::Break(
                        crate::game::extras::sorting::sunset_sort_buttons::press(game),
                    );
                }
                std::ops::ControlFlow::Continue(())
            }
            sunset_sort_press(&mut *self)
        } {
            return __horizon_mixins_return;
        }
        let Some(r) = self.renderer.as_ref() else {
            return false;
        };
        let batch = HudBatch::new([r.config.width, r.config.height]);
        let (ox, oy) = self.inventory_origin(&batch);
        let (cx, cy) = self.cursor;
        let hit = self
            .sort_buttons()
            .into_iter()
            .find(|((x, y), _)| {
                let (x, y) = (ox + x, oy + y);
                cx >= x && cx < x + SORT_BUTTON && cy >= y && cy < y + SORT_BUTTON
            });
        let Some((_, slots)) = hit else {
            return false;
        };
        self.sort_slots(&slots);
        true
    }
    /// The sort key: sorts the part under the cursor (the container when
    /// the cursor is over no slot).
    pub(in crate::game) fn sort_key_press(&mut self, key: KeyCode) -> bool {
        keybinds::key_name(key).is_some_and(|name| self.sort_input(name))
    }
    pub(in crate::game) fn sort_mouse_press(&mut self, button: MouseButton) -> bool {
        if let ::core::ops::ControlFlow::Break(__horizon_mixins_return) = {
            fn sunset_press(
                game: &mut crate::game::Game<'_>,
                button: winit::event::MouseButton,
            ) -> std::ops::ControlFlow<bool> {
                if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() {
                    return std::ops::ControlFlow::Continue(());
                }
                if crate::game::sunset::gui_open() {
                    crate::game::sunset::gui_press(game, button);
                    return std::ops::ControlFlow::Break(true);
                }
                if crate::game::sunset_loadouts::press(
                    game,
                    button == winit::event::MouseButton::Right,
                ) {
                    return std::ops::ControlFlow::Break(true);
                }
                if crate::game::sunset_render::peek_press(
                    game,
                    button == winit::event::MouseButton::Middle,
                ) {
                    return std::ops::ControlFlow::Break(true);
                }
                std::ops::ControlFlow::Continue(())
            }
            sunset_press(&mut *self, button)
        } {
            return __horizon_mixins_return;
        }
        self.sort_input(&keybinds::mouse_name(button))
    }
    fn sort_input(&mut self, name: &str) -> bool {
        let e = &self.settings.extras;
        if !e.sort || e.sort_key != name || self.recipe_search_focused() {
            return false;
        }
        let buttons = self.sort_buttons();
        let hovered = self.inventory_target().0;
        let slots = buttons
            .iter()
            .find(|(_, slots)| hovered.is_some_and(|h| slots.contains(&h)))
            .or(buttons.first())
            .map(|(_, slots)| slots.clone());
        let Some(slots) = slots else {
            return false;
        };
        self.sort_slots(&slots);
        true
    }
    /// Sorts `slots` with ordinary left clicks (needs an empty hand).
    fn sort_slots(&mut self, slots: &[usize]) {
        if let ::core::ops::ControlFlow::Break(__horizon_mixins_return) = {
            fn sunset_sort(
                game: &mut crate::game::Game<'_>,
                slots: &[usize],
            ) -> std::ops::ControlFlow<()> {
                if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() {
                    return std::ops::ControlFlow::Continue(());
                }
                if crate::game::sunset_extras::sort(game, slots) {
                    return std::ops::ControlFlow::Break(());
                }
                std::ops::ControlFlow::Continue(())
            }
            sunset_sort(&mut *self, slots)
        } {
            return __horizon_mixins_return;
        }
        if self.carried.is_some() {
            return;
        }
        let views: Vec<Option<ItemStack>> = slots
            .iter()
            .map(|&s| self.slot_view(s))
            .collect();
        let clicks = {
            let rank = |s: &ItemStack| {
                let order = self.creative_screen.order_of(s).unwrap_or(usize::MAX);
                (order, hover_name(&self.pack.names, s))
            };
            let max = |s: &ItemStack| self.max_stack(s);
            inv_sort::plan(&views, &rank, &max)
        };
        for i in clicks {
            self.tweak_click(slots[i], 0, Input::Pickup);
        }
    }
}
pub(crate) mod sunset_sort_buttons {
    use super::Game;
    use horizon_client::hud::{HudBatch, rgba};
    const SIDE: f32 = 9.0;
    const STEP: f32 = 10.0;
    const X: f32 = 172.0;
    fn buttons(game: &Game<'_>) -> Vec<((f32, f32), usize, Vec<usize>)> {
        let mut parts = game.sort_buttons();
        if game.screen == crate::game::Screen::Inventory && game.station.is_none()
            && !game.mount_active()
            && !parts.iter().any(|(_, slots)| slots.first() == Some(&9))
        {
            parts.push(((160.0, game.screen_height() - 95.0), (9..36).collect()));
        }
        parts
            .into_iter()
            .flat_map(|((_, y), slots)| {
                (0..3).map(move |op| ((X, y + op as f32 * STEP), op, slots.clone()))
            })
            .collect()
    }
    pub(crate) fn draw(game: &Game<'_>, batch: &mut HudBatch, ox: f32, oy: f32) {
        let atlas = &game.gui.atlas;
        let dark = rgba(55, 55, 55, 255);
        for ((x, y), op, _) in buttons(game) {
            let (x, y) = (ox + x, oy + y);
            let (cx, cy) = game.cursor;
            let hovered = cx >= x && cx < x + SIDE && cy >= y && cy < y + SIDE;
            batch.fill(atlas, x, y, SIDE, SIDE, dark);
            batch
                .fill(
                    atlas,
                    x + 1.0,
                    y + 1.0,
                    SIDE - 2.0,
                    SIDE - 2.0,
                    if hovered {
                        rgba(220, 220, 220, 255)
                    } else {
                        rgba(160, 160, 160, 255)
                    },
                );
            match op {
                0 => {
                    for (i, w) in [5.0, 4.0, 3.0].into_iter().enumerate() {
                        batch
                            .fill(
                                atlas,
                                x + 2.0,
                                y + 2.0 + 2.0 * i as f32,
                                w,
                                1.0,
                                dark,
                            );
                    }
                }
                1 => {
                    batch.fill(atlas, x + 2.0, y + 4.0, 5.0, 1.0, dark);
                    batch.fill(atlas, x + 4.0, y + 2.0, 1.0, 5.0, dark);
                }
                _ => {
                    batch.fill(atlas, x + 2.0, y + 4.0, 5.0, 1.0, dark);
                    batch.fill(atlas, x + 5.0, y + 3.0, 1.0, 3.0, dark);
                    batch.fill(atlas, x + 4.0, y + 2.0, 1.0, 1.0, dark);
                    batch.fill(atlas, x + 4.0, y + 6.0, 1.0, 1.0, dark);
                }
            }
        }
    }
    pub(crate) fn press(game: &mut Game<'_>) -> bool {
        let Some(r) = game.renderer.as_ref() else {
            return false;
        };
        let batch = HudBatch::new([r.config.width, r.config.height]);
        let (ox, oy) = game.inventory_origin(&batch);
        let (cx, cy) = game.cursor;
        let hit = buttons(game)
            .into_iter()
            .find(|((x, y), ..)| {
                let (x, y) = (ox + x, oy + y);
                cx >= x && cx < x + SIDE && cy >= y && cy < y + SIDE
            });
        let Some((_, op, slots)) = hit else {
            return false;
        };
        crate::game::sunset_extras::operate(game, op, &slots);
        true
    }
}
