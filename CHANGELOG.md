# Changelog

## 0.9.0 — first complete build (M0–M9)

### Added
- **M0** Cargo workspace (11 crates), winit window, wgpu renderer, action-mapped input, fixed 60 Hz timestep,
  GitHub Actions matrix (macOS arm64, Windows x64, Linux x64) with packaged artifacts.
- **M1** PSX pipeline: low-res target with integer upscale and letterbox, vertex snapping, affine UVs, Gouraud vertex
  lighting (ambient + 3 directional + 4 nearest points), 15-bit colour with 4×4 Bayer dither, per-vertex fog, blob
  shadows, fake-FMV and CRT/composite passes; glTF → mesh importer with affine-safe subdivision; texture
  quantizer/palettizer; procedural texture library.
- **M2** Combat: lock-on with part targeting, light/heavy/bash/special/heretic moves, guard, parry, weight-class dodges
  with i-frames, six stamina models, posture and Sever, locational damage, part destruction changing move sets,
  death drops as wrecks/carcasses, climbing huge bosses.
- **M3** Districts with destructible pre-fractured buildings, Hangar Bays and Deep Nests, saves, Cross-Harvest loop,
  "PLEASE INSERT DISC 2" swap.
- **M4** The Foundry and the Nest, weight and balance with CoM lean/drift, move-set composition + validation, 49
  quirks, heretic grafts (Sync / Machine-blood / Heretic Form), 15-block memory card, export/import codes.
- **M5** War map, four district states, Tide Clock, weather, war events, contracts, Breakwater finale.
- **M6** Habit recording, profile extraction, nemesis bosses, THE ROOKIE / THE GRUDGE / THE CHAMPION.
- **M7** Archive (Codex + fabrication, licences/bloodlines, War Record, bestiary as a 1998 strategy guide), Corrupted
  Block NG+, Tide Runs, Omens, Daily Tide, Disc Rot 1–10, Nemesis Gauntlet.
- **M8** Six districts + Breakwater, 4+4 chassis + ST. NULL and THE TIDE-WIDOW, 14 bosses, 182 parts, five endings
  including The Tide Between and Disc 3.
- **M9** Synthesized SFX and adaptive music with SPU mode and PS1 reverb presets, Link Cable Versus, photo mode with a
  magazine cover, accessibility options, headless screenshot and soak tools, packaging.

### Fixed during the balance and soak passes
- Bosses chained attacks without a punish window (cadence now counts idle time).
- Pale Choir verses split exponentially and hit at full strength.
- Hunger kaiju starved in long fights away from buildings.
- Breakwater seawall sealed the sea side off from the arena.
- Fighter list grew unbounded across respawns (slots are reused).
- One-shot presses were dropped on frames that ran zero sim ticks.
- Vertex snapping corrupted triangles crossing the near plane.
