use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::Graphics::Gdi::HMONITOR;
use windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows_core::Interface;
use winpilot_core::{Context, Error, Result};

use super::gpu::Gpu;
use super::layout::Size;

const PIXEL_FORMAT: DirectXPixelFormat = DirectXPixelFormat::B8G8R8A8UIntNormalized;
const POOL_BUFFERS: i32 = 2;

pub(crate) struct CaptureSession {
    session: GraphicsCaptureSession,
    pool: Direct3D11CaptureFramePool,
    pool_size: SizeInt32,
    closed: Arc<AtomicBool>,
    closed_token: Option<i64>,
    item: GraphicsCaptureItem,
}

pub(crate) struct CapturedFrame {
    frame: Direct3D11CaptureFrame,
}

pub(crate) fn ensure_supported() -> Result<()> {
    match GraphicsCaptureSession::IsSupported() {
        Ok(true) => Ok(()),
        Ok(false) => Err(Error::unsupported("Windows.Graphics.Capture is not available on this system")),
        Err(error) => Err(Error::unsupported(format!("Windows.Graphics.Capture is not available: {error}"))),
    }
}

pub(crate) fn window_item(hwnd: HWND) -> Result<GraphicsCaptureItem> {
    let interop = capture_interop()?;
    unsafe { interop.CreateForWindow(hwnd) }.context("starting a capture of the window")
}

pub(crate) fn monitor_item(monitor: HMONITOR) -> Result<GraphicsCaptureItem> {
    let interop = capture_interop()?;
    unsafe { interop.CreateForMonitor(monitor) }.context("starting a capture of the monitor")
}

fn capture_interop() -> Result<IGraphicsCaptureItemInterop> {
    windows_core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
        .map_err(|error| Error::unsupported(format!("Windows.Graphics.Capture is not available: {error}")))
}

impl CaptureSession {
    pub(crate) fn open(gpu: &Gpu, item: GraphicsCaptureItem, cursor: bool) -> Result<Self> {
        let pool_size = item.Size().context("reading the capture size")?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&gpu.winrt, PIXEL_FORMAT, POOL_BUFFERS, pool_size)
            .context("creating the capture frame pool")?;
        let session = match pool.CreateCaptureSession(&item) {
            Ok(session) => session,
            Err(error) => {
                let _ = pool.Close();
                return Err(error).context("creating the capture session");
            }
        };
        let closed = Arc::new(AtomicBool::new(false));
        let mut opened = Self { session, pool, pool_size, closed, closed_token: None, item };
        opened.closed_token = Some(opened.watch_for_close()?);
        opened.session.SetIsCursorCaptureEnabled(cursor).context("choosing whether to capture the cursor")?;
        opened.session.StartCapture().context("starting the capture session")?;
        Ok(opened)
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    fn watch_for_close(&self) -> Result<i64> {
        let closed = Arc::clone(&self.closed);
        let handler = TypedEventHandler::new(move |_, _| {
            closed.store(true, Ordering::Release);
            Ok(())
        });
        self.item.Closed(&handler).context("watching for the capture source to close")
    }

    pub(crate) fn latest(&self) -> Option<CapturedFrame> {
        let mut latest = None;
        while let Ok(frame) = self.pool.TryGetNextFrame() {
            latest = Some(CapturedFrame { frame });
        }
        latest
    }

    pub(crate) fn follow_size(&mut self, gpu: &Gpu, content: Size) -> Result<()> {
        let wanted = SizeInt32 { Width: content.width as i32, Height: content.height as i32 };
        if wanted == self.pool_size || content.width == 0 || content.height == 0 {
            return Ok(());
        }
        self.pool
            .Recreate(&gpu.winrt, PIXEL_FORMAT, POOL_BUFFERS, wanted)
            .context("resizing the capture frame pool")?;
        self.pool_size = wanted;
        Ok(())
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        if let Some(token) = self.closed_token.take() {
            let _ = self.item.RemoveClosed(token);
        }
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

impl CapturedFrame {
    pub(crate) fn content_size(&self) -> Result<Size> {
        let size = self.frame.ContentSize().context("reading the captured frame size")?;
        Ok(Size::new(size.Width.max(0) as u32, size.Height.max(0) as u32))
    }

    pub(crate) fn texture(&self) -> Result<ID3D11Texture2D> {
        let surface = self.frame.Surface().context("reading the captured surface")?;
        let access: IDirect3DDxgiInterfaceAccess = surface.cast().context("reaching the captured texture")?;
        unsafe { access.GetInterface::<ID3D11Texture2D>() }.context("reaching the captured texture")
    }
}

impl Drop for CapturedFrame {
    fn drop(&mut self) {
        let _ = self.frame.Close();
    }
}
