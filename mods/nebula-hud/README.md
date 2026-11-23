# Nebula HUD

A Horizon native (WebAssembly) mod: FPS, clicks per second (left | right) and block position with facing in one
dark panel with a violet edge. Built on the same SDK calls as Horizon's own FPS Counter, Keystrokes and Coordinates
HUD mods, so it stacks with them and gives way to F3's debug screen.

Options > Mods > Nebula HUD: corner, and a switch for each row.

## Build

```sh
cargo install --git https://github.com/horizon-rust/horizon cargo-horizon-mod
cd mods/nebula-hud
cargo horizon-mod build        # target/horizon-mod/nebula_hud.hmod
cargo horizon-mod run          # build, then start Horizon with it on
```

`src/model.rs` holds the counters and layout arithmetic with no game calls: `rustc --test src/model.rs` runs its tests.
