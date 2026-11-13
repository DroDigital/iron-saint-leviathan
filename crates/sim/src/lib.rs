//! Deterministic gameplay simulation for IRON SAINT // LEVIATHAN.
//!
//! Runs at a fixed 60 Hz with seeded RNG. Contains no rendering or audio: the app reads
//! [`fighter::SimEvent`]s and world state to present it.

pub mod ai;
pub mod archive;
pub mod build;
pub mod city;
pub mod fighter;
pub mod geom;
pub mod parts;
pub mod rng;
pub mod run;
pub mod stamina;
pub mod tide;
pub mod war;
pub mod world;

pub use isl_data as data;

pub const TICK_HZ: u32 = 60;
