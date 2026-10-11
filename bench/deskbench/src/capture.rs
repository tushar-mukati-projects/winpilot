use std::ffi::c_void;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Foundation::{HMODULE, POINT, RECT};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_UNKNOWN};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::WinRT::Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::core::{Interface, Result};

use crate::util::Frame;

pub struct MonitorInfo {
    pub hmon: HMONITOR,
    pub rect: RECT,
}

impl MonitorInfo {
    pub fn width(&self) -> u32 {
        (self.rect.right - self.rect.left) as u32
    }
    pub fn height(&self) -> u32 {
        (self.rect.bottom - self.rect.top) as u32
    }
}

pub fn primary_monitor() -> MonitorInfo {
    unsafe {
        let hmon = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(hmon, &mut mi);
        MonitorInfo { hmon, rect: mi.rcMonitor }
    }
}

pub fn gdi_capture(m: &MonitorInfo) -> Result<Frame> {
    unsafe {
        let (w, h) = (m.width() as i32, m.height() as i32);
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let mut bmi = BITMAPINFO::default();
        bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w;
        bmi.bmiHeader.biHeight = -h;
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB.0;
        let mut bits: *mut c_void = std::ptr::null_mut();
        let hbmp = match CreateDIBSection(Some(mem), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(b) => b,
            Err(e) => {
                let _ = DeleteDC(mem);
                ReleaseDC(None, screen);
                return Err(e);
            }
        };
        let old = SelectObject(mem, HGDIOBJ(hbmp.0));
        let r = BitBlt(mem, 0, 0, w, h, Some(screen), m.rect.left, m.rect.top, SRCCOPY);
        let len = (w * h * 4) as usize;
        let mut data = Vec::<u8>::with_capacity(len);
        if r.is_ok() {
            std::ptr::copy_nonoverlapping(bits as *const u8, data.as_mut_ptr(), len);
            data.set_len(len);
        }
        SelectObject(mem, old);
        let _ = DeleteObject(HGDIOBJ(hbmp.0));
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        r?;
        Ok(Frame { width: w as u32, height: h as u32, bgra: data })
    }
}

fn make_copy(device: &ID3D11Device, src: &ID3D11Texture2D, staging: bool) -> Result<ID3D11Texture2D> {
    unsafe {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        src.GetDesc(&mut desc);
        desc.Usage = if staging { D3D11_USAGE_STAGING } else { D3D11_USAGE_DEFAULT };
        desc.BindFlags = 0;
        desc.CPUAccessFlags = if staging { D3D11_CPU_ACCESS_READ.0 as u32 } else { 0 };
        desc.MiscFlags = 0;
        desc.MipLevels = 1;
        desc.ArraySize = 1;
        desc.SampleDesc.Count = 1;
        desc.SampleDesc.Quality = 0;
        let mut out = None;
        device.CreateTexture2D(&desc, None, Some(&mut out))?;
        Ok(out.unwrap())
    }
}

#[derive(Default)]
pub struct Readback {
    last: Option<ID3D11Texture2D>,
    staging: Option<ID3D11Texture2D>,
}

impl Readback {
    fn store(&mut self, device: &ID3D11Device, ctx: &ID3D11DeviceContext, tex: &ID3D11Texture2D) -> Result<()> {
        if self.last.is_none() {
            self.last = Some(make_copy(device, tex, false)?);
            self.staging = Some(make_copy(device, tex, true)?);
        }
        unsafe { ctx.CopyResource(self.last.as_ref().unwrap(), tex) };
        Ok(())
    }

    fn has_frame(&self) -> bool {
        self.last.is_some()
    }

    fn read(&self, ctx: &ID3D11DeviceContext) -> Result<Frame> {
        let (Some(last), Some(staging)) = (&self.last, &self.staging) else {
            return Err(windows::core::Error::new(DXGI_ERROR_WAIT_TIMEOUT, "no frame captured yet"));
        };
        unsafe {
            ctx.CopyResource(staging, last);
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            staging.GetDesc(&mut desc);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            ctx.Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let row = desc.Width as usize * 4;
            let mut data = Vec::<u8>::with_capacity(row * desc.Height as usize);
            let src = mapped.pData as *const u8;
            for y in 0..desc.Height as usize {
                data.extend_from_slice(std::slice::from_raw_parts(src.add(y * mapped.RowPitch as usize), row));
            }
            ctx.Unmap(staging, 0);
            Ok(Frame { width: desc.Width, height: desc.Height, bgra: data })
        }
    }
}

pub struct Dupl {
    device: ID3D11Device,
    ctx: ID3D11DeviceContext,
    dupl: IDXGIOutputDuplication,
    rb: Readback,
    pub acquires: u32,
    pub zero_lpt: u32,
    pub adapter_name: String,
    pub api: &'static str,
}

pub fn dxgi_open(m: &MonitorInfo) -> Result<Dupl> {
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut i = 0;
        while let Ok(adapter) = factory.EnumAdapters1(i) {
            i += 1;
            let mut j = 0;
            while let Ok(output) = adapter.EnumOutputs(j) {
                j += 1;
                let desc = output.GetDesc()?;
                if desc.Monitor != m.hmon {
                    continue;
                }
                let ad = adapter.GetDesc1()?;
                let adapter_name = crate::util::wide_to_string(&ad.Description);
                let mut device = None;
                let mut ctx = None;
                D3D11CreateDevice(&adapter, D3D_DRIVER_TYPE_UNKNOWN, HMODULE::default(), D3D11_CREATE_DEVICE_BGRA_SUPPORT, None, D3D11_SDK_VERSION, Some(&mut device), None, Some(&mut ctx))?;
                let device = device.unwrap();
                let ctx = ctx.unwrap();
                let (dupl, api) = match output.cast::<IDXGIOutput5>().and_then(|o5| o5.DuplicateOutput1(&device, 0, &[DXGI_FORMAT_B8G8R8A8_UNORM])) {
                    Ok(d) => (d, "DuplicateOutput1"),
                    Err(_) => (output.cast::<IDXGIOutput1>()?.DuplicateOutput(&device)?, "DuplicateOutput"),
                };
                return Ok(Dupl { device, ctx, dupl, rb: Readback::default(), acquires: 0, zero_lpt: 0, adapter_name, api });
            }
        }
        Err(windows::core::Error::new(DXGI_ERROR_NOT_FOUND, "no DXGI output matches the primary monitor"))
    }
}

pub fn dxgi_grab(d: &mut Dupl, timeout_ms: u32) -> Result<(Frame, bool)> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms as u64);
    let mut fresh = false;
    loop {
        let left = deadline.saturating_duration_since(Instant::now()).as_millis() as u32;
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut res: Option<IDXGIResource> = None;
        match unsafe { d.dupl.AcquireNextFrame(left, &mut info, &mut res) } {
            Ok(()) => {
                d.acquires += 1;
                if info.LastPresentTime == 0 {
                    d.zero_lpt += 1;
                }
                if info.LastPresentTime != 0 {
                    let tex: ID3D11Texture2D = res.unwrap().cast()?;
                    d.rb.store(&d.device, &d.ctx, &tex)?;
                    fresh = true;
                }
                unsafe { d.dupl.ReleaseFrame()? };
            }
            Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => {}
            Err(e) => return Err(e),
        }
        if fresh || d.rb.has_frame() || Instant::now() >= deadline {
            break;
        }
    }
    Ok((d.rb.read(&d.ctx)?, fresh))
}

pub struct D3d {
    device: ID3D11Device,
    ctx: ID3D11DeviceContext,
    winrt: IDirect3DDevice,
}

pub fn d3d_default() -> Result<D3d> {
    unsafe {
        let mut device = None;
        let mut ctx = None;
        D3D11CreateDevice(None, D3D_DRIVER_TYPE_HARDWARE, HMODULE::default(), D3D11_CREATE_DEVICE_BGRA_SUPPORT, None, D3D11_SDK_VERSION, Some(&mut device), None, Some(&mut ctx))?;
        let device: ID3D11Device = device.unwrap();
        let dxgi: IDXGIDevice = device.cast()?;
        let winrt: IDirect3DDevice = CreateDirect3D11DeviceFromDXGIDevice(&dxgi)?.cast()?;
        Ok(D3d { device, ctx: ctx.unwrap(), winrt })
    }
}

pub struct Wgc {
    _item: GraphicsCaptureItem,
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    rb: Readback,
    pub border_off: bool,
}

impl Drop for Wgc {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

pub fn wgc_start(d: &D3d, m: &MonitorInfo) -> Result<Wgc> {
    let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
    let item: GraphicsCaptureItem = unsafe { interop.CreateForMonitor(m.hmon)? };
    let size = item.Size()?;
    let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&d.winrt, DirectXPixelFormat::B8G8R8A8UIntNormalized, 2, size)?;
    let session = pool.CreateCaptureSession(&item)?;
    let _ = session.SetIsCursorCaptureEnabled(false);
    let border_off = session.SetIsBorderRequired(false).is_ok();
    session.StartCapture()?;
    Ok(Wgc { _item: item, pool, session, rb: Readback::default(), border_off })
}

pub fn wgc_grab(d: &D3d, w: &mut Wgc, wait: Duration) -> Result<(Frame, bool)> {
    let deadline = Instant::now() + wait;
    let mut latest = None;
    loop {
        while let Ok(f) = w.pool.TryGetNextFrame() {
            latest = Some(f);
        }
        if latest.is_some() || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_micros(250));
    }
    let fresh = latest.is_some();
    if let Some(f) = latest {
        unsafe {
            let access: IDirect3DDxgiInterfaceAccess = f.Surface()?.cast()?;
            let tex: ID3D11Texture2D = access.GetInterface()?;
            w.rb.store(&d.device, &d.ctx, &tex)?;
        }
        let _ = f.Close();
    }
    Ok((w.rb.read(&d.ctx)?, fresh))
}

pub fn xcap_capture() -> std::result::Result<Frame, String> {
    let monitors = xcap::Monitor::all().map_err(|e| e.to_string())?;
    let m = monitors.into_iter().find(|m| m.is_primary().unwrap_or(false)).ok_or("no primary monitor")?;
    let img = m.capture_image().map_err(|e| e.to_string())?;
    let (w, h) = img.dimensions();
    let mut bgra = img.into_raw();
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    Ok(Frame { width: w, height: h, bgra })
}

type Slot = Arc<Mutex<Option<Frame>>>;

struct OneShot {
    slot: Slot,
}

impl windows_capture::capture::GraphicsCaptureApiHandler for OneShot {
    type Flags = Slot;
    type Error = String;

    fn new(ctx: windows_capture::capture::Context<Self::Flags>) -> std::result::Result<Self, Self::Error> {
        Ok(OneShot { slot: ctx.flags })
    }

    fn on_frame_arrived(&mut self, frame: &mut windows_capture::frame::Frame, control: windows_capture::graphics_capture_api::InternalCaptureControl) -> std::result::Result<(), Self::Error> {
        let (w, h) = (frame.width(), frame.height());
        let buf = frame.buffer().map_err(|e| e.to_string())?;
        let mut tmp = Vec::new();
        let data = buf.as_nopadding_buffer(&mut tmp).to_vec();
        *self.slot.lock().unwrap() = Some(Frame { width: w, height: h, bgra: data });
        control.stop();
        Ok(())
    }
}

pub fn windows_capture_wgc() -> std::result::Result<Frame, String> {
    use windows_capture::capture::GraphicsCaptureApiHandler;
    use windows_capture::settings::*;
    let slot: Slot = Arc::new(Mutex::new(None));
    let monitor = windows_capture::monitor::Monitor::primary().map_err(|e| e.to_string())?;
    let settings = Settings::new(
        monitor,
        CursorCaptureSettings::WithoutCursor,
        DrawBorderSettings::WithoutBorder,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Bgra8,
        slot.clone(),
    );
    OneShot::start(settings).map_err(|e| e.to_string())?;
    slot.lock().unwrap().take().ok_or_else(|| "no frame".to_string())
}

pub struct WcDupl(windows_capture::dxgi_duplication_api::DxgiDuplicationApi);

pub fn windows_capture_dxgi_open() -> std::result::Result<WcDupl, String> {
    use windows_capture::dxgi_duplication_api::{DxgiDuplicationApi, DxgiDuplicationFormat};
    let monitor = windows_capture::monitor::Monitor::primary().map_err(|e| e.to_string())?;
    DxgiDuplicationApi::new_options(monitor, &[DxgiDuplicationFormat::Bgra8]).map(WcDupl).map_err(|e| e.to_string())
}

pub fn windows_capture_dxgi_grab(d: &mut WcDupl, timeout_ms: u32) -> std::result::Result<Frame, String> {
    let mut frame = d.0.acquire_next_frame(timeout_ms).map_err(|e| e.to_string())?;
    let buf = frame.buffer().map_err(|e| e.to_string())?;
    let (w, h) = (buf.width(), buf.height());
    let mut tmp = Vec::new();
    let data = buf.as_nopadding_buffer(&mut tmp).to_vec();
    Ok(Frame { width: w, height: h, bgra: data })
}
