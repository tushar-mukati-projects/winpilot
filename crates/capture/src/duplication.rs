use std::time::Instant;

use windows::Win32::Foundation::{E_ACCESSDENIED, E_INVALIDARG, E_OUTOFMEMORY, HMODULE};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE,
    D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
    ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_MODE_ROTATION_IDENTITY,
    DXGI_MODE_ROTATION_UNSPECIFIED, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET, DXGI_ERROR_NOT_CURRENTLY_AVAILABLE,
    DXGI_ERROR_SESSION_DISCONNECTED, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO, IDXGIAdapter1, IDXGIOutput,
    IDXGIOutput1, IDXGIOutput5, IDXGIOutputDuplication, IDXGIResource,
};
use windows_core::{HRESULT, Interface};

use crate::pixels::{BYTES_PER_PIXEL, Block, Pixels, PixelsMut, copy_block};

const POLL_TIMEOUT_MS: u32 = 0;
const TRANSIENT_CODES: [HRESULT; 6] = [
    E_ACCESSDENIED,
    E_INVALIDARG,
    E_OUTOFMEMORY,
    DXGI_ERROR_NOT_CURRENTLY_AVAILABLE,
    DXGI_ERROR_SESSION_DISCONNECTED,
    DXGI_ERROR_WAIT_TIMEOUT,
];
const LOST_CODES: [HRESULT; 3] = [DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DxgiFailure {
    AccessLost,
    Transient,
    Unsupported,
}

fn classify_creation(error: &windows_core::Error) -> DxgiFailure {
    match classify_runtime(error) {
        DxgiFailure::Transient if !TRANSIENT_CODES.contains(&error.code()) => DxgiFailure::Unsupported,
        failure => failure,
    }
}

fn classify_runtime(error: &windows_core::Error) -> DxgiFailure {
    if LOST_CODES.contains(&error.code()) { DxgiFailure::AccessLost } else { DxgiFailure::Transient }
}

struct Staging {
    texture: ID3D11Texture2D,
    width: u32,
    height: u32,
}

pub(crate) struct Duplication {
    opened_at: Instant,
    last_frame_at: Option<Instant>,
    staging: Option<Staging>,
    session: IDXGIOutputDuplication,
    context: ID3D11DeviceContext,
    device: ID3D11Device,
}

impl Duplication {
    pub(crate) fn open(adapter: &IDXGIAdapter1, output: &IDXGIOutput, now: Instant) -> Result<Self, DxgiFailure> {
        let (device, context) = create_device(adapter)?;
        let session = duplicate_output(output, &device).map_err(|error| classify_creation(&error))?;
        ensure_unrotated(&session)?;
        Ok(Self { opened_at: now, last_frame_at: None, staging: None, session, context, device })
    }

    pub(crate) const fn opened_at(&self) -> Instant {
        self.opened_at
    }

    pub(crate) const fn last_frame_at(&self) -> Option<Instant> {
        self.last_frame_at
    }

    pub(crate) fn frame_size(&self) -> Option<(u32, u32)> {
        self.staging.as_ref().map(|staging| (staging.width, staging.height))
    }

    pub(crate) fn poll(&mut self, now: Instant) -> Result<(), DxgiFailure> {
        if self.acquire_latest()? {
            self.last_frame_at = Some(now);
        }
        Ok(())
    }

    pub(crate) fn read(&self, from: Block, target: &mut PixelsMut, to: (usize, usize)) -> Result<(), DxgiFailure> {
        let Some(staging) = &self.staging else {
            return Err(DxgiFailure::Transient);
        };
        let mapped = MappedTexture::map(&self.context, &staging.texture)?;
        let source = Pixels { bytes: mapped.bytes(staging.width, staging.height), stride: mapped.row_pitch };
        copy_block(&source, from, target, to).map_err(|_| DxgiFailure::Transient)
    }

    fn acquire_latest(&mut self) -> Result<bool, DxgiFailure> {
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource: Option<IDXGIResource> = None;
        let acquired = unsafe { self.session.AcquireNextFrame(POLL_TIMEOUT_MS, &raw mut info, &raw mut resource) };
        match acquired {
            Err(error) if error.code() == DXGI_ERROR_WAIT_TIMEOUT => return Ok(false),
            Err(error) => return Err(classify_runtime(&error)),
            Ok(()) => {}
        }
        let stored = if info.LastPresentTime == 0 { Ok(false) } else { self.store(resource) };
        let released = unsafe { self.session.ReleaseFrame() };
        let stored = stored?;
        released.map_err(|error| classify_runtime(&error))?;
        Ok(stored)
    }

    fn store(&mut self, resource: Option<IDXGIResource>) -> Result<bool, DxgiFailure> {
        let Some(resource) = resource else {
            return Ok(false);
        };
        let texture: ID3D11Texture2D = resource.cast().map_err(|error| classify_runtime(&error))?;
        let staging = self.staging_matching(&texture)?;
        unsafe { self.context.CopyResource(&staging, &texture) };
        Ok(true)
    }

    fn staging_matching(&mut self, texture: &ID3D11Texture2D) -> Result<ID3D11Texture2D, DxgiFailure> {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.GetDesc(&raw mut desc) };
        if let Some(staging) = &self.staging
            && (staging.width, staging.height) == (desc.Width, desc.Height)
        {
            return Ok(staging.texture.clone());
        }
        let texture = create_staging(&self.device, desc)?;
        self.staging = Some(Staging { texture: texture.clone(), width: desc.Width, height: desc.Height });
        Ok(texture)
    }
}

fn create_device(adapter: &IDXGIAdapter1) -> Result<(ID3D11Device, ID3D11DeviceContext), DxgiFailure> {
    let mut device = None;
    let mut context = None;
    let created = unsafe {
        D3D11CreateDevice(
            adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&raw mut device),
            None,
            Some(&raw mut context),
        )
    };
    created.map_err(|error| classify_creation(&error))?;
    match (device, context) {
        (Some(device), Some(context)) => Ok((device, context)),
        _ => Err(DxgiFailure::Unsupported),
    }
}

fn duplicate_output(output: &IDXGIOutput, device: &ID3D11Device) -> windows_core::Result<IDXGIOutputDuplication> {
    let formats = [DXGI_FORMAT_B8G8R8A8_UNORM];
    let modern =
        output.cast::<IDXGIOutput5>().and_then(|output| unsafe { output.DuplicateOutput1(device, 0, &formats) });
    if let Ok(duplication) = modern {
        return Ok(duplication);
    }
    let output: IDXGIOutput1 = output.cast()?;
    unsafe { output.DuplicateOutput(device) }
}

fn ensure_unrotated(duplication: &IDXGIOutputDuplication) -> Result<(), DxgiFailure> {
    let rotation = unsafe { duplication.GetDesc() }.Rotation;
    if rotation == DXGI_MODE_ROTATION_IDENTITY || rotation == DXGI_MODE_ROTATION_UNSPECIFIED {
        return Ok(());
    }
    Err(DxgiFailure::Unsupported)
}

fn create_staging(device: &ID3D11Device, mut desc: D3D11_TEXTURE2D_DESC) -> Result<ID3D11Texture2D, DxgiFailure> {
    if desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM && desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM_SRGB {
        return Err(DxgiFailure::Unsupported);
    }
    desc.Usage = D3D11_USAGE_STAGING;
    desc.BindFlags = 0;
    desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
    desc.MiscFlags = 0;
    desc.MipLevels = 1;
    desc.ArraySize = 1;
    desc.SampleDesc = DXGI_SAMPLE_DESC { Count: 1, Quality: 0 };
    let mut staging = None;
    unsafe { device.CreateTexture2D(&raw const desc, None, Some(&raw mut staging)) }
        .map_err(|error| classify_runtime(&error))?;
    staging.ok_or(DxgiFailure::Transient)
}

struct MappedTexture<'a> {
    context: &'a ID3D11DeviceContext,
    texture: &'a ID3D11Texture2D,
    data: *const u8,
    row_pitch: usize,
}

impl<'a> MappedTexture<'a> {
    fn map(context: &'a ID3D11DeviceContext, texture: &'a ID3D11Texture2D) -> Result<Self, DxgiFailure> {
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe { context.Map(texture, 0, D3D11_MAP_READ, 0, Some(&raw mut mapped)) }
            .map_err(|error| classify_runtime(&error))?;
        let data = mapped.pData.cast_const().cast::<u8>();
        let mapping = Self { context, texture, data, row_pitch: mapped.RowPitch as usize };
        if data.is_null() {
            return Err(DxgiFailure::Transient);
        }
        Ok(mapping)
    }

    fn bytes(&self, width: u32, height: u32) -> &[u8] {
        let rows = height.saturating_sub(1) as usize;
        let length = rows * self.row_pitch + width as usize * BYTES_PER_PIXEL;
        unsafe { std::slice::from_raw_parts(self.data, length) }
    }
}

impl Drop for MappedTexture<'_> {
    fn drop(&mut self) {
        unsafe { self.context.Unmap(self.texture, 0) };
    }
}

#[cfg(test)]
mod tests {
    use windows::Win32::Foundation::{E_FAIL, E_NOINTERFACE};
    use windows::Win32::Graphics::Dxgi::DXGI_ERROR_UNSUPPORTED;

    use super::*;

    fn creation(code: HRESULT) -> DxgiFailure {
        classify_creation(&windows_core::Error::from_hresult(code))
    }

    fn runtime(code: HRESULT) -> DxgiFailure {
        classify_runtime(&windows_core::Error::from_hresult(code))
    }

    #[test]
    fn creation_failures_are_permanent_unless_known_transient() {
        assert!(matches!(creation(DXGI_ERROR_UNSUPPORTED), DxgiFailure::Unsupported));
        assert!(matches!(creation(E_NOINTERFACE), DxgiFailure::Unsupported));
        assert!(matches!(creation(E_FAIL), DxgiFailure::Unsupported));
        assert!(matches!(creation(E_ACCESSDENIED), DxgiFailure::Transient));
        assert!(matches!(creation(E_INVALIDARG), DxgiFailure::Transient));
        assert!(matches!(creation(DXGI_ERROR_NOT_CURRENTLY_AVAILABLE), DxgiFailure::Transient));
        assert!(matches!(creation(DXGI_ERROR_ACCESS_LOST), DxgiFailure::AccessLost));
    }

    #[test]
    fn runtime_failures_reopen_or_retry_later() {
        assert!(matches!(runtime(DXGI_ERROR_ACCESS_LOST), DxgiFailure::AccessLost));
        assert!(matches!(runtime(DXGI_ERROR_DEVICE_REMOVED), DxgiFailure::AccessLost));
        assert!(matches!(runtime(E_FAIL), DxgiFailure::Transient));
    }
}
