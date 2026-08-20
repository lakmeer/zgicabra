//! Terminal plotting library for using in CLI applications.
//! Should work well in any unicode terminal with monospaced font.
//!
//! Vendored from [textplots-rs](https://github.com/loony-bean/textplots-rs)
//! (lib.rs + utils.rs + scale.rs combined into a single module), then trimmed
//! to the subset this project actually uses: fixed-range colored line charts
//! positioned at an arbitrary terminal cell.
//!
//! # Usage
//! ```rust
//! use crate::plot::{Chart, ColorPlot, Shape};
//! use rgb::RGB8;
//!
//! let data = [(0.0, 0.0), (1.0, 1.0), (2.0, 0.5)];
//!
//! Chart::new(60, 15, 0.0, 2.0, 0.0, 1.0)
//!     .position(1, 1)
//!     .linecolorplot(&Shape::Lines(&data), RGB8::new(255, 255, 255))
//!     .display();
//! ```

use drawille::Canvas as BrailleCanvas;
use drawille::PixelColor;
use rgb::RGB8;
use std::fmt::{Display, Formatter, Result};

/// Maps `x` from the domain `[d_start, d_end]` to the range `[r_start,
/// r_end]`, clamped to the range.
fn scale(x: f32, d_start: f32, d_end: f32, r_start: f32, r_end: f32) -> f32 {
    let p = (x - d_start) / (d_end - d_start);
    (r_start + p * (r_end - r_start)).max(r_start).min(r_end)
}

/// Controls the drawing.
pub struct Chart<'a> {
    /// Canvas width in points (2 points per terminal character).
    width: u32,
    /// Canvas height in points (4 points per terminal character).
    height: u32,
    /// X-axis start value.
    xmin: f32,
    /// X-axis end value.
    xmax: f32,
    /// Y-axis start value.
    ymin: f32,
    /// Y-axis end value.
    ymax: f32,
    /// Collection of shapes to be presented on the canvas.
    shapes: Vec<(&'a Shape<'a>, RGB8)>,
    /// Underlying canvas object.
    canvas: BrailleCanvas,
    /// Terminal column the chart is drawn at (1-based).
    screen_x: u16,
    /// Terminal row the chart is drawn at (1-based).
    screen_y: u16,
}

/// Specifies different kinds of plotted data.
pub enum Shape<'a> {
    /// Points connected with lines.
    Lines(&'a [(f32, f32)]),
}

/// Provides an interface for drawing colored plots.
pub trait ColorPlot<'a> {
    /// Draws a [line chart](https://en.wikipedia.org/wiki/Line_chart) of points connected by straight line segments using the specified color
    fn linecolorplot(&'a mut self, shape: &'a Shape, color: RGB8) -> &'a mut Chart<'a>;
}

impl<'a> Display for Chart<'a> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        // get frame and replace space with U+2800 (BRAILLE PATTERN BLANK)
        let frame = self.canvas.frame().replace(' ', "\u{2800}");

        // Re-anchor every line to its terminal column so the chart draws
        // correctly at any x offset (a bare "\n" otherwise resets the
        // cursor to column 1).
        for (i, line) in frame.lines().enumerate() {
            writeln!(
                f,
                "{}{}",
                termion::cursor::Goto(self.screen_x, self.screen_y + i as u16),
                line
            )?;
        }
        Ok(())
    }
}

impl<'a> Chart<'a> {
    /// Creates a new `Chart` object with a fixed y axis range.
    ///
    /// `width` and `height` are given in terminal characters (a braille cell
    /// packs 2x4 points per character).
    ///
    /// # Panics
    ///
    /// Panics if `width` is less than 16 or `height` is less than 1.
    pub fn new(width: u32, height: u32, xmin: f32, xmax: f32, ymin: f32, ymax: f32) -> Self {
        if width < 16 {
            panic!("width should be at least 16 characters");
        }

        if height < 1 {
            panic!("height should be at least 1 character");
        }

        let (width, height) = (width * 2, height * 4);

        Self {
            xmin,
            xmax,
            ymin,
            ymax,
            width,
            height,
            shapes: Vec::new(),
            canvas: BrailleCanvas::new(width, height),
            screen_x: 1,
            screen_y: 1,
        }
    }

    /// Sets the terminal position (1-based column and row) the chart will
    /// draw itself at.
    pub fn position(&'a mut self, x: u16, y: u16) -> &'a mut Chart<'a> {
        self.screen_x = x;
        self.screen_y = y;
        self
    }

    /// Draws horizontal dotted line at the given canvas row (used for the
    /// x-axis).
    fn hline(&mut self, j: u32) {
        if j <= self.height {
            for i in 0..=self.width {
                if i % 3 == 0 {
                    self.canvas.set(i, self.height - j);
                }
            }
        }
    }

    /// Prints canvas content.
    pub fn display(&mut self) {
        self.axis();
        self.figures();

        println!("{}", self);
    }

    /// Shows the x-axis.
    fn axis(&mut self) {
        if self.ymin <= 0.0 && self.ymax >= 0.0 {
            self.hline(scale(0.0, self.ymin, self.ymax, 0.0, self.height as f32) as u32);
        }
    }

    /// Draws all plotted shapes onto the canvas.
    fn figures(&mut self) {
        for (shape, color) in &self.shapes {
            let Shape::Lines(dt) = shape;

            // translate (x, y) points into screen coordinates
            let points: Vec<_> = dt
                .iter()
                .filter_map(|(x, y)| {
                    let i = scale(*x, self.xmin, self.xmax, 0.0, self.width as f32).round() as u32;
                    let j = scale(*y, self.ymin, self.ymax, 0.0, self.height as f32).round() as u32;
                    if i <= self.width && j <= self.height {
                        Some((i, self.height - j))
                    } else {
                        None
                    }
                })
                .collect();

            let pixel_color = PixelColor::TrueColor { r: color.r, g: color.g, b: color.b };

            for pair in points.windows(2) {
                let (x1, y1) = pair[0];
                let (x2, y2) = pair[1];
                self.canvas.line_colored(x1, y1, x2, y2, pixel_color);
            }
        }
    }
}

impl<'a> ColorPlot<'a> for Chart<'a> {
    fn linecolorplot(&'a mut self, shape: &'a Shape, color: RGB8) -> &'a mut Chart<'a> {
        self.shapes.push((shape, color));
        self
    }
}
