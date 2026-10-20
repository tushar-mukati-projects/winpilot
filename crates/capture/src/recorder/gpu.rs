use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11DeviceContext};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice;
use windows::core::Interface;
use winpilot_core::{Error, Result};

use crate::d3d_device::create_device;

pub(crate) struct Gpu {
    pub winrt: IDirect3DDevice,
    pub context: ID3D11DeviceContext,
    pub device: ID3D11Device,
}

impl Gpu {
    pub(crate) fn create() -> Result<Self> {
        let (device, context) = create_device(None, D3D_DRIVER_TYPE_HARDWARE)
            .map_err(|error| Error::unsupported(format!("no Direct3D 11 device for screen recording: {error}")))?
            .ok_or_else(|| Error::unsupported("Direct3D 11 returned no device for screen recording"))?;
        let winrt = winrt_device(&device)?;
        Ok(Self { winrt, context, device })
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        let _ = self.winrt.Close();
    }
}

fn winrt_device(device: &ID3D11Device) -> Result<IDirect3DDevice> {
    let unsupported = |error: windows::core::Error| {
        Error::unsupported(format!("Direct3D cannot be shared with Windows.Graphics.Capture: {error}"))
    };
    let dxgi: IDXGIDevice = device.cast().map_err(unsupported)?;
    let inspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }.map_err(unsupported)?;
    inspectable.cast().map_err(unsupported)
}
