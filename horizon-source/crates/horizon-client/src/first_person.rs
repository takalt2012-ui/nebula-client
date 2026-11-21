//! The local player's hand: the arm swing (`LivingEntity.SwingState`,
//! `getModifiedSwingDuration`), the first-person equip heights
//! (`FirstPersonHandsAndItems.tick` and `itemUsed`), and the click handling
//! that starts swings: `Minecraft.startAttack`, `continueAttack`,
//! `startUseItem` and `handleKeybinds`' attack and use part, with
//! `MultiPlayerGameMode`'s block breaking (`startDestroyBlock`,
//! `continueDestroyBlock`, `stopDestroyBlock`).
//!
//! What those read and do in the world is behind [`HandWorld`].
/// The ranged item state read by the first-person hand renderer.
#[derive(Clone, Copy, Debug)]
pub enum ItemPose {
    Swing,
    /// A STAB swing (`SwingAnimationType` 2): a spear's jab.
    Stab,
    Using,
    Eat { remaining: f32, duration: f32, partial: f32 },
    Trident { ticks_used: f32 },
    /// Riptide's spin (`isAutoSpinAttack`): the item held up like a fork.
    SpinAttack,
    Bow { ticks_used: f32 },
    /// A brush in use (`ItemUseAnimation.BRUSH`): the ticks left of the use
    /// and the partial tick.
    Brush { remaining: f32, partial: f32 },
    /// A spear being charged (`ItemUseAnimation.SPEAR`): the time held and
    /// the ticks since its last hit's feedback.
    Spear { kinetic: crate::spear::Kinetic, time_held: f32, ticks_since_hit: f32 },
    Crossbow { ticks_used: Option<f32>, charge_duration: f32, charged: bool },
}
/// `submitArmWithItem`: the pose of an item in the hand on the `left` or
/// right arm, before the model transform. Only the main hand's charged
/// crossbow is turned aside.
pub fn item_pose(
    left: bool,
    inverse_height: f32,
    attack: f32,
    pose: ItemPose,
) -> glam::Mat4 {
    use glam::{Mat4, Vec3};
    use horizon_core::geometry::trig::Mth;
    let invert = if left { -1.0 } else { 1.0 };
    let pi = std::f32::consts::PI;
    let mut m = Mat4::from_translation(
        Vec3::new(invert * 0.56, -0.52 - inverse_height * 0.6, -0.72),
    );
    if let ItemPose::Eat { remaining, duration, partial } = pose {
        let time = remaining - partial + 1.0;
        let fraction = time / duration;
        let mut eat = Mat4::IDENTITY;
        if fraction < 0.8 {
            eat
                *= Mat4::from_translation(
                    Vec3::new(0.0, ((time / 4.0 * pi).mcos() * 0.1).abs(), 0.0),
                );
        }
        let jiggle = 1.0 - horizon_core::jmath::pow(f64::from(fraction), 27.0) as f32;
        eat
            *= Mat4::from_translation(
                Vec3::new(jiggle * 0.6 * invert, jiggle * -0.5, 0.0),
            );
        eat *= Mat4::from_rotation_y((invert * jiggle * 90.0).to_radians());
        eat *= Mat4::from_rotation_x((jiggle * 10.0).to_radians());
        eat *= Mat4::from_rotation_z((invert * jiggle * 30.0).to_radians());
        return eat * m;
    }
    if matches!(pose, ItemPose::Using) {
        return m;
    }
    if let ItemPose::Brush { remaining, partial } = pose {
        return m * brush_transform(left, remaining, partial);
    }
    if let ItemPose::Spear { kinetic, time_held, ticks_since_hit } = pose {
        return Mat4::from_translation(Vec3::new(invert * 0.56, -0.52, -0.72))
            * crate::spear::first_person_use(
                &kinetic,
                ticks_since_hit,
                time_held,
                !left,
            );
    }
    if matches!(pose, ItemPose::Stab) {
        return m * crate::spear::first_person_attack(attack, !left);
    }
    if matches!(pose, ItemPose::SpinAttack) {
        return m * Mat4::from_translation(Vec3::new(invert * -0.4, 0.8, 0.3))
            * Mat4::from_rotation_y((invert * 65.0f32).to_radians())
            * Mat4::from_rotation_z((invert * -85.0f32).to_radians());
    }
    let charging = match pose {
        ItemPose::Bow { ticks_used } => {
            let power = ticks_used / 20.0;
            Some((ticks_used, ((power * power + power * 2.0) / 3.0).min(1.0), false))
        }
        ItemPose::Crossbow {
            ticks_used: Some(ticks),
            charge_duration,
            charged: false,
        } => Some((ticks, (ticks / charge_duration).min(1.0), true)),
        ItemPose::Trident { ticks_used } => {
            Some((ticks_used, (ticks_used / 10.0).min(1.0), false))
        }
        _ => None,
    };
    if let Some((ticks, power, crossbow)) = charging {
        let trident = matches!(pose, ItemPose::Trident { .. });
        let (x, y, z, pitch, yaw) = if trident {
            (-0.5, 0.7, 0.1, -55.0f32, 35.3f32)
        } else if crossbow {
            (-0.4785682, -0.094387, 0.05731531, -11.935f32, 65.3f32)
        } else {
            (-0.2785682, 0.18344387, 0.15731531, -13.935f32, 35.3f32)
        };
        m *= Mat4::from_translation(Vec3::new(invert * x, y, z));
        m *= Mat4::from_rotation_x(pitch.to_radians());
        m *= Mat4::from_rotation_y((invert * yaw).to_radians());
        m *= Mat4::from_rotation_z((invert * -9.785f32).to_radians());
        if power > 0.1 {
            let shake = ((ticks - 0.1) * 1.3).msin() * (power - 0.1);
            m *= Mat4::from_translation(Vec3::new(0.0, shake * 0.004, 0.0));
        }
        m
            *= Mat4::from_translation(
                Vec3::new(0.0, 0.0, power * if trident { 0.2 } else { 0.04 }),
            );
        m *= Mat4::from_scale(Vec3::new(1.0, 1.0, 1.0 + power * 0.2));
        m *= Mat4::from_rotation_y((invert * -45.0f32).to_radians());
    } else {
        let root = attack.sqrt();
        m
            *= Mat4::from_translation(
                Vec3::new(
                    invert * -0.4 * (root * pi).msin(),
                    0.2 * (root * std::f32::consts::TAU).msin(),
                    -0.2 * (attack * pi).msin(),
                ),
            );
        m
            *= Mat4::from_rotation_y(
                (invert * (45.0 - (attack * attack * pi).msin() * 20.0)).to_radians(),
            );
        m *= Mat4::from_rotation_z((invert * (root * pi).msin() * -20.0).to_radians());
        m *= Mat4::from_rotation_x(((root * pi).msin() * -80.0).to_radians());
        m *= Mat4::from_rotation_y((invert * -45.0f32).to_radians());
        if matches!(pose, ItemPose::Crossbow { charged : true, .. }) && attack < 0.001 {
            m *= Mat4::from_translation(Vec3::new(invert * -0.641864, 0.0, 0.0));
            m *= Mat4::from_rotation_y((invert * 10.0f32).to_radians());
        }
    }
    m
}
/// Combat feel Legacy's first-person item (`docs/pvp.md`): Minecraft 1.8.9's
/// `ItemRenderer.renderItemInFirstPerson` for a flat item, drawn through 26.3's
/// model. 1.8.9 posed the item (`transformFirstPersonItem`: the arm's place,
/// 45° about Y, the swing's turns, a 0.4 scale), then applied its own
/// `firstperson` display (`[0, -135, 25]`, `[0, 4, 2]`, `1.7`, rotated Y, X,
/// Z) and `RenderItem`'s 0.5 scale. 26.3 applies `firstperson_righthand`
/// (`[0, -90, 25]`, `[1.13, 3.2, 1.13]`, `0.68`) itself after this pose, so
/// the pose ends with its inverse ([`MODERN_FLAT_DISPLAY`]). `block` is a
/// sword held up while use is held: 1.8.9's `doBlockTransformations`, with
/// the swing's turns kept as 1.7.10 kept them (hitting while blocking swings
/// the sword). None for the poses 1.8.9 had no form of (crossbow, trident,
/// spear, brush), which stay modern.
pub fn legacy_item_pose(
    left: bool,
    inverse_height: f32,
    attack: f32,
    pose: ItemPose,
    block: bool,
) -> Option<glam::Mat4> {
    use glam::{Mat4, Vec3};
    let deg = |d: f32| d.to_radians();
    let pi = std::f32::consts::PI;
    let sin = |v: f32| v.sin();
    let first_person = |swing: f32| {
        let f = sin(swing * swing * pi);
        let f1 = sin(swing.sqrt() * pi);
        Mat4::from_translation(Vec3::new(0.56, -0.52 - inverse_height * 0.6, -0.72))
            * Mat4::from_rotation_y(deg(45.0)) * Mat4::from_rotation_y(deg(f * -20.0))
            * Mat4::from_rotation_z(deg(f1 * -20.0))
            * Mat4::from_rotation_x(deg(f1 * -80.0)) * Mat4::from_scale(Vec3::splat(0.4))
    };
    let m = match pose {
        _ if block => {
            first_person(attack) * Mat4::from_translation(Vec3::new(-0.5, 0.2, 0.0))
                * Mat4::from_rotation_y(deg(30.0)) * Mat4::from_rotation_x(deg(-80.0))
                * Mat4::from_rotation_y(deg(60.0))
        }
        ItemPose::Swing => {
            let root = attack.sqrt();
            Mat4::from_translation(
                Vec3::new(
                    -0.4 * sin(root * pi),
                    0.2 * sin(root * pi * 2.0),
                    -0.2 * sin(attack * pi),
                ),
            ) * first_person(attack)
        }
        ItemPose::Using => first_person(0.0),
        ItemPose::Eat { remaining, duration, partial } => {
            let time = remaining - partial + 1.0;
            let fraction = time / duration;
            let bob = if fraction >= 0.8 {
                0.0
            } else {
                ((time / 4.0 * pi).cos() * 0.1).abs()
            };
            let j = 1.0 - horizon_core::jmath::pow(f64::from(fraction), 27.0) as f32;
            Mat4::from_translation(Vec3::new(0.0, bob, 0.0))
                * Mat4::from_translation(Vec3::new(j * 0.6, j * -0.5, 0.0))
                * Mat4::from_rotation_y(deg(j * 90.0))
                * Mat4::from_rotation_x(deg(j * 10.0))
                * Mat4::from_rotation_z(deg(j * 30.0)) * first_person(0.0)
        }
        ItemPose::Bow { ticks_used } => {
            let power = (ticks_used / 20.0 * (ticks_used / 20.0)
                + ticks_used / 20.0 * 2.0) / 3.0;
            let power = power.min(1.0);
            let mut m = first_person(0.0) * Mat4::from_rotation_z(deg(-18.0))
                * Mat4::from_rotation_y(deg(-12.0)) * Mat4::from_rotation_x(deg(-8.0))
                * Mat4::from_translation(Vec3::new(-0.9, 0.2, 0.0));
            if power > 0.1 {
                let shake = sin((ticks_used - 0.1) * 1.3) * (power - 0.1);
                m *= Mat4::from_translation(Vec3::new(0.0, shake * 0.01, 0.0));
            }
            m * Mat4::from_translation(Vec3::new(0.0, 0.0, power * 0.1))
                * Mat4::from_scale(Vec3::new(1.0, 1.0, 1.0 + power * 0.2))
        }
        _ => return None,
    };
    let legacy_display = Mat4::from_translation(Vec3::new(0.0, 4.0, 2.0) / 16.0)
        * Mat4::from_rotation_y(deg(-135.0)) * Mat4::from_rotation_z(deg(25.0))
        * Mat4::from_scale(Vec3::splat(1.7)) * Mat4::from_scale(Vec3::splat(0.5));
    let right = m * legacy_display * modern_flat_display(false).inverse();
    let mirror = Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0));
    Some(if left { mirror * right * mirror } else { right })
}
/// 26.3's `firstperson_righthand` display of a flat item (`item/generated` and
/// `item/handheld`), on the `left` hand mirrored as `ItemTransform.apply` does,
/// without the model's centring translation.
pub fn modern_flat_display(left: bool) -> glam::Mat4 {
    use glam::{Mat4, Vec3};
    let s = if left { -1.0 } else { 1.0 };
    Mat4::from_translation(Vec3::new(s * 1.13, 3.2, 1.13) / 16.0)
        * Mat4::from_rotation_y((s * -90.0f32).to_radians())
        * Mat4::from_rotation_z((s * 25.0f32).to_radians())
        * Mat4::from_scale(Vec3::splat(0.68))
}
/// Whether a model layer's matrix is 26.3's flat-item first-person display (a resource
/// pack's own display keeps the modern pose).
pub fn is_modern_flat_display(matrix: &glam::Mat4, left: bool) -> bool {
    let expected = modern_flat_display(left)
        * glam::Mat4::from_translation(glam::Vec3::splat(-0.5));
    matrix.abs_diff_eq(expected, 1e-4)
}
/// How `submitArmWithItem` holds a stack with a `minecraft:map_id`: in both
/// hands when it is the main hand's and the off hand is empty
/// (`renderTwoHandedMap`), else in its own hand (`renderOneHandedMap`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapHold {
    TwoHanded,
    OneHanded,
}
impl MapHold {
    pub fn of(hand: Hand, off_hand_empty: bool) -> Self {
        if hand == Hand::Main && off_hand_empty {
            Self::TwoHanded
        } else {
            Self::OneHanded
        }
    }
}
/// `FirstPersonHandsAndItemsRenderer.applyBrushTransform`: the brush in use,
/// swept back and forth by the ticks left of its use (`remaining`).
fn brush_transform(left: bool, remaining: f32, partial: f32) -> glam::Mat4 {
    use glam::{Mat4, Vec3};
    use horizon_core::geometry::trig::Mth;
    let ticks = remaining % 10.0;
    let delta = ticks - partial + 1.0;
    let scaled = 1.0 - delta / 10.0;
    let swipe = -15.0 + 75.0 * (scaled * 2.0 * std::f32::consts::PI).mcos();
    let sweep = Mat4::from_rotation_x(swipe.to_radians());
    if left {
        Mat4::from_translation(Vec3::new(0.1, 0.83, 0.35))
            * Mat4::from_rotation_x((-80.0f32).to_radians())
            * Mat4::from_rotation_y((-90.0f32).to_radians()) * sweep
            * Mat4::from_translation(Vec3::new(-0.3, 0.22, 0.35))
    } else {
        Mat4::from_translation(Vec3::new(-0.25, 0.22, 0.35))
            * Mat4::from_rotation_x((-80.0f32).to_radians())
            * Mat4::from_rotation_y(90.0f32.to_radians()) * sweep
    }
}
/// `renderPlayerArm` up to `renderPlayerHand`: the pose of the bare `left`
/// or right arm.
pub fn player_arm_pose(left: bool, inverse_height: f32, attack: f32) -> glam::Mat4 {
    use glam::{Mat4, Vec3};
    use horizon_core::geometry::trig::Mth;
    let f = if left { -1.0 } else { 1.0 };
    let pi = std::f32::consts::PI;
    let root = attack.sqrt();
    let x = -0.3 * (root * pi).msin();
    let y = 0.4 * (root * std::f32::consts::TAU).msin();
    let z = -0.4 * (attack * pi).msin();
    let z_rot = (attack * attack * pi).msin();
    let y_rot = (root * pi).msin();
    let deg = |d: f32| d.to_radians();
    Mat4::from_translation(
        Vec3::new(
            f * (x + 0.640_000_05),
            y - 0.6 + inverse_height * -0.6,
            z + -0.719_999_97,
        ),
    ) * Mat4::from_rotation_y(deg(f * 45.0))
        * Mat4::from_rotation_y(deg(f * y_rot * 70.0))
        * Mat4::from_rotation_z(deg(f * z_rot * -20.0))
        * Mat4::from_translation(Vec3::new(-f, 3.6, 3.5))
        * Mat4::from_rotation_z(deg(f * 120.0)) * Mat4::from_rotation_x(deg(200.0))
        * Mat4::from_rotation_y(deg(f * -135.0))
        * Mat4::from_translation(Vec3::new(f * 5.6, 0.0, 0.0))
}
/// `renderTwoHandedMap` up to its arms: the swing, the tilt from the view
/// pitch (`calculateMapTilt`) and the turn towards the player.
pub fn two_handed_map_base(pitch: f32, inverse_height: f32, attack: f32) -> glam::Mat4 {
    use glam::{Mat4, Vec3};
    use horizon_core::geometry::trig::Mth;
    let pi = std::f32::consts::PI;
    let y_swing = -0.2 * (attack * pi).msin();
    let z_swing = -0.4 * (attack.sqrt() * pi).msin();
    let tilt = (1.0 - pitch / 45.0 + 0.1).clamp(0.0, 1.0);
    let tilt = -(tilt * pi).mcos() * 0.5 + 0.5;
    Mat4::from_translation(Vec3::new(0.0, -y_swing / 2.0, z_swing))
        * Mat4::from_translation(
            Vec3::new(0.0, 0.04 + inverse_height * -1.2 + tilt * -0.5, -0.72),
        ) * Mat4::from_rotation_x((tilt * -85.0).to_radians())
}
/// The arms under a held map, as (left arm, pose before `renderPlayerHand`):
/// `renderOneHandedMap`'s arm turned 10 degrees outwards, or
/// `renderTwoHandedMap`'s two `renderMapHand` arms under the map's
/// `two_handed_map_base`. Vanilla skips them for an invisible player.
pub fn map_arm_poses(
    hold: MapHold,
    left: bool,
    pitch: f32,
    inverse_height: f32,
    attack: f32,
) -> Vec<(bool, glam::Mat4)> {
    use glam::{Mat4, Vec3};
    let deg = |d: f32| d.to_radians();
    match hold {
        MapHold::OneHanded => {
            let f = if left { -1.0 } else { 1.0 };
            let m = Mat4::from_translation(Vec3::new(f * 0.125, -0.125, 0.0))
                * Mat4::from_rotation_z(deg(f * 10.0))
                * player_arm_pose(left, inverse_height, attack);
            vec![(left, m)]
        }
        MapHold::TwoHanded => {
            let base = two_handed_map_base(pitch, inverse_height, attack)
                * Mat4::from_rotation_y(deg(90.0));
            [false, true]
                .into_iter()
                .map(|left| {
                    let f = if left { -1.0 } else { 1.0 };
                    let hand = Mat4::from_rotation_y(deg(92.0))
                        * Mat4::from_rotation_x(deg(45.0))
                        * Mat4::from_rotation_z(deg(f * -41.0))
                        * Mat4::from_translation(Vec3::new(f * 0.3, -1.1, 0.45));
                    (left, base * hand)
                })
                .collect()
        }
    }
}
/// `renderOneHandedMap` up to `renderMap`: the pose of a map in the hand on
/// the `left` or right arm (its arm: [`map_arm_poses`]).
pub fn one_handed_map_pose(left: bool, inverse_height: f32, attack: f32) -> glam::Mat4 {
    use glam::{Mat4, Vec3};
    use horizon_core::geometry::trig::Mth;
    let invert = if left { -1.0 } else { 1.0 };
    let pi = std::f32::consts::PI;
    let root = attack.sqrt();
    let x_swing = (root * pi).msin();
    Mat4::from_translation(Vec3::new(invert * 0.125, -0.125, 0.0))
        * Mat4::from_translation(
            Vec3::new(invert * 0.51, -0.08 + inverse_height * -1.2, -0.75),
        )
        * Mat4::from_translation(
            Vec3::new(
                invert * -0.5 * x_swing,
                0.4 * (root * std::f32::consts::TAU).msin() - 0.3 * x_swing,
                -0.3 * (attack * pi).msin(),
            ),
        ) * Mat4::from_rotation_x((x_swing * -45.0).to_radians())
        * Mat4::from_rotation_y((invert * x_swing * -30.0).to_radians())
}
/// `FirstPersonHandsAndItems.evaluateWhichHandsToRender`: whether the main
/// and the off hand show, from what each holds (item and whether it is a
/// charged crossbow) and the item in use with its hand. A bow or crossbow
/// being drawn hides the other hand, and a charged crossbow in the main hand
/// hides the off hand unless the off hand is the one in use.
pub fn hands_to_render(
    held: [Option<(&str, bool)>; 2],
    using: Option<(Hand, &str)>,
) -> [bool; 2] {
    let bow_like = |id: &str| matches!(id, "minecraft:bow" | "minecraft:crossbow");
    let charged_crossbow = |h: Option<(&str, bool)>| {
        h.is_some_and(|(id, c)| c && id == "minecraft:crossbow")
    };
    if !held.iter().flatten().any(|&(id, _)| bow_like(id)) {
        return [true, true];
    }
    match using {
        None => [true, !charged_crossbow(held[0])],
        Some((hand, id)) if bow_like(id) => [hand == Hand::Main, hand == Hand::Off],
        Some((hand, _)) => [true, !(hand == Hand::Main && charged_crossbow(held[1]))],
    }
}
/// `InteractionHand`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hand {
    Main,
    Off,
}
impl Hand {
    pub const BOTH: [Hand; 2] = [Hand::Main, Hand::Off];
}
/// `SwingAnimation`: its type (`SwingAnimationType` ordinal) and duration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwingAnimation {
    pub kind: u8,
    pub duration: i32,
}
impl SwingAnimation {
    /// `SwingAnimation.DEFAULT`: WHACK for 6 ticks.
    pub const DEFAULT: SwingAnimation = SwingAnimation {
        kind: 1,
        duration: 6,
    };
}
/// `LivingEntity.SwingDescription`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwingDescription {
    pub hand: Hand,
    pub animation: SwingAnimation,
    pub duration_ticks: i32,
}
/// `LivingEntity.SwingState`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SwingState {
    pub current: Option<SwingDescription>,
    pub ticks: i32,
    pub old_animation: f32,
    pub animation: f32,
}
impl SwingState {
    /// `startIfAble`: a swing restarts unless the current one is in its
    /// first half (past its first tick).
    pub fn start_if_able(
        &mut self,
        hand: Hand,
        animation: SwingAnimation,
        duration: i32,
    ) -> bool {
        if let Some(c) = self.current && self.ticks <= c.duration_ticks / 2
            && self.ticks > 0
        {
            return false;
        }
        let swing = SwingDescription {
            hand,
            animation,
            duration_ticks: duration,
        };
        if self.current != Some(swing) {
            self.old_animation = 0.0;
            self.animation = 0.0;
        }
        self.current = Some(swing);
        self.ticks = 0;
        true
    }
    /// `tick` (from `LivingEntity.baseTick`).
    pub fn tick(&mut self) {
        self.old_animation = self.animation;
        if let Some(c) = self.current {
            if c.duration_ticks > 0 {
                self.animation = (self.ticks as f32 / c.duration_ticks as f32).min(1.0);
            }
            let ticks = self.ticks;
            self.ticks += 1;
            if ticks > c.duration_ticks {
                self.current = None;
                self.animation = 0.0;
            }
        } else {
            self.animation = 0.0;
        }
    }
    /// `getAnimation(partialTicks)`: a restarted swing keeps moving forward.
    pub fn animation(&self, partial: f32) -> f32 {
        if self.animation >= self.old_animation {
            crate::camera::lerp(partial, self.old_animation, self.animation)
        } else {
            crate::camera::lerp(partial, self.old_animation, self.animation + 1.0)
        }
    }
    pub fn swinging(&self) -> bool {
        self.current.is_some()
    }
    /// The hand of the current swing (`state.attackHand`), main by default.
    pub fn attack_hand(&self) -> Hand {
        self.current.map_or(Hand::Main, |c| c.hand)
    }
}
/// `LivingEntity.getModifiedSwingDuration`: `dig_speed` is
/// `MobEffectUtil.getDigSpeedAmplification` while haste or conduit power is
/// active, `fatigue` the mining fatigue amplifier.
pub fn swing_duration(
    animation: SwingAnimation,
    dig_speed: Option<i32>,
    fatigue: Option<i32>,
) -> i32 {
    let duration = animation.duration;
    if let Some(amplification) = dig_speed {
        duration - (1 + amplification)
    } else if let Some(amplifier) = fatigue {
        duration + (1 + amplifier) * 2
    } else {
        duration
    }
}
/// `FirstPersonHandsAndItems`' equip heights.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandHeights {
    pub main: f32,
    pub old_main: f32,
    pub off: f32,
    pub old_off: f32,
}
impl Default for HandHeights {
    fn default() -> Self {
        HandHeights {
            main: 0.0,
            old_main: 0.0,
            off: 0.0,
            old_off: 0.0,
        }
    }
}
impl HandHeights {
    /// `FirstPersonHandsAndItems.tick`'s heights. `changed` says per hand
    /// whether the visible item differs from the held one once
    /// `shouldInstantlyReplaceVisibleItem` has run; `swap_scale` is
    /// `getItemSwapScale(1.0F)`. Returns whether each hand's visible item
    /// is replaced at the end.
    pub fn tick(
        &mut self,
        hands_busy: bool,
        swap_scale: f32,
        changed: [bool; 2],
    ) -> [bool; 2] {
        if let ::core::ops::ControlFlow::Break(__horizon_mixins_return) = {
            fn sunset_equip_hold(
                this: &mut HandHeights,
                hands_busy: bool,
                swap_scale: f32,
                changed: [bool; 2],
            ) -> std::ops::ControlFlow<[bool; 2]> {
                if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() {
                    return std::ops::ControlFlow::Continue(());
                }
                use crate::first_person::sunset_hand_view as view;
                if view::NO_EQUIP_DROP.load(std::sync::atomic::Ordering::Relaxed) {
                    (this.main, this.old_main, this.off, this.old_off) = (
                        1.0,
                        1.0,
                        1.0,
                        1.0,
                    );
                    return std::ops::ControlFlow::Break(changed);
                }
                if view::LEGACY.load(std::sync::atomic::Ordering::Relaxed) {
                    return std::ops::ControlFlow::Break(
                        this.tick_with(hands_busy, swap_scale, changed, true),
                    );
                }
                std::ops::ControlFlow::Continue(())
            }
            sunset_equip_hold(&mut *self, hands_busy, swap_scale, changed)
        } {
            return __horizon_mixins_return;
        }
        self.tick_with(hands_busy, swap_scale, changed, false)
    }
    /// [`Self::tick`]; `legacy` (Combat feel Legacy) raises the main hand fully whatever the
    /// attack strength, as 1.8.9's `ItemRenderer.updateEquippedItem` did (no dip after a hit).
    pub fn tick_with(
        &mut self,
        hands_busy: bool,
        swap_scale: f32,
        changed: [bool; 2],
        legacy: bool,
    ) -> [bool; 2] {
        self.old_main = self.main;
        self.old_off = self.off;
        if hands_busy {
            self.main = (self.main - 0.4).clamp(0.0, 1.0);
            self.off = (self.off - 0.4).clamp(0.0, 1.0);
        } else {
            let attack = if legacy { 1.0 } else { swap_scale };
            let main_target = if changed[0] { 0.0 } else { attack * attack * attack };
            let off_target = if changed[1] { 0.0 } else { 1.0 };
            self.main += (main_target - self.main).clamp(-0.4, 0.4);
            self.off += (off_target - self.off).clamp(-0.4, 0.4);
        }
        [self.main < 0.1, self.off < 0.1]
    }
    /// `itemUsed`: the hand drops to the bottom and rises again.
    pub fn item_used(&mut self, hand: Hand) {
        match hand {
            Hand::Main => self.main = 0.0,
            Hand::Off => self.off = 0.0,
        }
    }
    /// `Mth.lerp(partialTicks, oMainHandHeight, mainHandHeight)` and the
    /// off hand's.
    pub fn interpolated(&self, partial: f32) -> [f32; 2] {
        [
            crate::camera::lerp(partial, self.old_main, self.main),
            crate::camera::lerp(partial, self.old_off, self.off),
        ]
    }
}
/// `FirstPersonHandsAndItemsRenderer.submit`: the displayed stack's model
/// scales the interpolated equip displacement independently for each hand.
pub fn equip_displacement(height: f32, swap_animation_scale: f32) -> f32 {
    swap_animation_scale * (1.0 - height)
}
/// `Mth.wrapDegrees`: an angle difference in -180..=180.
fn wrap_degrees(d: f32) -> f32 {
    let d = d % 360.0;
    if d >= 180.0 { d - 360.0 } else if d < -180.0 { d + 360.0 } else { d }
}
/// `LocalPlayer.xBob` and `yBob`: the hands lag behind the view when it
/// turns. `LocalPlayer.applyInput` eases them halfway to the rotation every
/// tick; `FirstPersonHandsAndItemsRenderer` turns the hands by a tenth of
/// the gap left.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HandSway {
    pub x_bob: f32,
    pub y_bob: f32,
    pub x_bob_o: f32,
    pub y_bob_o: f32,
    /// The yaw `tick_wrapped` has followed so far, unwrapped like vanilla's.
    y_rot: f32,
}
impl HandSway {
    /// `LocalPlayer.applyInput` with the player's `getXRot` and `getYRot`.
    pub fn tick(&mut self, x_rot: f32, y_rot: f32) {
        self.y_bob_o = self.y_bob;
        self.x_bob_o = self.x_bob;
        self.x_bob += (x_rot - self.x_bob) * 0.5;
        self.y_bob += (y_rot - self.y_bob) * 0.5;
    }
    /// `tick` for a yaw kept in -180..180 (a boat turns its rider through the wrap). Vanilla's
    /// yaw runs on without wrapping, so `yRot - yBob` is always the short way round; this
    /// follows the wrapped yaw by its short steps to give `tick` the same unwrapped yaw.
    pub fn tick_wrapped(&mut self, x_rot: f32, y_rot: f32) {
        self.y_rot += wrap_degrees(y_rot - self.y_rot);
        self.tick(x_rot, self.y_rot);
    }
    /// The degrees the hands turn about X, then Y, before either is drawn:
    /// `(viewXRot - xBob) * 0.1` and `(viewYRot - yBob) * 0.1`, the bobs
    /// interpolated as `extractRenderState` does.
    pub fn rotation(&self, partial: f32, view_x_rot: f32, view_y_rot: f32) -> [f32; 2] {
        if let ::core::ops::ControlFlow::Break(__horizon_mixins_return) = {
            fn sunset_sway() -> std::ops::ControlFlow<[f32; 2]> {
                if !horizon_protocol::connection::sunset_lifecycle::RUNTIME.running() {
                    return std::ops::ControlFlow::Continue(());
                }
                if crate::first_person::sunset_hand_view::NO_SWAY
                    .load(std::sync::atomic::Ordering::Relaxed)
                {
                    return std::ops::ControlFlow::Break([0.0, 0.0]);
                }
                std::ops::ControlFlow::Continue(())
            }
            sunset_sway()
        } {
            return __horizon_mixins_return;
        }
        let x_bob = crate::camera::lerp(partial, self.x_bob_o, self.x_bob);
        let y_bob = crate::camera::lerp(partial, self.y_bob_o, self.y_bob);
        [(view_x_rot - x_bob) * 0.1, wrap_degrees(view_y_rot - y_bob) * 0.1]
    }
    /// The hands' view space to camera-relative world space, turned by the
    /// sway. The hand pass draws with the camera's view, whose rotation a
    /// ridden boat's turn spreads over the frames of a tick
    /// (`LocalPlayer.getViewYRot`): anything else swings the hands back and
    /// forth by that turn, up to 10 degrees, 20 times a second (#4583).
    pub fn to_world(&self, partial: f32, camera: &crate::camera::Camera) -> glam::Mat4 {
        let [x, y] = self.rotation(partial, camera.x_rot, camera.y_rot);
        crate::camera::view_matrix(camera.y_rot, camera.x_rot).inverse()
            * glam::Mat4::from_rotation_x(x.to_radians())
            * glam::Mat4::from_rotation_y(y.to_radians())
    }
}
/// `HitResult` at the crosshair (`Minecraft.hitResult`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    Miss,
    Block { pos: [i32; 3], face: u8 },
    Entity,
}
/// `InteractionResult` as `startUseItem` reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseResult {
    /// `Success` with its `SwingSource`: whether the client swings.
    Success { swing: bool },
    Pass,
    Fail,
}
impl UseResult {
    pub fn should_swing(self, before: u32, after: u32) -> bool {
        matches!(self, Self::Success { swing : true }) && before == after
    }
}
pub use horizon_items::use_animation::consumable_animation;
/// What the click handling reads and does in the world.
pub trait HandWorld {
    fn hit(&mut self) -> Hit;
    fn is_air(&mut self, pos: [i32; 3]) -> bool;
    /// `MultiPlayerGameMode.localPlayerMode`.
    fn creative(&self) -> bool;
    fn spectator(&self) -> bool;
    /// `Abilities.instabuild`.
    fn instabuild(&self) -> bool;
    fn hands_busy(&self) -> bool;
    fn using_item(&self) -> bool;
    /// `ItemStack.isItemEnabled` for the item in a hand.
    fn item_enabled(&mut self, hand: Hand) -> bool;
    fn held_empty(&mut self, hand: Hand) -> bool;
    fn held_count(&mut self, hand: Hand) -> i32;
    /// `Player.hasInfiniteMaterials`.
    fn infinite_materials(&self) -> bool;
    /// `Player.cannotAttackWithItem(heldItem, 0)`.
    fn cannot_attack(&mut self) -> bool;
    /// The main-hand item is a `PIERCING_WEAPON`.
    fn piercing_weapon(&mut self) -> bool;
    fn attack_animation(&mut self) -> SwingAnimation;
    fn interact_animation(&mut self, hand: Hand) -> SwingAnimation;
    /// `MultiPlayerGameMode.piercingAttack`.
    fn piercing_attack(&mut self, animation: SwingAnimation);
    /// `Player.attack` on the targeted entity when the item's attack range
    /// allows it; false when out of range.
    fn attack_entity(&mut self) -> bool;
    /// `Player.blockActionRestricted` for the game mode.
    fn block_action_restricted(&mut self, pos: [i32; 3]) -> bool;
    /// `WorldBorder.isWithinBounds`.
    fn within_border(&mut self, pos: [i32; 3]) -> bool;
    /// `BlockState.attack` on the first hit of a break.
    fn attack_block(&mut self, pos: [i32; 3]);
    /// `BlockState.getDestroyProgress` for the player.
    fn destroy_progress(&mut self, pos: [i32; 3]) -> f32;
    /// `MultiPlayerGameMode.destroyBlock`.
    fn destroy_block(&mut self, pos: [i32; 3]) -> bool;
    /// A key for the main-hand stack, kept as `destroyingItem`.
    fn main_hand_key(&mut self) -> String;
    /// `ItemStack.isSameItemSameComponents(selected, destroyingItem)` for
    /// the stack `main_hand_key` gave when the break started. Vanilla keeps
    /// a reference to that stack, so it compares what it holds now.
    fn same_destroying_item(&mut self, key: &str) -> bool {
        self.main_hand_key() == key
    }
    /// Breaking progress shown (`destroyBlockProgress`), or cleared with `None`.
    fn destroy_stage(&mut self, pos: [i32; 3], stage: Option<i32>);
    /// `player.swing(hand, animation, false)`.
    fn swing(&mut self, hand: Hand, animation: SwingAnimation);
    /// `Player.resetAttackStrengthTicker`.
    fn reset_attack_strength(&mut self);
    /// The targeted entity's block position is inside the world border.
    fn entity_within_border(&mut self) -> bool;
    /// The use attempts of `startUseItem`: on the targeted entity (`None`
    /// when it is out of interaction range), the targeted block, and the
    /// item alone.
    fn use_on_entity(&mut self, hand: Hand) -> Option<UseResult>;
    fn use_on_block(&mut self, hand: Hand) -> UseResult;
    fn use_item(&mut self, hand: Hand) -> UseResult;
    /// `LocalPlayer.itemUsed`.
    fn item_used(&mut self, hand: Hand);
    /// `MultiPlayerGameMode.releaseUsingItem`.
    fn release_using(&mut self);
}
/// `MultiPlayerGameMode`'s block breaking.
#[derive(Clone, Debug, PartialEq)]
pub struct Destroying {
    pub delay: i32,
    pub active: bool,
    pub pos: [i32; 3],
    pub face: u8,
    pub item: String,
    pub progress: f32,
    pub ticks: f32,
}
impl Default for Destroying {
    /// `destroyBlockPos` starts at (-1, -1, -1) and `destroyingItem` empty
    /// (an empty key).
    fn default() -> Self {
        Destroying {
            delay: 0,
            active: false,
            pos: [-1; 3],
            face: 0,
            item: String::new(),
            progress: 0.0,
            ticks: 0.0,
        }
    }
}
impl Destroying {
    /// `getDestroyStage`.
    pub fn stage(&self) -> i32 {
        if self.progress > 0.0 { (self.progress * 10.0) as i32 } else { -1 }
    }
    fn same_target(&self, w: &mut impl HandWorld, pos: [i32; 3]) -> bool {
        pos == self.pos && w.same_destroying_item(&self.item)
    }
    /// `startDestroyBlock`.
    pub fn start(&mut self, w: &mut impl HandWorld, pos: [i32; 3], face: u8) -> bool {
        if w.block_action_restricted(pos) || !w.within_border(pos) {
            return false;
        }
        if w.instabuild() {
            w.destroy_block(pos);
            self.delay = 5;
        } else if !self.active || !self.same_target(w, pos) {
            let not_air = !w.is_air(pos);
            if not_air && self.progress == 0.0 {
                w.attack_block(pos);
            }
            if not_air && w.destroy_progress(pos) >= 1.0 {
                w.destroy_block(pos);
            } else {
                self.active = true;
                self.pos = pos;
                self.face = face;
                self.item = w.main_hand_key();
                self.progress = 0.0;
                self.ticks = 0.0;
                w.destroy_stage(pos, Some(self.stage()));
            }
        }
        true
    }
    /// `stopDestroyBlock`.
    pub fn stop(&mut self, w: &mut impl HandWorld) {
        if self.active {
            self.active = false;
            self.progress = 0.0;
            w.destroy_stage(self.pos, None);
            w.reset_attack_strength();
        }
    }
    /// `continueDestroyBlock`: true while the hand keeps swinging.
    pub fn continue_destroy(
        &mut self,
        w: &mut impl HandWorld,
        pos: [i32; 3],
        face: u8,
    ) -> bool {
        if self.delay > 0 {
            self.delay -= 1;
            return true;
        }
        if w.instabuild() && w.within_border(pos) {
            self.delay = 5;
            w.destroy_block(pos);
            return true;
        }
        if !self.same_target(w, pos) {
            return self.start(w, pos, face);
        }
        if w.is_air(pos) {
            self.active = false;
            return false;
        }
        self.progress += w.destroy_progress(pos);
        self.ticks += 1.0;
        if self.progress >= 1.0 {
            self.active = false;
            w.destroy_block(pos);
            self.progress = 0.0;
            self.ticks = 0.0;
            self.delay = 5;
        } else if self.face != face {
            self.face = face;
        }
        w.destroy_stage(self.pos, Some(self.stage()));
        true
    }
}
/// `Minecraft`'s attack and use handling.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clicks {
    pub miss_time: i32,
    pub right_click_delay: i32,
    pub destroying: Destroying,
}
impl Clicks {
    /// `Minecraft.startAttack`: whether the click ended the attack (broke
    /// the block outright).
    pub fn start_attack(&mut self, w: &mut impl HandWorld) -> bool {
        if self.miss_time > 0 || w.hands_busy() {
            return false;
        }
        if w.spectator() {
            return true;
        }
        if !w.item_enabled(Hand::Main) || w.cannot_attack() {
            return false;
        }
        let animation = w.attack_animation();
        if w.piercing_weapon() {
            w.piercing_attack(animation);
            return true;
        }
        let mut end = false;
        let mut miss = false;
        match w.hit() {
            Hit::Entity => {
                if w.attack_entity() {
                    w.reset_attack_strength();
                    if w.instabuild() {
                        self.destroying.delay = 5;
                    }
                }
            }
            Hit::Block { pos, face } if !w.is_air(pos) => {
                self.destroying.start(w, pos, face);
                end = w.is_air(pos);
            }
            Hit::Block { .. } | Hit::Miss => miss = true,
        }
        if miss {
            if !w.creative() {
                self.miss_time = 10;
            }
            w.reset_attack_strength();
        }
        w.swing(Hand::Main, animation);
        end
    }
    /// `Minecraft.continueAttack`.
    pub fn continue_attack(&mut self, w: &mut impl HandWorld, down: bool) {
        if !down {
            self.miss_time = 0;
        }
        if self.miss_time > 0 || w.using_item() || w.piercing_weapon() {
            return;
        }
        let animation = w.attack_animation();
        match w.hit() {
            Hit::Block { pos, face } if down => {
                if !w.is_air(pos) && self.destroying.continue_destroy(w, pos, face) {
                    w.swing(Hand::Main, animation);
                }
            }
            _ => self.destroying.stop(w),
        }
    }
    /// `Minecraft.startUseItem`.
    pub fn start_use_item(&mut self, w: &mut impl HandWorld) {
        if self.destroying.active {
            return;
        }
        self.right_click_delay = 4;
        if w.hands_busy() {
            return;
        }
        for hand in Hand::BOTH {
            if !w.item_enabled(hand) {
                return;
            }
            let animation = w.interact_animation(hand);
            match w.hit() {
                Hit::Entity => {
                    if !w.entity_within_border() {
                        return;
                    }
                    if let Some(UseResult::Success { swing }) = w.use_on_entity(hand) {
                        if swing {
                            w.swing(hand, animation);
                        }
                        return;
                    }
                }
                Hit::Block { .. } => {
                    let count = w.held_count(hand);
                    match w.use_on_block(hand) {
                        UseResult::Success { swing } => {
                            if swing {
                                w.swing(hand, animation);
                                if !w.held_empty(hand)
                                    && (w.held_count(hand) != count || w.infinite_materials())
                                {
                                    w.item_used(hand);
                                }
                            }
                            return;
                        }
                        UseResult::Fail => return,
                        UseResult::Pass => {}
                    }
                }
                Hit::Miss => {}
            }
            if !w.held_empty(hand) && let UseResult::Success { swing } = w.use_item(hand)
            {
                if swing {
                    w.swing(hand, animation);
                }
                w.item_used(hand);
                return;
            }
        }
    }
    /// The attack and use part of `Minecraft.tick`: `rightClickDelay`, the
    /// clicks `handleKeybinds` consumes, held use and attack, then
    /// `missTime`. `attack_down` is `keyAttack.isDown()` with no screen and
    /// the mouse grabbed.
    pub fn tick(
        &mut self,
        w: &mut impl HandWorld,
        attack_clicks: u32,
        use_clicks: u32,
        attack_down: bool,
        use_down: bool,
    ) {
        if self.right_click_delay > 0 {
            self.right_click_delay -= 1;
        }
        let mut instant = false;
        if w.using_item() {
            if !use_down {
                w.release_using();
            }
        } else {
            for _ in 0..attack_clicks {
                instant |= self.start_attack(w);
            }
            for _ in 0..use_clicks {
                self.start_use_item(w);
            }
        }
        if use_down && self.right_click_delay == 0 && !w.using_item() {
            self.start_use_item(w);
        }
        self.continue_attack(w, !instant && attack_down);
        if self.miss_time > 0 {
            self.miss_time -= 1;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn equip_displacement_uses_the_visible_items_scale() {
        use horizon_assets::item_model::{ClientItem, ItemModels, StackView};
        use serde_json::json;
        let mut models = ItemModels::default();
        for (item, scale) in [("minecraft:spear", 1.95), ("minecraft:sword", 1.0)] {
            models
                .definitions
                .insert(
                    item.into(),
                    ClientItem::decode(
                            &json!(
                                { "model" : { "type" : "minecraft:empty" },
                                "swap_animation_scale" : scale, }
                            ),
                        )
                        .unwrap(),
                );
            models
                .prototypes
                .insert(
                    item.into(),
                    [("minecraft:item_model".into(), json!(item))].into(),
                );
        }
        let spear = StackView::new("minecraft:spear", 1);
        let sword = StackView::new("minecraft:sword", 1);
        let height = 0.5;
        assert_eq!(
            equip_displacement(height, models.swap_animation_scale(& spear)), 0.975
        );
        assert_eq!(
            equip_displacement(height, models.swap_animation_scale(& sword)), 0.5
        );
        assert_eq!(equip_displacement(1.0, models.swap_animation_scale(& spear)), 0.0);
        let mut overridden = spear;
        overridden
            .patch
            .insert("minecraft:item_model".into(), Some(json!("minecraft:sword")));
        assert_eq!(
            equip_displacement(height, models.swap_animation_scale(& overridden)), 0.5
        );
    }
    /// #4583: while a boat turns the player steadily, the hands keep their
    /// place on screen through every frame of a tick.
    #[test]
    fn the_hands_hold_still_on_screen_while_a_boat_turns() {
        const TURN: f32 = 10.0;
        let mut sway = HandSway::default();
        let mut yaw = 0.0;
        for _ in 0..30 {
            sway.tick(0.0, yaw);
            yaw += TURN;
        }
        let on_screen = |partial: f32| {
            let camera = crate::camera::Camera {
                y_rot: yaw - TURN * (1.0 - partial),
                ..Default::default()
            };
            crate::camera::view_matrix(camera.y_rot, 0.0)
                * sway.to_world(partial, &camera)
        };
        let first = on_screen(0.0);
        for partial in [0.25, 0.5, 0.75, 0.99] {
            let frame = on_screen(partial);
            assert!(frame.abs_diff_eq(first, 1.0e-3), "{partial}: {frame:?}");
        }
    }
    #[test]
    fn drawing_a_bow_or_holding_a_charged_crossbow_hides_a_hand() {
        let (bow, apple) = (
            Some(("minecraft:bow", false)),
            Some(("minecraft:apple", false)),
        );
        let charged = Some(("minecraft:crossbow", true));
        assert_eq!(hands_to_render([apple, apple], None), [true, true]);
        assert_eq!(hands_to_render([bow, apple], None), [true, true]);
        assert_eq!(hands_to_render([charged, apple], None), [true, false]);
        assert_eq!(hands_to_render([apple, charged], None), [true, true]);
        let drawing = |hand| Some((hand, "minecraft:bow"));
        assert_eq!(hands_to_render([bow, apple], drawing(Hand::Main)), [true, false]);
        assert_eq!(hands_to_render([apple, bow], drawing(Hand::Off)), [false, true]);
        let eating = |hand| Some((hand, "minecraft:apple"));
        assert_eq!(hands_to_render([apple, charged], eating(Hand::Main)), [true, false]);
        assert_eq!(hands_to_render([charged, apple], eating(Hand::Off)), [true, true]);
    }
    #[test]
    fn a_swing_lasts_its_duration_and_restarts_after_half() {
        let mut s = SwingState::default();
        assert!(s.start_if_able(Hand::Main, SwingAnimation::DEFAULT, 6));
        let mut seen = Vec::new();
        for _ in 0..9 {
            s.tick();
            seen.push(s.animation);
        }
        assert_eq!(
            seen, [0.0, 1.0 / 6.0, 2.0 / 6.0, 0.5, 4.0 / 6.0, 5.0 / 6.0, 1.0, 0.0, 0.0]
        );
        assert!(! s.swinging());
        s.start_if_able(Hand::Main, SwingAnimation::DEFAULT, 6);
        s.tick();
        s.tick();
        assert!(! s.start_if_able(Hand::Main, SwingAnimation::DEFAULT, 6));
        s.tick();
        s.tick();
        assert!(s.start_if_able(Hand::Main, SwingAnimation::DEFAULT, 6));
        s.tick();
        assert_eq!(s.animation(0.5), (s.old_animation + s.animation + 1.0) / 2.0);
    }
    #[test]
    fn haste_shortens_and_fatigue_lengthens_swings() {
        let d = SwingAnimation::DEFAULT;
        assert_eq!(swing_duration(d, None, None), 6);
        assert_eq!(swing_duration(d, Some(1), Some(0)), 4);
        assert_eq!(swing_duration(d, None, Some(2)), 12);
    }
    #[test]
    fn legacy_hand_puts_a_flat_item_where_1_8_9_drew_it() {
        use glam::{Mat4, Vec3};
        let pose = legacy_item_pose(false, 0.0, 0.0, ItemPose::Swing, false).unwrap();
        let layer = modern_flat_display(false)
            * Mat4::from_translation(Vec3::splat(-0.5));
        let centre = (pose * layer).transform_point3(Vec3::splat(0.5));
        let c45 = std::f32::consts::FRAC_1_SQRT_2;
        let expected = Vec3::new(0.56 + 0.05 * c45, -0.52 + 0.1, -0.72 + 0.05 * c45);
        assert!(centre.abs_diff_eq(expected, 1e-4), "{centre} vs {expected}");
        let left = legacy_item_pose(true, 0.0, 0.0, ItemPose::Swing, false).unwrap();
        let left_layer = modern_flat_display(true)
            * Mat4::from_translation(Vec3::splat(-0.5));
        let l = (left * left_layer).transform_point3(Vec3::splat(0.5));
        assert!(l.abs_diff_eq(Vec3::new(- expected.x, expected.y, expected.z), 1e-4));
        let low = legacy_item_pose(false, 0.5, 0.0, ItemPose::Swing, false).unwrap();
        let c = (low * layer).transform_point3(Vec3::splat(0.5));
        assert!(c.abs_diff_eq(expected - Vec3::new(0.0, 0.3, 0.0), 1e-4));
        let block = legacy_item_pose(false, 0.0, 0.0, ItemPose::Using, true).unwrap();
        assert!(
            ! (block * layer).transform_point3(Vec3::splat(0.5)).abs_diff_eq(expected,
            0.01)
        );
        let trident = ItemPose::Trident {
            ticks_used: 3.0,
        };
        assert!(legacy_item_pose(false, 0.0, 0.0, trident, false).is_none());
    }
    #[test]
    fn legacy_hand_does_not_dip_after_a_hit() {
        let mut modern = HandHeights {
            main: 1.0,
            old_main: 1.0,
            off: 1.0,
            old_off: 1.0,
        };
        let mut legacy = modern;
        modern.tick_with(false, 0.0, [false; 2], false);
        legacy.tick_with(false, 0.0, [false; 2], true);
        assert!((modern.main - 0.6).abs() < 1e-6, "falls 0.4 a tick");
        assert_eq!(legacy.main, 1.0);
        legacy.tick_with(false, 1.0, [true, false], true);
        assert!((legacy.main - 0.6).abs() < 1e-6);
    }
    #[test]
    fn the_flat_display_check_knows_vanillas_matrix() {
        let m = modern_flat_display(false)
            * glam::Mat4::from_translation(glam::Vec3::splat(-0.5));
        assert!(is_modern_flat_display(& m, false));
        assert!(! is_modern_flat_display(& m, true));
        assert!(! is_modern_flat_display(& glam::Mat4::IDENTITY, false));
    }
}
#[cfg(test)]
mod item_pose_tests {
    use super::*;
    #[test]
    fn a_map_is_two_handed_only_in_the_main_hand_with_an_empty_off_hand() {
        assert_eq!(MapHold::of(Hand::Main, true), MapHold::TwoHanded);
        assert_eq!(MapHold::of(Hand::Main, false), MapHold::OneHanded);
        assert_eq!(MapHold::of(Hand::Off, true), MapHold::OneHanded);
        assert_eq!(MapHold::of(Hand::Off, false), MapHold::OneHanded);
    }
    #[test]
    fn one_handed_maps_follow_render_one_handed_map() {
        let cases: [(bool, f32, f32, [f32; 16]); 3] = [
            (
                false,
                0.0,
                0.0,
                [
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                    -0.0,
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                    0.0,
                    0.635,
                    -0.205,
                    -0.75,
                    1.0,
                ],
            ),
            (
                true,
                0.25,
                0.3,
                [
                    0.86894435,
                    -0.34686038,
                    -0.35302067,
                    0.0,
                    0.0,
                    0.713303,
                    -0.70085573,
                    0.0,
                    0.4949098,
                    0.6090046,
                    0.61982065,
                    0.0,
                    -0.1406033,
                    -0.9197687,
                    -0.9926984,
                    1.0,
                ],
            ),
            (
                false,
                0.6,
                0.85,
                [
                    0.9919303,
                    0.024030307,
                    0.12448644,
                    0.0,
                    0.0,
                    0.9818737,
                    -0.18953651,
                    0.0,
                    -0.12678458,
                    0.18800701,
                    0.97395027,
                    0.0,
                    0.5136029,
                    -1.1862278,
                    -0.88621765,
                    1.0,
                ],
            ),
        ];
        for (left, inverse_height, attack, want) in cases {
            let got = one_handed_map_pose(left, inverse_height, attack).to_cols_array();
            for (g, w) in got.iter().zip(want) {
                assert!(
                    (g - w).abs() < 1e-5, "{left} {inverse_height} {attack}: {got:?}"
                );
            }
        }
    }
    #[test]
    fn maps_draw_the_arms_under_them_as_vanilla() {
        let near = |got: glam::Mat4, want: [f32; 16], what: &str| {
            let got = got.to_cols_array();
            for (g, w) in got.iter().zip(want) {
                assert!((g - w).abs() < 1e-5, "{what}: {got:?}");
            }
        };
        #[rustfmt::skip]
        let one: [(bool, f32, f32, [f32; 16]); 3] = [
            (
                false,
                0.0,
                0.0,
                [
                    -0.2350201,
                    -0.7860474,
                    -0.5717472,
                    0.0,
                    0.24694103,
                    0.52063686,
                    -0.8172866,
                    0.0,
                    0.9400987,
                    -0.33326668,
                    0.07174723,
                    0.0,
                    0.65913033,
                    -1.1543381,
                    -0.73980355,
                    1.0,
                ],
            ),
            (
                true,
                0.25,
                0.3,
                [
                    -0.4967606,
                    0.84289914,
                    -0.20676011,
                    0.0,
                    0.7202682,
                    0.2674863,
                    -0.6400506,
                    0.0,
                    -0.4841926,
                    -0.46687466,
                    -0.7399901,
                    0.0,
                    -0.59715486,
                    -1.4160433,
                    -0.7328731,
                    1.0,
                ],
            ),
            (
                false,
                0.6,
                0.85,
                [
                    -0.47358897,
                    -0.8403256,
                    -0.2637542,
                    0.0,
                    0.081554025,
                    0.25634,
                    -0.96313995,
                    0.0,
                    0.87696207,
                    -0.47764266,
                    -0.052867785,
                    0.0,
                    0.61887383,
                    -1.6482887,
                    -0.7229744,
                    1.0,
                ],
            ),
        ];
        for (left, inverse_height, attack, want) in one {
            let arms = map_arm_poses(
                MapHold::OneHanded,
                left,
                0.0,
                inverse_height,
                attack,
            );
            assert_eq!(arms.len(), 1, "one arm under a one-handed map");
            assert_eq!(arms[0].0, left, "the arm of the hand holding it");
            near(arms[0].1, want, &format!("one {left} {inverse_height} {attack}"));
        }
        #[rustfmt::skip]
        let two: [(f32, f32, f32, [f32; 16], [f32; 16]); 2] = [
            (
                0.0,
                0.0,
                0.0,
                [
                    -0.7380599,
                    0.44766384,
                    0.50484115,
                    0.0,
                    -0.67428374,
                    -0.46198538,
                    -0.57611704,
                    0.0,
                    -0.024677532,
                    -0.76561517,
                    0.64282537,
                    0.0,
                    0.5091893,
                    -0.16204369,
                    0.3544525,
                    1.0,
                ],
                [
                    -0.77043974,
                    -0.3951866,
                    -0.50025004,
                    0.0,
                    0.637035,
                    -0.5076031,
                    -0.58010805,
                    0.0,
                    -0.024677532,
                    -0.76561517,
                    0.64282537,
                    0.0,
                    -0.48071152,
                    -0.12760738,
                    0.35746527,
                    1.0,
                ],
            ),
            (
                30.0,
                0.2,
                0.5,
                [
                    -0.7380599,
                    -0.11453738,
                    0.6649426,
                    0.0,
                    -0.67428374,
                    0.16124588,
                    -0.72065324,
                    0.0,
                    -0.024677532,
                    -0.98024535,
                    -0.1962398,
                    0.0,
                    0.5091893,
                    -0.9508533,
                    -0.13439482,
                    1.0,
                ],
                [
                    -0.77043974,
                    0.14373614,
                    -0.6210978,
                    0.0,
                    0.637035,
                    0.13586378,
                    -0.75876695,
                    0.0,
                    -0.024677532,
                    -0.98024535,
                    -0.1962398,
                    0.0,
                    -0.48071152,
                    -0.9316926,
                    -0.1056232,
                    1.0,
                ],
            ),
        ];
        for (pitch, inverse_height, attack, right, left) in two {
            let arms = map_arm_poses(
                MapHold::TwoHanded,
                false,
                pitch,
                inverse_height,
                attack,
            );
            assert_eq!(arms.len(), 2, "both arms under a two-handed map");
            assert_eq!((arms[0].0, arms[1].0), (false, true), "right, then left");
            near(arms[0].1, right, &format!("two right {pitch}"));
            near(arms[1].1, left, &format!("two left {pitch}"));
        }
    }
    #[test]
    fn ranged_draw_uses_native_charge_curves_and_caps() {
        for (ticks, stretch) in [
            (0.0, 1.0),
            (10.0, 1.0 + 0.2 * 1.25 / 3.0),
            (20.0, 1.2),
            (40.0, 1.2),
        ] {
            let m = item_pose(false, 0.0, 0.0, ItemPose::Bow { ticks_used: ticks });
            assert!((m.determinant() - stretch).abs() < 0.000001);
        }
        for duration in [25.0, 20.0, 15.0, 10.0] {
            let m = item_pose(
                false,
                0.0,
                0.0,
                ItemPose::Crossbow {
                    ticks_used: Some(duration),
                    charge_duration: duration,
                    charged: false,
                },
            );
            assert!((m.determinant() - 1.2).abs() < 0.000001);
        }
    }
    #[test]
    fn left_handed_ranged_poses_mirror_the_right_hand() {
        let reflect = glam::Mat4::from_scale(glam::Vec3::new(-1.0, 1.0, 1.0));
        for pose in [
            ItemPose::Swing,
            ItemPose::Bow { ticks_used: 10.0 },
            ItemPose::Crossbow {
                ticks_used: Some(10.0),
                charge_duration: 25.0,
                charged: false,
            },
            ItemPose::Crossbow {
                ticks_used: None,
                charge_duration: 25.0,
                charged: true,
            },
        ] {
            let left = item_pose(true, 0.2, 0.0, pose).to_cols_array();
            let right = (reflect * item_pose(false, 0.2, 0.0, pose) * reflect)
                .to_cols_array();
            for (a, b) in left.into_iter().zip(right) {
                assert!((a - b).abs() < 0.000001);
            }
        }
    }
    #[test]
    fn drawing_bow_has_its_own_pose() {
        assert_ne!(
            item_pose(false, 0.0, 0.0, ItemPose::Bow { ticks_used : 10.0 }),
            item_pose(false, 0.0, 0.0, ItemPose::Swing)
        );
    }
    #[test]
    fn charging_crossbow_has_its_own_pose() {
        assert_ne!(
            item_pose(false, 0.0, 0.0, ItemPose::Crossbow { ticks_used : Some(10.0),
            charge_duration : 25.0, charged : false }), item_pose(false, 0.0, 0.0,
            ItemPose::Swing)
        );
    }
    #[test]
    fn charged_crossbow_is_aimed_while_idle() {
        assert_ne!(
            item_pose(false, 0.0, 0.0, ItemPose::Crossbow { ticks_used : None,
            charge_duration : 25.0, charged : true }), item_pose(false, 0.0, 0.0,
            ItemPose::Swing)
        );
    }
}
#[cfg(test)]
mod continuous_use_tests {
    use super::*;
    #[test]
    fn eating_and_throwing_have_use_poses_not_attack_poses() {
        let swing = item_pose(false, 0.0, 0.3, ItemPose::Swing);
        assert_ne!(
            swing, item_pose(false, 0.0, 0.3, ItemPose::Eat { remaining : 10.0, duration
            : 32.0, partial : 0.5 })
        );
        assert_ne!(
            swing, item_pose(false, 0.0, 0.3, ItemPose::Trident { ticks_used : 5.0 })
        );
        assert_ne!(swing, item_pose(false, 0.0, 0.3, ItemPose::Using));
    }
}
#[cfg(test)]
mod action_result_tests {
    use super::*;
    #[test]
    fn only_successful_actions_swing_once() {
        assert!(UseResult::Success { swing : true } .should_swing(2, 2));
        for result in [
            UseResult::Fail,
            UseResult::Pass,
            UseResult::Success { swing: false },
        ] {
            assert!(! result.should_swing(2, 2));
        }
        assert!(! UseResult::Success { swing : true } .should_swing(2, 3));
    }
    #[test]
    fn consumption_pose_requires_eat_or_drink_animation() {
        use horizon_world::consumable::UseAnimation::*;
        assert_eq!(consumable_animation(& serde_json::json!({})), Eat);
        assert_eq!(
            consumable_animation(& serde_json::json!({ "animation" : "drink" })), Drink
        );
        for animation in ["none", "block", "bow", "brush"] {
            assert_eq!(
                consumable_animation(& serde_json::json!({ "animation" : animation })),
                None
            );
        }
    }
    /// The hands' sway turns the short way round when the yaw wraps from 179 to -179 degrees (a
    /// boat turning its rider), instead of swinging them 36 degrees the long way.
    #[test]
    fn hand_sway_takes_the_short_way_over_the_yaw_wrap() {
        let mut sway = HandSway::default();
        sway.tick_wrapped(0.0, 179.0);
        for _ in 0..20 {
            sway.tick_wrapped(0.0, 179.0);
        }
        sway.tick_wrapped(0.0, -179.0);
        let [_, y] = sway.rotation(1.0, 0.0, -179.0);
        assert!(
            y.abs() < 0.5, "a 2 degree turn sways the hands by under half a degree: {y}"
        );
    }
}
pub mod sunset_hand_view {
    use std::sync::RwLock;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    pub static NO_SWAY: AtomicBool = AtomicBool::new(false);
    pub static NO_SHADOW: AtomicBool = AtomicBool::new(false);
    pub static VIEW: RwLock<Option<[glam::Mat4; 2]>> = RwLock::new(None);
    pub static TOTEM: RwLock<Option<TotemPop>> = RwLock::new(None);
    pub static TOTEM_EXTRA_TICKS: AtomicU32 = AtomicU32::new(0);
    pub static SWING: AtomicU32 = AtomicU32::new(0);
    pub static SLOW_SWING: AtomicBool = AtomicBool::new(false);
    pub static NO_EQUIP_DROP: AtomicBool = AtomicBool::new(false);
    pub static SWORD_BLOCK: AtomicBool = AtomicBool::new(false);
    pub const SLOW_SWING_TICKS: i32 = 10;
    pub const FULL_LIGHT: u32 = 0xF0F0;
    #[derive(Clone, Copy, PartialEq, Debug)]
    pub struct TotemPop {
        pub scale: f32,
        pub x: f32,
        pub y: f32,
    }
    const HALF_SCREEN: f32 = 0.700_207_5;
    impl TotemPop {
        pub fn apply(self, pose: glam::Mat4, aspect: f32) -> glam::Mat4 {
            let depth = -pose.w_axis.z;
            let shift = glam::Vec3::new(
                self.x * HALF_SCREEN * aspect * depth,
                self.y * HALF_SCREEN * depth,
                0.0,
            );
            glam::Mat4::from_translation(shift) * pose
                * glam::Mat4::from_scale(glam::Vec3::splat(self.scale))
        }
    }
    pub fn totem_pose(pose: glam::Mat4, aspect: f32) -> glam::Mat4 {
        match TOTEM.read() {
            Ok(t) => t.map_or(pose, |t| t.apply(pose, aspect)),
            Err(_) => pose,
        }
    }
    pub fn hand_light(light: u32) -> u32 {
        if NO_SHADOW.load(Ordering::Relaxed) { FULL_LIGHT } else { light }
    }
    pub fn posed(left: bool, pose: glam::Mat4) -> glam::Mat4 {
        match VIEW.read() {
            Ok(view) => view.map_or(pose, |v| pose * v[usize::from(left)]),
            Err(_) => pose,
        }
    }
    thread_local! {
        static POSING : std::cell::Cell < bool > = const { std::cell::Cell::new(false) };
    }
    pub fn posing() -> bool {
        POSING.with(std::cell::Cell::get)
    }
    pub fn set_posing(v: bool) {
        POSING.with(|p| p.set(v));
    }
    use glam::{Mat4, Vec3};
    pub const SWINGS: &[&str] = &[
        "Vanilla",
        "1",
        "2",
        "3",
        "Punch",
        "Chop",
        "Slide",
        "Wave",
        "Stab",
        "Spin",
        "Lazy",
        "Smooth",
        "Push",
    ];
    pub const SWINGS_18: &[&str] = &[
        "1.8",
        "Punch",
        "Shove",
        "Smooth",
        "Chop",
        "Slide",
        "Wave",
        "Stab",
        "Spin",
        "Lazy",
        "Push",
        "Swank",
    ];
    pub const BLOCKS_18: &[&str] = &[
        "1.8",
        "1.7",
        "Push",
        "Slide",
        "Swank",
        "Swirl",
        "Tap",
        "Stab",
        "Spin",
        "Smooth",
        "Down",
        "Lift",
        "Wave",
        "Chop",
        "Roll",
    ];
    pub const BLOCKS: &[&str] = &["Across", "1.8 style"];
    pub static LEGACY: AtomicBool = AtomicBool::new(false);
    pub static SWING_18: AtomicU32 = AtomicU32::new(0);
    pub static BLOCK_18: AtomicU32 = AtomicU32::new(0);
    pub static BLOCK: AtomicU32 = AtomicU32::new(0);
    pub fn index_of(choices: &[&str], name: &str) -> u32 {
        choices.iter().position(|c| *c == name).unwrap_or(0) as u32
    }
    fn r(axis: Vec3, deg: f32) -> Mat4 {
        let axis = axis.normalize_or_zero();
        if axis == Vec3::ZERO {
            Mat4::IDENTITY
        } else {
            Mat4::from_axis_angle(axis, deg.to_radians())
        }
    }
    fn t(x: f32, y: f32, z: f32) -> Mat4 {
        Mat4::from_translation(Vec3::new(x, y, z))
    }
    fn sc(k: f32) -> Mat4 {
        Mat4::from_scale(Vec3::splat(k))
    }
    fn wave(s: f32) -> f32 {
        (s.max(0.0).sqrt() * std::f32::consts::PI).sin()
    }
    fn lift(s: f32) -> f32 {
        (s * s * std::f32::consts::PI).sin()
    }
    fn game_swing(side: f32, attack: f32) -> Mat4 {
        let (pi, root) = (std::f32::consts::PI, attack.max(0.0).sqrt());
        t(
            side * -0.4 * (root * pi).sin(),
            0.2 * (root * std::f32::consts::TAU).sin(),
            -0.2 * (attack * pi).sin(),
        ) * r(Vec3::Y, side * (45.0 - lift(attack) * 20.0))
            * r(Vec3::Z, side * wave(attack) * -20.0) * r(Vec3::X, wave(attack) * -80.0)
            * r(Vec3::Y, side * -45.0)
    }
    fn styled_swing(style: u32, side: f32, s: f32, sword: bool) -> Mat4 {
        let (w, l) = (wave(s), lift(s));
        let (pi, root) = (std::f32::consts::PI, s.max(0.0).sqrt());
        let back = r(Vec3::Y, side * -45.0);
        let turn = r(Vec3::Y, side * 45.0);
        match style {
            1 => {
                r(Vec3::Y, side * (45.0 - l * 20.0)) * r(Vec3::Z, side * w * -10.0)
                    * r(Vec3::X, w * -80.0) * back
            }
            2 => turn * r(Vec3::Z, side * w * -10.0) * r(Vec3::X, w * -40.0) * back,
            3 if sword && side > 0.0 => across(side, w),
            3 => {
                r(Vec3::Y, side * (45.0 - l * 20.0)) * r(Vec3::Z, side * w * -20.0)
                    * r(Vec3::X, w * -80.0) * back
            }
            4 => t(0.0, 0.02 * w, -0.32 * w) * r(Vec3::X, -12.0 * w),
            5 => turn * r(Vec3::X, -95.0 * w) * back,
            6 => {
                t(side * -0.5 * w, 0.0, -0.05 * w) * r(Vec3::Y, side * (45.0 - l * 20.0))
                    * r(Vec3::X, -40.0 * w) * back
            }
            7 => turn * r(Vec3::Z, side * w * -55.0) * r(Vec3::X, w * -15.0) * back,
            8 => t(side * -0.05 * w, 0.08 * w, -0.42 * w) * r(Vec3::X, 22.0 * w),
            9 => turn * r(Vec3::X, -360.0 * s) * back,
            10 => {
                t(
                    side * -0.2 * (root * pi).sin(),
                    0.1 * (root * std::f32::consts::TAU).sin(),
                    -0.1 * (s * pi).sin(),
                ) * r(Vec3::Y, side * (45.0 - l * 10.0)) * r(Vec3::Z, side * w * -10.0)
                    * r(Vec3::X, w * -40.0) * back
            }
            11 => {
                r(Vec3::Y, side * (45.0 - l * 20.0)) * r(Vec3::Z, side * l * -20.0)
                    * r(Vec3::X, l * -80.0) * back
            }
            12 => t(0.0, -0.1 * w, -0.26 * w) * r(Vec3::X, -32.0 * w),
            _ => game_swing(side, s),
        }
    }
    fn across(side: f32, w: f32) -> Mat4 {
        t(0.0, 0.1, -0.2) * r(Vec3::Y, side * 65.0) * r(Vec3::X, side * -100.0)
            * r(Vec3::Z, side * 20.0) * r(Vec3::Y, side * -220.0)
            * r(Vec3::Z, side * w * -40.0) * r(Vec3::Y, side * -45.0)
    }
    pub fn swing_swap(
        left: bool,
        attack: f32,
        s: f32,
        sword: bool,
        block: bool,
    ) -> Option<Mat4> {
        let style = if block && sword { 3 } else { SWING.load(Ordering::Relaxed) };
        let side = if left { -1.0 } else { 1.0 };
        (style != 0)
            .then(|| {
                game_swing(side, attack).inverse() * styled_swing(style, side, s, sword)
            })
    }
    #[derive(Clone, Copy, Debug, Default)]
    struct Clock {
        started: u64,
        progress: f32,
        playing: bool,
        queued: bool,
        at: f32,
    }
    static CLOCK: std::sync::Mutex<Clock> = std::sync::Mutex::new(Clock {
        started: 0,
        progress: 0.0,
        playing: false,
        queued: false,
        at: 0.0,
    });
    fn clock_step(c: &mut Clock, started: u64, ticks: i32, time: f32) -> f32 {
        let dt = (time - c.at).clamp(0.0, 0.1);
        c.at = time;
        if started != c.started {
            c.started = started;
            if c.playing {
                c.queued = true;
            } else {
                (c.playing, c.progress) = (true, 0.0);
                return 0.0;
            }
        }
        if c.playing {
            c.progress += dt / (ticks.max(1) as f32 / 20.0);
            if c.progress >= 1.0 {
                if c.queued {
                    c.queued = false;
                    c.progress = (c.progress - 1.0).min(0.5);
                } else {
                    (c.playing, c.progress) = (false, 0.0);
                }
            }
        }
        if c.playing { c.progress } else { 0.0 }
    }
    pub fn swing_clock(started: u64, ticks: i32, time: f32) -> f32 {
        clock_step(
            &mut CLOCK.lock().unwrap_or_else(|e| e.into_inner()),
            started,
            ticks,
            time,
        )
    }
    fn hand_18(s: f32) -> Mat4 {
        r(Vec3::Y, 45.0) * r(Vec3::Y, lift(s) * -20.0) * r(Vec3::Z, wave(s) * -20.0)
            * r(Vec3::X, wave(s) * -80.0) * sc(0.4)
    }
    fn used_18(s: f32) -> Mat4 {
        let root = s.max(0.0).sqrt();
        t(
            -0.4 * (root * std::f32::consts::PI).sin(),
            0.2 * (root * std::f32::consts::TAU).sin(),
            -0.2 * (s * std::f32::consts::PI).sin(),
        )
    }
    fn block_18() -> Mat4 {
        t(-0.5, 0.2, 0.0) * r(Vec3::Y, 30.0) * r(Vec3::X, -80.0) * r(Vec3::Y, 60.0)
    }
    fn native_tail() -> Mat4 {
        t(0.0, 0.25, 0.125) * r(Vec3::Y, -135.0) * r(Vec3::Z, 25.0) * sc(1.7) * sc(0.5)
            * super::modern_flat_display(false).inverse()
    }
    fn tail_18() -> Mat4 {
        sc(2.0) * native_tail()
    }
    fn swing_18(style: u32, s: f32) -> Mat4 {
        let (w, l) = (wave(s), lift(s));
        let still = hand_18(0.0);
        match style {
            1 => t(0.0, 0.0, -0.35 * w) * still * r(Vec3::X, -15.0 * w),
            2 => hand_18(s),
            3 => {
                let root = s.max(0.0).sqrt();
                t(
                    -0.2 * (root * std::f32::consts::PI).sin(),
                    0.1 * (root * std::f32::consts::TAU).sin(),
                    -0.1 * (s * std::f32::consts::PI).sin(),
                ) * r(Vec3::Y, 45.0) * r(Vec3::Y, l * -10.0) * r(Vec3::Z, l * -15.0)
                    * r(Vec3::X, l * -60.0) * sc(0.4)
            }
            4 => still * r(Vec3::X, -75.0 * w),
            5 => t(-0.3 * w, 0.0, 0.0) * still * r(Vec3::new(1.0, 0.5, 0.0), -50.0 * w),
            6 => still * r(Vec3::Z, 40.0 * w) * r(Vec3::X, -20.0 * w),
            7 => t(0.0, 0.05 * w, -0.45 * w) * still * r(Vec3::Y, -20.0 * w),
            8 => still * r(Vec3::Z, -360.0 * s),
            9 => {
                let root = s.max(0.0).sqrt();
                t(
                    -0.2 * (root * std::f32::consts::PI).sin(),
                    0.1 * (root * std::f32::consts::TAU).sin(),
                    -0.1 * (s * std::f32::consts::PI).sin(),
                ) * r(Vec3::Y, 45.0) * r(Vec3::Y, l * -10.0) * r(Vec3::Z, w * -10.0)
                    * r(Vec3::X, w * -40.0) * sc(0.4)
            }
            10 => t(0.0, -0.1 * w, -0.25 * w) * still * r(Vec3::X, -30.0 * w),
            11 => {
                still * r(Vec3::new(-1.0, 0.0, 1.2), -45.0 * w)
                    * r(Vec3::new(1.0, -0.5 * w, 0.0), -35.0 * w)
            }
            _ => used_18(s) * hand_18(s),
        }
    }
    fn block_anim_18(style: u32, s: f32, time: f32) -> Mat4 {
        let (w, l) = (wave(s), lift(s));
        let still = hand_18(0.0);
        let block = block_18();
        match style {
            1 => hand_18(s) * block,
            2 => still * t(0.0, 0.0, -0.6 * w) * block,
            3 => still * r(Vec3::new(1.0, 0.6, 0.0), -45.0 * w) * block,
            4 => {
                still * r(Vec3::new(-8.0, 0.0, 9.0), -30.0 * w)
                    * r(Vec3::new(1.0, -0.5 * w, 0.0), -40.0 * w) * block
            }
            5 => still * block * r(Vec3::new(1.0, 1.0, 0.0), -60.0 * w),
            6 => still * r(Vec3::X, -25.0 * w) * block,
            7 => still * t(0.0, 0.25 * w, -0.5 * w) * block,
            8 => still * block * r(Vec3::X, -(time * 180.0) % 360.0),
            9 => still * r(Vec3::Y, -15.0 * l) * r(Vec3::X, -35.0 * l) * block,
            10 => still * t(0.0, -0.25, 0.0) * r(Vec3::X, -30.0 * w) * block,
            11 => still * t(0.0, 0.3 * w, 0.0) * r(Vec3::Z, -20.0 * w) * block,
            12 => still * block * r(Vec3::Z, 35.0 * w),
            13 => still * r(Vec3::X, -60.0 * w) * block,
            14 => still * block * r(Vec3::Y, -50.0 * w),
            _ => still * block,
        }
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Drawn {
        Modern,
        Legacy { block: bool },
    }
    fn drawn(drawn: Drawn, attack: f32) -> Mat4 {
        match drawn {
            Drawn::Modern => game_swing(1.0, attack),
            Drawn::Legacy { block: false } => {
                used_18(attack) * hand_18(attack) * native_tail()
            }
            Drawn::Legacy { block: true } => hand_18(attack) * block_18() * native_tail(),
        }
    }
    pub fn legacy_pose(
        pose: Mat4,
        attack: f32,
        s: f32,
        block: bool,
        time: f32,
        how: Drawn,
        view: Mat4,
    ) -> Option<Mat4> {
        let native = matches!(how, Drawn::Legacy { .. });
        if !LEGACY.load(Ordering::Relaxed) && !native
            && !(block && BLOCK.load(Ordering::Relaxed) == 1)
        {
            return None;
        }
        let held = if block {
            block_anim_18(BLOCK_18.load(Ordering::Relaxed), s, time)
        } else {
            swing_18(SWING_18.load(Ordering::Relaxed), s)
        };
        let back = drawn(how, attack).inverse();
        back.is_finite().then(|| pose * back * view * held * tail_18())
    }
    pub fn view_of(left: bool) -> Mat4 {
        match VIEW.read() {
            Ok(view) => view.map_or(Mat4::IDENTITY, |v| v[usize::from(left)]),
            Err(_) => Mat4::IDENTITY,
        }
    }
    #[cfg(test)]
    mod animation_tests {
        use super::*;
        fn close(a: Mat4, b: Mat4) -> bool {
            a.abs_diff_eq(b, 1e-4)
        }
        #[test]
        fn the_swing_clock_plays_each_swing_out_and_chains_fast_hits() {
            let mut c = Clock::default();
            let frames = |c: &mut Clock, started: u64, from: f32, n: usize| -> Vec<f32> {
                (0..n)
                    .map(|i| clock_step(c, started, 6, from + i as f32 * 0.01))
                    .collect()
            };
            let one = frames(&mut c, 1, 0.0, 40);
            assert_eq!(one[0], 0.0);
            assert!(
                one.windows(2).take(29).all(| w | w[1] > w[0]),
                "it plays forward: {one:?}"
            );
            assert_eq!(* one.last().unwrap(), 0.0, "and rests once played");
            let mut c = Clock::default();
            let mut last = 0.0;
            let mut drops = 0;
            for i in 0..90 {
                let v = clock_step(&mut c, 1 + i / 10, 6, i as f32 * 0.01);
                if v + 0.5 < last {
                    drops += 1;
                }
                last = v;
            }
            assert!(drops <= 3, "a swing ends before the next begins: {drops}");
        }
        #[test]
        fn every_animation_is_finite_through_the_swing() {
            for i in 0..=20 {
                let s = i as f32 / 20.0;
                for style in 0..SWINGS.len() as u32 {
                    for side in [1.0, -1.0] {
                        assert!(
                            styled_swing(style, side, s, true).is_finite(),
                            "1.9+ {style} at {s}"
                        );
                    }
                }
                for style in 0..SWINGS_18.len() as u32 {
                    assert!(swing_18(style, s).is_finite(), "1.8 swing {style} at {s}");
                }
                for style in 0..BLOCKS_18.len() as u32 {
                    assert!(
                        block_anim_18(style, s, 3.7).is_finite(),
                        "1.8 block {style} at {s}"
                    );
                }
            }
        }
        #[test]
        fn at_rest_the_swings_hold_still_and_the_game_s_own_is_unchanged() {
            assert!(close(game_swing(1.0, 0.0), Mat4::IDENTITY));
            for style in [1, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12] {
                assert!(
                    close(styled_swing(style, 1.0, 0.0, false), Mat4::IDENTITY),
                    "1.9+ {style}"
                );
            }
            for style in 0..SWINGS_18.len() as u32 {
                assert!(
                    close(swing_18(style, 0.0), hand_18(0.0)), "1.8 {}", SWINGS_18[style
                    as usize]
                );
            }
            for style in (0..BLOCKS_18.len() as u32).filter(|&s| s != 8 && s != 10) {
                assert!(
                    close(block_anim_18(style, 0.0, 0.0), hand_18(0.0) * block_18()),
                    "block {}", BLOCKS_18[style as usize]
                );
            }
        }
        #[test]
        fn the_1_8_pose_matches_horizon_s_own_and_undoes_whatever_the_game_drew() {
            let arm = t(0.56, -0.52 - 0.3 * 0.6, -0.72);
            let s = 0.4;
            let native = |block: bool| {
                super::super::legacy_item_pose(
                        false,
                        0.3,
                        s,
                        super::super::ItemPose::Swing,
                        block,
                    )
                    .unwrap()
            };
            assert!(
                close(arm * drawn(Drawn::Legacy { block : false }, s), native(false))
            );
            assert!(close(arm * drawn(Drawn::Legacy { block : true }, s), native(true)));
            LEGACY.store(true, Ordering::Relaxed);
            SWING_18.store(0, Ordering::Relaxed);
            BLOCK_18.store(0, Ordering::Relaxed);
            let want = arm * used_18(s) * hand_18(s) * tail_18();
            let from_modern = legacy_pose(
                    arm * game_swing(1.0, s),
                    s,
                    s,
                    false,
                    0.0,
                    Drawn::Modern,
                    Mat4::IDENTITY,
                )
                .unwrap();
            let from_native = legacy_pose(
                    native(false),
                    s,
                    s,
                    false,
                    0.0,
                    Drawn::Legacy { block: false },
                    Mat4::IDENTITY,
                )
                .unwrap();
            assert!(close(from_modern, want) && close(from_native, want));
            let blocked = legacy_pose(
                    native(false),
                    s,
                    0.0,
                    true,
                    0.0,
                    Drawn::Legacy { block: false },
                    Mat4::IDENTITY,
                )
                .unwrap();
            assert!(close(blocked, arm * hand_18(0.0) * block_18() * tail_18()));
            LEGACY.store(false, Ordering::Relaxed);
            BLOCK.store(0, Ordering::Relaxed);
            assert!(
                legacy_pose(arm * game_swing(1.0, s), s, s, true, 0.0, Drawn::Modern,
                Mat4::IDENTITY).is_none()
            );
            BLOCK.store(1, Ordering::Relaxed);
            assert!(
                legacy_pose(arm * game_swing(1.0, s), s, s, true, 0.0, Drawn::Modern,
                Mat4::IDENTITY).is_some()
            );
            BLOCK.store(0, Ordering::Relaxed);
            assert_eq!(index_of(BLOCKS_18, "Swank"), 4);
            assert_eq!(index_of(SWINGS, "3"), 3);
            assert_eq!(index_of(SWINGS, "nonsense"), 0);
        }
    }
}
