use windows::Win32::Graphics::Direct3D11::{D3D11_BOX, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D};
use winpilot_core::{Context, Error, Rect, Result};

use super::layout::Size;
use crate::pixels::{BYTES_PER_PIXEL, Block, PixelsMut, copy_block};
use crate::staging_texture::{MappedTexture, create_staging, texture_desc};

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
        let mapped = MappedTexture::map(context, &staging).context("reading the captured frame")?;
        let row_bytes = size.width as usize * BYTES_PER_PIXEL;
        pixels.resize(row_bytes * size.height as usize, 0);
        let whole = Block { x: 0, y: 0, width: size.width, height: size.height };
        let mut target = PixelsMut { bytes: pixels, stride: row_bytes };
        copy_block(&mapped.pixels(), whole, &mut target, (0, 0))
    }

    fn texture_for(&mut self, device: &ID3D11Device, source: &ID3D11Texture2D, size: Size) -> Result<ID3D11Texture2D> {
        if let Some((texture, made_for)) = &self.texture
            && *made_for == size
        {
            return Ok(texture.clone());
        }
        let mut desc = texture_desc(source);
        desc.Width = size.width;
        desc.Height = size.height;
        let texture = create_staging(device, desc)
            .context("creating the recording staging texture")?
            .ok_or_else(|| Error::internal("Direct3D returned no staging texture"))?;
        self.texture = Some((texture.clone(), size));
        Ok(texture)
    }
}

pub(crate) fn texture_size(texture: &ID3D11Texture2D) -> Size {
    let desc = texture_desc(texture);
    Size::new(desc.Width, desc.Height)
}
