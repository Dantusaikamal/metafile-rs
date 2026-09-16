//! Deterministic, self-contained SVG serialization.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use metafile_core::{
    ArcKind, Bitmap, BitmapPlacement, BitmapSampling, Brush, BrushStyle, ClipOperation, ClipRegion,
    Color, HorizontalTextAlignment, LineCap, LineJoin, MetafileError, Paint, Path, PathSegment,
    Pen, PenStyle, Point, Rect, Renderer, ResourceLimits, Result, Stroke, TextRun,
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
    next_mask_id: u32,
    mask_ids: BTreeMap<String, u32>,
    next_paint_id: u32,
    paint_ids: BTreeMap<String, u32>,
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
            next_mask_id: 0,
            mask_ids: BTreeMap::new(),
            next_paint_id: 0,
            paint_ids: BTreeMap::new(),
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
        // Metafile device pixels are not required to be square. SVG's default
        // `meet` behavior would silently preserve the logical viewBox aspect
        // ratio when the declared physical frame has a different ratio.
        let mut svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\" preserveAspectRatio=\"none\">", n(display_w), n(display_h), n(bounds.left), n(bounds.top), n(width), n(height));
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

    fn clip_attr(&mut self, clip: Option<&ClipRegion>) -> String {
        let Some(region) = clip else {
            return String::new();
        };
        if clip_has_boolean(region) {
            let id = self.ensure_mask_id(region);
            format!(" mask=\"url(#mask{id})\"")
        } else {
            let id = self.ensure_clip_id(region);
            format!(" clip-path=\"url(#clip{id})\"")
        }
    }

    fn ensure_mask_id(&mut self, region: &ClipRegion) -> u32 {
        let key = clip_key(region);
        if let Some(id) = self.mask_ids.get(&key) {
            return *id;
        }
        let id = self.next_mask_id;
        self.next_mask_id += 1;
        self.mask_ids.insert(key, id);
        let mask_bounds = clip_region_bounds(region).unwrap_or(Rect {
            left: -f64::from(self.limits.max_dimension),
            top: -f64::from(self.limits.max_dimension),
            right: f64::from(self.limits.max_dimension),
            bottom: f64::from(self.limits.max_dimension),
        });
        let canvas = |fill| mask_rect(mask_bounds, fill);
        let body = match region {
            ClipRegion::Intersection(regions) => {
                let mut body = canvas("white");
                for region in regions {
                    let child = self.ensure_mask_id(region);
                    body = format!("<g mask=\"url(#mask{child})\">{body}</g>");
                }
                body
            }
            ClipRegion::Combine {
                operation,
                left,
                right,
            } => {
                let left_id = self.ensure_mask_id(left);
                let right_id = self.ensure_mask_id(right);
                match operation {
                    ClipOperation::Union => format!(
                        "<g mask=\"url(#mask{left_id})\">{}</g><g mask=\"url(#mask{right_id})\">{}</g>",
                        canvas("white"), canvas("white")
                    ),
                    ClipOperation::Exclude | ClipOperation::Complement => {
                        let (include, subtract) = if *operation == ClipOperation::Complement {
                            (right_id, left_id)
                        } else {
                            (left_id, right_id)
                        };
                        format!(
                            "<g mask=\"url(#mask{include})\">{}<g mask=\"url(#mask{subtract})\">{}</g></g>",
                            canvas("white"), canvas("black")
                        )
                    }
                    ClipOperation::Xor => format!(
                        "<g mask=\"url(#mask{left_id})\">{}</g><g mask=\"url(#mask{right_id})\">{}</g><g mask=\"url(#mask{left_id})\"><g mask=\"url(#mask{right_id})\">{}</g></g>",
                        canvas("white"), canvas("white"), canvas("black")
                    ),
                }
            }
            _ => format!(
                "{}<g fill=\"white\">{}</g>",
                canvas("black"),
                clip_element(region)
            ),
        };
        let mask_bounds = padded_mask_bounds(mask_bounds);
        self.definitions.push(format!(
            "<mask id=\"mask{id}\" maskUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" mask-type=\"luminance\">{body}</mask>",
            n(mask_bounds.left), n(mask_bounds.top), n(mask_bounds.width()), n(mask_bounds.height())
        ));
        id
    }

    fn ensure_clip_id(&mut self, region: &ClipRegion) -> u32 {
        if let ClipRegion::Intersection(regions) = region {
            let mut flattened = Vec::new();
            flatten_clip_regions(regions, &mut flattened);
            if flattened.is_empty() {
                return self.ensure_clip_id(&ClipRegion::Polygon(Vec::new()));
            }
            let mut id = self.ensure_clip_id(flattened[0]);
            let mut compound_key = clip_key(flattened[0]);
            for part in &flattened[1..] {
                compound_key.push('&');
                compound_key.push_str(&clip_key(part));
                if let Some(existing) = self.clip_ids.get(&compound_key) {
                    id = *existing;
                    continue;
                }
                let next_id = self.next_clip_id;
                self.next_clip_id += 1;
                self.clip_ids.insert(compound_key.clone(), next_id);
                self.definitions.push(format!(
                    "<clipPath id=\"clip{next_id}\"><g clip-path=\"url(#clip{id})\">{}</g></clipPath>",
                    clip_element(part)
                ));
                id = next_id;
            }
            return id;
        }
        let key = clip_key(region);
        if let Some(id) = self.clip_ids.get(&key) {
            return *id;
        }
        let id = self.next_clip_id;
        self.next_clip_id += 1;
        self.clip_ids.insert(key, id);
        self.definitions.push(format!(
            "<clipPath id=\"clip{id}\">{}</clipPath>",
            clip_element(region)
        ));
        id
    }

    fn paint_reference(&mut self, paint: &Paint) -> Result<String> {
        if let Paint::Solid(color) = paint {
            return Ok(color.css());
        }
        let key = paint_key(paint);
        if let Some(id) = self.paint_ids.get(&key) {
            return Ok(format!("url(#paint{id})"));
        }
        let id = self.next_paint_id;
        self.next_paint_id += 1;
        self.paint_ids.insert(key, id);
        let definition = match paint {
            Paint::Solid(_) => unreachable!(),
            Paint::Hatch {
                style,
                foreground,
                background,
            } => hatch_definition(id, *style, *foreground, *background),
            Paint::LinearGradient {
                start,
                end,
                stops,
                wrap_mode,
                transform,
                gamma_corrected,
            } => gradient_definition(
                id,
                *start,
                *end,
                stops,
                *wrap_mode,
                *transform,
                *gamma_corrected,
            ),
            Paint::Texture {
                bitmap,
                transform,
                wrap_mode,
                opacity,
                ..
            } => texture_definition(id, bitmap, *transform, *wrap_mode, *opacity)?,
        };
        self.definitions.push(definition);
        Ok(format!("url(#paint{id})"))
    }
}

impl Renderer for SvgRenderer {
    fn drawing_bounds(&self) -> Option<Rect> {
        self.bounds
    }
    fn line(&mut self, from: Point, to: Point, pen: &Pen, clip: Option<&ClipRegion>) -> Result<()> {
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
    fn polyline(&mut self, points: &[Point], pen: &Pen, clip: Option<&ClipRegion>) -> Result<()> {
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
        clip: Option<&ClipRegion>,
    ) -> Result<()> {
        if points.is_empty() {
            return Ok(());
        }
        let clip = self.clip_attr(clip);
        self.push(
            format!(
                "<polygon points=\"{}\" {}{clip}/>",
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
        clip: Option<&ClipRegion>,
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
    fn path(
        &mut self,
        path: &Path,
        pen: &Pen,
        brush: &Brush,
        fill_mode: u16,
        stroke: bool,
        fill: bool,
        clip: Option<&ClipRegion>,
    ) -> Result<()> {
        let mut data = String::new();
        let mut points = Vec::new();
        for figure in &path.figures {
            write!(data, "M {} {}", n(figure.start.x), n(figure.start.y))
                .map_err(|error| MetafileError::SvgGeneration(error.to_string()))?;
            points.push(figure.start);
            for segment in &figure.segments {
                match *segment {
                    PathSegment::Line(to) => {
                        write!(data, " L {} {}", n(to.x), n(to.y))
                            .map_err(|error| MetafileError::SvgGeneration(error.to_string()))?;
                        points.push(to);
                    }
                    PathSegment::Cubic {
                        control1,
                        control2,
                        to,
                    } => {
                        write!(
                            data,
                            " C {} {} {} {} {} {}",
                            n(control1.x),
                            n(control1.y),
                            n(control2.x),
                            n(control2.y),
                            n(to.x),
                            n(to.y)
                        )
                        .map_err(|error| MetafileError::SvgGeneration(error.to_string()))?;
                        points.extend([control1, control2, to]);
                    }
                }
            }
            if figure.closed {
                data.push_str(" Z");
            }
        }
        if points.is_empty() {
            return Ok(());
        }
        let actual_pen = if stroke {
            pen.clone()
        } else {
            Pen {
                style: PenStyle::Null,
                ..pen.clone()
            }
        };
        let actual_brush = if fill {
            brush.clone()
        } else {
            Brush {
                style: BrushStyle::Null,
                ..brush.clone()
            }
        };
        let clip = self.clip_attr(clip);
        self.push(
            format!(
                "<path d=\"{}\" {}{clip}/>",
                data.trim(),
                paint(&actual_pen, &actual_brush, fill_mode)
            ),
            stroke_bounds(points_bounds(&points), &actual_pen),
        )
    }
    fn styled_path(
        &mut self,
        path: &Path,
        stroke: Option<&Stroke>,
        fill: Option<&Paint>,
        fill_mode: u16,
        clip: Option<&ClipRegion>,
    ) -> Result<()> {
        let data = path_data(path)?;
        let points = path_points(path);
        if points.is_empty() {
            return Ok(());
        }
        let fill = match fill {
            Some(paint) => self.paint_reference(paint)?,
            None => "none".into(),
        };
        let (stroke_paint, stroke_width, cap, join, miter, dash, dash_offset) = match stroke {
            None => (
                "none".into(),
                0.0,
                "butt",
                "miter",
                4.0,
                String::new(),
                String::new(),
            ),
            Some(stroke) => {
                let paint = self.paint_reference(&stroke.paint)?;
                let cap = match stroke.line_cap {
                    LineCap::Butt => "butt",
                    LineCap::Round => "round",
                    LineCap::Square => "square",
                };
                let join = match stroke.line_join {
                    LineJoin::Miter => "miter",
                    LineJoin::Round => "round",
                    LineJoin::Bevel => "bevel",
                };
                let dash = (!stroke.dash_pattern.is_empty()).then(|| {
                    format!(
                        " stroke-dasharray=\"{}\"",
                        stroke
                            .dash_pattern
                            .iter()
                            .map(|value| n(*value))
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                });
                let dash_offset = (stroke.dash_offset != 0.0)
                    .then(|| format!(" stroke-dashoffset=\"{}\"", n(stroke.dash_offset)));
                (
                    paint,
                    stroke.width.max(0.0),
                    cap,
                    join,
                    stroke.miter_limit,
                    dash.unwrap_or_default(),
                    dash_offset.unwrap_or_default(),
                )
            }
        };
        let rule = if fill_mode == 2 { "nonzero" } else { "evenodd" };
        let clip = self.clip_attr(clip);
        let bounds = stroke.map_or_else(
            || points_bounds(&points),
            |stroke| {
                let pen = Pen {
                    width: stroke.width,
                    ..Pen::default()
                };
                stroke_bounds(points_bounds(&points), &pen)
            },
        );
        self.push(
            format!(
                "<path d=\"{data}\" stroke=\"{stroke_paint}\" stroke-width=\"{}\" stroke-linecap=\"{cap}\" stroke-linejoin=\"{join}\" stroke-miterlimit=\"{}\"{dash}{dash_offset} fill=\"{fill}\" fill-rule=\"{rule}\"{clip}/>",
                n(stroke_width),
                n(miter)
            ),
            bounds,
        )
    }
    fn rectangle(
        &mut self,
        rect: Rect,
        radius: Option<Point>,
        pen: &Pen,
        brush: &Brush,
        clip: Option<&ClipRegion>,
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
    fn ellipse(
        &mut self,
        rect: Rect,
        pen: &Pen,
        brush: &Brush,
        clip: Option<&ClipRegion>,
    ) -> Result<()> {
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
        clip: Option<&ClipRegion>,
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
    fn pixel(&mut self, point: Point, color: Color, clip: Option<&ClipRegion>) -> Result<()> {
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
            VerticalTextAlignment::Center => "central",
            VerticalTextAlignment::Bottom => "text-after-edge",
            VerticalTextAlignment::Baseline => "alphabetic",
        };
        let decoration = match (run.font.underline, run.font.strike_out) {
            (true, true) => "underline line-through",
            (true, false) => "underline",
            (false, true) => "line-through",
            _ => "none",
        };
        let local_rotation = if run.font.escapement_tenths != 0 {
            format!(
                " rotate({} {} {})",
                n(-f64::from(run.font.escapement_tenths) / 10.0),
                n(run.position.x),
                n(run.position.y)
            )
        } else {
            String::new()
        };
        let transform =
            if run.transform == metafile_core::Transform::IDENTITY && local_rotation.is_empty() {
                String::new()
            } else {
                format!(
                    " transform=\"matrix({} {} {} {} {} {}){}\"",
                    n(run.transform.m11),
                    n(run.transform.m12),
                    n(run.transform.m21),
                    n(run.transform.m22),
                    n(run.transform.dx),
                    n(run.transform.dy),
                    local_rotation
                )
            };
        let clip = self.clip_attr(run.clip.as_ref());
        let size = run.font.height.abs().max(1.0);
        let line_count = run.text.lines().count().max(1);
        let line_advance = size * 1.2;
        let first_line_offset = match run.vertical_align {
            VerticalTextAlignment::Center => {
                -((line_count.saturating_sub(1)) as f64 * line_advance) / 2.0
            }
            VerticalTextAlignment::Bottom => {
                -((line_count.saturating_sub(1)) as f64) * line_advance
            }
            VerticalTextAlignment::Top | VerticalTextAlignment::Baseline => 0.0,
        };
        let estimated_width = if run.dx.is_empty() {
            size * run
                .text
                .lines()
                .map(|line| line.chars().count())
                .max()
                .unwrap_or(0) as f64
                * 0.6
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
            VerticalTextAlignment::Top => {
                (run.position.y, run.position.y + size * line_count as f64)
            }
            VerticalTextAlignment::Center => {
                let height = size + (line_count.saturating_sub(1)) as f64 * line_advance;
                (run.position.y - height / 2.0, run.position.y + height / 2.0)
            }
            VerticalTextAlignment::Bottom => (
                run.position.y - size - (line_count.saturating_sub(1)) as f64 * line_advance,
                run.position.y,
            ),
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
        let output_bounds = transformed_rect_bounds(estimated_bounds, run.transform);
        if let (Some(bg), Some(background_path)) = (run.background, run.background_path.as_ref()) {
            let data = path_data(background_path)?;
            let bounds = path_bounds(background_path)?;
            self.push(
                format!(
                    "<path d=\"{data}\" fill=\"{}\" stroke=\"none\"{clip}/>",
                    bg.css()
                ),
                bounds,
            )?;
        } else if let (Some(bg), Some(background_rect)) = (run.background, run.background_rect) {
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
                output_bounds,
            )?;
        }
        let (content, text_x, effective_anchor) = if run.dx.is_empty() {
            let content = if run.text.contains('\n') {
                run.text
                    .lines()
                    .enumerate()
                    .fold(String::new(), |mut output, (index, line)| {
                        let dy = if index == 0 {
                            n(first_line_offset)
                        } else {
                            n(line_advance)
                        };
                        write!(
                            output,
                            "<tspan x=\"{}\" dy=\"{dy}\">{}</tspan>",
                            n(run.position.x),
                            xml(line)
                        )
                        .expect("writing to String cannot fail");
                        output
                    })
            } else {
                xml(&run.text)
            };
            (content, run.position.x, anchor)
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
        let direction = if run.right_to_left {
            " direction=\"rtl\" unicode-bidi=\"bidi-override\""
        } else {
            ""
        };
        let (open, text_clip, close) = if transform.is_empty() {
            (String::new(), clip, String::new())
        } else {
            (format!("<g{clip}>"), String::new(), "</g>".into())
        };
        self.push(format!("{open}<text x=\"{}\" y=\"{}\" fill=\"{}\" font-family=\"{}\" font-size=\"{}\" font-weight=\"{}\" font-style=\"{}\" text-decoration=\"{decoration}\" text-anchor=\"{effective_anchor}\" dominant-baseline=\"{baseline}\"{direction}{transform}{text_clip}>{content}</text>{close}",n(text_x),n(run.position.y),run.color.css(),xml(&run.font.family),n(size),run.font.weight,if run.font.italic{"italic"}else{"normal"}),output_bounds)
    }
    fn bitmap(
        &mut self,
        placement: BitmapPlacement,
        bitmap: &Bitmap,
        sampling: BitmapSampling,
        clip: Option<&ClipRegion>,
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
        let transform = format!(
            "matrix({} {} {} {} {} {})",
            n(placement.x_axis.x),
            n(placement.x_axis.y),
            n(placement.y_axis.x),
            n(placement.y_axis.y),
            n(placement.origin.x),
            n(placement.origin.y)
        );
        let image_rendering = match sampling {
            BitmapSampling::Auto => "",
            BitmapSampling::Pixelated => " image-rendering=\"pixelated\"",
            BitmapSampling::Smooth => " image-rendering=\"smooth\"",
        };
        let clip = self.clip_attr(clip);
        let corners = placement.corners();
        self.push(
            format!(
                "<image x=\"0\" y=\"0\" width=\"1\" height=\"1\" preserveAspectRatio=\"none\" href=\"data:image/png;base64,{}\"{image_rendering} transform=\"{transform}\"{clip}/>",
                STANDARD.encode(bytes)
            ),
            points_bounds(&corners),
        )
    }
}

fn paint_key(paint: &Paint) -> String {
    match paint {
        Paint::Solid(color) => format!("s:{},{},{},{}", color.r, color.g, color.b, color.a),
        Paint::Hatch {
            style,
            foreground,
            background,
        } => format!(
            "h:{style}:{},{},{},{}:{},{},{},{}",
            foreground.r,
            foreground.g,
            foreground.b,
            foreground.a,
            background.r,
            background.g,
            background.b,
            background.a
        ),
        Paint::LinearGradient {
            start,
            end,
            stops,
            wrap_mode,
            transform,
            gamma_corrected,
        } => format!(
            "g:{}:{}:{}:{}:{wrap_mode}:{}:{}:{}:{}:{}:{}:{}:{gamma_corrected}",
            n(start.x),
            n(start.y),
            n(end.x),
            n(end.y),
            stops
                .iter()
                .map(|stop| format!(
                    "{}:{},{},{},{}",
                    n(stop.offset),
                    stop.color.r,
                    stop.color.g,
                    stop.color.b,
                    stop.color.a
                ))
                .collect::<Vec<_>>()
                .join(";"),
            n(transform.m11),
            n(transform.m12),
            n(transform.m21),
            n(transform.m22),
            n(transform.dx),
            n(transform.dy)
        ),
        Paint::Texture {
            bitmap,
            transform,
            wrap_mode,
            opacity,
            do_not_transform,
        } => format!(
            "t:{}:{}:{wrap_mode}:{}:{}:{}:{}:{}:{}:{}:{do_not_transform}:{}",
            bitmap.width,
            bitmap.height,
            n(*opacity),
            n(transform.m11),
            n(transform.m12),
            n(transform.m21),
            n(transform.m22),
            n(transform.dx),
            n(transform.dy),
            STANDARD.encode(&bitmap.rgba)
        ),
    }
}

fn hatch_definition(id: u32, style: u32, foreground: Color, background: Color) -> String {
    let strokes = match style {
        0 => "M 0 0 H 8 M 0 4 H 8",
        1 => "M 0 0 V 8 M 4 0 V 8",
        2 => "M -2 8 L 8 -2 M 2 10 L 10 2",
        3 => "M -2 0 L 8 10 M 2 -2 L 10 6",
        4 => "M 0 0 H 8 M 0 4 H 8 M 0 0 V 8 M 4 0 V 8",
        _ => "M -2 8 L 8 -2 M 2 10 L 10 2 M -2 0 L 8 10 M 2 -2 L 10 6",
    };
    format!(
        "<pattern id=\"paint{id}\" patternUnits=\"userSpaceOnUse\" width=\"8\" height=\"8\"><rect width=\"8\" height=\"8\" fill=\"{}\"/><path d=\"{strokes}\" fill=\"none\" stroke=\"{}\" stroke-width=\"1\"/></pattern>",
        background.css(),
        foreground.css()
    )
}

fn gradient_definition(
    id: u32,
    start: Point,
    end: Point,
    stops: &[metafile_core::GradientStop],
    wrap_mode: u32,
    transform: metafile_core::Transform,
    gamma_corrected: bool,
) -> String {
    let spread = match wrap_mode {
        0 => "pad",
        1 | 3 => "reflect",
        2 | 4 => "repeat",
        _ => "pad",
    };
    let stops = stops.iter().fold(String::new(), |mut output, stop| {
        write!(
            output,
            "<stop offset=\"{}\" stop-color=\"{}\"/>",
            n(stop.offset.clamp(0.0, 1.0)),
            stop.color.css()
        )
        .expect("writing to String cannot fail");
        output
    });
    format!(
        "<linearGradient id=\"paint{id}\" gradientUnits=\"userSpaceOnUse\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" spreadMethod=\"{spread}\" color-interpolation=\"{}\" gradientTransform=\"matrix({} {} {} {} {} {})\">{stops}</linearGradient>",
        n(start.x),
        n(start.y),
        n(end.x),
        n(end.y),
        if gamma_corrected { "linearRGB" } else { "sRGB" },
        n(transform.m11),
        n(transform.m12),
        n(transform.m21),
        n(transform.m22),
        n(transform.dx),
        n(transform.dy)
    )
}

fn texture_definition(
    id: u32,
    bitmap: &Bitmap,
    transform: metafile_core::Transform,
    wrap_mode: u32,
    opacity: f64,
) -> Result<String> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, bitmap.width, bitmap.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| MetafileError::SvgGeneration(error.to_string()))?;
        writer
            .write_image_data(&bitmap.rgba)
            .map_err(|error| MetafileError::SvgGeneration(error.to_string()))?;
    }
    let (width, height) = match wrap_mode {
        1 => (bitmap.width.saturating_mul(2), bitmap.height),
        2 => (bitmap.width, bitmap.height.saturating_mul(2)),
        3 => (
            bitmap.width.saturating_mul(2),
            bitmap.height.saturating_mul(2),
        ),
        _ => (bitmap.width, bitmap.height),
    };
    let encoded = STANDARD.encode(bytes);
    let image = |extra: &str| {
        format!(
        "<image width=\"{}\" height=\"{}\" opacity=\"{}\" href=\"data:image/png;base64,{encoded}\"{extra}/>",
        bitmap.width, bitmap.height, n(opacity.clamp(0.0, 1.0))
    )
    };
    let mut images = image("");
    if wrap_mode == 1 || wrap_mode == 3 {
        images.push_str(&image(&format!(
            " transform=\"translate({} 0) scale(-1 1)\"",
            bitmap.width.saturating_mul(2)
        )));
    }
    if wrap_mode == 2 || wrap_mode == 3 {
        images.push_str(&image(&format!(
            " transform=\"translate(0 {}) scale(1 -1)\"",
            bitmap.height.saturating_mul(2)
        )));
    }
    if wrap_mode == 3 {
        images.push_str(&image(&format!(
            " transform=\"translate({} {}) scale(-1 -1)\"",
            bitmap.width.saturating_mul(2),
            bitmap.height.saturating_mul(2)
        )));
    }
    Ok(format!(
        "<pattern id=\"paint{id}\" patternUnits=\"userSpaceOnUse\" width=\"{width}\" height=\"{height}\" patternTransform=\"matrix({} {} {} {} {} {})\">{images}</pattern>",
        n(transform.m11), n(transform.m12), n(transform.m21), n(transform.m22),
        n(transform.dx), n(transform.dy)
    ))
}

fn path_points(path: &Path) -> Vec<Point> {
    let mut points = Vec::new();
    for figure in &path.figures {
        points.push(figure.start);
        for segment in &figure.segments {
            match *segment {
                PathSegment::Line(point) => points.push(point),
                PathSegment::Cubic {
                    control1,
                    control2,
                    to,
                } => points.extend([control1, control2, to]),
            }
        }
    }
    points
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
    let line_cap = match pen.line_cap {
        LineCap::Butt => "butt",
        LineCap::Round => "round",
        LineCap::Square => "square",
    };
    let line_join = match pen.line_join {
        LineJoin::Miter => "miter",
        LineJoin::Round => "round",
        LineJoin::Bevel => "bevel",
    };
    format!("stroke=\"{stroke}\" stroke-width=\"{}\" stroke-linecap=\"{line_cap}\" stroke-linejoin=\"{line_join}\"{dash} fill=\"{fill}\" fill-rule=\"{rule}\"",n(width))
}

fn path_data(path: &Path) -> Result<String> {
    let mut data = String::new();
    for figure in &path.figures {
        write!(data, "M {} {}", n(figure.start.x), n(figure.start.y))
            .map_err(|error| MetafileError::SvgGeneration(error.to_string()))?;
        for segment in &figure.segments {
            match *segment {
                PathSegment::Line(to) => {
                    write!(data, " L {} {}", n(to.x), n(to.y))
                        .map_err(|error| MetafileError::SvgGeneration(error.to_string()))?;
                }
                PathSegment::Cubic {
                    control1,
                    control2,
                    to,
                } => {
                    write!(
                        data,
                        " C {} {} {} {} {} {}",
                        n(control1.x),
                        n(control1.y),
                        n(control2.x),
                        n(control2.y),
                        n(to.x),
                        n(to.y)
                    )
                    .map_err(|error| MetafileError::SvgGeneration(error.to_string()))?;
                }
            }
        }
        if figure.closed {
            data.push_str(" Z");
        }
    }
    Ok(data)
}

fn flatten_clip_regions<'a>(regions: &'a [ClipRegion], output: &mut Vec<&'a ClipRegion>) {
    let mut pending: Vec<_> = regions.iter().rev().collect();
    while let Some(region) = pending.pop() {
        if let ClipRegion::Intersection(nested) = region {
            pending.extend(nested.iter().rev());
        } else {
            output.push(region);
        }
    }
}

fn clip_has_boolean(region: &ClipRegion) -> bool {
    match region {
        ClipRegion::Combine { .. } => true,
        ClipRegion::Intersection(regions) => regions.iter().any(clip_has_boolean),
        _ => false,
    }
}

fn mask_rect(bounds: Rect, fill: &str) -> String {
    let bounds = padded_mask_bounds(bounds);
    format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{fill}\"/>",
        n(bounds.left),
        n(bounds.top),
        n(bounds.width()),
        n(bounds.height())
    )
}

fn padded_mask_bounds(bounds: Rect) -> Rect {
    let bounds = bounds.normalized();
    Rect {
        left: bounds.left - 1.0,
        top: bounds.top - 1.0,
        right: bounds.right + 1.0,
        bottom: bounds.bottom + 1.0,
    }
}

fn clip_region_bounds(region: &ClipRegion) -> Option<Rect> {
    match region {
        ClipRegion::Rect(rect) => Some(rect.normalized()),
        ClipRegion::Polygon(points) => (!points.is_empty()).then(|| points_bounds(points)),
        ClipRegion::Path { path, .. } => path_bounds(path).ok(),
        ClipRegion::Intersection(regions) => {
            let mut bounds = regions.iter().filter_map(clip_region_bounds);
            let first = bounds.next()?;
            bounds.try_fold(first, Rect::intersection)
        }
        ClipRegion::Combine {
            operation,
            left,
            right,
        } => match operation {
            ClipOperation::Union | ClipOperation::Xor => {
                match (clip_region_bounds(left), clip_region_bounds(right)) {
                    (Some(left), Some(right)) => Some(left.union(right)),
                    (bounds, None) | (None, bounds) => bounds,
                }
            }
            ClipOperation::Exclude => clip_region_bounds(left),
            ClipOperation::Complement => clip_region_bounds(right),
        },
    }
}

fn clip_key(region: &ClipRegion) -> String {
    match region {
        ClipRegion::Rect(rect) => {
            let r = rect.normalized();
            format!(
                "r:{}:{}:{}:{}",
                n(r.left),
                n(r.top),
                n(r.right),
                n(r.bottom)
            )
        }
        ClipRegion::Polygon(points) => format!("p:{}", point_list(points)),
        ClipRegion::Path { path, fill_mode } => {
            format!("d:{fill_mode}:{}", path_data_for_clip(path))
        }
        ClipRegion::Intersection(regions) => {
            let mut flattened = Vec::new();
            flatten_clip_regions(regions, &mut flattened);
            flattened
                .into_iter()
                .map(clip_key)
                .collect::<Vec<_>>()
                .join("&")
        }
        ClipRegion::Combine {
            operation,
            left,
            right,
        } => {
            format!("b:{operation:?}:{}:{}", clip_key(left), clip_key(right))
        }
    }
}

fn clip_element(region: &ClipRegion) -> String {
    match region {
        ClipRegion::Rect(rect) => {
            let r = rect.normalized();
            format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
                n(r.left),
                n(r.top),
                n(r.width()),
                n(r.height())
            )
        }
        ClipRegion::Polygon(points) => {
            format!("<polygon points=\"{}\"/>", point_list(points))
        }
        ClipRegion::Path { path, fill_mode } => format!(
            "<path d=\"{}\" clip-rule=\"{}\"/>",
            path_data_for_clip(path),
            if *fill_mode == 2 {
                "nonzero"
            } else {
                "evenodd"
            }
        ),
        ClipRegion::Intersection(_) => String::new(),
        ClipRegion::Combine { .. } => String::new(),
    }
}

fn path_data_for_clip(path: &Path) -> String {
    let mut data = String::new();
    for figure in &path.figures {
        let _ = write!(data, "M {} {}", n(figure.start.x), n(figure.start.y));
        for segment in &figure.segments {
            match *segment {
                PathSegment::Line(to) => {
                    let _ = write!(data, " L {} {}", n(to.x), n(to.y));
                }
                PathSegment::Cubic {
                    control1,
                    control2,
                    to,
                } => {
                    let _ = write!(
                        data,
                        " C {} {} {} {} {} {}",
                        n(control1.x),
                        n(control1.y),
                        n(control2.x),
                        n(control2.y),
                        n(to.x),
                        n(to.y)
                    );
                }
            }
        }
        if figure.closed {
            data.push_str(" Z");
        }
    }
    data
}

fn path_bounds(path: &Path) -> Result<Rect> {
    let mut points = Vec::new();
    for figure in &path.figures {
        points.push(figure.start);
        for segment in &figure.segments {
            match *segment {
                PathSegment::Line(point) => points.push(point),
                PathSegment::Cubic {
                    control1,
                    control2,
                    to,
                } => points.extend([control1, control2, to]),
            }
        }
    }
    points
        .first()
        .map(|_| points_bounds(&points))
        .ok_or_else(|| MetafileError::SvgGeneration("empty path has no bounds".into()))
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

fn transformed_rect_bounds(rect: Rect, transform: metafile_core::Transform) -> Rect {
    let points = [
        transform.transform_point(Point::new(rect.left, rect.top)),
        transform.transform_point(Point::new(rect.right, rect.top)),
        transform.transform_point(Point::new(rect.right, rect.bottom)),
        transform.transform_point(Point::new(rect.left, rect.bottom)),
    ];
    points_bounds(&points)
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
            background_path: None,
            horizontal_align: HorizontalTextAlignment::Left,
            vertical_align: VerticalTextAlignment::Top,
            clip: None,
            dx: vec![],
            transform: metafile_core::Transform::IDENTITY,
            right_to_left: false,
        })
        .unwrap();
        let s = r.finish(None, None).unwrap();
        assert!(s.contains("a&lt;&amp;&quot;"));
        assert!(s.contains("preserveAspectRatio=\"none\""));
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
                BitmapPlacement::from_rect(Rect {
                    left: 10.0,
                    top: 20.0,
                    right: 0.0,
                    bottom: 10.0,
                }),
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
            svg.contains("image-rendering=\"pixelated\" transform=\"matrix(-10 0 0 -10 10 20)\""),
            "{svg}"
        );
    }

    #[test]
    fn generic_path_serializes_cubic_and_close() {
        let mut renderer = SvgRenderer::new(ResourceLimits::default());
        renderer
            .path(
                &Path {
                    figures: vec![metafile_core::PathFigure {
                        start: Point::new(1.0, 2.0),
                        segments: vec![PathSegment::Cubic {
                            control1: Point::new(3.0, 4.0),
                            control2: Point::new(5.0, 6.0),
                            to: Point::new(7.0, 8.0),
                        }],
                        closed: true,
                    }],
                },
                &Pen::default(),
                &Brush::default(),
                1,
                true,
                true,
                None,
            )
            .unwrap();
        let svg = renderer.finish(None, None).unwrap();
        assert!(svg.contains("d=\"M 1 2 C 3 4 5 6 7 8 Z\""), "{svg}");
    }

    #[test]
    fn multiline_centering_offsets_the_first_baseline() {
        let mut renderer = SvgRenderer::new(ResourceLimits::default());
        let font = metafile_core::Font {
            height: 10.0,
            ..metafile_core::Font::default()
        };
        renderer
            .text(&TextRun {
                position: Point::new(50.0, 50.0),
                text: "first\nsecond".into(),
                font,
                color: Color::BLACK,
                background: None,
                background_rect: None,
                background_path: None,
                horizontal_align: HorizontalTextAlignment::Center,
                vertical_align: VerticalTextAlignment::Center,
                clip: None,
                dx: Vec::new(),
                transform: metafile_core::Transform::IDENTITY,
                right_to_left: false,
            })
            .unwrap();
        let svg = renderer.finish(None, None).unwrap();
        assert!(
            svg.contains("<tspan x=\"50\" dy=\"-6\">first</tspan>"),
            "{svg}"
        );
        assert!(
            svg.contains("<tspan x=\"50\" dy=\"12\">second</tspan>"),
            "{svg}"
        );
    }

    #[test]
    fn boolean_clip_masks_use_region_derived_bounds() {
        let clip = ClipRegion::Combine {
            operation: ClipOperation::Exclude,
            left: Box::new(ClipRegion::Rect(Rect {
                left: 10.0,
                top: 20.0,
                right: 110.0,
                bottom: 120.0,
            })),
            right: Box::new(ClipRegion::Rect(Rect {
                left: 40.0,
                top: 50.0,
                right: 80.0,
                bottom: 90.0,
            })),
        };
        let mut renderer = SvgRenderer::new(ResourceLimits::default());
        renderer
            .pixel(Point::new(20.0, 30.0), Color::BLACK, Some(&clip))
            .unwrap();
        let svg = renderer.finish(None, None).unwrap();
        assert!(svg.contains("mask=\"url(#mask0)\""), "{svg}");
        assert!(
            svg.contains("x=\"9\" y=\"19\" width=\"102\" height=\"102\""),
            "{svg}"
        );
        assert!(!svg.contains("32768"), "{svg}");
    }
}
