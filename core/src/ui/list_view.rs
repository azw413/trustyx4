use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::Size,
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::{Point, Primitive},
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
    Drawable,
};

use super::geom::Rect;
use super::view::{RenderQueue, UiContext, View};

pub struct ListItem<'a> {
    pub label: &'a str,
}

pub struct ListView<'a> {
    pub title: Option<&'a str>,
    pub footer: Option<&'a str>,
    pub empty_label: Option<&'a str>,
    pub items: &'a [ListItem<'a>],
    pub selected: usize,
    pub margin_x: i32,
    pub header_y: i32,
    pub list_top: i32,
    pub line_height: i32,
    pub clear: bool,
}

impl<'a> ListView<'a> {
    pub fn new(items: &'a [ListItem<'a>]) -> Self {
        Self {
            title: None,
            footer: None,
            empty_label: None,
            items,
            selected: 0,
            margin_x: 16,
            header_y: 24,
            list_top: 60,
            line_height: 24,
            clear: true,
        }
    }
}

impl View for ListView<'_> {
    fn render(&mut self, ctx: &mut UiContext<'_>, rect: Rect, rq: &mut RenderQueue) {
        if self.clear {
            ctx.buffers.clear(BinaryColor::On).ok();
        }

        let header_style = MonoTextStyle::new(&FONT_10X20, BinaryColor::Off);
        if let Some(title) = self.title {
            Text::new(title, Point::new(self.margin_x, self.header_y), header_style)
                .draw(ctx.buffers)
                .ok();
        }

        if let Some(footer) = self.footer {
            let y = rect.y + rect.h - 16;
            Text::new(footer, Point::new(self.margin_x, y), header_style)
                .draw(ctx.buffers)
                .ok();
        }

        if self.items.is_empty() {
            Text::new(
                self.empty_label.unwrap_or("No items"),
                Point::new(self.margin_x, self.list_top),
                header_style,
            )
            .draw(ctx.buffers)
            .ok();
        } else {
            let max_lines = ((rect.h - self.list_top - 40) / self.line_height).max(1) as usize;
            let start = self.selected.saturating_sub(max_lines / 2);
            let end = (start + max_lines).min(self.items.len());

            for (idx, item) in self.items[start..end].iter().enumerate() {
                let actual_idx = start + idx;
                let y = self.list_top + (idx as i32 * self.line_height);
                if actual_idx == self.selected {
                    Rectangle::new(
                        Point::new(rect.x, y - 18),
                        Size::new(rect.w as u32, self.line_height as u32),
                    )
                    .into_styled(PrimitiveStyle::with_fill(BinaryColor::Off))
                    .draw(ctx.buffers)
                    .ok();
                    let selected_style = MonoTextStyle::new(&FONT_10X20, BinaryColor::On);
                    Text::new(item.label, Point::new(self.margin_x, y), selected_style)
                        .draw(ctx.buffers)
                        .ok();
                } else {
                    Text::new(item.label, Point::new(self.margin_x, y), header_style)
                        .draw(ctx.buffers)
                        .ok();
                }
            }
        }

        rq.push(rect, crate::display::RefreshMode::Fast);
    }
}
