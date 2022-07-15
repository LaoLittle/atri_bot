use bytes::Bytes;
use skia_safe::{Bitmap, BlurStyle, Color, Data, Font, Image, MaskFilter, Paint, Shader, Surface, TextBlob};
use skia_safe::paint::Style;

use crate::data::font::get_dir_font;
use crate::fun::drawmeme::{Meme, MemeArg};

pub struct Zero;

impl Meme for Zero {
    fn draw(args: &[MemeArg]) -> Bitmap {
        unimplemented!()
    }
}

pub fn zero(num: u8, img: &[u8]) -> Option<Image> {
    let data = unsafe { Data::new_bytes(&img) };
    let image = Image::from_encoded(data)?;

    let typeface = get_dir_font(&String::from("MiSans-Regular.ttf")).unwrap_or_default();

    let w21 = (image.width() >> 1) as f32;
    let h21 = (image.height() >> 1) as f32;
    let radius = w21.min(h21) * 0.24;

    let font = Font::from_typeface(typeface, radius * 0.6);

    let text = TextBlob::new(format!("{}%", num), &font)?;

    let info = image.image_info();
    let mut surface = Surface::new_raster(info, info.min_row_bytes(), None)?;
    let mut paint = Paint::default();
    paint.set_color(Color::WHITE);

    let canvas = surface.canvas();
    canvas.clear(Color::BLACK);

    paint.set_alpha(155);
    canvas.draw_image(image, (0.0, 0.0), Some(&paint));

    let filter = MaskFilter::blur(BlurStyle::Solid, radius * 0.2, true)?;
    paint
        .set_alpha(255)
        .set_style(Style::Stroke)
        .set_stroke_width(radius * 0.19)
        .set_mask_filter(filter);
    canvas.draw_circle((w21, h21), radius, &paint);

    paint
        .set_style(Style::Fill)
        .set_mask_filter(None);
    let text_bounds = text.bounds();
    canvas.draw_text_blob(&text, (w21 - text_bounds.width() / 2.0, h21 + text_bounds.height() / 4.0), &paint);

    Some(surface.image_snapshot())
}