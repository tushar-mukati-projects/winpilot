use windows::Win32::Foundation::E_POINTER;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CPU_ACCESS_READ, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;

use crate::pixels::{BYTES_PER_PIXEL, Pixels};

pub(crate) struct MappedTexture<'a> {
    context: &'a ID3D11DeviceContext,
    texture: &'a ID3D11Texture2D,
    data: *const u8,
    row_pitch: usize,
    width: u32,
    height: u32,
}

pub(crate) fn texture_desc(texture: &ID3D11Texture2D) -> D3D11_TEXTURE2D_DESC {
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    unsafe { texture.GetDesc(&raw mut desc) };
    desc
}

pub(crate) fn create_staging(
    device: &ID3D11Device,
    mut desc: D3D11_TEXTURE2D_DESC,
) -> windows::core::Result<Option<ID3D11Texture2D>> {
    desc.MipLevels = 1;
    desc.ArraySize = 1;
    desc.SampleDesc = DXGI_SAMPLE_DESC { Count: 1, Quality: 0 };
    desc.Usage = D3D11_USAGE_STAGING;
    desc.BindFlags = 0;
    desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
    desc.MiscFlags = 0;
    let mut texture = None;
    unsafe { device.CreateTexture2D(&raw const desc, None, Some(&raw mut texture)) }?;
    Ok(texture)
}

impl<'a> MappedTexture<'a> {
    pub(crate) fn map(context: &'a ID3D11DeviceContext, texture: &'a ID3D11Texture2D) -> windows::core::Result<Self> {
        let desc = texture_desc(texture);
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe { context.Map(texture, 0, D3D11_MAP_READ, 0, Some(&raw mut mapped)) }?;
        let data = mapped.pData.cast_const().cast::<u8>();
        let row_pitch = mapped.RowPitch as usize;
        let mapping = Self { context, texture, data, row_pitch, width: desc.Width, height: desc.Height };
        if data.is_null() {
            return Err(E_POINTER.into());
        }
        Ok(mapping)
    }

    pub(crate) fn pixels(&self) -> Pixels<'_> {
        let rows = self.height.saturating_sub(1) as usize;
        let length = rows * self.row_pitch + self.width as usize * BYTES_PER_PIXEL;
        Pixels { bytes: unsafe { std::slice::from_raw_parts(self.data, length) }, stride: self.row_pitch }
    }
}

impl Drop for MappedTexture<'_> {
    fn drop(&mut self) {
        unsafe { self.context.Unmap(self.texture, 0) };
    }
}
