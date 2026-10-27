//! Painted skins received from the server: the PNG arrives in chunks, then becomes a
//! texture. Until then (or if it fails) the player keeps the standard look.

use macroquad::prelude::*;
use toon_shared::skin::{tex, Skin};

pub struct SkinAsset {
    pub skin: Skin,
    png: Vec<u8>,
    got: usize,
    pub tex: Option<Texture2D>,
    /// Average colours of the sleeve and hand (for the first-person arms).
    pub sleeve: [u8; 3],
    pub hand: [u8; 3],
    failed: bool,
}

fn average(img: &Image, rect: [f32; 4]) -> [u8; 3] {
    let (w, h) = (img.width() as f32, img.height() as f32);
    let (x0, y0) = ((rect[0] * w) as u32, (rect[1] * h) as u32);
    let (x1, y1) = ((rect[2] * w) as u32, (rect[3] * h) as u32);
    let mut sum = [0u64; 3];
    let mut n = 0u64;
    for y in y0..y1 {
        for x in x0..x1 {
            let c = img.get_pixel(x, y);
            if c.a > 0.5 {
                sum[0] += (c.r * 255.0) as u64;
                sum[1] += (c.g * 255.0) as u64;
                sum[2] += (c.b * 255.0) as u64;
                n += 1;
            }
        }
    }
    if n == 0 {
        return [200, 200, 200];
    }
    [(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8]
}

impl SkinAsset {
    pub fn new(skin: Skin, len: u32) -> SkinAsset {
        SkinAsset {
            skin: skin.sanitized(),
            png: vec![0; (len as usize).min(tex::MAX_PNG)],
            got: 0,
            tex: None,
            sleeve: [200, 200, 200],
            hand: [255, 214, 170],
            failed: false,
        }
    }

    pub fn add(&mut self, offset: u32, data: &[u8]) {
        let o = offset as usize;
        if o + data.len() <= self.png.len() {
            self.png[o..o + data.len()].copy_from_slice(data);
            self.got += data.len();
        }
    }

    /// Turns the finished PNG into a texture (call from the render thread).
    pub fn try_load(&mut self) {
        if self.tex.is_some() || self.failed || self.got < self.png.len() {
            return;
        }
        match Image::from_file_with_format(&self.png, Some(ImageFormat::Png)) {
            Ok(img) if img.width() as u32 == tex::SIZE && img.height() as u32 == tex::SIZE => {
                self.sleeve = average(&img, tex::cell(tex::ARM_RIGHT));
                self.hand = average(&img, tex::half(tex::HANDS_BOOTS, false));
                let t = Texture2D::from_image(&img);
                t.set_filter(FilterMode::Nearest);
                self.tex = Some(t);
                self.png = Vec::new();
            }
            _ => self.failed = true,
        }
    }
}
