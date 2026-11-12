# IRON SAINT // LEVIATHAN

A PlayStation 1–styled action soulslike, written from scratch in Rust **without a game engine** (winit + wgpu +
kira + gilrs). Two sides fight over one coastal city, Harrowmouth: **Saints** (giant mechs grown from kaiju tissue)
and **Kaiju** (monsters from the sea). You play both. Saint parts only drop while you play Kaiju; Kaiju parts only
drop while you play Saint. Your saved builds come back as bosses on the other side, driven by your own habits.

> 1997 on the surface, 2026 underneath.

The full design brief is in [`docs/DESIGN_PROMPT.md`](docs/DESIGN_PROMPT.md); every deliberate deviation, the
inconsistencies found in it and the improvements made are in [`docs/DESIGN_NOTES.md`](docs/DESIGN_NOTES.md).

## Build and run

```bash
cargo run --release
```

Linux needs `libasound2-dev libudev-dev libxkbcommon-dev`. macOS (Apple Silicon native) and Windows need nothing extra.

```bash
cargo test --workspace                    # 50+ unit/integration tests incl. determinism and boss balance
cargo xtask validate                      # content rules + PSX poly budgets
cargo xtask check-licenses                # every file in assets/ must be in ASSETS.md (CI gate)
cargo xtask pak                           # pack data/ into game.pak (release builds load it)
cargo run --release -- --screenshots out  # render the screenshot scenarios headlessly
cargo run --release -- --soak 600 60      # bot soak test: 10 simulated minutes, render every 60th frame
cargo run --features dev                  # hot-reload data/*.ron, frame stats overlay
```

Saves, settings, photos and exported codes live in the OS user-data directory (`directories` crate); set
`ISL_DATA_DIR` to override. `ISL_NO_AUDIO=1` disables audio.

## Controls (fully rebindable in SETTINGS > CONTROLS)

| Action | Gamepad | Keyboard / mouse |
|---|---|---|
| Move / camera | Left stick / right stick | WASD / mouse (arrows) |
| Light attack | R1 | J / LMB |
| Heavy attack · **Sever** a posture-broken part | R2 | K / RMB |
| Guard (hold or toggle) · Guard + Light = bash | L1 | Left Shift / L |
| Parry | L2 | I / Mouse 4 |
| Dodge (dash / hop / shoulder-slide by weight) | B / Circle | Space |
| Special (backpack / mutation) | Y / Triangle | U or R |
| Heal (Repair Charge / Regrowth) | X / Square | H or 1 |
| Interact · rest · recover/harvest remains · climb | A / Cross | E |
| Lock on · switch target · cycle locked **part** | R3 · D-pad ◀▶ · D-pad ▲ | Q / MMB · Z / C · Tab |
| Heretic Form (>50 % heretic parts) | L3 | F |
| Photo mode · Pause | Select · Start | P · Esc |

## What's in the game

* **War Campaign** – six districts (Harbor, Cathedral Row, Foundry Quarter, Old Rail Yards, Lighthouse Point, Drowned
  Market) in four states (Fortified / Contested / Flooded / Ruined) plus the **Breakwater** finale; Tide Clock and
  weather (clear, fog bank, storm with lightning reveals, blackout); random war events (Demo Disc invasion, evacuation,
  the Cathedral bell, Night of Static, supply drops); district contracts; persistent destruction; wrecks and carcasses
  you recover or – from the other side – strip for parts; "PLEASE INSERT DISC 2" swaps; five endings incl. the true
  ending and a secret Disc 3.
* **Cross-Harvest** – 182 parts, 152 moves, 49 quirks (37 rollable), Pristine / Scarred / Mangled conditions, heretic
  grafts (Sync loss and hallucinations for Saints, Machine-blood for Kaiju, Heretic Form above 50 %), weight & balance
  with centre-of-mass lean and drift, move-set composition and validation, six stamina models (Heat, Battery, Fuel,
  Hunger, Blood, Breath), posture and Sever, locational damage, scale combat with climbable weak points.
* **The Foundry / The Nest** – turntable preview, stat deltas, CoM gizmo, paint/decals, repair, quirk re-roll and
  transplant (unlocked by cross-side harvests), Codex fabrication, save to the **15-block memory card**, export codes.
* **Nemesis** – habit recording (dodge bias, favourite moves/combos, heal threshold, spacing, parry rate, feint
  reactions) drives your saved builds as bosses; named rivals THE ROOKIE, THE GRUDGE, THE CHAMPION.
* **Modes** – Tide Runs (seeded roguelite that alternates discs every fight), Daily Tide, Omens, Disc Rot 1–10 NG+,
  Nemesis Gauntlet, Link Cable Versus (split-screen), Test Arena with an inert dummy and a habit dummy, photo mode
  with a fake magazine cover.
* **PSX presentation** – 320×240 (and 256×224, 368×240, 512×448, 640×480, 16:9) low-res target, vertex snapping,
  affine texture mapping, 15-bit colour with 4×4 Bayer dithering, Gouraud vertex lighting, blob shadows, fog as draw
  distance, fake-FMV pass, optional CRT and composite; SPU-mode audio with PS1 reverb presets and adaptive music.
* **Accessibility** – full rebinding, hold/toggle guard, subtitle size, shape+sound telegraphs (never colour alone),
  colour-blind palettes, shake/jitter/dither sliders, photosensitive mode, assists (Steady Hands, Old Bones, Insurance)
  that never lock content.

Everything you see and hear is generated in code – textures, meshes, font, VFX, sound effects and music – so the
game ships with **zero third-party assets** (see [`ASSETS.md`](ASSETS.md)).

## Layout

| Crate | Responsibility |
|---|---|
| `app` | Binary: main loop, screens/state machine, missions, rigs, HUD, VFX, headless tools |
| `sim` | Deterministic 60 Hz gameplay: combat, builds, AI, city, tide, war, archive, tide runs |
| `render` | wgpu PSX pipeline, procedural textures and meshes |
| `audio` | kira wrapper, synthesized SFX, tracker-style adaptive music, SPU mode |
| `input` | Action mapping, rebinding, gamepads, rumble |
| `data` | RON content schema + validation |
| `ghost` | Habit recording → nemesis AI |
| `save` | Memory card, checksums, migrations, export codes |
| `ui` | Bitmap font + immediate-mode widgets |
| `assets` | `.pak`, mesh format, 15-bit quantizer |
| `xtask` | Import pipeline, pak, license check, budgets, packaging |

More in [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## License

Code: MIT OR Apache-2.0.
