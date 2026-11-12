//! IRON SAINT // LEVIATHAN - entry point.
//!
//! `iron-saint-leviathan`                     play
//! `iron-saint-leviathan --screenshots DIR`   render the screenshot scenarios headlessly
//! `iron-saint-leviathan --soak SECONDS`      bot soak test (no window)
//! `iron-saint-leviathan --version`

mod app;
mod camera;
mod district_mesh;
mod fmv;
mod foundry;
mod game;
mod headless;
mod hud;
mod menus;
mod mission;
mod settings;
mod vfx;
mod visuals;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn,iron_saint_leviathan=info".into())).init();
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("--version") => {
            println!("IRON SAINT // LEVIATHAN {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("--screenshots") => headless::screenshots(std::path::Path::new(args.get(2).map_or("screenshots", |s| s.as_str()))),
        Some("--soak") => {
            let secs: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(600.0);
            let every: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
            headless::soak(secs, every)
        }
        _ => app::run(),
    }
}
