//! Turning decoded game textures into GPU images.
//!
//! The PS2 samples textures bilinearly. Bevy's own sampler descriptor defaults to *nearest* filtering, which makes low-resolution game
//! textures look like coarse blocks, so every texture here gets linear filtering, a mip chain (so floors do not shimmer in the distance)
//! and anisotropic filtering (so floors stay sharp at a grazing angle).

use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use gow2_formats::texture::Texture;

/// Halves an RGBA8 image by averaging 2 x 2 blocks, weighting colours by alpha so cut-out edges do not darken.
fn halve(src: &[u8], w: usize, h: usize) -> (Vec<u8>, usize, usize) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = vec![0u8; nw * nh * 4];
    for y in 0..nh {
        for x in 0..nw {
            let (mut r, mut g, mut b, mut a, mut n) = (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (sx, sy) = ((x * 2 + dx).min(w - 1), (y * 2 + dy).min(h - 1));
                let p = &src[(sy * w + sx) * 4..(sy * w + sx) * 4 + 4];
                let al = p[3] as f32;
                r += p[0] as f32 * al;
                g += p[1] as f32 * al;
                b += p[2] as f32 * al;
                a += al;
                n += 1.0;
            }
            let o = &mut out[(y * nw + x) * 4..(y * nw + x) * 4 + 4];
            if a > 0.0 {
                o[0] = (r / a) as u8;
                o[1] = (g / a) as u8;
                o[2] = (b / a) as u8;
            }
            o[3] = (a / n) as u8;
        }
    }
    (out, nw, nh)
}

/// All mip levels of an RGBA8 image, level 0 first, as one buffer, and the level count.
pub fn mip_chain(rgba: &[u8], w: u32, h: u32) -> (Vec<u8>, u32) {
    let (mut data, mut levels) = (rgba.to_vec(), 1);
    let (mut cur, mut cw, mut ch) = (rgba.to_vec(), w as usize, h as usize);
    while cw > 1 || ch > 1 {
        let (next, nw, nh) = halve(&cur, cw, ch);
        data.extend_from_slice(&next);
        cur = next;
        cw = nw;
        ch = nh;
        levels += 1;
    }
    (data, levels)
}

/// The texture padded with transparent pixels at the right and bottom up to power-of-two sizes. The PS2 sets texture sizes as powers of two, and the UI movie's
/// texture coordinates are fractions of that padded size (the 256 x 96 HUD frame uses v from 0 to 0.75 of a 256 x 128 texture), so a UI sprite has to be padded
/// to be sampled where the movie expects.
pub fn pad_to_pow2(t: &Texture) -> Texture {
    let (w, h) = (t.width.next_power_of_two(), t.height.next_power_of_two());
    if (w, h) == (t.width, t.height) {
        return t.clone();
    }
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..t.height as usize {
        let (s, d) = (y * t.width as usize * 4, y * w as usize * 4);
        rgba[d..d + t.width as usize * 4].copy_from_slice(&t.rgba[s..s + t.width as usize * 4]);
    }
    Texture { width: w, height: h, rgba }
}
/// A game texture as a Bevy image. `repeat` tiles it (level and model textures wrap); UI sprites clamp.
pub fn texture_image(t: &Texture, repeat: bool) -> Image {
    let (data, levels) = mip_chain(&t.rgba, t.width, t.height);
    let mut img = Image::new(
        Extent3d { width: t.width, height: t.height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.texture_descriptor.mip_level_count = levels;
    let mode = if repeat { ImageAddressMode::Repeat } else { ImageAddressMode::ClampToEdge };
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: mode,
        address_mode_v: mode,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mip_chain_has_every_level_down_to_one_pixel() {
        let rgba = vec![255u8; 16 * 8 * 4];
        let (data, levels) = mip_chain(&rgba, 16, 8);
        // 16x8, 8x4, 4x2, 2x1, 1x1
        assert_eq!(levels, 5);
        assert_eq!(data.len(), (16 * 8 + 8 * 4 + 4 * 2 + 2 + 1) * 4);
    }

    #[test]
    fn averaging_does_not_darken_the_colour_of_a_cut_out_edge() {
        // two opaque white pixels next to two fully transparent black ones
        let src = [255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0];
        let (out, w, h) = halve(&src, 2, 2);
        assert_eq!((w, h), (1, 1));
        assert_eq!(&out[..3], &[255, 255, 255], "colour weighted by alpha");
        assert_eq!(out[3], 127, "alpha is the plain average");
    }
}

