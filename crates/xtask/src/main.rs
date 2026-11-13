//! `cargo xtask <command>`
//!
//! * `pak [out]`              validate data/ and pack it into game.pak
//! * `check-licenses`         every file under assets/ must be listed in ASSETS.md (CI gate)
//! * `validate`               content validation + PSX poly budgets per chassis/boss
//! * `import-mesh in.gltf out.mesh [max_edge]`   glTF -> compact mesh (subdivided)
//! * `import-texture in.png out.png [16|256]`    downscale <=256, 15-bit, palettize
//! * `package <bin> <outdir> <platform>`         assemble a distributable folder / .app bundle

use anyhow::{Context, Result, bail};
use isl_data::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("workspace root")
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        Some("pak") => pak(args.get(1).map(PathBuf::from).unwrap_or_else(|| root().join("target/game.pak"))),
        Some("check-licenses") => check_licenses(),
        Some("validate") => validate(),
        Some("import-mesh") => import_mesh(&args[1], &args[2], args.get(3).and_then(|s| s.parse().ok()).unwrap_or(6.0)),
        Some("import-texture") => import_texture(&args[1], &args[2], args.get(3).and_then(|s| s.parse().ok()).unwrap_or(16)),
        Some("package") => package(Path::new(&args[1]), Path::new(&args[2]), &args[3]),
        _ => {
            eprintln!("usage: cargo xtask <pak|check-licenses|validate|import-mesh|import-texture|package>");
            Ok(())
        }
    }
}

fn pak(out: PathBuf) -> Result<()> {
    let data = root().join("data");
    let files = Content::read_dir(&data)?;
    Content::from_files(&files).context("content must validate before packing")?;
    let bytes: BTreeMap<String, Vec<u8>> = files.into_iter().map(|(k, v)| (k, v.into_bytes())).collect();
    let n = bytes.len();
    let pak = isl_assets::pak::write(&bytes);
    if let Some(p) = out.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(&out, &pak)?;
    println!("packed {n} files ({} KiB) into {}", pak.len() / 1024, out.display());
    Ok(())
}

/// Parse ASSETS.md: every table row's first column is a path relative to assets/.
fn manifest_entries(md: &str) -> Vec<(String, String)> {
    md.lines()
        .filter(|l| l.starts_with('|') && !l.contains("---"))
        .filter_map(|l| {
            let cols: Vec<&str> = l.trim_matches('|').split('|').map(|c| c.trim()).collect();
            if cols.len() >= 3 && cols[0] != "Path" { Some((cols[0].trim_matches('`').to_string(), cols[2].to_string())) } else { None }
        })
        .collect()
}

fn check_licenses() -> Result<()> {
    let r = root();
    let md = std::fs::read_to_string(r.join("ASSETS.md")).context("ASSETS.md missing")?;
    let entries = manifest_entries(&md);
    let allowed = ["CC0", "Public Domain", "Original", "OFL", "Royalty-free (commercial, redistribution)"];
    let mut errors = vec![];
    for (path, lic) in &entries {
        if !allowed.iter().any(|a| lic.starts_with(a)) {
            errors.push(format!("{path}: license `{lic}` is not on the allow-list"));
        }
        if lic.contains("NC") || lic.contains("ND") || lic.to_lowercase().contains("personal") {
            errors.push(format!("{path}: forbidden license terms `{lic}`"));
        }
    }
    let mut stack = vec![r.join("assets")];
    let mut count = 0;
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let rel = p.strip_prefix(r.join("assets")).unwrap().to_string_lossy().replace('\\', "/");
            if rel.starts_with("licenses/") || rel == "README.md" {
                continue;
            }
            count += 1;
            if !entries.iter().any(|(e, _)| e == &rel) {
                errors.push(format!("assets/{rel} is not listed in ASSETS.md"));
            }
        }
    }
    if !errors.is_empty() {
        for e in &errors {
            eprintln!("LICENSE CHECK: {e}");
        }
        bail!("{} license manifest errors", errors.len());
    }
    println!("license manifest OK: {count} asset files, {} manifest entries", entries.len());
    Ok(())
}

fn part_tris(c: &Content, id: &str) -> usize {
    c.part(id).map_or(0, |p| isl_render::meshgen::part_mesh(&p.visual.prims).tris())
}

/// Triangles a full loadout draws, accounting for rigs that repeat parts (serpent segments, swarm bodies).
fn loadout_tris(c: &Content, parts: &[String; 7]) -> usize {
    let rig = c.part(&parts[0]).and_then(|p| p.rig).unwrap_or(RigKind::Humanoid);
    let t: Vec<usize> = parts.iter().map(|p| part_tris(c, p)).collect();
    match rig {
        RigKind::Serpent => t[0] * 9 + t[1] + t[2] + t[3] + t[4] + t[5] + t[6] * 2,
        RigKind::Swarm => (t[0] + t[5]) * 8 + t[1] + t[2] + t[4] + t[6] + t[3] * 3,
        _ => t.iter().sum(),
    }
}

fn validate() -> Result<()> {
    let c = Content::load_dir(&root().join("data"))?;
    println!(
        "parts {} (saint {}, kaiju {})",
        c.parts.len(),
        c.parts.iter().filter(|p| p.side == Side::Saint).count(),
        c.parts.iter().filter(|p| p.side == Side::Kaiju).count()
    );
    println!(
        "moves {}  quirks {}  chassis {}  enemies {}  bosses {}  districts {}",
        c.moves.len(),
        c.quirks.len(),
        c.chassis.len(),
        c.enemies.len(),
        c.bosses.len(),
        c.districts.len()
    );
    let mut bad = vec![];
    for ch in &c.chassis {
        let t = loadout_tris(&c, &ch.parts);
        println!("  chassis {:<14} {:>5} tris", ch.id, t);
        if !(300..=1500).contains(&t) {
            bad.push(format!("chassis {} has {t} tris (style budget 800-1500)", ch.id));
        }
    }
    for b in &c.bosses {
        let t = loadout_tris(&c, &b.parts);
        println!("  boss    {:<22} {:>5} tris", b.id, t);
        if t > 3000 {
            bad.push(format!("boss {} has {t} tris (budget 3000)", b.id));
        }
    }
    if !bad.is_empty() {
        for b in &bad {
            eprintln!("BUDGET: {b}");
        }
        bail!("poly budget violations");
    }
    println!("content OK");
    Ok(())
}

fn import_mesh(input: &str, out: &str, max_edge: f32) -> Result<()> {
    let (doc, buffers, _) = gltf::import(input)?;
    let mut m = isl_assets::mesh::MeshData::default();
    for mesh in doc.meshes() {
        for prim in mesh.primitives() {
            let r = prim.reader(|b| Some(&buffers[b.index()]));
            let base = m.positions.len() as u32;
            let pos: Vec<[f32; 3]> = r.read_positions().context("positions")?.collect();
            let n = pos.len();
            m.positions.extend(pos);
            m.normals.extend(r.read_normals().map(|x| x.collect::<Vec<_>>()).unwrap_or(vec![[0.0, 1.0, 0.0]; n]));
            m.uvs.extend(r.read_tex_coords(0).map(|x| x.into_f32().collect::<Vec<_>>()).unwrap_or(vec![[0.0, 0.0]; n]));
            m.colors.extend(r.read_colors(0).map(|x| x.into_rgba_u8().collect::<Vec<_>>()).unwrap_or(vec![[255; 4]; n]));
            match r.read_indices() {
                Some(ix) => m.indices.extend(ix.into_u32().map(|i| i + base)),
                None => m.indices.extend(base..base + n as u32),
            }
        }
    }
    let before = m.tri_count();
    m.subdivide(max_edge);
    std::fs::write(out, m.encode())?;
    println!("{input}: {before} -> {} tris (affine-safe subdivision at {max_edge} m)", m.tri_count());
    Ok(())
}

fn import_texture(input: &str, out: &str, colors: usize) -> Result<()> {
    let img = image::open(input)?.to_rgba8();
    let (w, h) = img.dimensions();
    let (w, h, px) = isl_assets::quantize::downscale(w, h, img.as_raw(), 256);
    let p = isl_assets::quantize::palettize(w, h, &px, colors.clamp(2, 256));
    image::save_buffer(out, &p.to_rgba(), w, h, image::ColorType::Rgba8)?;
    println!("{input}: {w}x{h}, {} colours (15-bit)", p.palette.len());
    Ok(())
}

fn copy_common(dist: &Path, pak_dst: &Path) -> Result<()> {
    let r = root();
    pak(pak_dst.to_path_buf())?;
    for f in ["README.md", "ASSETS.md", "LICENSE-MIT", "LICENSE-APACHE", "CHANGELOG.md"] {
        if r.join(f).exists() {
            std::fs::copy(r.join(f), dist.join(f))?;
        }
    }
    let lic = dist.join("licenses");
    std::fs::create_dir_all(&lic)?;
    if let Ok(rd) = std::fs::read_dir(r.join("assets/licenses")) {
        for e in rd.flatten() {
            std::fs::copy(e.path(), lic.join(e.file_name()))?;
        }
    }
    Ok(())
}

fn package(bin: &Path, out: &Path, platform: &str) -> Result<()> {
    std::fs::create_dir_all(out)?;
    let name = "IronSaintLeviathan";
    match platform {
        "macos" => {
            let app = out.join(format!("{name}.app"));
            let macos = app.join("Contents/MacOS");
            let res = app.join("Contents/Resources");
            std::fs::create_dir_all(&macos)?;
            std::fs::create_dir_all(&res)?;
            std::fs::copy(bin, macos.join(name))?;
            copy_common(&res, &res.join("game.pak"))?;
            let plist = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>IRON SAINT // LEVIATHAN</string>
<key>CFBundleDisplayName</key><string>IRON SAINT // LEVIATHAN</string>
<key>CFBundleIdentifier</key><string>com.harrowmouth.ironsaint</string>
<key>CFBundleVersion</key><string>{v}</string>
<key>CFBundleShortVersionString</key><string>{v}</string>
<key>CFBundleExecutable</key><string>{name}</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>LSMinimumSystemVersion</key><string>11.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSApplicationCategoryType</key><string>public.app-category.action-games</string>
<key>GCSupportsControllerUserInteraction</key><true/>
</dict></plist>
"#,
                v = env!("CARGO_PKG_VERSION")
            );
            std::fs::write(app.join("Contents/Info.plist"), plist)?;
            println!("macOS bundle at {} (ad-hoc sign with: codesign --force --deep -s - {})", app.display(), app.display());
        }
        _ => {
            let exe = if platform == "windows" { format!("{name}.exe") } else { name.to_string() };
            std::fs::copy(bin, out.join(&exe))?;
            copy_common(out, &out.join("game.pak"))?;
            println!("{platform} package at {}", out.display());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn manifest_parser() {
        let md = "| Path | Source | License | Author | Date | Changes |\n|---|---|---|---|---|---|\n| `textures/a.png` | x | CC0 | y | z | w |\n";
        let e = super::manifest_entries(md);
        assert_eq!(e, vec![("textures/a.png".to_string(), "CC0".to_string())]);
    }
}
