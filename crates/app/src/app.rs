//! The windowed application: winit event loop, fixed-timestep driver (inside `Game`/`Mission`),
//! frame pacing (menus capped at 30 fps, nothing rendered while minimized/unfocused), mouse capture.

use crate::game::{Game, Request, Screen};
use isl_audio::{Audio, AudioSettings};
use isl_input::Input;
use isl_render::{DrawList, PostSettings, Renderer, View};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

pub struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    game: Game,
    input: Input,
    audio: Audio,
    last: Instant,
    focused: bool,
    minimized: bool,
    grabbed: bool,
    next_frame: Instant,
}

impl App {
    fn new() -> App {
        let game = Game::new(false);
        let s = &game.settings;
        let audio = Audio::new(AudioSettings { master: s.master, sfx: s.sfx, music: s.music, spu: s.spu });
        let mut input = Input::new(s.bindings.clone());
        input.rumble_enabled = s.rumble;
        App {
            window: None,
            renderer: None,
            game,
            input,
            audio,
            last: Instant::now(),
            focused: true,
            minimized: false,
            grabbed: false,
            next_frame: Instant::now(),
        }
    }

    fn in_play(&self) -> bool {
        matches!(self.game.screen, Screen::Mission) && self.game.pause.is_none() && self.game.mission.as_ref().is_some_and(|m| !m.photo)
    }

    fn apply_video(&mut self) {
        let (w, h) = self.game.settings.low_res();
        if let Some(r) = self.renderer.as_mut() {
            r.set_low_res(w, h);
            r.set_vsync(self.game.settings.vsync);
        }
        if let Some(win) = &self.window {
            win.set_fullscreen(if self.game.settings.fullscreen { Some(Fullscreen::Borderless(None)) } else { None });
        }
    }

    fn frame(&mut self, el: &ActiveEventLoop) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.1);
        self.last = now;
        let versus = self.game.mission.as_ref().is_some_and(|m| m.kind == crate::mission::MissionKind::Versus);
        self.input.update(versus);
        self.game.update(dt, &mut self.input, &mut self.audio);
        for r in std::mem::take(&mut self.game.requests) {
            match r {
                Request::Quit => {
                    self.game.save_settings();
                    el.exit();
                }
                Request::ApplyVideo => self.apply_video(),
                Request::SavePhoto => self.save_photo(),
            }
        }
        // Mouse capture only during play.
        let play = self.in_play();
        if let Some(w) = &self.window
            && play != self.grabbed
        {
            self.grabbed = play;
            if play {
                let _ = w.set_cursor_grab(CursorGrabMode::Locked).or_else(|_| w.set_cursor_grab(CursorGrabMode::Confined));
                w.set_cursor_visible(false);
            } else {
                let _ = w.set_cursor_grab(CursorGrabMode::None);
                w.set_cursor_visible(true);
            }
        }
        let Some(r) = self.renderer.as_mut() else { return };
        if let Some(st) = self.game.take_static() {
            r.upload_static(&st);
        }
        let mut views: Vec<(View, DrawList)> = vec![];
        let mut post = PostSettings::default();
        self.game.draw(&mut views, &mut post);
        let refs: Vec<(View, &DrawList)> = views.iter().map(|(v, d)| (*v, d)).collect();
        let aspect = self.game.settings.aspect();
        r.render(&refs, &self.game.ui.verts, &post, aspect);
        self.game.last_stats = r.stats;
    }

    fn save_photo(&mut self) {
        let Some(r) = self.renderer.as_mut() else { return };
        let img = r.capture_last();
        let (w, h) = (r.low.w, r.low.h);
        let k = 3;
        let mut big = vec![0u8; (w * k * h * k * 4) as usize];
        for y in 0..h * k {
            for x in 0..w * k {
                let s = (((y / k) * w + x / k) * 4) as usize;
                let d = ((y * w * k + x) * 4) as usize;
                big[d..d + 4].copy_from_slice(&img[s..s + 4]);
                big[d + 3] = 255;
            }
        }
        let dir = self.game.paths.photos();
        let _ = std::fs::create_dir_all(&dir);
        let name = dir.join(format!(
            "harrowmouth_{}.png",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
        ));
        match image::save_buffer(&name, &big, w * k, h * k, image::ColorType::Rgba8) {
            Ok(_) => {
                self.game.toast(format!("PHOTO SAVED {}", name.file_name().map_or(String::new(), |n| n.to_string_lossy().to_string())))
            }
            Err(e) => self.game.toast(format!("PHOTO FAILED {e}")),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("IRON SAINT // LEVIATHAN")
            .with_inner_size(winit::dpi::LogicalSize::new(960.0, 720.0))
            .with_min_inner_size(winit::dpi::LogicalSize::new(320.0, 240.0));
        let window = Arc::new(el.create_window(attrs).expect("window"));
        let size = window.inner_size();
        let low = self.game.settings.low_res();
        match Renderer::new_windowed(window.clone(), (size.width, size.height), low, self.game.settings.vsync) {
            Ok(r) => {
                tracing::info!("renderer: {}", r.backend);
                self.renderer = Some(r);
            }
            Err(e) => {
                eprintln!("could not create renderer: {e}");
                el.exit();
                return;
            }
        }
        self.window = Some(window);
        self.apply_video();
        self.last = Instant::now();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.game.save_settings();
                // Quitting soon after a death marks the build for THE GRUDGE.
                if let Some(m) = &self.game.mission
                    && (m.dead_timer.is_some() || m.last_death_tick.is_some_and(|d| m.world.tick.saturating_sub(d) < 1200))
                    && let Some(p) = m.world.player(0)
                {
                    let (w, t) = self.game.campaign.as_ref().map_or((0, 0), |c| (c.war, c.tide.tide_count));
                    let lo = p.loadout.clone();
                    self.game.card.system.archive.set_rival("THE GRUDGE", lo, w, t);
                    self.game.save_card();
                }
                el.exit();
            }
            WindowEvent::Resized(sz) => {
                self.minimized = sz.width == 0 || sz.height == 0;
                if let Some(r) = self.renderer.as_mut() {
                    r.resize(sz.width, sz.height);
                }
            }
            WindowEvent::Focused(f) => {
                self.focused = f;
                if !f {
                    self.input.clear();
                    if self.game.settings.pause_unfocused && matches!(self.game.screen, Screen::Mission) && self.game.pause.is_none() {
                        self.game.pause = Some(0);
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    self.input.on_key(code, event.state);
                }
                if event.state.is_pressed()
                    && let Some(t) = &event.text
                    && matches!(self.game.screen, Screen::Codes { .. })
                {
                    self.game
                        .typed
                        .push_str(&t.to_uppercase().chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect::<String>());
                }
            }
            WindowEvent::MouseInput { state, button, .. } => self.input.on_mouse_button(button, state),
            WindowEvent::RedrawRequested => self.frame(el),
            _ => {}
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event
            && self.grabbed
        {
            self.input.on_mouse_motion(delta.0, delta.1);
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let Some(w) = &self.window else { return };
        if self.minimized || (!self.focused && self.game.settings.pause_unfocused && !matches!(self.game.screen, Screen::Cutscene(_))) {
            // Battery and thermals: don't render while hidden; poll slowly.
            el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(100)));
            return;
        }
        let menu = !matches!(self.game.screen, Screen::Mission | Screen::Cutscene(_));
        if menu {
            // Menus are capped at 30 fps.
            let now = Instant::now();
            if now >= self.next_frame {
                self.next_frame = now + Duration::from_micros(33_333);
                w.request_redraw();
            }
            el.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
        } else {
            el.set_control_flow(ControlFlow::Poll);
            w.request_redraw();
        }
    }
}

pub fn run() -> anyhow::Result<()> {
    let el = EventLoop::new()?;
    let mut app = App::new();
    el.run_app(&mut app)?;
    Ok(())
}
