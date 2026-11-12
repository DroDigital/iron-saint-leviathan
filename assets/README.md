# assets/

Third-party assets live here, one subfolder per kind (`textures/`, `meshes/`, `sfx/`, `music/`, `fonts/`).
**Every file must have a row in [`/ASSETS.md`](../ASSETS.md)** with source URL, author, license, download date and the
modifications made (the uniqueness pass). `cargo xtask check-licenses` enforces this and runs in CI.

The shipped game currently uses **zero** third-party files: textures, meshes, VFX sprites, the bitmap font, every sound
effect and all music are generated procedurally in code (see `crates/render/src/textures.rs`,
`crates/render/src/meshgen.rs`, `crates/ui/src/glyphs.rs`, `crates/audio/src/synth.rs`, `crates/audio/src/music.rs`).
