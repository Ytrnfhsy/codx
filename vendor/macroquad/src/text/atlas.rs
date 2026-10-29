use crate::{get_context, get_quad_context, math::Rect, texture::Image, Color};

use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct Sprite {
    pub rect: Rect,
}

#[derive(Debug, Eq, PartialEq, Hash, Clone, Copy)]
pub enum SpriteKey {
    Texture(miniquad::TextureId),
    Id(u64),
}
pub struct Atlas {
    texture: miniquad::TextureId,
    image: Image,
    pub sprites: HashMap<SpriteKey, Sprite>,
    cursor_x: u16,
    cursor_y: u16,
    max_line_height: u16,

    pub dirty: bool,

    filter: miniquad::FilterMode,

    unique_id: u64,
}

impl Drop for Atlas {
    fn drop(&mut self) {
        let ctx = &mut get_context().quad_context;
        ctx.delete_texture(self.texture);
    }
}

impl Atlas {
    // pixel gap between glyphs in the atlas
    const GAP: u16 = 2;
    // well..
    const UNIQUENESS_OFFSET: u64 = 100000;

    pub fn new(ctx: &mut dyn miniquad::RenderingBackend, filter: miniquad::FilterMode) -> Atlas {
        let image = Image::gen_image_color(512, 512, Color::new(0.0, 0.0, 0.0, 0.0));
        let texture = ctx.new_texture_from_rgba8(image.width, image.height, &image.bytes);
        ctx.texture_set_filter(
            texture,
            miniquad::FilterMode::Nearest,
            miniquad::MipmapFilterMode::None,
        );

        Atlas {
            image,
            texture,
            cursor_x: 0,
            cursor_y: 0,
            dirty: false,
            max_line_height: 0,
            sprites: HashMap::new(),
            filter,
            unique_id: Self::UNIQUENESS_OFFSET,
        }
    }

    pub fn new_unique_id(&mut self) -> SpriteKey {
        self.unique_id += 1;

        SpriteKey::Id(self.unique_id)
    }

    pub fn set_filter(&mut self, filter_mode: miniquad::FilterMode) {
        let ctx = get_quad_context();
        self.filter = filter_mode;
        ctx.texture_set_filter(self.texture, filter_mode, miniquad::MipmapFilterMode::None);
    }

    pub fn get(&self, key: SpriteKey) -> Option<Sprite> {
        self.sprites.get(&key).cloned()
    }

    pub const fn width(&self) -> u16 {
        self.image.width
    }

    pub const fn height(&self) -> u16 {
        self.image.height
    }

    pub fn texture(&mut self) -> miniquad::TextureId {
        let ctx = get_quad_context();
        if self.dirty {
            self.dirty = false;
            let (texture_width, texture_height) = ctx.texture_size(self.texture);
            if texture_width != self.image.width as _ || texture_height != self.image.height as _ {
                ctx.delete_texture(self.texture);

                self.texture = ctx.new_texture_from_rgba8(
                    self.image.width,
                    self.image.height,
                    &self.image.bytes[..],
                );
                ctx.texture_set_filter(self.texture, self.filter, miniquad::MipmapFilterMode::None);
            }

            ctx.texture_update(self.texture, &self.image.bytes);
        }

        self.texture
    }

    pub fn get_uv_rect(&self, key: SpriteKey) -> Option<Rect> {
        let ctx = get_quad_context();
        self.get(key).map(|sprite| {
            let (w, h) = ctx.texture_size(self.texture);

            Rect::new(
                sprite.rect.x / w as f32,
                sprite.rect.y / h as f32,
                sprite.rect.w / w as f32,
                sprite.rect.h / h as f32,
            )
        })
    }

    /// Finds room for a `width` x `height` sprite at the cursor (moving to a new row when
    /// the current one is full), or `None` when the atlas is full.
    fn place(&mut self, width: u16, height: u16) -> Option<(u16, u16)> {
        let (w, h) = (self.image.width as u32, self.image.height as u32);
        let gap = Self::GAP as u32;
        let (cx, cy) = (self.cursor_x as u32, self.cursor_y as u32);
        let (width, height) = (width as u32, height as u32);
        // one consistent rule for "fits in this row" (the original checked the row without
        // the gap and the bounds with it, so the atlas could double while half empty)
        let (x, y, line) = if cx + gap + width + gap <= w {
            (cx + gap, cy, self.max_line_height.max(height as u16))
        } else {
            (gap, cy + self.max_line_height as u32 + gap * 2, height as u16)
        };
        if x + width > w || y + height > h {
            return None;
        }
        self.cursor_x = (x + width + gap) as u16;
        self.cursor_y = y as u16;
        self.max_line_height = line;
        Some((x as u16, y as u16))
    }

    fn blit(&mut self, key: SpriteKey, sprite: &Image, x: u16, y: u16) {
        let (width, height) = (sprite.width as u32, sprite.height as u32);
        for j in 0..height {
            for i in 0..width {
                self.image
                    .set_pixel(x as u32 + i, y as u32 + j, sprite.get_pixel(i, j));
            }
        }
        self.sprites.insert(
            key,
            Sprite {
                rect: Rect::new(x as f32, y as f32, width as f32, height as f32),
            },
        );
    }

    /// Patched for Toon Warfare: the atlas grows in a loop instead of recursively (the
    /// recursion could overflow the stack) and never past 8192 px, where `u16` sizes would
    /// wrap around to 0 and crash the renderer.
    pub fn cache_sprite(&mut self, key: SpriteKey, sprite: Image) {
        const MAX: u16 = 8192;
        self.dirty = true;
        if let Some((x, y)) = self.place(sprite.width, sprite.height) {
            self.blit(key, &sprite, x, y);
            return;
        }
        let mut old: Vec<(SpriteKey, Image)> = {
            let old_image = self.image.clone();
            self.sprites
                .drain()
                .map(|(k, s)| (k, old_image.sub_image(s.rect)))
                .collect()
        };
        old.push((key, sprite));
        let mut size = (self.image.width, self.image.height);
        'grow: loop {
            if size.0 >= MAX {
                // full at the largest size: start over with just the newest sprite (the
                // older glyphs are cached again when they are drawn next)
                old.drain(..old.len() - 1);
            } else {
                size = (size.0 * 2, size.1.saturating_mul(2).min(MAX));
            }
            self.image = Image::gen_image_color(size.0, size.1, Color::new(0.0, 0.0, 0.0, 0.0));
            self.sprites.clear();
            self.cursor_x = 0;
            self.cursor_y = 0;
            self.max_line_height = 0;
            for (k, img) in &old {
                match self.place(img.width, img.height) {
                    Some((x, y)) => self.blit(*k, img, x, y),
                    None if size.0 >= MAX && old.len() == 1 => return,
                    None => continue 'grow,
                }
            }
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atlas() -> Atlas {
        Atlas {
            texture: miniquad::TextureId::from_raw_id(miniquad::RawId::OpenGl(0)),
            image: Image::gen_image_color(512, 512, Color::new(0.0, 0.0, 0.0, 0.0)),
            sprites: HashMap::new(),
            cursor_x: 0,
            cursor_y: 0,
            max_line_height: 0,
            dirty: false,
            filter: miniquad::FilterMode::Linear,
            unique_id: Atlas::UNIQUENESS_OFFSET,
        }
    }

    /// Thousands of glyphs of many sizes: the atlas grows (without recursion), stays
    /// within bounds and keeps every glyph, none overlapping.
    #[test]
    fn many_glyphs_fit_without_runaway_growth() {
        let mut a = atlas();
        let mut seed = 12345u64;
        let mut next = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as u16
        };
        for i in 0..6000u64 {
            let (w, h) = (3 + next() % 40, 5 + next() % 48);
            let mut img = Image::gen_image_color(w, h, Color::new(1.0, 1.0, 1.0, 1.0));
            img.set_pixel(0, 0, Color::new(i as f32 / 6000.0, 0.0, 0.0, 1.0));
            a.cache_sprite(SpriteKey::Id(i), img);
        }
        assert_eq!(a.sprites.len(), 6000);
        assert!(a.image.width <= 4096, "atlas {}x{}", a.image.width, a.image.height);
        let rects: Vec<Rect> = a.sprites.values().map(|s| s.rect).collect();
        for r in &rects {
            assert!(r.x + r.w <= a.image.width as f32 && r.y + r.h <= a.image.height as f32);
        }
        // no two glyphs share pixels (checked on a sample, it is quadratic)
        for (i, r) in rects.iter().enumerate().take(400) {
            for q in &rects[i + 1..] {
                assert!(!r.overlaps(q), "{r:?} overlaps {q:?}");
            }
        }
        std::mem::forget(a); // no GL context to free the texture in
    }
}
