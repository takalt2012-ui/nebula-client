# Nebula / Horizon session notes

Date: 2026-11-19

This records the changes and findings from the launcher and game-engine work in this session. Nebula is a separate launcher for the installed Horizon build; it is not the official Horizon launcher and it does not replace Horizon's managed game-start path.

## Changes made

### Horizon Nova instance settings

Edited `/home/twinsinvan/.local/share/horizon/instances/horizon-nova/` configuration:

- `options.txt`
  - `cutoutLeaves:true` — enables leaf cutout transparency (was false, which made leaf blocks opaque).
  - `improvedTransparency:true` — enables Horizon's order-independent transparency renderer for supported translucent content. This is the setting for general transparent rendering/blending; it can add GPU work.
  - `maxFps:260` — Horizon's maximum/unlimited cap value.
  - `soundDevice:""` — use the system default audio device. Explicit `pulse` selection was tried and failed to open, so it was reverted.
  - `soundCategory_block:1.0` — block sound volume is already at full volume.
- `horizon-graphics.txt` currently has the performance preset, 128-chunk LOD distance, reduced far detail, cave/entity culling, efficient frame pacing, 80% resolution scale with dynamic resolution enabled and 120 target FPS, hollow leaf culling, and automatic GPU culling. These are existing active settings from the optimization work; they trade some image quality for performance. The source describes auto GPU culling as conditional, not a guarantee that it improves every GPU.

The two transparency changes apply on the next game start or after Horizon reloads its graphics settings. They target leaves and supported translucent materials; they do not force alpha blending on every opaque block texture.

### Nebula launcher already in the project

- Native Rust/egui launcher with Nebula branding, animated sunset background, installed-build selection, Performance/Mods/Builds/Settings pages, basic HUD widget controls, and optional GameMode/MangoHud/performance launch settings.
- Launches Horizon through its managed `play` command to preserve Horizon's cache/session setup.
- No launcher UI redesign, FPS-counter source fix, or Horizon engine source-code change was made during this final sound-diagnosis step.

## Sound diagnosis

- User reports mob sounds and spawner/mob sounds play, while block-breaking sound behavior is faulty or unclear.
- Block category volume is `1.0`; the selected sound device is back to system default.
- Source inspection found Horizon routes `block.*` sounds to the block category and has hit/break sound calls in both local mining prediction and level-event handling. This does not prove the runtime event or asset playback is succeeding.
- User reports `/playsound minecraft:block.stone.break block @s` does not work in this build. Treat that command as unsupported/unverified, not as a successful diagnostic.
- Audio remains unresolved. No confirmed root cause or sound-engine fix has been made.

## Reported issues still open

1. In-game FPS counter is inaccurate. The native HUD derives its reading from a one-second frame count; the Nebula HUD mod uses a separate rolling frame-time counter. The exact counter the user means and the cause still need runtime comparison against Horizon's `horizon-ctl stats` output.
2. User says Nebula's UI still looks ugly. The visual redesign is not completed.
3. Block-breaking sound is not reliably audible; the `/playsound` test command is not available/working in this build. Needs a supported in-game test and/or runtime audio trace.
4. Verify leaf and general transparency visually after restarting. The transparency settings have been written, but not runtime-confirmed here.

## Useful diagnostics for next session

- Compare HUD readings with `horizon-ctl stats` over a steady 10-second scene; confirm whether the complaint is the vanilla Horizon FPS text or Nebula's basic HUD FPS widget.
- For audio, use Horizon's supported sound preview/UI or a known block-break event while checking `last-game.log`; do not assume vanilla `/playsound` command syntax exists in this build.
- Restart the Horizon Nova instance so it reloads `options.txt` and `horizon-graphics.txt` before judging the transparency changes.

## Follow-up: blending and lightweight shader

- Set `biomeBlendRadius:5` in the Nova instance's `options.txt`. Horizon exposes one biome colour blend radius (0–7); there is no separate ocean-blend radius in this build, so the same setting smooths biome water colours over the ocean as well.
- Added the custom Horizon-native shader pack at `/home/twinsinvan/.local/share/horizon/instances/horizon-nova/shaderpacks/nebula_radiant/` and selected it as `pickedShaderPack:horizon:nebula_radiant` in `horizon-graphics.txt`.
- Nebula Radiant is an original Horizon-native WGSL pack inspired by a vanilla-preserving shader style; it is not Complementary Reimagined or a code port. It adds a 1024-pixel shadow map, half-resolution reflective water, a coloured dawn/dusk sky and subtle colour grading. Shadows, reflections, warmth and saturation are configurable. It avoids bloom and volumetric ray marching; shadows/reflections add GPU cost.
- Pack is API 0.1 and uses the engine's documented WGSL shader-pack API. Validated successfully with `horizon-shader check` (shader API 0.1; sky, material and post stages accepted). Runtime appearance still needs confirmation after restarting the instance.

## Complementary Reimagined conversion

- User found the community Horizon Translator and will convert Complementary Reimagined themselves. No Complementary pack was downloaded, converted, or installed in this session.
- The locally installed and selected pack remains Nebula Radiant until the user selects their converted `*-horizon.zip` in Horizon's Shader Packs screen.
