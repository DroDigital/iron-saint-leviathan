//! The PSX rendering pipeline on wgpu (Metal / DX12 / Vulkan / GL).
//!
//! Frame flow:
//! 1. World passes into a **low-res target** (default 320x240) with depth: static district mesh,
//!    dynamic CPU-transformed geometry (Gouraud-lit on the CPU, like the GTE did), translucent and
//!    additive geometry. One pass per split-screen view.
//! 2. UI quads into the same low-res target (bitmap font).
//! 3. Optional fake-FMV pass (posterize, 8x8 macroblocks, chroma smear).
//! 4. Present: nearest-neighbour upscale with integer scaling and letterboxing, optional CRT and
//!    composite.
//!
//! Static geometry lives in persistent buffers; dynamic geometry is uploaded with
//! `queue.write_buffer` (no staging copies; ideal for unified memory on Apple Silicon).

pub mod meshgen;
pub mod textures;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use isl_ui::UiVertex;
use meshgen::Mesh;
use std::sync::Arc;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub color: [u8; 4],
    /// Texture layer (low 8 bits) | 0x100 = glow (fog-resistant).
    pub layer: u32,
}

pub const GLOW: u32 = 0x100;

#[derive(Default, Clone)]
pub struct DrawList {
    pub opaque: Vec<Vertex>,
    pub trans: Vec<Vertex>,
    pub add: Vec<Vertex>,
}

impl DrawList {
    pub fn clear(&mut self) {
        self.opaque.clear();
        self.trans.clear();
        self.add.clear();
    }
    pub fn tris(&self) -> usize {
        (self.opaque.len() + self.trans.len() + self.add.len()) / 3
    }
}

/// Lighting rig: ambient + up to 3 directional + up to 4 nearest point lights per object.
#[derive(Clone, Debug, Default)]
pub struct Lighting {
    pub ambient: Vec3,
    pub dirs: Vec<(Vec3, Vec3)>,
    pub points: Vec<(Vec3, Vec3, f32)>,
}

impl Lighting {
    /// Pick the 4 point lights nearest to `center`.
    pub fn nearest(&self, center: Vec3) -> Vec<(Vec3, Vec3, f32)> {
        let mut v: Vec<&(Vec3, Vec3, f32)> = self.points.iter().filter(|p| p.0.distance(center) < p.2 * 1.5).collect();
        v.sort_by(|a, b| a.0.distance_squared(center).total_cmp(&b.0.distance_squared(center)));
        v.into_iter().take(4).copied().collect()
    }
    pub fn shade(&self, p: Vec3, n: Vec3, pts: &[(Vec3, Vec3, f32)]) -> Vec3 {
        let mut c = self.ambient;
        for (d, col) in self.dirs.iter().take(3) {
            c += *col * n.dot(-*d).max(0.0);
        }
        for (lp, col, r) in pts {
            let dv = *lp - p;
            let dist = dv.length();
            if dist < *r {
                let att = (1.0 - dist / r).powi(2);
                c += *col * att * (n.dot(dv / dist.max(0.01)).max(0.0) * 0.7 + 0.3);
            }
        }
        c
    }
}

fn to_u8(x: f32) -> u8 {
    (x.clamp(0.0, 1.0) * 255.0) as u8
}

/// Transform, light and append a mesh. `tint` multiplies vertex colours; `alpha` < 255 goes to the
/// translucent list.
pub fn push_mesh(dl: &mut DrawList, mesh: &Mesh, model: Mat4, light: &Lighting, tint: Vec3, alpha: u8) {
    let center = model.transform_point3(Vec3::ZERO);
    let pts = light.nearest(center);
    let nm = glam::Mat3::from_mat4(model).inverse().transpose();
    let out = if alpha < 255 { &mut dl.trans } else { &mut dl.opaque };
    out.reserve(mesh.verts.len());
    for v in &mesh.verts {
        let p = model.transform_point3(v.pos);
        let base = Vec3::new(v.color[0] as f32, v.color[1] as f32, v.color[2] as f32) / 255.0 * tint;
        let c = if v.glow {
            base * 0.62
        } else {
            let n = (nm * v.normal).normalize_or_zero();
            base * light.shade(p, n, &pts) * 0.5
        };
        out.push(Vertex {
            pos: p.to_array(),
            uv: v.uv.to_array(),
            color: [to_u8(c.x), to_u8(c.y), to_u8(c.z), alpha],
            layer: v.layer as u32 | if v.glow { GLOW } else { 0 },
        });
    }
}

/// Pre-light a mesh into vertices (static district geometry).
pub fn bake(mesh: &Mesh, light: &Lighting) -> Vec<Vertex> {
    mesh.verts
        .iter()
        .map(|v| {
            let base = Vec3::new(v.color[0] as f32, v.color[1] as f32, v.color[2] as f32) / 255.0;
            let c = if v.glow {
                base * 0.62
            } else {
                let pts = light.nearest(v.pos);
                base * light.shade(v.pos, v.normal, &pts) * 0.5
            };
            Vertex {
                pos: v.pos.to_array(),
                uv: v.uv.to_array(),
                color: [to_u8(c.x), to_u8(c.y), to_u8(c.z), 255],
                layer: v.layer as u32 | if v.glow { GLOW } else { 0 },
            }
        })
        .collect()
}

/// Camera-facing quad (particles, sparks, blob shadows use `ground_quad`).
#[allow(clippy::too_many_arguments)]
pub fn billboard(out: &mut Vec<Vertex>, pos: Vec3, size: f32, right: Vec3, up: Vec3, layer: u32, color: [u8; 4]) {
    let r = right * size * 0.5;
    let u = up * size * 0.5;
    let q = [(pos - r - u, [0.0, 1.0]), (pos + r - u, [1.0, 1.0]), (pos + r + u, [1.0, 0.0]), (pos - r + u, [0.0, 0.0])];
    for i in [0, 1, 2, 0, 2, 3] {
        out.push(Vertex { pos: q[i].0.to_array(), uv: q[i].1, color, layer: layer | GLOW });
    }
}

pub fn ground_quad(out: &mut Vec<Vertex>, pos: Vec3, sx: f32, sz: f32, layer: u32, color: [u8; 4]) {
    let q = [
        (pos + Vec3::new(-sx, 0.0, -sz), [0.0, 0.0]),
        (pos + Vec3::new(sx, 0.0, -sz), [1.0, 0.0]),
        (pos + Vec3::new(sx, 0.0, sz), [1.0, 1.0]),
        (pos + Vec3::new(-sx, 0.0, sz), [0.0, 1.0]),
    ];
    for i in [0, 1, 2, 0, 2, 3] {
        out.push(Vertex { pos: q[i].0.to_array(), uv: q[i].1, color, layer: layer | GLOW });
    }
}

#[derive(Clone, Copy, Debug)]
pub struct View {
    pub view_proj: Mat4,
    pub cam: Vec3,
    pub fog_color: [f32; 3],
    pub fog_near: f32,
    pub fog_far: f32,
    pub flash: f32,
    /// x, y, w, h in low-res pixels.
    pub viewport: [u32; 4],
    pub draw_static: bool,
    pub clear: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PostSettings {
    pub snap: f32,
    pub dither: f32,
    pub crt: bool,
    pub composite: bool,
    pub scanline: f32,
    pub integer_scale: bool,
    pub fmv: f32,
    pub time: f32,
}

impl Default for PostSettings {
    fn default() -> Self {
        PostSettings { snap: 1.0, dither: 1.0, crt: false, composite: false, scanline: 0.35, integer_scale: true, fmv: 0.0, time: 0.0 }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    cam: [f32; 4],
    fog_color: [f32; 4],
    fog: [f32; 4],
    res: [f32; 4],
}

const GLOBALS_STRIDE: u64 = 256;
const MAX_VIEWS: u64 = 4;

struct DynBuf {
    buf: wgpu::Buffer,
    cap: u64,
    label: &'static str,
}

impl DynBuf {
    fn new(device: &wgpu::Device, label: &'static str, cap: u64) -> DynBuf {
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: cap,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        DynBuf { buf, cap, label }
    }
    fn write(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let need = (data.len() as u64).next_multiple_of(4);
        if need > self.cap {
            let cap = need.next_power_of_two();
            *self = DynBuf::new(device, self.label, cap);
        }
        queue.write_buffer(&self.buf, 0, data);
    }
}

pub struct LowRes {
    pub w: u32,
    pub h: u32,
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    fmv: wgpu::Texture,
    fmv_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    surface: Option<wgpu::Surface<'static>>,
    pub config: Option<wgpu::SurfaceConfiguration>,
    pub low: LowRes,
    low_format: wgpu::TextureFormat,
    world_opaque: wgpu::RenderPipeline,
    world_trans: wgpu::RenderPipeline,
    world_add: wgpu::RenderPipeline,
    ui_pipe: wgpu::RenderPipeline,
    fmv_pipe: wgpu::RenderPipeline,
    present_pipe: Option<wgpu::RenderPipeline>,
    globals: wgpu::Buffer,
    globals_bg: wgpu::BindGroup,
    tex_bg: wgpu::BindGroup,
    ui_uniform: wgpu::Buffer,
    ui_bg: wgpu::BindGroup,
    post_bgl: wgpu::BindGroupLayout,
    post_uniform_fmv: wgpu::Buffer,
    post_uniform_present: wgpu::Buffer,
    post_bg_fmv: wgpu::BindGroup,
    post_bg_present_a: wgpu::BindGroup,
    post_bg_present_b: wgpu::BindGroup,
    static_vb: Option<(wgpu::Buffer, u32)>,
    dyn_opaque: Vec<DynBuf>,
    dyn_trans: Vec<DynBuf>,
    dyn_add: Vec<DynBuf>,
    ui_vb: DynBuf,
    pub backend: String,
    pub stats: FrameStats,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FrameStats {
    pub draw_calls: u32,
    pub tris: u32,
}

fn make_low(device: &wgpu::Device, w: u32, h: u32, format: wgpu::TextureFormat) -> LowRes {
    let mk = |label, format, usage| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let rt = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC;
    let color = mk("low color", format, rt);
    let fmv = mk("low fmv", format, rt);
    let depth = mk("low depth", wgpu::TextureFormat::Depth32Float, wgpu::TextureUsages::RENDER_ATTACHMENT);
    LowRes {
        w,
        h,
        color_view: color.create_view(&Default::default()),
        fmv_view: fmv.create_view(&Default::default()),
        depth_view: depth.create_view(&Default::default()),
        color,
        fmv,
    }
}

pub type WindowHandle = Arc<dyn wgpu::WindowHandle + 'static>;

impl Renderer {
    /// Create a renderer for a window (`window` must implement the raw-window-handle traits).
    pub fn new_windowed<W>(window: Arc<W>, size: (u32, u32), low: (u32, u32), vsync: bool) -> Result<Renderer, String>
    where
        W: wgpu::rwh::HasWindowHandle + wgpu::rwh::HasDisplayHandle + std::fmt::Debug + Send + Sync + 'static,
    {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(window.clone())));
        let surface = instance.create_surface(window).map_err(|e| e.to_string())?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            apply_limit_buckets: false,
        }))
        .map_err(|e| e.to_string())?;
        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0]);
        let present_mode = if vsync {
            wgpu::PresentMode::Fifo
        } else if caps.present_modes.contains(&wgpu::PresentMode::Mailbox) {
            wgpu::PresentMode::Mailbox
        } else {
            wgpu::PresentMode::Fifo
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: Default::default(),
            width: size.0.max(1),
            height: size.1.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        let mut r = Self::from_adapter(&adapter, low, Some(format))?;
        surface.configure(&r.device, &config);
        r.surface = Some(surface);
        r.config = Some(config);
        Ok(r)
    }

    /// Headless renderer (tests, CI screenshots, soak tests). Uses any adapter incl. software.
    pub fn new_headless(low: (u32, u32)) -> Result<Renderer, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        }))
        .map_err(|e| e.to_string())?;
        Self::from_adapter(&adapter, low, None)
    }

    fn from_adapter(adapter: &wgpu::Adapter, low: (u32, u32), present_format: Option<wgpu::TextureFormat>) -> Result<Renderer, String> {
        let info = adapter.get_info();
        let backend = format!("{:?} / {}", info.backend, info.name);
        tracing::info!("adapter: {backend}");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("isl device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .map_err(|e| e.to_string())?;
        let low_format = wgpu::TextureFormat::Rgba8Unorm;
        let lowres = make_low(&device, low.0, low.1, low_format);

        // Texture array.
        let layers = textures::build_all();
        let ts = textures::SIZE;
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("psx textures"),
            size: wgpu::Extent3d { width: ts, height: ts, depth_or_array_layers: layers.len() as u32 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (i, l) in layers.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: 0, z: i as u32 },
                    aspect: wgpu::TextureAspect::All,
                },
                l,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(ts * 4), rows_per_image: Some(ts) },
                wgpu::Extent3d { width: ts, height: ts, depth_or_array_layers: 1 },
            );
        }
        let tex_view =
            tex.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
        let nearest = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nearest"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Font atlas.
        let font = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("font"),
            size: wgpu::Extent3d { width: isl_ui::ATLAS_W, height: isl_ui::ATLAS_H, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &font, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &isl_ui::atlas_rgba(),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(isl_ui::ATLAS_W * 4), rows_per_image: Some(isl_ui::ATLAS_H) },
            wgpu::Extent3d { width: isl_ui::ATLAS_W, height: isl_ui::ATLAS_H, depth_or_array_layers: 1 },
        );
        let font_view = font.create_view(&Default::default());

        // Layouts.
        let globals_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: None },
                count: None,
            }],
        });
        let tex_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("textures"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let ui_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let post_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: GLOBALS_STRIDE * MAX_VIEWS,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &globals,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<Globals>() as u64),
                }),
            }],
        });
        let tex_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("textures"),
            layout: &tex_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&tex_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&nearest) },
            ],
        });
        let ui_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui uniform"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ui_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui"),
            layout: &ui_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: ui_uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&font_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&nearest) },
            ],
        });
        let mkpost = |label| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: 48,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let post_uniform_fmv = mkpost("fmv uniform");
        let post_uniform_present = mkpost("present uniform");

        let world_sm = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("world"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/world.wgsl").into()),
        });
        let ui_sm = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/ui.wgsl").into()),
        });
        let post_sm = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("post"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/post.wgsl").into()),
        });

        let world_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("world"),
            bind_group_layouts: &[Some(&globals_bgl), Some(&tex_bgl)],
            immediate_size: 0,
        });
        let ui_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui"),
            bind_group_layouts: &[Some(&ui_bgl)],
            immediate_size: 0,
        });
        let post_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post"),
            bind_group_layouts: &[Some(&post_bgl)],
            immediate_size: 0,
        });

        let vattrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Unorm8x4, 3 => Uint32];
        let vlayout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &vattrs,
        };
        let world_pipe = |label: &'static str, blend: Option<wgpu::BlendState>, depth_write: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&world_layout),
                vertex: wgpu::VertexState {
                    module: &world_sm,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(vlayout.clone())],
                },
                primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &world_sm,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format: low_format, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let world_opaque = world_pipe("world opaque", None, true);
        let world_trans = world_pipe("world trans", Some(wgpu::BlendState::ALPHA_BLENDING), false);
        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        };
        let world_add = world_pipe("world add", Some(additive), false);

        let uattrs = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Unorm8x4];
        let ui_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui"),
            layout: Some(&ui_layout),
            vertex: wgpu::VertexState {
                module: &ui_sm,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<UiVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &uattrs,
                })],
            },
            primitive: Default::default(),
            // Drawn inside the world pass, so it must match its depth attachment.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &ui_sm,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: low_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let full_pipe = |label: &'static str, fs: &'static str, format: wgpu::TextureFormat| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&post_layout),
                vertex: wgpu::VertexState {
                    module: &post_sm,
                    entry_point: Some("vs_full"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &post_sm,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let fmv_pipe = full_pipe("fmv", "fs_fmv", low_format);
        let present_pipe = present_format.map(|f| full_pipe("present", "fs_present", f));

        let (post_bg_fmv, post_bg_present_a, post_bg_present_b) =
            Self::post_bgs(&device, &post_bgl, &lowres, &post_uniform_fmv, &post_uniform_present);
        let mk = |n: &'static str| (0..MAX_VIEWS).map(|_| DynBuf::new(&device, n, 1 << 16)).collect::<Vec<_>>();
        Ok(Renderer {
            dyn_opaque: mk("dyn opaque"),
            dyn_trans: mk("dyn trans"),
            dyn_add: mk("dyn add"),
            ui_vb: DynBuf::new(&device, "ui vb", 1 << 16),
            device,
            queue,
            surface: None,
            config: None,
            low: lowres,
            low_format,
            world_opaque,
            world_trans,
            world_add,
            ui_pipe,
            fmv_pipe,
            present_pipe,
            globals,
            globals_bg,
            tex_bg,
            ui_uniform,
            ui_bg,
            post_bgl,
            post_uniform_fmv,
            post_uniform_present,
            post_bg_fmv,
            post_bg_present_a,
            post_bg_present_b,
            static_vb: None,
            backend,
            stats: FrameStats::default(),
        })
    }

    fn post_bgs(
        device: &wgpu::Device,
        bgl: &wgpu::BindGroupLayout,
        low: &LowRes,
        uf: &wgpu::Buffer,
        up: &wgpu::Buffer,
    ) -> (wgpu::BindGroup, wgpu::BindGroup, wgpu::BindGroup) {
        let bg = |view: &wgpu::TextureView, u: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("post"),
                layout: bgl,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                    wgpu::BindGroupEntry { binding: 1, resource: u.as_entire_binding() },
                ],
            })
        };
        (bg(&low.color_view, uf), bg(&low.color_view, up), bg(&low.fmv_view, up))
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        if let (Some(s), Some(c)) = (&self.surface, &mut self.config) {
            c.width = w.max(1);
            c.height = h.max(1);
            s.configure(&self.device, c);
        }
    }

    pub fn set_vsync(&mut self, vsync: bool) {
        if let (Some(s), Some(c)) = (&self.surface, &mut self.config) {
            c.present_mode = if vsync { wgpu::PresentMode::Fifo } else { wgpu::PresentMode::AutoNoVsync };
            s.configure(&self.device, c);
        }
    }

    /// Change the internal resolution (320x240, 256x224, 368x240, 512x448, 640x480, widescreen).
    pub fn set_low_res(&mut self, w: u32, h: u32) {
        if self.low.w == w && self.low.h == h {
            return;
        }
        self.low = make_low(&self.device, w, h, self.low_format);
        let (a, b, c) = Self::post_bgs(&self.device, &self.post_bgl, &self.low, &self.post_uniform_fmv, &self.post_uniform_present);
        self.post_bg_fmv = a;
        self.post_bg_present_a = b;
        self.post_bg_present_b = c;
    }

    /// Upload static (pre-lit) district geometry into a persistent buffer.
    pub fn upload_static(&mut self, verts: &[Vertex]) {
        if verts.is_empty() {
            self.static_vb = None;
            return;
        }
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("static district"),
            size: std::mem::size_of_val(verts) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&buf, 0, bytemuck::cast_slice(verts));
        self.static_vb = Some((buf, verts.len() as u32));
    }

    /// Integer-scaled, letterboxed destination rect for the low-res image.
    pub fn present_rect(&self, out_w: u32, out_h: u32, integer: bool, aspect: f32) -> [f32; 4] {
        let (lw, lh) = (self.low.w as f32, self.low.h as f32);
        let (ow, oh) = (out_w as f32, out_h as f32);
        // Display aspect (4:3 or 16:9) independent of the internal pixel grid.
        let disp_w = lh * aspect;
        let k = (ow / disp_w).min(oh / lh);
        let k = if integer && k >= 1.0 { k.floor() } else { k };
        let (w, h) = (disp_w * k, lh * k);
        let _ = lw;
        let x = ((ow - w) * 0.5).floor();
        let y = ((oh - h) * 0.5).floor();
        [x, y, x + w, y + h]
    }

    fn encode_world(&mut self, enc: &mut wgpu::CommandEncoder, views: &[(View, &DrawList)], ui: &[UiVertex], post: &PostSettings) {
        self.stats = FrameStats::default();
        // Upload per-view globals and geometry.
        for (vi, (v, dl)) in views.iter().enumerate().take(MAX_VIEWS as usize) {
            let g = Globals {
                view_proj: v.view_proj.to_cols_array_2d(),
                cam: [v.cam.x, v.cam.y, v.cam.z, post.time],
                fog_color: [v.fog_color[0], v.fog_color[1], v.fog_color[2], v.flash],
                fog: [v.fog_near, v.fog_far, post.snap, post.dither],
                res: [self.low.w as f32, self.low.h as f32, v.viewport[2] as f32, v.viewport[3] as f32],
            };
            self.queue.write_buffer(&self.globals, vi as u64 * GLOBALS_STRIDE, bytemuck::bytes_of(&g));
            self.dyn_opaque[vi].write(&self.device, &self.queue, bytemuck::cast_slice(&dl.opaque));
            self.dyn_trans[vi].write(&self.device, &self.queue, bytemuck::cast_slice(&dl.trans));
            self.dyn_add[vi].write(&self.device, &self.queue, bytemuck::cast_slice(&dl.add));
        }
        self.ui_vb.write(&self.device, &self.queue, bytemuck::cast_slice(ui));
        self.queue.write_buffer(&self.ui_uniform, 0, bytemuck::cast_slice(&[self.low.w as f32, self.low.h as f32, 0.0, 0.0]));

        let q = |c: f32| (c.clamp(0.0, 1.0) * 31.0).round() / 31.0;
        let clear = views.first().map_or([0.0; 3], |v| v.0.fog_color.map(q));
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("low-res world"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.low.color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: clear[0] as f64, g: clear[1] as f64, b: clear[2] as f64, a: 1.0 }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.low.depth_view,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        for (vi, (v, dl)) in views.iter().enumerate().take(MAX_VIEWS as usize) {
            let [x, y, w, h] = v.viewport;
            pass.set_viewport(x as f32, y as f32, w.max(1) as f32, h.max(1) as f32, 0.0, 1.0);
            pass.set_scissor_rect(x, y, w.max(1).min(self.low.w - x.min(self.low.w - 1)), h.max(1).min(self.low.h - y.min(self.low.h - 1)));
            pass.set_bind_group(0, &self.globals_bg, &[(vi as u64 * GLOBALS_STRIDE) as u32]);
            pass.set_bind_group(1, &self.tex_bg, &[]);
            pass.set_pipeline(&self.world_opaque);
            if v.draw_static {
                if let Some((b, n)) = &self.static_vb {
                    pass.set_vertex_buffer(0, b.slice(..));
                    pass.draw(0..*n, 0..1);
                    self.stats.draw_calls += 1;
                    self.stats.tris += n / 3;
                }
            }
            let draws: [(&wgpu::RenderPipeline, &DynBuf, usize); 3] = [
                (&self.world_opaque, &self.dyn_opaque[vi], dl.opaque.len()),
                (&self.world_trans, &self.dyn_trans[vi], dl.trans.len()),
                (&self.world_add, &self.dyn_add[vi], dl.add.len()),
            ];
            for (pipe, buf, n) in draws {
                if n == 0 {
                    continue;
                }
                pass.set_pipeline(pipe);
                pass.set_vertex_buffer(0, buf.buf.slice(..(n * std::mem::size_of::<Vertex>()) as u64));
                pass.draw(0..n as u32, 0..1);
                self.stats.draw_calls += 1;
                self.stats.tris += n as u32 / 3;
            }
        }
        pass.set_viewport(0.0, 0.0, self.low.w as f32, self.low.h as f32, 0.0, 1.0);
        pass.set_scissor_rect(0, 0, self.low.w, self.low.h);
        if !ui.is_empty() {
            pass.set_pipeline(&self.ui_pipe);
            pass.set_bind_group(0, &self.ui_bg, &[]);
            pass.set_vertex_buffer(0, self.ui_vb.buf.slice(..std::mem::size_of_val(ui) as u64));
            pass.draw(0..ui.len() as u32, 0..1);
            self.stats.draw_calls += 1;
        }
        drop(pass);
        if post.fmv > 0.0 {
            self.queue.write_buffer(
                &self.post_uniform_fmv,
                0,
                bytemuck::cast_slice(&[post.fmv, post.time, self.low.w as f32, self.low.h as f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
            );
            let mut p = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fmv"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.low.fmv_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            p.set_pipeline(&self.fmv_pipe);
            p.set_bind_group(0, &self.post_bg_fmv, &[]);
            p.draw(0..3, 0..1);
        }
    }

    /// Render a frame to the window. Returns false if the surface is unavailable (minimized).
    pub fn render(&mut self, views: &[(View, &DrawList)], ui: &[UiVertex], post: &PostSettings, aspect: f32) -> bool {
        let Some(surface) = &self.surface else { return false };
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                if let Some(c) = &self.config {
                    surface.configure(&self.device, c);
                }
                return false;
            }
            _ => return false,
        };
        let out_view = frame.texture.create_view(&Default::default());
        let (ow, oh) = self.config.as_ref().map_or((640, 480), |c| (c.width, c.height));
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        self.encode_world(&mut enc, views, ui, post);
        let rect = self.present_rect(ow, oh, post.integer_scale, aspect);
        let u: [f32; 12] = [
            rect[0],
            rect[1],
            rect[2],
            rect[3],
            ow as f32,
            oh as f32,
            self.low.w as f32,
            self.low.h as f32,
            post.crt as u8 as f32,
            post.scanline,
            post.composite as u8 as f32,
            post.time,
        ];
        self.queue.write_buffer(&self.post_uniform_present, 0, bytemuck::cast_slice(&u));
        {
            let mut p = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("present"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &out_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(pp) = &self.present_pipe {
                p.set_pipeline(pp);
                p.set_bind_group(0, if post.fmv > 0.0 { &self.post_bg_present_b } else { &self.post_bg_present_a }, &[]);
                p.draw(0..3, 0..1);
            }
        }
        self.queue.submit([enc.finish()]);
        self.queue.present(frame);
        true
    }

    /// Render offscreen and read back the low-res image as RGBA8 (photo mode, tests, CI).
    pub fn render_to_image(&mut self, views: &[(View, &DrawList)], ui: &[UiVertex], post: &PostSettings) -> Vec<u8> {
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("offscreen") });
        self.encode_world(&mut enc, views, ui, post);
        let src = if post.fmv > 0.0 { &self.low.fmv } else { &self.low.color };
        self.readback(enc, src)
    }

    /// Read back the last rendered low-res frame (photo mode).
    pub fn capture_last(&mut self) -> Vec<u8> {
        let enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("capture") });
        let t = self.low.color.clone();
        self.readback(enc, &t)
    }

    fn readback(&self, mut enc: wgpu::CommandEncoder, src: &wgpu::Texture) -> Vec<u8> {
        let (w, h) = (self.low.w, self.low.h);
        let bpr = (w * 4).next_multiple_of(256);
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (bpr * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: src, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);
        let slice = buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
        let Ok(data) = slice.get_mapped_range() else { return vec![0; (w * h * 4) as usize] };
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for row in 0..h {
            let s = (row * bpr) as usize;
            out.extend_from_slice(&data[s..s + (w * 4) as usize]);
        }
        drop(data);
        buf.unmap();
        out
    }
}

/// PSX projection: vertical FOV in radians, near/far.
/// Right-handed, depth 0..1 (wgpu clip space).
pub fn projection(fov: f32, aspect: f32, far: f32) -> Mat4 {
    let near = 0.5;
    let f = 1.0 / (fov * 0.5).tan();
    Mat4::from_cols_array(&[
        f / aspect,
        0.0,
        0.0,
        0.0,
        0.0,
        f,
        0.0,
        0.0,
        0.0,
        0.0,
        far / (near - far),
        -1.0,
        0.0,
        0.0,
        near * far / (near - far),
        0.0,
    ])
}

/// Right-handed look-at view matrix.
pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
    let f = (target - eye).normalize_or(Vec3::NEG_Z);
    let s = f.cross(up).normalize_or(Vec3::X);
    let u = s.cross(f);
    Mat4::from_cols_array(&[s.x, u.x, -f.x, 0.0, s.y, u.y, -f.y, 0.0, s.z, u.z, -f.z, 0.0, -s.dot(eye), -u.dot(eye), f.dot(eye), 1.0])
}

/// Orthographic, RH, depth 0..1 (war map, turntables).
pub fn ortho(l: f32, r: f32, b: f32, t: f32, n: f32, f: f32) -> Mat4 {
    Mat4::from_cols_array(&[
        2.0 / (r - l),
        0.0,
        0.0,
        0.0,
        0.0,
        2.0 / (t - b),
        0.0,
        0.0,
        0.0,
        0.0,
        1.0 / (n - f),
        0.0,
        -(r + l) / (r - l),
        -(t + b) / (t - b),
        n / (n - f),
        1.0,
    ])
}
