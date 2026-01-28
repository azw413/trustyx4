use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::Point,
    text::Text,
    Drawable,
};

use super::geom::Rect;
use super::view::{RenderQueue, UiContext, View};

pub struct TextView<'a> {
    pub text: &'a str,
    pub offset_x: i32,
    pub offset_y: i32,
    pub color: BinaryColor,
    pub refresh: crate::display::RefreshMode,
}

impl<'a> TextView<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            offset_x: 0,
            offset_y: 0,
            color: BinaryColor::Off,
            refresh: crate::display::RefreshMode::Fast,
        }
    }
}

impl View for TextView<'_> {
    fn render(&mut self, ctx: &mut UiContext<'_>, rect: Rect, rq: &mut RenderQueue) {
        let style = MonoTextStyle::new(&FONT_10X20, self.color);
        let pos = Point::new(rect.x + self.offset_x, rect.y + self.offset_y);
        Text::new(self.text, pos, style).draw(ctx.buffers).ok();
        rq.push(rect, self.refresh);
    }
}
