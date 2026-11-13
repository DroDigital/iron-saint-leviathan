use glam::Vec2;
use isl_data::*;
use isl_ghost::Recorder;
use isl_sim::build::Loadout;
use isl_sim::fighter::Command;
use isl_sim::tide::TideClock;
use isl_sim::world::*;
use std::path::Path;

#[test]
fn records_dodge_bias_from_play() {
    let c = Content::load_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data")).unwrap();
    let lo = Loadout::premade(c.chassis_def("st_wren").unwrap(), &c, 1);
    let cfg = WorldConfig {
        district: "harbor".into(),
        state: DistrictState::Contested,
        side: Side::Saint,
        tide: TideClock::default(),
        seed: 3,
        mode: Mode::Arena,
        player_fx: vec![],
        enemy_fx: vec![],
        assists: Assists::default(),
        persistent: vec![],
        wrecks: vec![],
        spawns: Some(vec![SpawnSpec::Enemy("dummy_kaiju".into())]),
        density: 1.0,
        war: 1,
        bell: false,
        static_night: vec![],
        demo_disc: None,
    };
    let mut w = World::new(&c, cfg, vec![lo]);
    let mut rec = Recorder::new(0);
    for t in 0..2400u64 {
        // Dodge left three times for every right dodge, pausing so the Battery can recharge.
        let k = t / 80;
        let cmd = Command {
            lock: t == 2,
            dodge: t % 80 == 10,
            move_dir: if k % 4 == 3 { Vec2::new(1.0, 0.0) } else { Vec2::new(-1.0, 0.0) },
            light: t % 160 == 45,
            ..Default::default()
        };
        // Face +Z so that -X is the fighter's left.
        w.fighters[0].yaw = 0.0;
        w.step(&c, &[cmd]);
        let ev: Vec<_> = w.events.drain(..).collect();
        rec.observe(&w, &c, &ev);
    }
    let p = &rec.profile;
    assert!(p.dodge_total() >= 20);
    assert!((p.left_bias() - 0.75).abs() < 0.1, "left bias {}", p.left_bias());
    assert!(p.favourite().is_some());
}
