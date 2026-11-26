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
