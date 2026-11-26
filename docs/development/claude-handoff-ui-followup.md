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
