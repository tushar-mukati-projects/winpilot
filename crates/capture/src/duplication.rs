mod failure;

use std::time::Instant;

use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_MODE_ROTATION_IDENTITY,
    DXGI_MODE_ROTATION_UNSPECIFIED,
};
use windows::Win32::Graphics::Dxgi::{
    DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO, IDXGIAdapter1, IDXGIOutput, IDXGIOutput1, IDXGIOutput5,
    IDXGIOutputDuplication, IDXGIResource,
};
use windows::core::Interface;

use self::failure::{classify_creation, classify_runtime};
use crate::d3d_device::create_device;
use crate::pixels::{Block, PixelsMut, copy_block};
use crate::staging_texture::{MappedTexture, create_staging, texture_desc};

pub(crate) use self::failure::DxgiFailure;

const POLL_TIMEOUT_MS: u32 = 0;

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
        let (device, context) = create_device(Some(adapter), D3D_DRIVER_TYPE_UNKNOWN)
            .map_err(|error| classify_creation(&error))?
            .ok_or(DxgiFailure::Unsupported)?;
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
        let mapped = MappedTexture::map(&self.context, &staging.texture).map_err(|error| classify_runtime(&error))?;
        copy_block(&mapped.pixels(), from, target, to).map_err(|_| DxgiFailure::Transient)
    }

    fn acquire_latest(&mut self) -> Result<bool, DxgiFailure> {
        let (info, resource) = match next_frame(&self.session) {
            Err(error) if error.code() == DXGI_ERROR_WAIT_TIMEOUT => return Ok(false),
            Err(error) => return Err(classify_runtime(&error)),
            Ok(frame) => frame,
        };
        let stored = if info.LastPresentTime == 0 { Ok(false) } else { self.store(resource) };
        let released = release_frame(&self.session);
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
        let desc = texture_desc(texture);
        if let Some(staging) = &self.staging
            && (staging.width, staging.height) == (desc.Width, desc.Height)
        {
            return Ok(staging.texture.clone());
        }
        if desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM && desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM_SRGB {
            return Err(DxgiFailure::Unsupported);
        }
        let texture = create_staging(&self.device, desc)
            .map_err(|error| classify_runtime(&error))?
            .ok_or(DxgiFailure::Transient)?;
        self.staging = Some(Staging { texture: texture.clone(), width: desc.Width, height: desc.Height });
        Ok(texture)
    }
}

fn next_frame(
    session: &IDXGIOutputDuplication,
) -> windows::core::Result<(DXGI_OUTDUPL_FRAME_INFO, Option<IDXGIResource>)> {
    let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
    let mut resource = None;
    unsafe { session.AcquireNextFrame(POLL_TIMEOUT_MS, &raw mut info, &raw mut resource) }?;
    Ok((info, resource))
}

fn release_frame(session: &IDXGIOutputDuplication) -> windows::core::Result<()> {
    unsafe { session.ReleaseFrame() }
}

fn duplicate_output(output: &IDXGIOutput, device: &ID3D11Device) -> windows::core::Result<IDXGIOutputDuplication> {
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
