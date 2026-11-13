# Asset manifest

Every file under `assets/` must appear in this table (`cargo xtask check-licenses` fails CI otherwise).

**Allowed licenses:** CC0 / Public Domain, OFL (fonts), or royalty-free with explicit permission for commercial use *and*
redistribution inside a compiled game. **Never:** NC, ND, "personal use only", or no clear license text. Re-verify each
license page at download time. Sound bundles that forbid AI training (e.g. Sonniss) may be used as game audio only.

**Uniqueness pass (required before shipping any third-party asset):** palette-quantize to the district palette, recolour,
kitbash, re-proportion, add custom decals or grime. Hero assets (chassis, bosses, landmarks) must never come from
instantly recognizable packs (Kenney, Mixamo, Synty-style).

| Path | Source | License | Author | Downloaded | Modifications |
|------|--------|---------|--------|------------|---------------|

*No third-party assets are currently shipped.* Everything the game draws or plays is generated in code:

| Generated content | Where |
|-------------------|-------|
| 32 PSX textures (rust, concrete, wet asphalt, kaiju hide, scales, wing veins...) quantized to 15-bit, 16 colours | `crates/render/src/textures.rs` |
| All chassis, boss, enemy and city geometry (primitive kitbash from `data/parts/*.ron`) | `crates/render/src/meshgen.rs`, `crates/app/src/district_mesh.rs` |
| Kaiju animation (spines, wings, swarms, burrowers) | `crates/app/src/visuals.rs` |
| VFX sprites and particles | `crates/render/src/textures.rs`, `crates/app/src/vfx.rs` |
| 5x7 bitmap font (hand-drawn) | `crates/ui/src/glyphs.rs` |
| 33 sound effects | `crates/audio/src/synth.rs` |
| Adaptive tracker-style music (5 layers per district) | `crates/audio/src/music.rs` |
