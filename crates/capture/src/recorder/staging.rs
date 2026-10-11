use deckhand_core::{Context, Error, Rect, Result};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_TEXTURE2D_DESC,
    D3D11_USAGE_STAGING, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;

use super::layout::Size;

const BYTES_PER_PIXEL: usize = 4;

#[derive(Default)]
pub(crate) struct Staging {
    texture: Option<(ID3D11Texture2D, Size)>,
}

impl Staging {
    pub(crate) fn read(
        &mut self,
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        source: &ID3D11Texture2D,
        area: Rect,
        pixels: &mut Vec<u8>,
    ) -> Result<()> {
        let size = Size::new(area.width as u32, area.height as u32);
        let staging = self.texture_for(device, source, size)?;
        let region = D3D11_BOX {
            left: area.x as u32,
            top: area.y as u32,
            front: 0,
            right: area.right() as u32,
            bottom: area.bottom() as u32,
            back: 1,
        };
        unsafe { context.CopySubresourceRegion(&staging, 0, 0, 0, 0, source, 0, Some(&raw const region)) };
        let mapped = Mapped::map(context, &staging)?;
        mapped.copy_rows(size, pixels);
        Ok(())
    }

    fn texture_for(&mut self, device: &ID3D11Device, source: &ID3D11Texture2D, size: Size) -> Result<ID3D11Texture2D> {
        if let Some((texture, made_for)) = &self.texture
            && *made_for == size
        {
            return Ok(texture.clone());
        }
        let texture = create_staging(device, source, size)?;
        self.texture = Some((texture.clone(), size));
        Ok(texture)
    }
}

pub(crate) fn texture_size(texture: &ID3D11Texture2D) -> Size {
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    unsafe { texture.GetDesc(&raw mut desc) };
    Size::new(desc.Width, desc.Height)
}

fn create_staging(device: &ID3D11Device, source: &ID3D11Texture2D, size: Size) -> Result<ID3D11Texture2D> {
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    unsafe { source.GetDesc(&raw mut desc) };
    desc.Width = size.width;
    desc.Height = size.height;
    desc.MipLevels = 1;
    desc.ArraySize = 1;
    desc.SampleDesc = DXGI_SAMPLE_DESC { Count: 1, Quality: 0 };
    desc.Usage = D3D11_USAGE_STAGING;
    desc.BindFlags = 0;
    desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
    desc.MiscFlags = 0;
    let mut texture = None;
    unsafe { device.CreateTexture2D(&raw const desc, None, Some(&raw mut texture)) }
        .context("creating the recording staging texture")?;
    texture.ok_or_else(|| Error::internal("Direct3D returned no staging texture"))
}

struct Mapped<'a> {
    context: &'a ID3D11DeviceContext,
    texture: &'a ID3D11Texture2D,
    data: *const u8,
    row_pitch: usize,
}

impl<'a> Mapped<'a> {
    fn map(context: &'a ID3D11DeviceContext, texture: &'a ID3D11Texture2D) -> Result<Self> {
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe { context.Map(texture, 0, D3D11_MAP_READ, 0, Some(&raw mut mapped)) }
            .context("reading the captured frame")?;
        let data = mapped.pData.cast_const().cast::<u8>();
        let mapping = Self { context, texture, data, row_pitch: mapped.RowPitch as usize };
        if data.is_null() {
            return Err(Error::internal("Direct3D mapped the captured frame to no memory"));
        }
        Ok(mapping)
    }

    fn copy_rows(&self, size: Size, pixels: &mut Vec<u8>) {
        let row_bytes = size.width as usize * BYTES_PER_PIXEL;
        pixels.clear();
        pixels.reserve(row_bytes * size.height as usize);
        for row in 0..size.height as usize {
            pixels.extend_from_slice(self.row(row, row_bytes));
        }
    }

    fn row(&self, row: usize, row_bytes: usize) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.data.add(row * self.row_pitch), row_bytes) }
    }
}

impl Drop for Mapped<'_> {
    fn drop(&mut self) {
        unsafe { self.context.Unmap(self.texture, 0) };
    }
}
