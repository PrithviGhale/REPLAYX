//! Opt-in diagnostic trace. No logger/probes are constructed in normal recording.
//! Only a small ID stripe is read back, asynchronously, with bounded staging slots.
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, Write},
    sync::{Arc, Mutex},
};
use windows::core::{Interface, BOOL};
use windows::Win32::{
    Graphics::{
        Direct3D11::*,
        Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_NV12},
    },
    System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency},
};
#[derive(Clone)]
pub struct Log(Arc<Mutex<BufWriter<File>>>);
pub fn qpc() -> i64 {
    unsafe {
        let (mut n, mut f) = (0, 0);
        let _ = QueryPerformanceCounter(&mut n);
        let _ = QueryPerformanceFrequency(&mut f);
        if f == 0 {
            0
        } else {
            (n as i128 * 10_000_000 / f as i128) as i64
        }
    }
}
impl Log {
    pub fn from_args() -> Result<Option<Self>> {
        let args: Vec<_> = std::env::args().collect();
        let Some(i) = args.iter().position(|a| a == "--trace-file") else {
            return Ok(None);
        };
        let path = args.get(i + 1).context("Trace filename required")?;
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        Ok(Some(Self(Arc::new(Mutex::new(BufWriter::with_capacity(
            65536, file,
        ))))))
    }
    pub fn emit(&self, stage: &str, mut value: Value) -> Result<()> {
        value["stage"] = json!(stage);
        value["qpc_hns"] = json!(qpc());
        let mut out = self.0.lock().unwrap();
        serde_json::to_writer(&mut *out, &value)?;
        out.write_all(b"\n")?;
        Ok(())
    }
    pub fn flush(&self) -> Result<()> {
        self.0.lock().unwrap().flush()?;
        Ok(())
    }
}
#[derive(Clone, Copy)]
pub struct Roi {
    x: u32,
    y: u32,
    step: u32,
    block: u32,
    height: u32,
}
impl Roi {
    pub fn from_args() -> Result<Option<Self>> {
        let args: Vec<_> = std::env::args().collect();
        let Some(i) = args.iter().position(|a| a == "--trace-roi") else {
            return Ok(None);
        };
        let n = args
            .get(i + 1)
            .context("ROI x,y,step,block,height required")?
            .split(',')
            .map(str::parse)
            .collect::<std::result::Result<Vec<u32>, _>>()?;
        if n.len() != 5
            || n.iter().any(|v| *v > 8192)
            || n[2] == 0
            || n[3] == 0
            || n[3] > n[2]
            || n[4] == 0
        {
            bail!("Invalid diagnostic stripe ROI")
        }
        Ok(Some(Self {
            x: n[0],
            y: n[1],
            step: n[2],
            block: n[3],
            height: n[4],
        }))
    }
}
/// Diagnostic-only mapping from capture coordinates to a texture. Align the
/// copy rectangle for NV12, but preserve each bit's rational scaled position.
#[derive(Clone, Copy, Debug)]
struct SampleRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}
#[derive(Clone, Copy, Debug)]
struct Layout {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    samples: [SampleRect; 24],
}
impl Layout {
    fn new(roi: Roi, input: (u32, u32), output: (u32, u32)) -> Result<Self> {
        if input.0 == 0
            || input.1 == 0
            || output.0 == 0
            || output.1 == 0
            || roi.x + roi.step * 24 > input.0
            || roi.y + roi.height > input.1
        {
            bail!("Diagnostic stripe outside capture texture")
        }
        let low = |v: u32, dst: u32, src: u32| (v as u64 * dst as u64 / src as u64) as u32;
        let high = |v: u32, dst: u32, src: u32| (v as u64 * dst as u64).div_ceil(src as u64) as u32;
        let x = low(roi.x, output.0, input.0) & !1;
        let y = low(roi.y, output.1, input.1) & !1;
        let right = (high(roi.x + roi.step * 24, output.0, input.0) + 1) & !1;
        let bottom = (high(roi.y + roi.height, output.1, input.1) + 1) & !1;
        if right > output.0 || bottom > output.1 {
            bail!("Aligned diagnostic stripe outside texture")
        }
        let samples = std::array::from_fn(|i| {
            let start = roi.x + i as u32 * roi.step;
            let x0 = low(start, output.0, input.0);
            let y0 = low(roi.y, output.1, input.1);
            SampleRect {
                x: x0 - x,
                y: y0 - y,
                width: high(start + roi.block, output.0, input.0) - x0,
                height: high(roi.y + roi.height, output.1, input.1) - y0,
            }
        });
        Ok(Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
            samples,
        })
    }
}
struct Pending {
    stage: &'static str,
    capture: usize,
    source_hns: i64,
    texture: usize,
}
struct Slot {
    texture: ID3D11Texture2D,
    event: ID3D11Query,
    format: DXGI_FORMAT,
    layout: Layout,
    pending: Option<Pending>,
}
pub struct Probe {
    slots: Vec<Slot>,
    log: Log,
    pub dropped: u64,
    pub completed: u64,
}
impl Probe {
    pub unsafe fn new(
        device: &ID3D11Device,
        roi: Roi,
        input: (u32, u32),
        output: (u32, u32),
        log: Log,
    ) -> Result<Self> {
        let mut slots = Vec::new();
        for (format, size) in [
            (
                windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                input,
            ),
            (DXGI_FORMAT_NV12, output),
        ] {
            let layout = Layout::new(roi, input, size)?;
            for _ in 0..16 {
                let texture = super::common::texture(
                    device,
                    (layout.width, layout.height),
                    format,
                    D3D11_USAGE_STAGING,
                    D3D11_BIND_FLAG(0),
                    D3D11_CPU_ACCESS_READ,
                )?;
                let mut event = None;
                device.CreateQuery(
                    &D3D11_QUERY_DESC {
                        Query: D3D11_QUERY_EVENT,
                        MiscFlags: 0,
                    },
                    Some(&mut event),
                )?;
                slots.push(Slot {
                    texture,
                    event: event.unwrap(),
                    format,
                    layout,
                    pending: None,
                });
            }
        }
        Ok(Self {
            slots,
            log,
            dropped: 0,
            completed: 0,
        })
    }
    pub unsafe fn sample(
        &mut self,
        context: &ID3D11DeviceContext,
        source: &ID3D11Texture2D,
        stage: &'static str,
        capture: usize,
        source_hns: i64,
    ) -> Result<()> {
        self.poll(context)?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        source.GetDesc(&mut desc);
        let Some(slot) = self
            .slots
            .iter_mut()
            .find(|s| s.format == desc.Format && s.pending.is_none())
        else {
            self.dropped += 1;
            self.log.emit(
                "probe_dropped",
                json!({"capture":capture,"probe_stage":stage}),
            )?;
            return Ok(());
        };
        let r = slot.layout;
        if r.x + r.width > desc.Width || r.y + r.height > desc.Height {
            bail!("Diagnostic stripe outside texture")
        }
        let region = D3D11_BOX {
            left: r.x,
            top: r.y,
            front: 0,
            right: r.x + r.width,
            bottom: r.y + r.height,
            back: 1,
        };
        context.CopySubresourceRegion(&slot.texture, 0, 0, 0, 0, source, 0, Some(&region));
        context.End(&slot.event);
        slot.pending = Some(Pending {
            stage,
            capture,
            source_hns,
            texture: source.as_raw() as usize,
        });
        Ok(())
    }
    pub unsafe fn poll(&mut self, context: &ID3D11DeviceContext) -> Result<()> {
        for slot in &mut self.slots {
            let r = slot.layout;
            if slot.pending.is_none() {
                continue;
            }
            // GetData's S_FALSE is a successful HRESULT. Test the event BOOL too.
            let mut ready = BOOL(0);
            context.GetData(
                &slot.event,
                Some(&mut ready as *mut _ as _),
                std::mem::size_of::<BOOL>() as u32,
                D3D11_ASYNC_GETDATA_DONOTFLUSH.0 as u32,
            )?;
            if !ready.as_bool() {
                continue;
            }
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            match context.Map(
                &slot.texture,
                0,
                D3D11_MAP_READ,
                D3D11_MAP_FLAG_DO_NOT_WAIT.0 as u32,
                Some(&mut mapped),
            ) {
                Ok(()) => {}
                Err(e)
                    if e.code() == windows::Win32::Graphics::Dxgi::DXGI_ERROR_WAS_STILL_DRAWING =>
                {
                    continue
                }
                Err(e) => return Err(e.into()),
            }
            let bpp = if slot.format == DXGI_FORMAT_NV12 {
                1usize
            } else {
                4
            };
            let mut id = 0u32;
            let mut means = [0u32; 24];
            for (i, mean) in means.iter_mut().enumerate() {
                let mut sum = 0u64;
                let sample = r.samples[i];
                for y in sample.y..sample.y + sample.height {
                    for x in sample.x..sample.x + sample.width {
                        let offset = y as usize * mapped.RowPitch as usize + x as usize * bpp;
                        sum += *(mapped.pData as *const u8).add(offset) as u64;
                    }
                }
                *mean = (sum / (sample.height * sample.width) as u64) as u32;
                if *mean > 128 {
                    id |= 1 << i;
                }
            }
            context.Unmap(&slot.texture, 0);
            let p = slot.pending.take().unwrap();
            self.completed += 1;
            self.log.emit(p.stage,json!({"capture":p.capture,"capture_hns":p.source_hns,"texture":p.texture,"content_id":id,"means":means}))?;
        }
        Ok(())
    }
    pub fn pending(&self) -> usize {
        self.slots.iter().filter(|s| s.pending.is_some()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn odd_stripes_and_non_integer_scaling_keep_every_bit_in_bounds() {
        let roi = Roi {
            x: 71,
            y: 97,
            step: 36,
            block: 12,
            height: 12,
        };
        for output in [(2048, 1280), (1920, 1080), (1280, 720)] {
            let layout = Layout::new(roi, (2048, 1280), output).unwrap();
            assert_eq!((layout.x | layout.y | layout.width | layout.height) & 1, 0);
            for sample in layout.samples {
                assert!(sample.width > 0 && sample.height > 0);
                assert!(sample.x + sample.width <= layout.width);
                assert!(sample.y + sample.height <= layout.height);
            }
        }
        let layout = Layout::new(roi, (2048, 1280), (1920, 1080)).unwrap();
        assert_eq!(
            (layout.x, layout.y, layout.width, layout.height),
            (66, 80, 812, 12)
        );
        assert_eq!((layout.samples[23].x, layout.samples[23].width), (776, 13));
    }
    #[test]
    fn stripe_validation_rejects_invalid_capture_bounds() {
        let roi = Roi {
            x: 71,
            y: 97,
            step: 36,
            block: 12,
            height: 12,
        };
        assert!(Layout::new(roi, (800, 600), (1920, 1080)).is_err());
        assert!(Layout::new(roi, (0, 0), (1920, 1080)).is_err());
    }
}
