use breadx::{prelude::*, protocol::xproto};
use breadx::display::AsyncDisplayExt;
use breadx_image::{AsyncDisplayExt as ImageAsyncDisplayExt, Image};
use rusttype::{point, Font, Scale, VMetrics};

use crate::options::{Color, Options};
use crate::ui;

pub struct Canvas {
    image: Image<Vec<u8>>,
    window: xproto::Window,
    width: u16,
    height: u16,
    font: Font<'static>,
    scale: Scale,
    v_metrics: VMetrics,
    gc: xproto::Gcontext,
}

impl Canvas {
    pub async fn new<D: AsyncDisplayExt>(
        display: &mut D,
        window: xproto::Window,
        width: u16,
        height: u16,
        options: &Options,
    ) -> Result<Canvas, Box<dyn std::error::Error>> {
        let depth = display.get_geometry_immediate(window).await?.depth;
        let format = xproto::ImageFormat::Z_PIXMAP;
        let len = breadx_image::storage_bytes(width, height, depth, None, format, 1);
        let storage = vec![0u8; len];
        let image = Image::with_display(storage, width, height, format, depth, display.setup())?;

        let font = ui::text::font(options.font_name.as_deref())?;
        let scale = Scale::uniform(options.font_size as f32);
        let v_metrics = font.v_metrics(scale);

        let pixmap = display.generate_xid().await?;
        let pixmap_gc = display.generate_xid().await?;

        display.create_pixmap_checked(depth, pixmap, window, width, height).await?;
        display.create_gc_checked(
            pixmap_gc,
            pixmap,
            xproto::CreateGCAux::new()
                .foreground(display.default_screen().white_pixel)
                .graphics_exposures(0),
        ).await?;
        display.put_ximage_checked(&image, pixmap, pixmap_gc, 0, 0).await?;

        Ok(Canvas {
            image,
            window,
            width,
            height,
            font,
            scale,
            v_metrics,
            gc: pixmap_gc,
        })
    }

    pub async fn draw<D: AsyncDisplay>(&self, display: &mut D) -> Result<(), Box<dyn std::error::Error>> {
        display.put_ximage_checked(&self.image, self.window, self.gc, 0, 0).await?;
        display.flush().await?;
        Ok(())
    }

    pub fn clear(&mut self) {
        let data = self.image.storage_mut();
        for i in data {
            *i = 0;
        }
    }

    pub fn draw_text(&mut self, input: &str, color: Color, row: u16) {
        self.render_glyphs(0, input, color, row);
    }

    pub fn text_height(&self) -> f32 {
        self.v_metrics.ascent - self.v_metrics.descent + self.v_metrics.line_gap
    }

    pub fn text_rows(&self) -> usize {
        self.height as usize / self.text_height() as usize
    }

    fn render_glyphs(&mut self, offset: u16, text: &str, color: Color, row: u16) {
        let glyphs = self
            .font
            .layout(
                text,
                self.scale,
                point(0.0, self.text_height() * row as f32 + self.v_metrics.ascent),
            )
            .into_iter();

        for glyph in glyphs {
            if let Some(bounding_box) = glyph.pixel_bounding_box() {
                let mut outside = false;
                let margin = 0;
                let dst_x = margin + offset + (bounding_box.min.x as u16);
                let dst_y = margin + (bounding_box.min.y as u16);
                let max_x = self.width - margin * 2;
                let max_y = self.height - margin * 2;
                glyph.draw(|p_x, p_y, v| {
                    let x = dst_x + p_x as u16;
                    let y = dst_y + p_y as u16;
                    if x < max_x && y < max_y {
                        let pixel = (((color.red * v) as u32) << 16u32)
                            | (((color.green * v) as u32) << 8u32)
                            | ((color.blue * v) as u32);
                        self.image.set_pixel(x as usize, y as usize, pixel);
                    } else {
                        outside = true;
                    }
                });
                if outside {
                    break;
                }
            }
        }
    }
}
