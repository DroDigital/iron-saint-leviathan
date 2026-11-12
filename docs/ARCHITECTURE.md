# Architecture

## Stack (as specified, no deviations)

Rust stable (edition 2024, Cargo workspace) · **winit 0.30** (latest stable; 0.31 is still beta) · **wgpu 30** (Metal /
DX12 / Vulkan / GL) with WGSL · **gilrs** · **kira 0.12** (default features off: no `cpal-realtime-dbus`, so Linux
builds need no D-Bus dev package; no codec features because no audio files ship) · **glam** (with the `libm` feature
so transcendental maths is identical on every platform) · **gltf**, **image** (PNG only), **serde + ron**, **rand_pcg**,
**directories**, **crc32fast**, **data-encoding**. No engine, no physics engine, no ECS framework.

Collision is a ~150-line custom kit (vertical capsules, AABBs, swept spheres) in `sim::geom`. Entities are a plain
`Vec<Fighter>` with slot reuse; the simulation is small enough that struct-of-arrays or `hecs` would add nothing.

## Data flow

```
data/*.ron ──(dev: read_dir, hot reload)──┐
game.pak  ──(release: pak::read)──────────┴─> isl_data::Content (validated)
                                                      │
  input (winit/gilrs) ─> isl_input::PlayerInput ─> app::Mission ─> sim::fighter::Command ─┐
                                                                                          v
                                          isl_sim::world::World::step()  (fixed 60 Hz, seeded PCG32)
                                                      │ SimEvents
         ┌────────────────────────────┬───────────────┼──────────────────────┬─────────────────────┐
         v                            v               v                      v                     v
   audio (cue → synth sfx)   vfx (sparks, debris)   ghost::Recorder   camera shake / rumble   HUD messages
                                                      │
                                             HabitProfile (memory card) ─> nemesis AiParams
```

* **`sim` is pure and deterministic.** Same seed + same command stream ⇒ identical state (tested). Players and AI
  both emit `Command`s; AI perceives a *delayed* snapshot (≥100 ms reaction, enforced by validation), so it can only
  react to animations that have started, never to inputs.
* **Fixed timestep with interpolation.** `Mission` accumulates real time, steps the world at 60 Hz (max 4 steps per
  frame), latches one-shot presses until a tick consumes them, and renders interpolated positions/yaws, so 120 Hz
  ProMotion displays are smooth.
* **Data-driven content** with load-time validation (`data/src/validate.rs`): unique ids, reference integrity,
  slot/side consistency, the Cross-Harvest rule (enemy parts must be of the enemy's own anatomy), the 300 ms boss
  telegraph rule, dodge-only moves no longer than the shortest i-frame window, district enemy sides, ≥30 quirks,
  10 Disc Rot tiers.

## Rendering (`crates/render`)

1. World passes into a low-res `Rgba8Unorm` target + `Depth32Float` (one viewport per split-screen view):
   static district mesh (persistent buffer, pre-lit), dynamic geometry (CPU-transformed and Gouraud-lit with ambient
   + 3 directional + the 4 nearest point lights, uploaded with `queue.write_buffer`), translucent, additive.
2. Vertex shader snaps clip-space XY to the viewport pixel grid (only in front of the near plane), passes UVs with
   `@interpolate(linear)` (affine), colours screen-linear (PSX Gouraud), per-vertex fog.
3. Fragment shader: nearest sampling from one 64×64 texture array (procedural, 15-bit, 16-colour palettes),
   PSX ×2 modulation, fog, 15-bit quantize with 4×4 Bayer dither.
4. UI quads (bitmap font atlas) into the same target, 15-bit quantized.
5. Optional fake-FMV pass (posterize, 8×8 macroblocks, chroma subsample + smear).
6. Present: integer-scaled, letterboxed nearest upscale; optional CRT (scanlines, aperture grille, bloom) and composite.

Typical mission frame: 4–7 draw calls, 10–26k triangles. Large polygons are subdivided at mesh generation (8 m cells)
so the affine warp never hides a telegraph. Poly budgets are enforced by `cargo xtask validate`.

## Audio (`crates/audio`)

All 33 SFX and all music are synthesized at startup (`synth.rs`, `music.rs`). SPU mode renders at 22.05 kHz with a
gentle low-pass; toggling re-renders. Four reverb tracks model the PS1 SPU presets (Room, Studio, Hall, Space Echo =
reverb + delay); districts pick one. Music: five sample-locked layers per district (explore, combat, boss I–III)
composed from the district's tempo/root/scale/seed in a background thread, crossfaded on the next beat. Positional
audio uses distance attenuation and listener-relative panning. No device ⇒ silent no-op.

## Saves (`crates/save`)

Files are `ISLMC\x1a | version | crc32 | RON`, written atomically (tmp → fsync → rename) with a `.bak` fallback that
is loaded automatically if the main file is damaged. The memory card has 15 blocks; campaign saves (1–3 blocks by
size), builds (1–3 by complexity, heresy and quirks) and the corrupted block all cost blocks. The Archive and habit
profile live in a system area that never costs blocks. `migrate.rs` keeps every historical schema frozen and converts
forward (v1 → v2 tested). Export codes are base32 with a CRC and 16-bit content hashes (collision-tested).

## Platforms and packaging

* macOS: native `aarch64-apple-darwin`; `cargo xtask package <bin> dist macos` builds `IronSaintLeviathan.app`
  (Info.plist, `game.pak` in `Resources`); CI ad-hoc signs it (`codesign --force --deep -s -`). **Notarization** for
  distribution: sign with a Developer ID (`codesign --options runtime --timestamp -s "Developer ID Application: …"`),
  zip, `xcrun notarytool submit app.zip --apple-id … --team-id … --wait`, then `xcrun stapler staple
  IronSaintLeviathan.app`. A universal binary is `lipo -create` of the arm64 and x86_64 builds.
* Windows x64: `.zip` with the exe, `game.pak`, docs and licenses.
* Linux x64 (X11 + Wayland via winit): `.tar.gz`.

Platform differences are confined to winit/wgpu/cpal/gilrs (rumble is unavailable on macOS through gilrs and is
silently skipped).

## Performance budget (M1 baseline)

* Release profile: `opt-level=3`, `lto="thin"`, `codegen-units=1`, `panic="abort"`.
* Present mode Fifo (vsync) by default, Mailbox/no-vsync selectable; menus capped at 30 fps; nothing renders while
  minimized or (configurable) unfocused; no busy-waiting.
* CPU work per frame is dominated by CPU vertex lighting of dynamic meshes (≈10–15k vertices) and the 60 Hz sim;
  both are well under 1 ms on desktop-class CPUs in release. The soak test (`--soak`) runs the whole game loop
  headless and is the regression harness for frame cost.
* **Profiling on macOS:** build release with debug symbols (`CARGO_PROFILE_RELEASE_DEBUG=true cargo build --release`),
  open Xcode → Instruments → *Metal System Trace* (GPU) and *Time Profiler* (CPU), target the binary. `tracing`
  spans can be enabled with `RUST_LOG=iron_saint_leviathan=trace`; the `dev` feature shows draw calls / triangles.
