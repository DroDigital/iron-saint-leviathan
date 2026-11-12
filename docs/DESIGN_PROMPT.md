# IRON SAINT // LEVIATHAN — Full Build Prompt

Nov 12, 2026 · @Alessandro

## How to use this prompt

Paste everything from **Role and mission** downward into a coding agent (for example Claude Code) opened in an empty repository, or save it in the repo as `DESIGN_PROMPT.md` and tell the agent: *"Read DESIGN\_PROMPT.md and execute it milestone by milestone, stopping for my review at the end of each milestone."* Everything below is written to the agent.

## 1. Role and mission

You are a senior game programmer and technical designer. Build **IRON SAINT // LEVIATHAN**, a PlayStation 1–styled action soulslike, from scratch **without a game engine**. Work in milestones (section 13), keep the code clean and data-driven, and stop for review at the end of every milestone.

**Design pillars — every decision must serve at least one:**

1. **Weight you can feel.** Heavy, readable, punishing, fair. Every hit has windup, impact and recovery.
2. **Two sides, one war.** You cannot fully master either side without playing the other.
3. **Your history is the enemy.** Your own builds, deaths and habits come back as bosses and loot.
4. **1997 on the surface, 2026 underneath.** Authentic PSX look and feel, modern stability, controls and accessibility.

## 2. Hard technical constraints

**No engine.** Do not use Unity, Unreal, Godot, Bevy, Fyrox, GameMaker, Defold or any other engine or engine-like framework. Small focused libraries for windowing, graphics API abstraction, audio, input, math and file formats are allowed.

**Default stack** (deviate only with a written justification in `docs/ARCHITECTURE.md`):

- **Rust** (stable), Cargo workspace.
- **winit** for windows and events; **wgpu** for graphics (Metal on macOS, DX12/Vulkan on Windows, Vulkan on Linux) with **WGSL** shaders.
- **gilrs** for gamepads, **kira** for audio, **glam** for math, **gltf** for model and animation import, **image** for textures, **serde + ron** for data files, **rand\_pcg** for seeded RNG.
- Collision: write simple custom capsule / sphere / AABB / swept-volume collision. PSX-scale needs don't justify a full physics engine.
- Check crates.io for the latest stable version of every dependency. Never pin versions from memory.

**Platforms:** macOS (native **arm64**, Apple Silicon; an x86\_64 + arm64 universal binary is optional), Windows 10/11 x64, Linux x64 (X11 and Wayland). One codebase. Platform differences live only in thin `cfg` shims.

**Apple M1 baseline:** an 8 GB M1 MacBook Air must hold a locked 60 fps (see section 12). Native arm64 only, never Rosetta.

**Also required:**

- GitHub Actions CI matrix that builds and tests all three platforms and uploads artifacts: a macOS `.app` bundle (ad-hoc signed; document notarization), a Windows `.zip`, and a Linux AppImage or `.tar.gz`.
- Controller-first, with full keyboard and mouse support and complete rebinding.
- Saves and settings in each OS's correct user-data directory (use the `directories` crate).
- Runs offline, no accounts, no telemetry.

## 3. PSX visual and audio direction

### Rendering

- Render to a **low-resolution target** (default 320×240; options 256×224, 368×240, 512×448, 640×480) and upscale to the window with nearest-neighbour sampling, integer scaling where possible, and letterboxing. Offer a 4:3 / 16:9 widescreen choice.
- **Vertex snapping (jitter):** snap clip-space vertex positions to the internal pixel grid, with a strength slider.
- **Affine texture mapping** (no perspective correction). Subdivide large polygons on import so the warp stays charming and never hides a boss telegraph.
- **Textures:** nearest filtering, no mipmaps, at most 256×256. Quantize at import time to 15-bit colour, palettized to 16 or 256 colours per texture.
- **15-bit colour output** with ordered 4×4 Bayer dithering. Dithering can be toggled.
- **Lighting:** Gouraud vertex lighting only (ambient, up to 3 directional lights and 4 nearest point lights per object). Blob shadows only.
- **Distance fog** that doubles as draw distance, with fog colour and density set per district and per weather.
- **Optional post effects, off by default:** CRT (scanlines, shadow mask, slight bloom) and composite (chroma bleed).
- **Poly budgets** (style rules, not hardware limits): player chassis 800–1,500 triangles, bosses up to 3,000, whole scene around 20k.
- **UI:** bitmap fonts, a chunky HUD, a memory-card-style save screen, and short loading screens that show lore (actual loads must stay fast).
- **"Fake FMV" cutscenes** rendered in-engine through a pass that mimics PS1 video compression (posterize, 8×8 macroblocking, slight chroma smear). This avoids video codec dependencies.

### Audio

- **SPU mode** (on by default, can be toggled): resample to around 22 kHz, gentle low-pass, and reverb presets modelled on PS1 SPU modes (Room, Studio, Hall, Space Echo) chosen per district.
- **Adaptive music in layers:** explore, combat, and boss phases 1/2/3, crossfaded on beat. Tracker-style compositions (`.it` / `.xm` rendered to OGG) suit the era.
- Positional 3D audio with distance attenuation and panning. Kaiju footsteps also shake the camera and controller (rumble).

## 4. Game premise

Two playable sides fight over one coastal city, **Harrowmouth**. **Saints** are giant mechs that defend it. **Kaiju** rise from the sea to drown it. The player alternates between the two sides across a persistent war.

**Disc metaphor:** "Disc 1" is the Saint side and "Disc 2" is the Kaiju side. Switching sides plays a stylised *"Please insert Disc 2"* swap screen. One virtual memory card holds the shared war.

**Starting rosters (natural rivals share a row):**

| Saint | Playstyle | Kaiju rival | Playstyle |
| --- | --- | --- | --- |
| IRON SAINT | Sword and shield, balanced | LEVIATHAN | Coiling sea serpent, grabs and constricts |
| ST. CINDER | Heavy flamethrowers, Heat stamina | GOLIATH MOTH | Flying spore clouds, drawn to fire and city lights |
| ST. WREN | Fast grappling-hook skirmisher, parry-focused | THE PALE CHOIR | Swarm; HP is body count; splits and recombines |
| ST. ANCHOR | Harbor mech tethered to the city power grid | MAWGRAVE | Burrower that ambushes from beneath the streets |

There is one secret unlockable per side (section 7).

**Lore spine:** Saints were always grown from kaiju tissue. The pilot and the monster were never separate. The sync screen's glitches foreshadow this, and the true ending pays it off.

## 5. Combat and soulslike core

- **Third-person lock-on combat** driven by stamina: light, heavy, guard, parry, dodge, and one special per build. Lock-on can switch between a boss's individual parts.
- **Stamina depends on the build's reactor (Saint) or heart (Kaiju):**
  - **Saint reactors:** *Heat* (overheating forces a vent stagger), *Battery* (recharges only after a pause), *Fuel* (finite each sortie; refills at a hangar).
  - **Kaiju hearts:** *Hunger* (eat buildings or units to refill), *Blood* (damage dealt refills it), *Breath* (rhythmic; attacks timed to the breath cycle cost less).
- **Posture:** both the player and enemies have a posture meter. Breaking an enemy's posture opens a **Sever** window, a cinematic tear-off of the targeted part.
- **Locational damage:** every part has its own HP. Destroyed parts change the move set (no left arm means no shield bash) and **drop as loot** (section 6).
- **Weight and balance:** total load plus centre of mass set turn speed, stagger resistance and dodge type (light builds dash, medium builds hop, heavy builds shoulder-slide). Lopsided builds lean and drift.
- **Scale combat:** huge bosses get a dedicated camera mode and climbable weak points (grab, climb, rip), in the spirit of Shadow of the Colossus.
- **Checkpoints:** *Hangar Bays* for Saints, *Deep Nests* for Kaiju. Resting restores healing, respawns enemies and advances the Tide Clock by one hour (section 8).
- **Healing:** Saints use *Repair Charges*, a limited budget refilled at hangars. Kaiju use *Regrowth*, which spends banked biomass and is slower but also heals severed parts.
- **Death:** you drop your currency (*Salvage* for Saints, *Biomass* for Kaiju) as a physical **wreck or carcass** where you fell. Recover it in one trip or lose it. Either way the wreck persists in the world and the other side can harvest it (section 6).
- **Destructible city:** buildings break into pre-fractured chunks. Damage persists per district and changes routes, cover and kaiju food supply.
- **Enemies:** Saint missions face parasites, spawn swarms and juvenile kaiju. Kaiju missions face tanks, drones, artillery and AA batteries. Boss telegraphs: windups of at least 300 ms, each with a distinct sound cue and silhouette change. Delayed feints are allowed, but every one must be learnable.
- **Rules:** no input-reading AI cheats, i-frames visible in the training arena, no unavoidable damage.

## 6. Cross-Harvest: assembling your own Saint and Kaiju

**The core rule: Saint parts drop only while playing as a Kaiju, and Kaiju parts drop only while playing as a Saint.** Premade chassis are always available. Custom builds can only be assembled from parts harvested on the other side.

### Slots

| Saint slot | Kaiju slot | What it controls |
| --- | --- | --- |
| Frame | Body | Base HP, weight capacity, size class |
| Left arm / Right arm | Left claw / Right claw | Primary attacks and guard |
| Legs | Locomotion (legs, tail, wings, fins) | Movement type, dodge type, terrain access |
| Reactor | Heart | Stamina type (section 5) |
| Head | Senses | Lock-on range, weak-point reveal, fog vision |
| Backpack | Mutation | Special ability |

### Part data (one RON file per part)

Each part has `id`, `slot`, `side`, `weight`, `stats`, `moves` (the attack set it brings), `quirks`, `origin` (who it came from: a premade chassis, a boss, a nemesis, or your own corpse), `condition`, and `memory` (behaviour biases inherited from its previous owner).

### Getting parts

1. **Sever in combat:** a clean sever gives a *Pristine* part. A kill that destroys the part first gives *Scarred* or *Mangled*.
2. **Your own remains:** your Saint wrecks can be stripped while playing Kaiju, and your Kaiju carcasses harvested while playing Saint. A failed run on one side always pays off on the other.
3. **Nemesis bosses** (section 8) drop pieces of your own former builds.
4. **Memory card imports** (section 8) let you trade parts with friends.

### Condition and quirks

- **Pristine:** full stats, no quirk.
- **Scarred:** small stat loss plus one random quirk (positive or mixed).
- **Mangled:** larger loss plus one or two quirks (mixed or negative), which you can repair at a cost.
- **Example quirks:** *Twitching* (attacks sometimes fire early, +15% damage), *Old Grudge* (+damage against the part's former owner), *Hollow* (−weight, −HP), *Leaking Core* (stamina drains during idle, faster recovery), *Remembers the Sea* (Saint part: faster swimming, slower on land), *Dodges Left* (left dodge has extra i-frames).
- Start with at least **30 quirks**, defined in `data/quirks.ron`.

### Heretic grafts

You may mount **same-side** parts too: a Kaiju part on a Saint, or a Saint part on a Kaiju.

- **Saint heresy:** each heretic part lowers **Sync**. Low Sync makes the HUD glitch, lets controls drift, and spawns hallucinated enemies.
- **Kaiju heresy:** each heretic part raises **Machine-blood**. The city's sensors read you as friendly, which opens stealth routes, but machine-blooded parts cannot regrow.
- With more than 50% heretic parts the build unlocks a **Heretic Form** super move but suffers permanent instability. These are the strongest and riskiest builds in the game.

### Assembly screens

- **The Foundry** (Saint hangar) and **The Nest** (Kaiju trench) show a turntable preview with PSX lighting, a stat comparison, and a centre-of-mass gizmo that shows lean.
- **Move sets are composed from parts:** arm moves + leg movement + reactor stamina + backpack special. Validation guarantees every build has a light attack, a heavy attack, a dodge, a heal, and at least one way to deal posture damage.
- **Test Arena:** a training dummy, plus a dummy that replays your recorded habits.
- **Customization:** paint, decals, stencilled names for Saints; scar patterns, bioluminescent markings and names for Kaiju.
- **Memory card blocks:** like the PS1 card, you get **15 blocks**. A saved build costs 1–3 blocks depending on complexity, and campaign saves cost blocks too. Choosing what to keep is part of the game.

## 7. Meta progression

Meta progression should add **options, breadth and knowledge**, not raw power. Permanent stat bonuses are capped at +10% total, so skill always matters most.

### Three layers

1. **Campaign (one war):** Salvage and Biomass levels, harvested parts, and district states.
2. **The Archive (permanent, across all wars and modes):**
   - **Part Codex:** severing a part once registers it. Registered parts can be **fabricated** later as a Scarred copy for a currency cost, so bad RNG never blocks a build forever.
   - **Pilot Licenses (Saints) and Bloodlines (Kaiju):** a mastery track per chassis (ranks 1–10) that unlocks alternate starting loadouts, palettes, a signature trait and a lore entry.
   - **War Record:** wars fought, endings reached and nemeses defeated. Milestones unlock new starting **Conditions** (for example: start with the harbor already flooded, or one Saint per side only) and new districts.
   - **Bestiary and Pilot Files:** lore that fills in as you fight, framed as a scanned 1998 strategy guide.
3. **The Corrupted Block (NG+ carry-over):** finishing a war writes one corrupted memory card block. It carries **one part or one trait** of your choice into the next war, and that item is permanently corrupted with an added quirk. This continues the lore loop.

### Unlocks that require playing the other side

- **ST. NULL** (secret Saint, a hollow frame that copies the last part it struck): sever the heads of all four starting Saints while playing Kaiju.
- **THE TIDE-WIDOW** (secret Kaiju, which leaves floods that heal it): harvest all four starting Kaiju hearts while playing Saint.
- Each chassis's License or Bloodline rank 5 requires 3 wins **against** that chassis from the other side.
- Advanced Foundry and Nest features (re-rolling quirks, transplanting a move between parts) unlock through total cross-side harvests.

## 8. Replayability systems

### Game modes

- **War Campaign** (15–25 h): the persistent tug-of-war over Harrowmouth.
- **Tide Runs** (30–60 min roguelite): a seeded run across a few procedurally remixed district layouts with a random chassis and draft picks of parts after each fight. Rewards go to the Archive.
- **Nemesis Gauntlet:** a boss rush against your own saved builds and ghosts.
- **Link Cable Versus:** local split-screen, one Saint against one Kaiju, using custom builds.

### The war map

- Six districts, each in one of four states: **Fortified, Contested, Flooded, Ruined**. Each state has its own layout variant, enemy set, lighting, music layer and loot table.
- A Saint win pushes a district toward Fortified and a Kaiju win pushes it toward Flooded. Finales require particular map states (section 9).

### Tide Clock and weather

- In-game hours advance when you rest. **High tide and night** favour Kaiju (more water access, darker fog). **Dawn and full power** favour Saints.
- Weather rolls each Tide: clear, fog bank, storm (lightning briefly reveals hidden kaiju), blackout (the city grid is down, so ST. ANCHOR is weakened and GOLIATH MOTH loses its lure).

### Nemesis system ("your history is the enemy")

- The `ghost` module records **habit statistics**, not full replays: dodge direction bias, favourite attacks and combos, heal timing (HP% when healing), aggression (distance kept), parry attempt rate and reaction to feints.
- Every saved custom build can appear as a boss on the other side, **driven by your habit profile**. Its intro card reads like: *"Pilot: YOU (War 3, Tide 14). Habits: dodges left 71%; heals below 30%."*
- Beating a nemesis drops its parts back to you with added scars. **Your best build is always your hardest boss.**
- Named rival nemeses persist and evolve: *THE ROOKIE* (your first loadout), *THE GRUDGE* (the build from your rage-quit run, detected by a quit soon after a death), *THE CHAMPION* (your most-used build at its best).

### Omens (run modifiers)

Omens are stackable modifiers used in Tide Runs and optionally in NG+, each with a reward multiplier. Examples: *Glass Hull* (you die in one hit), *Famine* (double Hunger drain), *Black Signal* (no HUD), *Riptide* (every district partly flooded), *Pilgrim* (no lock-on), *Echo* (enemies copy your last used move).

### Other replay hooks

- **Daily Tide:** a seed generated offline from the date. No server, comparable with friends by seed.
- **Disc Rot** difficulty tiers 1–10 for NG+, each adding one escalating rule.
- **Random war events:** a Demo Disc Kaiju invasion (a bonus monster with a cursed part), evacuation escorts, a bell that summons the Cathedral's sleeping Saint, and a Night of Static (all ghosts walk at once).
- **District contracts:** optional bounties (sever three left arms, win without healing) that pay in rare quirks.
- **Memory card export:** builds and parts export as short base32 codes or small files that friends can import. Some quirks drop only from imported parts.
- **Photo mode** with PSX filters and a fake magazine-cover overlay.

## 9. Content scope

### Vertical slice (Milestone 3)

One district (the Harbor), IRON SAINT and LEVIATHAN playable, three enemy types per side, one boss per side, and the full Cross-Harvest loop (sever → harvest → swap discs → use the part).

### Version 1.0

- **Districts:** the Harbor, Cathedral Row, Foundry Quarter, Old Rail Yards, Lighthouse Point, the Drowned Market, plus the finale zone, **the Breakwater**.
- **Chassis:** 4 + 4 starting chassis and 1 + 1 secret unlockables.
- **Parts:** about 120 (around 60 per side) and at least 30 quirks.
- **Bosses:** 12 handcrafted (6 per side) plus unlimited nemesis bosses.
- **Endings:**
  - *Drown the City* (Kaiju dominant): the city drowns and every Saint part in your collection rusts.
  - *The City Stands* (Saint dominant): the city stands and every Kaiju part in your collection crumbles.
  - *Become the Saint* (a Kaiju with more than 50% heretic parts reaches the cockpit).
  - **The Tide Between** (true ending): balance the map at exactly 50/50, then fight your best custom Saint against your best custom Kaiju. You pick a side and your ghost plays the other. Winning fuses the two into a final form built from every part you ever harvested.
- **Secret:** a hidden "Disc 3" ending reached through the corrupted memory card block chain.

## 10. Free asset sourcing and license rules

### License rules (strict)

- Use only assets that are **CC0 / public domain**, or **royalty-free with explicit permission for commercial use and redistribution inside a compiled game**.
- **Never** use assets marked NC (non-commercial), ND (no derivatives), "personal use only", or with no clear license text. If an itch.io or OpenGameArt page doesn't state its license plainly, skip it.
- Some royalty-free sound bundles (for example Sonniss) **forbid using their files to train AI models**. Use them only as game audio, never as model training data.
- Keep **`ASSETS.md`** plus `assets/licenses/`: one entry per asset with source URL, author, license, download date and the modifications you made. **CI must fail** if any file under `assets/` is missing from the manifest.
- Re-verify each license page when you download. These sources were checked in November 2026 but terms can change.

### Uniqueness rules (avoid the stock-asset look)

- **Do not** use Kenney, Mixamo, Synty-style or other instantly recognizable packs as visible hero assets (player chassis, bosses, key landmarks). They're fine for internal greyboxing only.
- Every third-party asset must go through a **uniqueness pass** before it ships: palette-quantize to the district palette, recolour, kitbash with other pieces, re-proportion, and add custom decals or grime. No shipped asset should be recognizable as coming from a stock pack.
- **Prefer procedural generation where it's cheap:**
  - **Textures:** noise-based rust, concrete, wet asphalt, kaiju hide and scales, generated and then quantized.
  - **VFX:** particles, sparks, spray, spore clouds, blood-mist and shockwave rings from code with tiny generated sprite atlases. No VFX packs needed.
  - **Kaiju animation:** **procedural** (IK legs, spline spines, tail and tentacle chains with spring physics, breathing and swelling via vertex shaders). Kaiju are not humanoid, so this is both necessary and a big uniqueness win.
  - **SFX:** layer and pitch-shift library sounds into new sounds. Never ship a well-known library sound unaltered.

### Recommended free sources

| Need | Source | License | How to use it |
| --- | --- | --- | --- |
| Textures | ambientCG | CC0, including redistribution | Downscale to 64–256 px and palettize. Photoreal CC0 becomes unique PSX texture art. |
| Textures | Poly Haven, ShareTextures, cgbookcase | CC0 | Same pipeline. |
| Humanoid animation | Quaternius Universal Animation Library 1 and 2 (free tiers) | CC0 | Retarget onto humanoid Saint frames and pilots, then edit timing for weight. |
| Props and buildings | CC0-tagged PSX packs on itch.io; OpenGameArt with the CC0 filter | CC0 (check each page) | Kitbash into city blocks; apply the uniqueness pass. |
| Sound effects | Sonniss GDC Game Audio Bundles | Royalty-free, commercial use, no attribution; no AI training | Mechanical, impact, creature and ambience layers. |
| Sound effects | freesound.org (filter to CC0 only) | CC0 | Small one-off sounds. |
| Music | Original tracker music | Yours | Preferred for identity. Use CC0 music from OpenGameArt only as a stopgap. |
| Fonts | OFL-licensed pixel fonts, or a hand-made bitmap font | OFL / yours | A hand-made font is preferred for the HUD. |

### Free tools

Blender (modelling, rigging, glTF export), Krita or GIMP (textures), LibreSprite (pixel UI), Audacity (SFX editing), Furnace, MilkyTracker or OpenMPT (tracker music).

## 11. Architecture and code structure

Cargo workspace layout:

| Crate | Responsibility |
| --- | --- |
| `app` | Binary: main loop, state machine (boot, title, war map, mission, Foundry or Nest, disc swap) |
| `sim` | **Deterministic** gameplay at a fixed 60 Hz: combat, AI, collision, parts, posture, tide clock. No rendering or audio dependencies. Heavily unit-tested. |
| `render` | wgpu PSX pipeline: low-res target, vertex snap, affine mapping, dithering, fog, fake-FMV pass, CRT pass, upscale |
| `audio` | kira wrapper, SPU-mode filter and reverb presets, adaptive music layers |
| `input` | Action mapping, rebinding, gamepad and keyboard/mouse, rumble |
| `assets` | Runtime loading of packed `.pak` files |
| `xtask` | Offline import pipeline: glTF to a compact mesh format, texture quantizer and palettizer, atlas packer, license manifest check |
| `data` | RON definitions for parts, quirks, enemies, bosses, districts, loot, omens, with schema validation at load |
| `ghost` | Habit recording, profile extraction, nemesis AI parameter mapping |
| `save` | Versioned, checksummed saves; memory-card block accounting; migrations; export and import codes |
| `ui` | Custom immediate-mode bitmap UI |

**Rules:**

- **Fixed-timestep simulation** (60 Hz) with interpolated rendering, so 120 Hz ProMotion displays look smooth.
- **Seeded RNG everywhere** (`rand_pcg`), so Tide Runs, daily seeds and bug reports reproduce exactly.
- **Entity storage:** use a lightweight ECS (for example `hecs`) or plain struct-of-arrays. Keep it simple and avoid framework-sized ECS layers.
- **Data-driven content:** everything in section 6 and section 8 lives in RON and **hot-reloads in dev builds**.
- A **dev-only feature flag** may add `egui` debug overlays (hitboxes, posture values, AI state, habit profile, frame timings). It must never ship in release builds.
- **Built-in dev tools:** a part tester, a boss arena with any boss and any build, a district state switcher, and a tide and weather switcher.

## 12. Performance budget (Apple M1 baseline)

- **Target:** locked 60 fps on an 8 GB M1 MacBook Air in the heaviest district during a boss fight, with the window at native Retina size. Aim for CPU frame time under 6 ms and GPU under 4 ms.
- **Present mode:** Fifo (vsync) by default, with Mailbox as an option. Simulation stays at 60 Hz with interpolated rendering; support 120 Hz displays.
- **Unified memory:** avoid needless staging copies. Use `queue.write_buffer` for small per-frame uniforms and persistent vertex/index buffers for static district geometry.
- **Draw calls:** batch by material and use one texture array or atlas per district. Instance debris, crowds and swarm units (THE PALE CHOIR). Stay under 500 draw calls.
- **Memory:** under 1 GB RAM total. Under 3 s to load a district from SSD.
- **Release profile:** `opt-level=3`, `lto="thin"`, `codegen-units=1`, `panic="abort"`; native `aarch64-apple-darwin` target for Mac.
- **Battery and thermals:** cap menus at 30 fps, stop rendering while minimized or unfocused (configurable), and avoid busy-waiting.
- **Profiling:** `tracing` spans plus an in-game frame graph in dev builds. Document how to capture with Xcode Instruments (Metal System Trace).
- **Low-end test matrix:** M1 8 GB on current macOS; a Windows laptop with an Intel UHD 620–class iGPU; Linux on Mesa (Intel or AMD iGPU). If those three run well, everything above them will too.

## 13. Build order and milestones

Stop for review at the end of each milestone with a short report, screenshots or a GIF, and known issues.

1. **M0 — Skeleton:** workspace, window, wgpu clear screen, input actions, fixed timestep, CI green on all three OSes.
2. **M1 — PSX renderer:** low-res target, vertex snap, affine mapping, dithering, fog, Gouraud lighting, glTF import, texture quantizer, test scene with a spinning Saint.
3. **M2 — Combat core:** IRON SAINT against a dummy. Lock-on, stamina, posture, dodge, part HP, severing, death drop.
4. **M3 — Vertical slice (playtest gate):** the Harbor district, LEVIATHAN playable, three enemies per side, one boss per side, checkpoints, saves, and the full Cross-Harvest loop including the disc swap.
5. **M4 — Assembly:** Foundry and Nest, weight and balance, move set composition and validation, quirks, heretic grafts, 15-block memory card.
6. **M5 — The war:** war map, district states, tide clock, weather, events.
7. **M6 — Nemesis:** habit recording, profile extraction, nemesis bosses, named rivals.
8. **M7 — Meta and modes:** the Archive, Licenses and Bloodlines, Corrupted Block, Tide Runs, Omens, Daily Tide, Disc Rot, Nemesis Gauntlet.
9. **M8 — Content:** expand to the version 1.0 scope, all endings, the secret chassis.
10. **M9 — Polish and ship:** SPU audio mode, fake FMV, CRT pass, Link Cable Versus, photo mode, accessibility pass, M1 performance pass, packaging for all three platforms.

## 14. Working rules and acceptance criteria

### How you work

- Before each milestone, write `docs/plan-Mx.md` (scope, risks, test plan). After it, update `CHANGELOG.md`.
- **Greybox first:** use procedural placeholder art with clear colours until real assets arrive. Never block gameplay on art.
- **Unit tests are required for:** damage and posture math, sever rules, weight and balance, move set validation, quirk effects, save migrations, RNG determinism, and habit-profile extraction.
- **Ask me before you:** change the core stack, add any asset whose license is not CC0 or clearly commercial royalty-free, add a dependency that pulls in a large framework, or cut a pillar feature.

### Accessibility (required)

- Full rebinding, hold-or-toggle options, and adjustable subtitle size.
- Telegraphs communicated by shape and sound, never by colour alone. Colourblind palettes.
- Sliders for screen shake, vertex jitter and dither intensity. A photosensitivity mode that disables flashes and CRT flicker.
- Optional assists that never lock content or achievements: a wider parry window (*Steady Hands*), slower bosses (*Old Bones*), and a forgiving death drop (*Insurance*).

### Acceptance criteria for version 1.0

- [ ] CI produces runnable builds for macOS (arm64), Windows x64 and Linux x64.
- [ ] Locked 60 fps on an 8 GB M1 in the heaviest district during a boss fight.
- [ ] A complete playthrough from a new war to every ending works, including the true ending.
- [ ] Every part type can be obtained only through the opposite side (or fabrication from the Codex), as designed.
- [ ] Nemesis bosses visibly reflect the player's recorded habits.
- [ ] Every shipped asset is logged in `ASSETS.md` with a valid license, and the CI license check passes.
- [ ] Saves from earlier versions migrate without data loss.
- [ ] No crash in a 2-hour soak test on each platform.
