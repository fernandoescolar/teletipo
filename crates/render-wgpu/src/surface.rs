//! wgpu surface, device, and pipeline construction helpers.
//!
//! Extracted from `pipeline::GpuState::new` so that the constructor focuses on
//! wiring already-built resources together. The functions in this module are
//! intentionally stateless: every wgpu object they return is owned by the
//! caller (typically [`crate::pipeline::GpuState`]).

use anyhow::{Context, Result};
use winit::dpi::PhysicalSize;
use winit::window::Window;

use crate::atlas::TEXT_ATLAS_SIZE;
use crate::geometry::{
    SHADER_WGSL, TEXT_SHADER_WGSL, TEXT_VERTEX_BUF_CAPACITY, VERTEX_BUF_CAPACITY,
};
use crate::types::{RenderConfig, VsyncMode};

/// Bundle returned by [`init_surface`].
pub(crate) struct SurfaceInit<'a> {
    pub surface: wgpu::Surface<'a>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub size: PhysicalSize<u32>,
}

/// Resources for the background (flat-colour) pipeline.
pub(crate) struct BgPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub vertex_buf: wgpu::Buffer,
}

/// Atlas texture + bind-group layout + bind-group used by the text pipeline.
pub(crate) struct AtlasResources {
    pub texture: wgpu::Texture,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
}

/// Resources for the text (atlas-sampled) pipeline.
pub(crate) struct TextPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub vertex_buf: wgpu::Buffer,
}

/// Create the wgpu instance, surface, adapter, device, and initial surface
/// configuration. Picks a non-sRGB format (theme colours are authored in sRGB
/// space already) and applies the requested vsync mode.
pub(crate) async fn init_surface<'a>(
    window: &'a Window,
    render_config: &RenderConfig,
) -> Result<SurfaceInit<'a>> {
    let size = window.inner_size();
    let instance = wgpu::Instance::default();
    let surface = instance
        .create_surface(window)
        .context("create wgpu surface")?;

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        })
        .await
        .context("request adapter")?;

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: Some("teletipo-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
            },
            None,
        )
        .await
        .context("request device")?;

    let caps = surface.get_capabilities(&adapter);
    // Prefer a non-sRGB surface format.  Our colours are already in sRGB space
    // (theme hex values, ANSI palette entries).  Choosing an sRGB target would
    // cause the GPU to apply an additional linear→sRGB gamma encode step, making
    // every colour appear significantly lighter / "washed out".
    let format = caps
        .formats
        .iter()
        .find(|f| !f.is_srgb())
        .copied()
        .unwrap_or(caps.formats[0]);

    let present_mode = match render_config.vsync {
        VsyncMode::On => wgpu::PresentMode::Fifo,
        VsyncMode::Off => wgpu::PresentMode::Immediate,
        VsyncMode::Adaptive => wgpu::PresentMode::AutoVsync,
    };

    let mut config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: size.width.max(1),
        height: size.height.max(1),
        present_mode,
        alpha_mode: caps.alpha_modes[0],
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };
    if !caps.present_modes.contains(&present_mode) {
        config.present_mode = wgpu::PresentMode::Fifo;
    }
    surface.configure(&device, &config);

    Ok(SurfaceInit {
        surface,
        device,
        queue,
        config,
        size,
    })
}

/// Build the background (flat-colour) render pipeline and its vertex buffer.
pub(crate) fn build_bg_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> BgPipeline {
    let bg_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("teletipo-bg-shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER_WGSL.into()),
    });
    let bg_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("teletipo-bg-layout"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });
    let bg_vattrs = [
        wgpu::VertexAttribute {
            offset: 0,
            shader_location: 0,
            format: wgpu::VertexFormat::Float32x2,
        },
        wgpu::VertexAttribute {
            offset: 8,
            shader_location: 1,
            format: wgpu::VertexFormat::Float32x4,
        },
    ];
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("teletipo-bg-pipeline"),
        layout: Some(&bg_layout),
        vertex: wgpu::VertexState {
            module: &bg_shader,
            entry_point: "vs_main",
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: 24,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &bg_vattrs,
            }],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &bg_shader,
            entry_point: "fs_main",
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
    });
    let vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("teletipo-vertex-buf"),
        size: VERTEX_BUF_CAPACITY,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    BgPipeline {
        pipeline,
        vertex_buf,
    }
}

/// Allocate the glyph atlas texture and build its bind-group layout/binding
/// used by the text pipeline's fragment shader.
pub(crate) fn build_atlas_resources(device: &wgpu::Device) -> AtlasResources {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("teletipo-atlas"),
        size: wgpu::Extent3d {
            width: TEXT_ATLAS_SIZE,
            height: TEXT_ATLAS_SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("teletipo-atlas-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
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
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("teletipo-atlas-bg"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    AtlasResources {
        texture,
        bind_group_layout,
        bind_group,
    }
}

/// Build the text (atlas-sampled) render pipeline and its vertex buffer.
pub(crate) fn build_text_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    atlas_bgl: &wgpu::BindGroupLayout,
) -> TextPipeline {
    let text_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("teletipo-text-shader"),
        source: wgpu::ShaderSource::Wgsl(TEXT_SHADER_WGSL.into()),
    });
    let text_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("teletipo-text-layout"),
        bind_group_layouts: &[atlas_bgl],
        push_constant_ranges: &[],
    });
    let text_vattrs = [
        wgpu::VertexAttribute {
            offset: 0,
            shader_location: 0,
            format: wgpu::VertexFormat::Float32x2,
        },
        wgpu::VertexAttribute {
            offset: 8,
            shader_location: 1,
            format: wgpu::VertexFormat::Float32x2,
        },
        wgpu::VertexAttribute {
            offset: 16,
            shader_location: 2,
            format: wgpu::VertexFormat::Float32x4,
        },
    ];
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("teletipo-text-pipeline"),
        layout: Some(&text_layout),
        vertex: wgpu::VertexState {
            module: &text_shader,
            entry_point: "vs_text",
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: 32,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &text_vattrs,
            }],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &text_shader,
            entry_point: "fs_text",
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
    });
    let vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("teletipo-text-vertex-buf"),
        size: TEXT_VERTEX_BUF_CAPACITY,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    TextPipeline {
        pipeline,
        vertex_buf,
    }
}
