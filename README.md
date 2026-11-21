# Nebula Client

Nebula Client is an independent native Linux interface for installed Horizon builds. It has its own branded interface, an animated sunset scene, a visible Performance Mode switch, and optional GameMode and MangoHud integration.

For game starts it invokes Horizon's installed `play` command. Horizon must perform this launch step for managed builds: it acquires the cache leases, applies the selected instance, and hands a renewed account session to the game. Nebula provides the interface and game-scoped performance environment; it does not handle credentials or bypass Horizon's installation safeguards.

## Build

```bash
cargo build --release
```

The release executable is `target/release/nebula-client`. Performance Mode uses a dedicated Mesa shader cache. Loading I/O priority, GameMode, MangoHud, and renderer-specific OpenGL variables are separate settings.
