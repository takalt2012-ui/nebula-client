# Nebula UI follow-up — Claude handoff

Branch: `nebula-redesign`
Repository: `takalt2012-ui/nebula-client`

This note records the recent sunset restoration work and the remaining UI issues visible in the latest launcher screenshot. Please read this before making further changes, then inspect the current branch and source rather than assuming the descriptions below identify the exact implementation.

## Work already completed

- Restored the original immersive sunset landscape asset in `assets/immersive-voxel.png` (commit `11a295c`).
- Changed the background texture filtering from `NEAREST` to `LINEAR` for smoother scaling (commit `a9949ba`).
- Added the archive release-note page at `docs/release-notes/sunset-landscape-restored.md` (commit `10a673a`). It links to the intended v0.2.3 GitHub release.
- The sunset image is now back in the launcher; do not replace, crop, or redesign it unless explicitly asked.

## Issues in the latest screenshot

### 1. Play button is still using the old purple

The large primary **Play** button is visibly the previous saturated purple, even though the launcher has the sunset scene and the newer visual treatment. It looks disconnected from the rest of the palette.

Please:
- Trace the button's actual style/token source and find why the newer palette is not applied.
- Remove stale hard-coded color or obsolete theme-token usage if that is the cause.
- Use the current Nebula palette consistently; keep the button premium, restrained, and readable over the image.
- Check other primary/action buttons for the same regression.
- Do not invent a completely new UI or change the sunset background.

### 2. The animated 3D player model is missing

The screenshot shows the Nebula star with a rotating progress ring near the logo. That is **not** the player-model animation.

The center of the screen instead displays: `Sign in to preview your skin`. The expected spinning Minecraft player/skin preview is absent.

Please:
- Find the skin/player preview component and its render/visibility conditions.
- Check whether the sign-in state incorrectly replaces or hides the model, whether the preview is not mounted, or whether loading/error handling leaves it invisible.
- Restore the actual animated 3D player model in its intended location and make its rotation visibly work.
- Preserve the sign-in prompt when authentication is genuinely required, but do not mistake the logo spinner for the model.
- Test both signed-out and signed-in states, and verify behavior when no skin is available.

## Visual direction and constraints

- Keep the original immersive sunset landscape exactly as restored.
- Aim for polished, intentional, premium UI—not a generic or “vibecoded” redesign.
- Avoid reverting to the old purple accent just because it is currently visible.
- Keep layout balanced and avoid clipping or unnecessary empty space.
- Make focused fixes, then build/run the launcher and inspect a real screenshot before reporting completion.

## Release status

The release-note document points to v0.2.3, but the release itself was not confirmed published in the handoff conversation. Verify the current GitHub release/tag state before claiming it is released. Do not create a release without checking the build and the user's intended version.

## Suggested completion checklist

- [ ] Identify and fix the old-purple Play button styling source.
- [ ] Restore the real animated player/skin preview.
- [ ] Verify signed-out, signed-in, and no-skin states.
- [ ] Run the project/build checks and inspect the launcher screenshot.
- [ ] Summarize changed files and commits; do not silently alter the sunset asset.


## Latest screenshot update — account control and skin preview

The newest screenshot shows the account signed in (username displayed in the top-right), and a message saying `Skin preview unavailable`. The earlier `Sign in to preview your skin` state has changed, but the actual animated Minecraft player model is still not visible.

### 3. Skin preview remains unavailable after sign-in

Please trace the full skin-preview flow, not just authentication visibility:
- Inspect the account/profile data returned by the sign-in helper and how its username/UUID is mapped.
- Check skin URL/provider resolution, fetch errors, texture decoding, and the 3D renderer's initialization.
- Make loading, no-skin, and failure states explicit and useful; do not leave the center area permanently blank.
- If a valid skin can be resolved, render the actual 3D Minecraft model and animate its rotation. The Nebula logo spinner is not a substitute.
- Test with the currently signed-in account as well as a signed-out account. Avoid logging credentials or tokens.

### 4. Make the login/account button clearly clickable

The top-right account area currently looks like the username text is being copied/selected, and it is unclear which part the user should press.

Please redesign this as one cohesive, accessible account control:
- Make the username and circular avatar part of the same obvious clickable hit target.
- Use a pointer cursor, hover/focus/pressed feedback, and a clear menu/open state.
- Keep the text selectable only if that is intentional; prevent accidental text-selection from making the control feel broken.
- Ensure clicking either the username or avatar opens the same account/sign-in menu.
- For signed-out state, use a clear action label such as **Sign in** with an appropriate icon/button treatment; avoid a floating label beside a separate avatar where the press target is ambiguous.
- For signed-in state, show the username and avatar together and make the account menu's action labels clear.
- Preserve the existing premium visual direction and sunset background; do not turn this into a broad redesign.

Updated screenshot notes: signed-in username is visible; account avatar is a purple circle with an initial; skin preview says `Skin preview unavailable`; Play button still appears in the old purple. Treat these as observations, then inspect the code to find the causes.


## Latest recording review — 2026-11-27

The user supplied a screen recording of the current Immersive launcher. I reviewed the frames and inspected the current `nebula-redesign` source, including `src/immersive.rs`, `src/skin_viewer.rs`, and the background loader in `src/main.rs`.

### Skin model: now loads, but the rendering looks broken

The recording shows the account signed in and a model is being drawn, but it appears as a large, flat, separated neon/rainbow shape on the left—not a convincing Minecraft player preview. The previous handoff's "preview unavailable" description is now stale: the newest code has advanced past that failure state.

Relevant code observations:
- The viewer is a custom egui painter implementation in `src/skin_viewer.rs`, not a true 3D model renderer.
- It projects individual cuboid faces manually and draws them as flat meshes.
- `draw_box` currently selects only one front/back face, one side face, and the top; it omits the bottom and does not do robust per-face depth sorting/occlusion.
- The model uses fixed preview geometry/scale and is positioned at `screen.left() + 190`, which makes it look oversized and detached in the recording.
- The skin-fetch commit `4a3844a` already fixed Mojang's uppercase `SKIN/CAPE` keys and HTTP texture URLs. Do not repeat that fix without evidence.

Please:
1. Validate that the downloaded/cached skin image is actually the account's correct 64x64 (or supported legacy) skin and not a fallback/invalid texture. Avoid assuming the rainbow colors are the skin itself until inspecting the image.
2. Fix UV orientation, face geometry, depth/occlusion, and outer-layer rendering so it reads as a complete Minecraft player from front and side while rotating smoothly.
3. Make the preview a smaller, intentionally placed feature with a proper visual anchor and breathing room. Do not let it dominate or collide with the brand/dock.
4. Prefer a reliable 3D/depth-correct approach or implement explicit face sorting/occlusion; a pile of unsorted painter quads is not enough.
5. Test classic 64x64 skins, slim-arm skins, transparent outer layers, no-skin/default skin, and rotations around the full turn. Keep it performant.

### Background: visibly soft in the recording

The recording's forest background looks substantially blurred/soft. Current code evidence:
- `src/main.rs` loads `assets/immersive-bc-forest.png` as the Immersive background with `TextureOptions::LINEAR`.
- `src/immersive.rs` paints a dark-green palette wash over it at alpha 110. There is no blur shader in this background path.
- Therefore, the softness may be baked into the image asset/resolution; the green wash also reduces contrast/clarity but is not itself a blur.

Please inspect the actual source image dimensions and sharpness. Make the Immersive background noticeably clearer/sharper (use a genuinely sharp/high-resolution source or improve the asset, and reduce any unnecessary wash if it obscures detail). Do not switch to nearest-neighbor pixelation as a fake sharpening fix. Preserve the original restored sunset asset and do not edit the Legacy background or Legacy look.

### UI polish direction — Immersive only

The user wants the newer Immersive UI to feel more professional, taking *UI ideas/elements* from MinecraftX Launcher without copying its identity or redesigning Nebula into a clone.

Good elements to adapt:
- A clean, well-structured launcher home with clear hierarchy.
- Refined account/profile pill and a consistent icon rail.
- A polished build/version selector with clear selected state.
- A strong, well-aligned Play action and concise status information.
- Carefully spaced surfaces/cards, crisp borders, restrained depth, readable typography, and coherent hover/focus states.

Keep Nebula's own palette, branding, and layout character. No wholesale clone, no gratuitous gradients/glows, no over-decoration, and no "AI template" look. Improve existing Immersive elements in place.

**Hard scope boundary:** Do not change the Classic interface, Legacy look, legacy theme colors, or legacy background. Keep all visual changes scoped to the non-Legacy Immersive path (the existing `!legacy_look()` guard where appropriate). Preserve the original sunset scene exactly.

### Current branch and collaboration

The latest branch tip observed during review was `4a3844a` (skin metadata/URL fix). The account-control commit `599e3f6` is in its ancestry. Keep working on `nebula-redesign`, commit and push normally. Do not rewrite commit history or force-push.

### Updated acceptance checklist

- [ ] Skin preview looks like a complete, correctly textured Minecraft player—not disconnected neon planes.
- [ ] Correct UVs and face occlusion across front/side/full rotation; support classic/slim and outer layers.
- [ ] Preview is properly scaled and anchored in the layout.
- [ ] Background is demonstrably sharper/clearer from a better source or asset, not pixelated.
- [ ] Immersive UI polish takes inspiration from launcher UX patterns without copying MinecraftX.
- [ ] Classic and Legacy UI are unchanged.
- [ ] Build/run and inspect a fresh screenshot/video; report exact commits and what was verified.


### Additional skin issue — outer layer transparency

The user adds that their Minecraft skin's **outer layer is transparent**. Treat this as a separate rendering bug/requirement, not as an intentionally invisible layer.

Please:
- Preserve the skin PNG's alpha channel when decoding/loading and uploading/drawing the texture; do not flatten it onto an opaque background or discard alpha.
- Render the second-layer overlay parts (hat, jacket, sleeves, pants) with their intended transparent pixels and correct slight outward offset to avoid z-fighting.
- Ensure transparent pixels reveal the underlying base skin, while opaque pixels in the outer layer remain visible.
- Check alpha blending and texture format/color handling in the skin preview path. Do not simply force all outer-layer pixels opaque or hide the outer layer.
- Test a skin with visibly semi-transparent/transparent outer-layer pixels and compare it against the original PNG.
