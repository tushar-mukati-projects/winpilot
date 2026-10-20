use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext,
};
use windows::Win32::Graphics::Dxgi::IDXGIAdapter;

pub(crate) fn create_device(
    adapter: Option<&IDXGIAdapter>,
    driver_type: D3D_DRIVER_TYPE,
) -> windows::core::Result<Option<(ID3D11Device, ID3D11DeviceContext)>> {
    let mut device = None;
    let mut context = None;
    unsafe {
        D3D11CreateDevice(
            adapter,
            driver_type,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&raw mut device),
            None,
            Some(&raw mut context),
        )
    }?;
    Ok(device.zip(context))
}
