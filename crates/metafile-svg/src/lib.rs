//! Deterministic, self-contained SVG serialization.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use metafile_core::{
    ArcKind, Bitmap, BitmapSampling, Brush, BrushStyle, Color, HorizontalTextAlignment,
    MetafileError, Pen, PenStyle, Point, Rect, Renderer, ResourceLimits, Result, TextRun,
    VerticalTextAlignment,
};
use std::collections::BTreeMap;
use std::fmt::Write as _;

#[derive(Debug)]
pub struct SvgRenderer {
    elements: Vec<String>,
    definitions: Vec<String>,
    bounds: Option<Rect>,
    next_clip_id: u32,
    clip_ids: BTreeMap<String, u32>,
    output_bytes: usize,
    limits: ResourceLimits,
}

impl SvgRenderer {
    #[must_use]
    pub fn new(limits: ResourceLimits) -> Self {
        Self {
            elements: Vec::new(),
            definitions: Vec::new(),
            bounds: None,
            next_clip_id: 0,
            clip_ids: BTreeMap::new(),
            output_bytes: 0,
            limits,
        }
    }

    #[must_use]
    pub const fn drawing_bounds(&self) -> Option<Rect> {
        self.bounds
    }

    pub fn finish(
        self,
        preferred_bounds: Option<Rect>,
        physical_size: Option<(f64, f64)>,
    ) -> Result<String> {
        let bounds = preferred_bounds
            .or(self.bounds)
            .unwrap_or(Rect {
                left: 0.0,
                top: 0.0,
                right: 1.0,
                bottom: 1.0,
            })
            .normalized();
        let width = bounds.width().max(1.0);
        let height = bounds.height().max(1.0);
        let (display_w, display_h) = physical_size.unwrap_or((width, height));
        if !bounds.is_finite()
            || !display_w.is_finite()
            || !display_h.is_finite()
            || display_w <= 0.0
            || display_h <= 0.0
        {
            return Err(MetafileError::SvgGeneration(
                "non-finite or non-positive SVG bounds".into(),
            ));
        }
        if display_w > f64::from(self.limits.max_dimension)
            || display_h > f64::from(self.limits.max_dimension)
        {
            return Err(MetafileError::ResourceLimitExceeded {
                resource: "SVG dimensions",
                actual: width.max(height).ceil() as u64,
                limit: u64::from(self.limits.max_dimension),
            });
        }
        let pixels = (display_w.ceil() as u64).saturating_mul(display_h.ceil() as u64);
        if pixels > self.limits.max_pixels {
            return Err(MetafileError::ResourceLimitExceeded {
                resource: "SVG bounds-equivalent pixels",
                actual: pixels,
                limit: self.limits.max_pixels,
            });
        }
        let mut svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\">", n(display_w), n(display_h), n(bounds.left), n(bounds.top), n(width), n(height));
        if !self.definitions.is_empty() {
            svg.push_str("<defs>");
            for d in self.definitions {
                svg.push_str(&d);
            }
            svg.push_str("</defs>");
        }
        for e in self.elements {
            svg.push_str(&e);
        }
        svg.push_str("</svg>");
        if svg.len() > self.limits.max_svg_bytes {
            return Err(MetafileError::ResourceLimitExceeded {
                resource: "SVG output bytes",
                actual: svg.len() as u64,
                limit: self.limits.max_svg_bytes as u64,
            });
        }
        Ok(svg)
    }

    fn push(&mut self, element: String, bounds: Rect) -> Result<()> {
        if !bounds.is_finite() {
            return Err(MetafileError::SvgGeneration(
                "non-finite element bounds".into(),
            ));
        }
        self.output_bytes = self.output_bytes.checked_add(element.len()).ok_or(
            MetafileError::ResourceLimitExceeded {
                resource: "SVG output bytes",
                actual: u64::MAX,
                limit: self.limits.max_svg_bytes as u64,
            },
        )?;
        if self.output_bytes > self.limits.max_svg_bytes {
            return Err(MetafileError::ResourceLimitExceeded {
                resource: "SVG output bytes",
                actual: self.output_bytes as u64,
                limit: self.limits.max_svg_bytes as u64,
            });
        }
        self.bounds = Some(self.bounds.map_or(bounds.normalized(), |b| b.union(bounds)));
        self.elements.push(element);
        Ok(())
    }

    fn clip_attr(&mut self, clip: Option<Rect>) -> String {
        let Some(r) = clip else {
            return String::new();
        };
        let r = r.normalized();
        let key = format!("{}:{}:{}:{}", n(r.left), n(r.top), n(r.right), n(r.bottom));
        if let Some(id) = self.clip_ids.get(&key) {
            return format!(" clip-path=\"url(#clip{id})\"");
        }
        let id = self.next_clip_id;
        self.next_clip_id += 1;
        self.clip_ids.insert(key, id);
        self.definitions.push(format!("<clipPath id=\"clip{id}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/></clipPath>", n(r.left), n(r.top), n(r.width()), n(r.height())));
        format!(" clip-path=\"url(#clip{id})\"")
    }
}

impl Renderer for SvgRenderer {
    fn drawing_bounds(&self) -> Option<Rect> {
        self.bounds
    }
    fn line(&mut self, from: Point, to: Point, pen: &Pen, clip: Option<Rect>) -> Result<()> {
        let clip = self.clip_attr(clip);
        let style = paint(
            pen,
            &Brush {
                style: BrushStyle::Null,
                color: Color::BLACK,
            },
            1,
        );
        self.push(
            format!(
                "<path d=\"M {} {} L {} {}\" {style}{clip}/>",
                n(from.x),
                n(from.y),
                n(to.x),
                n(to.y)
            ),
            stroke_bounds(points_bounds(&[from, to]), pen),
        )
    }
    fn polyline(&mut self, points: &[Point], pen: &Pen, clip: Option<Rect>) -> Result<()> {
        if points.is_empty() {
            return Ok(());
        }
        let clip = self.clip_attr(clip);
        self.push(
            format!(
                "<polyline points=\"{}\" {}{clip}/>",
                point_list(points),
                paint(
                    pen,
                    &Brush {
                        style: BrushStyle::Null,
                        color: Color::BLACK
                    },
                    1
                )
            ),
            stroke_bounds(points_bounds(points), pen),
        )
    }
    fn polygon(
        &mut self,
        points: &[Point],
        pen: &Pen,
        brush: &Brush,
        fill_mode: u16,
        clip: Option<Rect>,
    ) -> Result<()> {
        if points.is_empty() {
            return Ok(());
        }
        let clip = self.clip_attr(clip);
        let rule = if fill_mode == 2 { "nonzero" } else { "evenodd" };
        self.push(
            format!(
                "<polygon points=\"{}\" {} fill-rule=\"{rule}\"{clip}/>",
                point_list(points),
                paint(pen, brush, fill_mode)
            ),
            stroke_bounds(points_bounds(points), pen),
        )
    }
    fn poly_polygon(
        &mut self,
        polygons: &[Vec<Point>],
        pen: &Pen,
        brush: &Brush,
        fill_mode: u16,
        clip: Option<Rect>,
    ) -> Result<()> {
        let points: Vec<Point> = polygons.iter().flatten().copied().collect();
        if points.is_empty() {
            return Ok(());
        }
        let mut path = String::new();
        for polygon in polygons {
            if let Some(first) = polygon.first() {
                write!(path, "M {} {}", n(first.x), n(first.y))
                    .map_err(|e| MetafileError::SvgGeneration(e.to_string()))?;
                for point in &polygon[1..] {
                    write!(path, " L {} {}", n(point.x), n(point.y))
                        .map_err(|e| MetafileError::SvgGeneration(e.to_string()))?;
                }
                path.push_str(" Z ");
            }
        }
        let clip = self.clip_attr(clip);
        self.push(
            format!(
                "<path d=\"{}\" {}{clip}/>",
                path.trim(),
                paint(pen, brush, fill_mode)
            ),
            stroke_bounds(points_bounds(&points), pen),
        )
    }
    fn rectangle(
        &mut self,
        rect: Rect,
        radius: Option<Point>,
        pen: &Pen,
        brush: &Brush,
        clip: Option<Rect>,
    ) -> Result<()> {
        let r = rect.normalized();
        let clip = self.clip_attr(clip);
        let radius = radius.map_or(String::new(), |p| {
            format!(
                " rx=\"{}\" ry=\"{}\"",
                n(p.x.abs() / 2.0),
                n(p.y.abs() / 2.0)
            )
        });
        self.push(
            format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{radius} {}{clip}/>",
                n(r.left),
                n(r.top),
                n(r.width()),
                n(r.height()),
                paint(pen, brush, 1)
            ),
            stroke_bounds(r, pen),
        )
    }
    fn ellipse(&mut self, rect: Rect, pen: &Pen, brush: &Brush, clip: Option<Rect>) -> Result<()> {
        let r = rect.normalized();
        let clip = self.clip_attr(clip);
        self.push(
            format!(
                "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\" {}{clip}/>",
                n(r.left.midpoint(r.right)),
                n(r.top.midpoint(r.bottom)),
                n(r.width() / 2.0),
                n(r.height() / 2.0),
                paint(pen, brush, 1)
            ),
            stroke_bounds(r, pen),
        )
    }
    fn arc(
        &mut self,
        rect: Rect,
        start: Point,
        end: Point,
        kind: ArcKind,
        clockwise: bool,
        pen: &Pen,
        brush: &Brush,
        clip: Option<Rect>,
    ) -> Result<()> {
        let r = rect.normalized();
        let cx = r.left.midpoint(r.right);
        let cy = r.top.midpoint(r.bottom);
        let rx = r.width() / 2.0;
        let ry = r.height() / 2.0;
        let angle =
            |p: Point| ((p.y - cy) / ry.max(f64::EPSILON)).atan2((p.x - cx) / rx.max(f64::EPSILON));
        let a0 = angle(start);
        let a1 = angle(end);
        let p0 = Point::new(cx + rx * a0.cos(), cy + ry * a0.sin());
        let p1 = Point::new(cx + rx * a1.cos(), cy + ry * a1.sin());
        let mut delta = if clockwise { a1 - a0 } else { a0 - a1 };
        if delta <= 0.0 {
            delta += std::f64::consts::TAU;
        }
        let large = i32::from(delta > std::f64::consts::PI);
        let mut d = format!("M {} {}", n(p0.x), n(p0.y));
        if kind == ArcKind::Pie {
            d = format!("M {} {} L {} {}", n(cx), n(cy), n(p0.x), n(p0.y));
        }
        let sweep = i32::from(clockwise);
        if (p0.x - p1.x).abs() < 0.000_001 && (p0.y - p1.y).abs() < 0.000_001 {
            let opposite = Point::new(2.0 * cx - p0.x, 2.0 * cy - p0.y);
            write!(
                d,
                " A {} {} 0 0 {sweep} {} {} A {} {} 0 0 {sweep} {} {}",
                n(rx),
                n(ry),
                n(opposite.x),
                n(opposite.y),
                n(rx),
                n(ry),
                n(p1.x),
                n(p1.y)
            )
            .map_err(|e| MetafileError::SvgGeneration(e.to_string()))?;
        } else {
            write!(
                d,
                " A {} {} 0 {large} {sweep} {} {}",
                n(rx),
                n(ry),
                n(p1.x),
                n(p1.y)
            )
            .map_err(|e| MetafileError::SvgGeneration(e.to_string()))?;
        }
        if matches!(kind, ArcKind::Pie | ArcKind::Chord) {
            d.push_str(" Z");
        }
        let actual_brush = if kind == ArcKind::Arc {
            Brush {
                style: BrushStyle::Null,
                color: brush.color,
            }
        } else {
            brush.clone()
        };
        let clip = self.clip_attr(clip);
        self.push(
            format!("<path d=\"{d}\" {}{clip}/>", paint(pen, &actual_brush, 1)),
            stroke_bounds(r, pen),
        )
    }
    fn pixel(&mut self, point: Point, color: Color, clip: Option<Rect>) -> Result<()> {
        let c = self.clip_attr(clip);
        self.push(
            format!(
                "<rect x=\"{}\" y=\"{}\" width=\"1\" height=\"1\" fill=\"{}\"{c}/>",
                n(point.x),
                n(point.y),
                color.css()
            ),
            Rect {
                left: point.x,
                top: point.y,
                right: point.x + 1.0,
                bottom: point.y + 1.0,
            },
        )
    }
    fn text(&mut self, run: &TextRun) -> Result<()> {
        let anchor = match run.horizontal_align {
            HorizontalTextAlignment::Left => "start",
            HorizontalTextAlignment::Center => "middle",
            HorizontalTextAlignment::Right => "end",
        };
        let baseline = match run.vertical_align {
            VerticalTextAlignment::Top => "text-before-edge",
            VerticalTextAlignment::Bottom => "text-after-edge",
            VerticalTextAlignment::Baseline => "alphabetic",
        };
        let decoration = match (run.font.underline, run.font.strike_out) {
            (true, true) => "underline line-through",
            (true, false) => "underline",
            (false, true) => "line-through",
            _ => "none",
        };
        let transform = if run.font.escapement_tenths != 0 {
            format!(
                " transform=\"rotate({} {} {})\"",
                n(-f64::from(run.font.escapement_tenths) / 10.0),
                n(run.position.x),
                n(run.position.y)
            )
        } else {
            String::new()
        };
        let clip = self.clip_attr(run.clip);
        let size = run.font.height.abs().max(1.0);
        let estimated_width = if run.dx.is_empty() {
            size * run.text.chars().count() as f64 * 0.6
        } else {
            run.dx.iter().sum::<f64>().abs()
        };
        let (text_left, text_right) = match run.horizontal_align {
            HorizontalTextAlignment::Left => (run.position.x, run.position.x + estimated_width),
            HorizontalTextAlignment::Center => (
                run.position.x - estimated_width / 2.0,
                run.position.x + estimated_width / 2.0,
            ),
            HorizontalTextAlignment::Right => (run.position.x - estimated_width, run.position.x),
        };
        let (text_top, text_bottom) = match run.vertical_align {
            VerticalTextAlignment::Top => (run.position.y, run.position.y + size),
            VerticalTextAlignment::Bottom => (run.position.y - size, run.position.y),
            VerticalTextAlignment::Baseline => {
                (run.position.y - size * 0.8, run.position.y + size * 0.2)
            }
        };
        let estimated_bounds = Rect {
            left: text_left,
            top: text_top,
            right: text_right,
            bottom: text_bottom,
        };
        if let (Some(bg), Some(background_rect)) = (run.background, run.background_rect) {
            let background_rect = background_rect.normalized();
            self.push(
                format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"{clip}/>",
                    n(background_rect.left),
                    n(background_rect.top),
                    n(background_rect.width()),
                    n(background_rect.height()),
                    bg.css()
                ),
                background_rect,
            )?;
        } else if let Some(bg) = run.background {
            self.push(
                format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\"{clip}/>",
                    n(estimated_bounds.left),
                    n(estimated_bounds.top),
                    n(estimated_bounds.width()),
                    n(estimated_bounds.height()),
                    bg.css()
                ),
                estimated_bounds,
            )?;
        }
        let (content, text_x, effective_anchor) = if run.dx.is_empty() {
            (xml(&run.text), run.position.x, anchor)
        } else {
            let total: f64 = run.dx.iter().sum();
            let start_x = match run.horizontal_align {
                HorizontalTextAlignment::Left => run.position.x,
                HorizontalTextAlignment::Center => run.position.x - total / 2.0,
                HorizontalTextAlignment::Right => run.position.x - total,
            };
            let mut x = start_x;
            let mut spans = String::new();
            for (index, ch) in run.text.chars().enumerate() {
                write!(
                    spans,
                    "<tspan x=\"{}\" y=\"{}\">{}</tspan>",
                    n(x),
                    n(run.position.y),
                    xml(&ch.to_string())
                )
                .map_err(|e| MetafileError::SvgGeneration(e.to_string()))?;
                x += run.dx.get(index).copied().unwrap_or(0.0);
            }
            (spans, start_x, "start")
        };
        self.push(format!("<text x=\"{}\" y=\"{}\" fill=\"{}\" font-family=\"{}\" font-size=\"{}\" font-weight=\"{}\" font-style=\"{}\" text-decoration=\"{decoration}\" text-anchor=\"{effective_anchor}\" dominant-baseline=\"{baseline}\"{transform}{clip}>{content}</text>",n(text_x),n(run.position.y),run.color.css(),xml(&run.font.family),n(size),run.font.weight,if run.font.italic{"italic"}else{"normal"}),estimated_bounds)
    }
    fn bitmap(
        &mut self,
        dest: Rect,
        bitmap: &Bitmap,
        sampling: BitmapSampling,
        clip: Option<Rect>,
    ) -> Result<()> {
        let mut bytes = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut bytes, bitmap.width, bitmap.height);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc
                .write_header()
                .map_err(|e| MetafileError::SvgGeneration(e.to_string()))?;
            writer
                .write_image_data(&bitmap.rgba)
                .map_err(|e| MetafileError::SvgGeneration(e.to_string()))?;
        }
        let r = dest.normalized();
        let sx = if dest.right < dest.left { -1.0 } else { 1.0 };
        let sy = if dest.bottom < dest.top { -1.0 } else { 1.0 };
        let transform = if sx < 0.0 || sy < 0.0 {
            format!(
                " transform=\"translate({} {}) scale({} {})\"",
                n(dest.left),
                n(dest.top),
                n(sx),
                n(sy)
            )
        } else {
            String::new()
        };
        let (x, y) = if transform.is_empty() {
            (r.left, r.top)
        } else {
            (0.0, 0.0)
        };
        let image_rendering = match sampling {
            BitmapSampling::Auto => "",
            BitmapSampling::Pixelated => " image-rendering=\"pixelated\"",
            BitmapSampling::Smooth => " image-rendering=\"smooth\"",
        };
        let clip = self.clip_attr(clip);
        self.push(format!("<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\" href=\"data:image/png;base64,{}\"{image_rendering}{transform}{clip}/>",n(x),n(y),n(r.width()),n(r.height()),STANDARD.encode(bytes)),r)
    }
}

fn paint(pen: &Pen, brush: &Brush, fill_mode: u16) -> String {
    let (stroke, width, dash) = match pen.style {
        PenStyle::Null => ("none".into(), 0.0, String::new()),
        PenStyle::Dash => (
            pen.color.css(),
            pen.width.max(1.0),
            " stroke-dasharray=\"6 4\"".into(),
        ),
        PenStyle::Dot => (
            pen.color.css(),
            pen.width.max(1.0),
            " stroke-dasharray=\"2 3\"".into(),
        ),
        PenStyle::DashDot => (
            pen.color.css(),
            pen.width.max(1.0),
            " stroke-dasharray=\"6 3 2 3\"".into(),
        ),
        PenStyle::DashDotDot => (
            pen.color.css(),
            pen.width.max(1.0),
            " stroke-dasharray=\"6 3 2 3 2 3\"".into(),
        ),
        _ => (pen.color.css(), pen.width.max(1.0), String::new()),
    };
    let fill = match brush.style {
        BrushStyle::Solid => brush.color.css(),
        _ => "none".into(),
    };
    let rule = if fill_mode == 2 { "nonzero" } else { "evenodd" };
    format!("stroke=\"{stroke}\" stroke-width=\"{}\" stroke-linecap=\"butt\" stroke-linejoin=\"miter\"{dash} fill=\"{fill}\" fill-rule=\"{rule}\"",n(width))
}
fn point_list(p: &[Point]) -> String {
    p.iter()
        .map(|p| format!("{},{}", n(p.x), n(p.y)))
        .collect::<Vec<_>>()
        .join(" ")
}
fn points_bounds(p: &[Point]) -> Rect {
    let mut r = Rect {
        left: p[0].x,
        top: p[0].y,
        right: p[0].x,
        bottom: p[0].y,
    };
    for p in &p[1..] {
        r = r.union(Rect {
            left: p.x,
            top: p.y,
            right: p.x,
            bottom: p.y,
        });
    }
    r
}
fn stroke_bounds(rect: Rect, pen: &Pen) -> Rect {
    if pen.style == PenStyle::Null {
        return rect;
    }
    let half = pen.width.max(1.0) / 2.0;
    Rect {
        left: rect.left - half,
        top: rect.top - half,
        right: rect.right + half,
        bottom: rect.bottom + half,
    }
}
fn n(v: f64) -> String {
    if v.abs() < 0.000_000_5 {
        return "0".into();
    }
    let s = format!("{v:.6}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}
fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escapes_xml_and_is_deterministic() {
        let mut r = SvgRenderer::new(ResourceLimits::default());
        r.text(&TextRun {
            position: Point::new(2.0, 12.0),
            text: "a<&\"".into(),
            font: metafile_core::Font::default(),
            color: Color::BLACK,
            background: None,
            background_rect: None,
            horizontal_align: HorizontalTextAlignment::Left,
            vertical_align: VerticalTextAlignment::Top,
            clip: None,
            dx: vec![],
        })
        .unwrap();
        let s = r.finish(None, None).unwrap();
        assert!(s.contains("a&lt;&amp;&quot;"));
        assert!(!s.contains("viewBox=\"2 0.0"));
    }

    #[test]
    fn full_ellipse_arc_uses_two_segments() {
        let mut renderer = SvgRenderer::new(ResourceLimits::default());
        renderer
            .arc(
                Rect {
                    left: 0.0,
                    top: 0.0,
                    right: 100.0,
                    bottom: 50.0,
                },
                Point::new(100.0, 25.0),
                Point::new(100.0, 25.0),
                ArcKind::Arc,
                false,
                &Pen::default(),
                &Brush::default(),
                None,
            )
            .unwrap();
        let svg = renderer.finish(None, None).unwrap();
        assert_eq!(svg.matches(" A 50 25").count(), 2, "{svg}");
    }

    #[test]
    fn bitmap_preserves_negative_destination_extent() {
        let mut renderer = SvgRenderer::new(ResourceLimits::default());
        renderer
            .bitmap(
                Rect {
                    left: 10.0,
                    top: 20.0,
                    right: 0.0,
                    bottom: 10.0,
                },
                &Bitmap {
                    width: 1,
                    height: 1,
                    rgba: vec![255, 0, 0, 255],
                },
                BitmapSampling::Pixelated,
                None,
            )
            .unwrap();
        let svg = renderer.finish(None, None).unwrap();
        assert!(
            svg.contains(
                "image-rendering=\"pixelated\" transform=\"translate(10 20) scale(-1 -1)\""
            ),
            "{svg}"
        );
    }
}
