//! Renders a small PSX test scene offscreen. Skips (passes) when no GPU adapter exists.

use glam::{Mat4, Vec3};
use isl_data::{Prim, Shape};
use isl_render::*;

#[test]
fn psx_scene_renders() {
    let mut r = match Renderer::new_headless((320, 240)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("skipping: no adapter ({e})");
            return;
        }
    };
    let mut dl = DrawList::default();
    let light =
        Lighting { ambient: Vec3::splat(0.35), dirs: vec![(Vec3::new(-0.4, -1.0, -0.3).normalize(), Vec3::splat(0.9))], points: vec![] };
    let mut ground = meshgen::Mesh::default();
    meshgen::add_box(
        &mut ground,
        Vec3::new(-40.0, -1.0, -40.0),
        Vec3::new(40.0, 0.0, 40.0),
        [140, 140, 140],
        textures::layer("asphalt") as u16,
        8.0,
        [true; 6],
        false,
    );
    push_mesh(&mut dl, &ground, Mat4::IDENTITY, &light, Vec3::ONE, 255);
    for (i, s) in [Shape::Box, Shape::Sphere, Shape::Cylinder, Shape::Cone, Shape::Wedge].iter().enumerate() {
        let p = Prim {
            shape: *s,
            at: [0.0; 3],
            size: [4.0, 6.0, 4.0],
            rot: [0.0, 30.0, 0.0],
            tex: "plate".into(),
            col: [200, 150, 120],
            glow: i == 4,
        };
        let m = meshgen::prim_mesh(&p);
        push_mesh(&mut dl, &m, Mat4::from_translation(Vec3::new(-12.0 + i as f32 * 6.0, 3.0, 0.0)), &light, Vec3::ONE, 255);
    }
    let vp = projection(1.0, 4.0 / 3.0, 200.0) * look_at(Vec3::new(0.0, 14.0, 30.0), Vec3::new(0.0, 3.0, 0.0), Vec3::Y);
    let view = View {
        view_proj: vp,
        cam: Vec3::new(0.0, 14.0, 30.0),
        fog_color: [0.3, 0.35, 0.4],
        fog_near: 20.0,
        fog_far: 90.0,
        flash: 0.0,
        viewport: [0, 0, 320, 240],
        draw_static: false,
        clear: true,
    };
    let mut ui = isl_ui::Ui::new(320.0, 240.0);
    ui.begin(320.0, 240.0, 0.0);
    ui.panel(8.0, 8.0, 120.0, 20.0);
    ui.text(14.0, 14.0, "IRON SAINT", isl_ui::col::GOLD);
    let img = r.render_to_image(&[(view, &dl)], &ui.verts, &PostSettings::default());
    assert_eq!(img.len(), 320 * 240 * 4);
    let distinct: std::collections::HashSet<[u8; 3]> = img.chunks(4).map(|p| [p[0], p[1], p[2]]).collect();
    assert!(distinct.len() > 20, "scene has detail ({} colours)", distinct.len());
    // 15-bit output: every world pixel is a 5-bit level (rows below the translucent UI panel).
    for p in img[320 * 4 * 40..].chunks(4) {
        for c in &p[..3] {
            assert_eq!(isl_assets::quantize::to15(*c), *c);
        }
    }
    if let Ok(dir) = std::env::var("ISL_SCREENSHOT_DIR") {
        image::save_buffer(format!("{dir}/render_smoke.png"), &img, 320, 240, image::ColorType::Rgba8).unwrap();
    }
    // Additive particles and the FMV pass also run.
    billboard(&mut dl.add, Vec3::new(0.0, 10.0, 0.0), 4.0, Vec3::X, Vec3::Y, textures::layer("particle"), [255, 160, 60, 200]);
    let post = PostSettings { fmv: 1.0, ..Default::default() };
    let img2 = r.render_to_image(&[(view, &dl)], &ui.verts, &post);
    assert_ne!(img, img2);
}
