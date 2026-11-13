//! Asset formats shared by the runtime and the offline `xtask` pipeline.
//!
//! * [`pak`]: a tiny archive (`ISLPAK1`, entry table, CRC per entry) holding the RON content
//!   database and any imported meshes/textures. Release builds read `game.pak`; dev builds read
//!   loose files from `data/` and hot-reload them.
//! * [`quantize`]: PSX texture rules - at most 256x256, 15-bit colour, palettized to 16 or 256
//!   colours. Used at import time and for the runtime procedural textures.
//! * [`mesh`]: compact indexed mesh format produced from glTF by `xtask import`.

pub mod mesh;
pub mod pak;
pub mod quantize;
