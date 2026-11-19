//! Native capture-to-encode building blocks shared by REPLAYX's development
//! diagnostics (`replayx-wgc-profile`, `replayx-native-proto`). Not part of
//! the recorder: one fixed-function D3D11 VideoProcessor pass (scale, BGRA ->
//! NV12, BT.709 limited range) and an optional move of the NV12 frame to the
//! NVIDIA GPU through system memory.
#![allow(dead_code)]
use anyhow::{Context, Result};
use std::{ffi::c_void, time::Instant};
use windows::{
    core::Interface,
    Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_UNKNOWN,
            Direct3D11::*,
            Dxgi::{CreateDXGIFactory1, IDXGIAdapter, IDXGIFactory1},
        },
    },
};

/// The work a native capture-to-encode path would do per chosen frame, for
/// measuring its cost before building one (development diagnostic only):
/// one fixed-function D3D11 VideoProcessor pass (scale to the encoded size,
/// BGRA -> NV12, BT.709 limited range) and, optionally, moving the NV12 frame
/// to the NVIDIA GPU where NVENC would encode it.
///
/// The transfer goes through system memory (non-blocking readback ring on the
/// capture GPU, upload on the NVIDIA GPU): the portable path. A cross-adapter
/// heap copy could be cheaper; this measures an upper bound of the transfer.
pub struct Native {
    pub video_device: ID3D11VideoDevice,
    pub video_context: ID3D11VideoContext,
    pub enumerator: ID3D11VideoProcessorEnumerator,
    pub processor: ID3D11VideoProcessor,
    pub nv12: ID3D11Texture2D,
    pub output_view: ID3D11VideoProcessorOutputView,
    /// Input views per capture texture (the frame pool reuses a few).
    pub input_views: Vec<(usize, ID3D11VideoProcessorInputView)>,
    /// Used when the capture texture cannot be read by the video processor
    /// directly (counted: it adds a copy).
    pub intermediate: Option<ID3D11Texture2D>,
    pub intermediate_copies: usize,
    /// Input views created (one per capture texture the pool reuses).
    pub input_views_created: usize,
    /// Output views for other NV12 targets (an encoder's input ring).
    pub target_views: Vec<(usize, ID3D11VideoProcessorOutputView)>,
    pub out: (u32, u32),
    pub transfer: Option<Transfer>,
}
pub struct Transfer {
    pub staging: Vec<ID3D11Texture2D>,
    pub in_flight: std::collections::VecDeque<usize>,
    pub next: usize,
    pub host: Vec<u8>,
    pub device: ID3D11Device,
    pub context: ID3D11DeviceContext,
    pub target: ID3D11Texture2D,
    pub adapter: String,
    pub uploaded: usize,
    pub not_ready: usize,
    pub upload_cpu_ms: Vec<f64>,
}

/// The first adapter with this PCI vendor id, and its name.
pub unsafe fn adapter_by_vendor(vendor: u32) -> Option<(IDXGIAdapter, String)> {
    let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
    let mut i = 0;
    while let Ok(adapter) = factory.EnumAdapters1(i) {
        i += 1;
        let desc = adapter.GetDesc1().ok()?;
        if desc.VendorId == vendor {
            let name = String::from_utf16_lossy(&desc.Description)
                .trim_end_matches('\0')
                .to_string();
            return Some((adapter.cast().ok()?, name));
        }
    }
    None
}

pub unsafe fn texture(
    device: &ID3D11Device,
    (width, height): (u32, u32),
    format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT,
    usage: D3D11_USAGE,
    bind: D3D11_BIND_FLAG,
    cpu: D3D11_CPU_ACCESS_FLAG,
) -> Result<ID3D11Texture2D> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: format,
        SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: usage,
        BindFlags: bind.0 as u32,
        CPUAccessFlags: cpu.0 as u32,
        MiscFlags: 0,
    };
    let mut t = None;
    device.CreateTexture2D(&desc, None, Some(&mut t))?;
    t.context("texture")
}

impl Native {
    pub unsafe fn new(
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        input: (u32, u32),
        out: (u32, u32),
        transfer: bool,
    ) -> Result<Self> {
        use windows::Win32::Graphics::Dxgi::Common::{
            DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709, DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709,
            DXGI_FORMAT_NV12, DXGI_FORMAT_R8_UNORM, DXGI_RATIONAL,
        };
        let video_device: ID3D11VideoDevice = device
            .cast()
            .context("This GPU has no Direct3D 11 video processor")?;
        let video_context: ID3D11VideoContext = context.cast()?;
        let rate = DXGI_RATIONAL {
            Numerator: 60,
            Denominator: 1,
        };
        let enumerator =
            video_device.CreateVideoProcessorEnumerator(&D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
                InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
                InputFrameRate: rate,
                InputWidth: input.0,
                InputHeight: input.1,
                OutputFrameRate: rate,
                OutputWidth: out.0,
                OutputHeight: out.1,
                Usage: D3D11_VIDEO_USAGE_OPTIMAL_SPEED,
            })?;
        let processor = video_device.CreateVideoProcessor(&enumerator, 0)?;
        // Full-range RGB in, BT.709 limited-range YCbCr out, as REPLAYX
        // records (vpp_qsv out_color_matrix=bt709:out_range=1).
        if let Ok(c1) = video_context.cast::<ID3D11VideoContext1>() {
            c1.VideoProcessorSetStreamColorSpace1(
                &processor,
                0,
                DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
            );
            c1.VideoProcessorSetOutputColorSpace1(
                &processor,
                DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709,
            );
        }
        video_context.VideoProcessorSetStreamFrameFormat(
            &processor,
            0,
            D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
        );
        let nv12 = texture(
            device,
            out,
            DXGI_FORMAT_NV12,
            D3D11_USAGE_DEFAULT,
            D3D11_BIND_RENDER_TARGET,
            D3D11_CPU_ACCESS_FLAG(0),
        )?;
        let mut output_view = None;
        video_device.CreateVideoProcessorOutputView(
            &nv12,
            &enumerator,
            &D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
                },
            },
            Some(&mut output_view),
        )?;
        let transfer = if transfer {
            let (adapter, name) =
                adapter_by_vendor(0x10de).context("No NVIDIA GPU for the transfer stage")?;
            let (mut nv_device, mut nv_context) = (None, None);
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_FLAG(0),
                None,
                D3D11_SDK_VERSION,
                Some(&mut nv_device),
                None,
                Some(&mut nv_context),
            )?;
            let nv_device: ID3D11Device = nv_device.unwrap();
            let staging = (0..4)
                .map(|_| {
                    texture(
                        device,
                        out,
                        DXGI_FORMAT_NV12,
                        D3D11_USAGE_STAGING,
                        D3D11_BIND_FLAG(0),
                        D3D11_CPU_ACCESS_READ,
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            // The NV12 bytes (luma rows, then interleaved chroma rows) in a
            // plain 8-bit texture of the same size: the same 3 MB upload.
            let target = texture(
                &nv_device,
                (out.0, out.1 * 3 / 2),
                DXGI_FORMAT_R8_UNORM,
                D3D11_USAGE_DEFAULT,
                D3D11_BIND_SHADER_RESOURCE,
                D3D11_CPU_ACCESS_FLAG(0),
            )?;
            Some(Transfer {
                staging,
                in_flight: Default::default(),
                next: 0,
                host: vec![0; (out.0 * out.1 * 3 / 2) as usize],
                device: nv_device,
                context: nv_context.unwrap(),
                target,
                adapter: name,
                uploaded: 0,
                not_ready: 0,
                upload_cpu_ms: Vec::new(),
            })
        } else {
            None
        };
        Ok(Self {
            video_device,
            video_context,
            enumerator,
            processor,
            nv12,
            output_view: output_view.unwrap(),
            input_views: Vec::new(),
            intermediate: None,
            intermediate_copies: 0,
            input_views_created: 0,
            target_views: Vec::new(),
            out,
            transfer,
        })
    }

    pub unsafe fn input_view(
        &mut self,
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        source: &ID3D11Texture2D,
    ) -> Result<ID3D11VideoProcessorInputView> {
        let key = source.as_raw() as usize;
        if let Some((_, v)) = self.input_views.iter().find(|(k, _)| *k == key) {
            return Ok(v.clone());
        }
        // WGC reuses a few pool textures; if it ever hands out new objects,
        // never let the cache grow without bound.
        if self.input_views.len() >= 16 {
            self.input_views.clear();
        }
        self.input_views_created += 1;
        let desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
            FourCC: 0,
            ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPIV {
                    MipSlice: 0,
                    ArraySlice: 0,
                },
            },
        };
        let mut view = None;
        if self
            .video_device
            .CreateVideoProcessorInputView(source, &self.enumerator, &desc, Some(&mut view))
            .is_ok()
        {
            let view = view.unwrap();
            self.input_views.push((key, view.clone()));
            return Ok(view);
        }
        // Not readable by the video processor: copy into a texture that is.
        if self.intermediate.is_none() {
            let mut d = D3D11_TEXTURE2D_DESC::default();
            source.GetDesc(&mut d);
            self.intermediate = Some(texture(
                device,
                (d.Width, d.Height),
                d.Format,
                D3D11_USAGE_DEFAULT,
                D3D11_BIND_FLAG(D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0),
                D3D11_CPU_ACCESS_FLAG(0),
            )?);
        }
        let mid = self.intermediate.clone().unwrap();
        context.CopyResource(&mid, source);
        self.intermediate_copies += 1;
        let key = mid.as_raw() as usize;
        if let Some((_, v)) = self.input_views.iter().find(|(k, _)| *k == key) {
            return Ok(v.clone());
        }
        let mut view = None;
        self.video_device.CreateVideoProcessorInputView(
            &mid,
            &self.enumerator,
            &desc,
            Some(&mut view),
        )?;
        let view = view.unwrap();
        self.input_views.push((key, view.clone()));
        Ok(view)
    }

    /// The same conversion into another NV12 texture of the output size
    /// (created with D3D11_BIND_RENDER_TARGET), e.g. one slot of an
    /// encoder's input ring that the encoder may still be reading.
    pub unsafe fn convert_to(
        &mut self,
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        source: &ID3D11Texture2D,
        target: &ID3D11Texture2D,
    ) -> Result<()> {
        let key = target.as_raw() as usize;
        let output = match self.target_views.iter().find(|(k, _)| *k == key) {
            Some((_, v)) => v.clone(),
            None => {
                let mut view = None;
                self.video_device.CreateVideoProcessorOutputView(
                    target,
                    &self.enumerator,
                    &D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                        ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                        Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                            Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
                        },
                    },
                    Some(&mut view),
                )?;
                let view = view.unwrap();
                self.target_views.push((key, view.clone()));
                view
            }
        };
        let input = self.input_view(device, context, source)?;
        let mut stream = D3D11_VIDEO_PROCESSOR_STREAM {
            Enable: true.into(),
            pInputSurface: std::mem::ManuallyDrop::new(Some(input)),
            ..Default::default()
        };
        let result = self.video_context.VideoProcessorBlt(
            &self.processor,
            &output,
            0,
            std::slice::from_ref(&stream),
        );
        std::mem::ManuallyDrop::drop(&mut stream.pInputSurface);
        result?;
        Ok(())
    }

    /// One fixed-function scale and colour conversion into the NV12 frame.
    pub unsafe fn convert(
        &mut self,
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        source: &ID3D11Texture2D,
    ) -> Result<()> {
        let view = self.input_view(device, context, source)?;
        let mut stream = D3D11_VIDEO_PROCESSOR_STREAM {
            Enable: true.into(),
            pInputSurface: std::mem::ManuallyDrop::new(Some(view)),
            ..Default::default()
        };
        let result = self.video_context.VideoProcessorBlt(
            &self.processor,
            &self.output_view,
            0,
            std::slice::from_ref(&stream),
        );
        std::mem::ManuallyDrop::drop(&mut stream.pInputSurface);
        result?;
        Ok(())
    }

    /// Queue the NV12 frame for readback, and move the oldest finished one
    /// to the NVIDIA GPU. Never waits for the capture GPU.
    pub unsafe fn transfer(&mut self, context: &ID3D11DeviceContext) -> Result<()> {
        let Some(t) = self.transfer.as_mut() else {
            return Ok(());
        };
        if t.in_flight.len() == t.staging.len() {
            // The ring is full: the oldest readback is still not done.
            t.in_flight.pop_front();
            t.not_ready += 1;
        }
        context.CopyResource(&t.staging[t.next], &self.nv12);
        t.in_flight.push_back(t.next);
        t.next = (t.next + 1) % t.staging.len();
        if t.in_flight.len() < 2 {
            return Ok(());
        }
        let oldest = *t.in_flight.front().unwrap();
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        if context
            .Map(
                &t.staging[oldest],
                0,
                D3D11_MAP_READ,
                D3D11_MAP_FLAG_DO_NOT_WAIT.0 as u32,
                Some(&mut mapped),
            )
            .is_err()
        {
            return Ok(()); // Not finished yet: try again next frame.
        }
        t.in_flight.pop_front();
        let began = Instant::now();
        let (w, h) = (self.out.0 as usize, self.out.1 as usize);
        let pitch = mapped.RowPitch as usize;
        let base = mapped.pData as *const u8;
        // Luma rows, then the interleaved chroma rows (NV12).
        for row in 0..h * 3 / 2 {
            std::ptr::copy_nonoverlapping(
                base.add(row * pitch),
                t.host.as_mut_ptr().add(row * w),
                w,
            );
        }
        context.Unmap(&t.staging[oldest], 0);
        t.context.UpdateSubresource(
            &t.target,
            0,
            None,
            t.host.as_ptr() as *const c_void,
            w as u32,
            0,
        );
        t.context.Flush();
        t.upload_cpu_ms.push(began.elapsed().as_secs_f64() * 1000.);
        t.uploaded += 1;
        Ok(())
    }
}
