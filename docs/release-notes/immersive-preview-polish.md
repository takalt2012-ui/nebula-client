# Immersive preview polish

A practical launcher update, not a redesign for the sake of redesign:

- The window now starts opaque, so other apps should no longer bleed through it. Transparency remains a switch in Settings.
- The modern interface uses the original violet family again, with quieter surfaces and less overdone rounding.
- Immersive mode gets a block-built landscape backdrop; the original sunset stays available in Legacy look.
- The immersive skin preview reads the selected Horizon account's public Minecraft skin and cape, caches them locally, and rotates only while you drag the model.
- The skin preview is deliberately kept out of Classic mode.

The preview fetch uses Mojang's public profile texture endpoint and the system `curl` command. It does not read or save Microsoft credentials.
