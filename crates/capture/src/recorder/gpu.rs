use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice;
use windows_core::Interface;
use winpilot_core::{Error, Result};

pub(crate) struct Gpu {
    pub winrt: IDirect3DDevice,
    pub context: ID3D11DeviceContext,
    pub device: ID3D11Device,
}

impl Gpu {
    pub(crate) fn create() -> Result<Self> {
        let (device, context) = hardware_device()?;
        let winrt = winrt_device(&device)?;
        Ok(Self { winrt, context, device })
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        let _ = self.winrt.Close();
    }
}

fn hardware_device() -> Result<(ID3D11Device, ID3D11DeviceContext)> {
    let mut device = None;
    let mut context = None;
    let created = unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&raw mut device),
            None,
            Some(&raw mut context),
        )
    };
    created.map_err(|error| Error::unsupported(format!("no Direct3D 11 device for screen recording: {error}")))?;
    match (device, context) {
        (Some(device), Some(context)) => Ok((device, context)),
        _ => Err(Error::unsupported("Direct3D 11 returned no device for screen recording")),
    }
}

fn winrt_device(device: &ID3D11Device) -> Result<IDirect3DDevice> {
    let unsupported = |error: windows_core::Error| {
        Error::unsupported(format!("Direct3D cannot be shared with Windows.Graphics.Capture: {error}"))
    };
    let dxgi: IDXGIDevice = device.cast().map_err(unsupported)?;
    let inspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }.map_err(unsupported)?;
    inspectable.cast().map_err(unsupported)
}
