use crate::{BORDER, MUTED, PRIMARY, SURFACE};
use iota_wallet_core::display::format_chart_label;
use iced::widget::canvas;
use iced::{mouse, Pixels, Point, Theme};

// -- Balance chart (canvas) --

pub(crate) struct BalanceChart {
    pub(crate) data: Vec<(u64, f64)>,
    cache: canvas::Cache,
}

impl BalanceChart {
    pub(crate) fn new() -> Self {
        Self {
            data: Vec::new(),
            cache: canvas::Cache::default(),
        }
    }

    pub(crate) fn update(&mut self, data: Vec<(u64, f64)>) {
        self.data = data;
        self.cache.clear();
    }

    pub(crate) fn clear(&mut self) {
        self.data.clear();
        self.cache.clear();
    }
}

impl<Message> canvas::Program<Message> for BalanceChart {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: iced::Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let geometry = self.cache.draw(renderer, bounds.size(), |frame| {
            let size = frame.size();
            frame.fill_rectangle(Point::ORIGIN, size, SURFACE);

            if self.data.is_empty() {
                return;
            }

            let pad_left = 50.0_f32;
            let pad_right = 10.0_f32;
            let pad_top = 10.0_f32;
            let pad_bottom = 20.0_f32;
            let w = size.width - pad_left - pad_right;
            let h = size.height - pad_top - pad_bottom;

            let min_bal = self
                .data
                .iter()
                .map(|(_, b)| *b)
                .fold(f64::INFINITY, f64::min);
            let max_bal = self
                .data
                .iter()
                .map(|(_, b)| *b)
                .fold(f64::NEG_INFINITY, f64::max);
            let range = (max_bal - min_bal).max(0.001);

            let n = self.data.len();

            // Grid lines + Y labels
            for i in 0..=4 {
                let frac = i as f64 / 4.0;
                let val = min_bal + frac * range;
                let y = pad_top + h - (frac as f32 * h);

                let grid = canvas::Path::line(Point::new(pad_left, y), Point::new(pad_left + w, y));
                frame.stroke(
                    &grid,
                    canvas::Stroke::default().with_color(BORDER).with_width(0.5),
                );

                let label = format_chart_label(val);
                frame.fill_text(canvas::Text {
                    content: label,
                    position: Point::new(2.0, y - 6.0),
                    color: MUTED,
                    size: Pixels(10.0),
                    ..canvas::Text::default()
                });
            }

            // Balance line + dots
            let divisor = if n > 1 { (n - 1) as f32 } else { 1.0 };
            if n > 1 {
                let line = canvas::Path::new(|b| {
                    for (i, (_, bal)) in self.data.iter().enumerate() {
                        let x = pad_left + (i as f32 / divisor) * w;
                        let y = pad_top + h - (((bal - min_bal) / range) as f32 * h);
                        if i == 0 {
                            b.move_to(Point::new(x, y));
                        } else {
                            b.line_to(Point::new(x, y));
                        }
                    }
                });
                frame.stroke(
                    &line,
                    canvas::Stroke::default()
                        .with_color(PRIMARY)
                        .with_width(2.0),
                );
            }
            for (i, (_, bal)) in self.data.iter().enumerate() {
                let x = pad_left + (i as f32 / divisor) * w;
                let y = pad_top + h - (((bal - min_bal) / range) as f32 * h);
                let dot = canvas::Path::circle(Point::new(x, y), 3.0);
                frame.fill(&dot, PRIMARY);
            }

            // X-axis epoch labels
            if let (Some(first), Some(last)) = (self.data.first(), self.data.last()) {
                frame.fill_text(canvas::Text {
                    content: format!("E{}", first.0),
                    position: Point::new(pad_left, pad_top + h + 4.0),
                    color: MUTED,
                    size: Pixels(10.0),
                    ..canvas::Text::default()
                });
                frame.fill_text(canvas::Text {
                    content: format!("E{}", last.0),
                    position: Point::new(pad_left + w - 20.0, pad_top + h + 4.0),
                    color: MUTED,
                    size: Pixels(10.0),
                    ..canvas::Text::default()
                });
            }
        });

        vec![geometry]
    }
}
