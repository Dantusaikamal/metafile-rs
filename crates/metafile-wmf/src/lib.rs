//! Safe, first-party WMF parsing, inspection, stateful playback, and SVG output.

mod dib;
mod reader;
pub mod record;

use encoding_rs::{Encoding, WINDOWS_1252};
use metafile_core::{
    ArcKind, Brush, BrushStyle, Color, DeviceContext, Diagnostic, Font, GdiObject, MetafileError,
    Pen, PenStyle, Point, Rect, Renderer, ResourceLimits, Result,
};
use metafile_svg::SvgRenderer;
use reader::Reader;
use record::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const PLACEABLE_MAGIC: u32 = 0x9ac6_cdd7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WmfType {
    Memory,
    Disk,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetafileInfo {
    pub format: String,
    pub wmf_type: WmfType,
    pub placeable: bool,
    pub bounds: Option<Rect>,
    pub units_per_inch: Option<u16>,
    pub estimated_width: Option<f64>,
    pub estimated_height: Option<f64>,
    pub version: u16,
    pub declared_size_bytes: u64,
    pub object_count: u16,
    pub maximum_record_words: u32,
    pub parsed_record_count: u32,
    pub has_eof: bool,
    pub warnings: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderOptions {
    pub strict: bool,
    pub limits: ResourceLimits,
}
impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            strict: false,
            limits: ResourceLimits::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderResult {
    pub svg: String,
    pub metadata: MetafileInfo,
    pub diagnostics: Vec<Diagnostic>,
}

struct Parsed<'a> {
    info: MetafileInfo,
    records: Vec<RecordEntry<'a>>,
    placeable_bounds: Option<Rect>,
    units_per_inch: Option<u16>,
}

pub fn inspect(bytes: &[u8]) -> Result<MetafileInfo> {
    inspect_with_options(bytes, &RenderOptions::default())
}
pub fn inspect_with_options(bytes: &[u8], options: &RenderOptions) -> Result<MetafileInfo> {
    Ok(parse(bytes, &options.limits)?.info)
}
pub fn to_svg(bytes: &[u8], options: RenderOptions) -> Result<RenderResult> {
    let parsed = parse(bytes, &options.limits)?;
    let mut diagnostics = parsed.info.warnings.clone();
    let mut svg = SvgRenderer::new(options.limits.clone());
    let mut player = Player::new(parsed.info.object_count, options.limits.clone());
    for entry in &parsed.records {
        player.play(entry, &mut svg, &mut diagnostics, options.strict)?;
    }
    if player.dc.clip.is_some() { /* clip state is emitted per operation */ }
    let drawn_bounds = svg.drawing_bounds();
    if parsed.placeable_bounds.is_none() && drawn_bounds.is_none() {
        diagnostics.push(Diagnostic::warning(
            "unreliable_bounds",
            "standard WMF contained no drawable extents; SVG uses a 1x1 fallback viewBox",
        ));
    }
    let output_bounds = parsed.placeable_bounds.or(drawn_bounds);
    let physical = match (parsed.placeable_bounds, parsed.units_per_inch) {
        (Some(b), Some(upi)) if upi > 0 => Some((
            b.width() * 96.0 / f64::from(upi),
            b.height() * 96.0 / f64::from(upi),
        )),
        _ => drawn_bounds.map(|b| (b.width().max(1.0), b.height().max(1.0))),
    };
    let svg = svg.finish(output_bounds, physical)?;
    let mut metadata = parsed.info;
    metadata.bounds = output_bounds;
    metadata.estimated_width = physical.map(|p| p.0);
    metadata.estimated_height = physical.map(|p| p.1);
    Ok(RenderResult {
        svg,
        metadata,
        diagnostics,
    })
}

fn parse<'a>(bytes: &'a [u8], limits: &ResourceLimits) -> Result<Parsed<'a>> {
    if bytes.len() > limits.max_input_bytes {
        return Err(MetafileError::ResourceLimitExceeded {
            resource: "input bytes",
            actual: bytes.len() as u64,
            limit: limits.max_input_bytes as u64,
        });
    }
    if bytes.len() < 18 {
        return Err(MetafileError::TruncatedInput {
            offset: 0,
            needed: 18,
            available: bytes.len(),
        });
    }
    let mut r = Reader::new(bytes);
    let mut placeable = false;
    let mut bounds = None;
    let mut units = None;
    let mut warnings = Vec::new();
    if r.u32()? == PLACEABLE_MAGIC {
        placeable = true;
        r.seek(0)?;
        let words = (0..10).map(|_| r.u16()).collect::<Result<Vec<_>>>()?;
        let checksum = r.u16()?;
        let actual = words.iter().fold(0u16, |a, w| a ^ w);
        if checksum != actual {
            return Err(MetafileError::InvalidChecksum {
                expected: checksum,
                actual,
            });
        }
        let left = i16::from_le_bytes(words[3].to_le_bytes());
        let top = i16::from_le_bytes(words[4].to_le_bytes());
        let right = i16::from_le_bytes(words[5].to_le_bytes());
        let bottom = i16::from_le_bytes(words[6].to_le_bytes());
        let upi = words[7];
        if upi == 0 {
            return Err(MetafileError::InvalidPlaceableHeader(
                "units-per-inch is zero".into(),
            ));
        }
        let b = Rect {
            left: f64::from(left),
            top: f64::from(top),
            right: f64::from(right),
            bottom: f64::from(bottom),
        };
        if b.width() == 0.0 || b.height() == 0.0 {
            return Err(MetafileError::InvalidPlaceableHeader(
                "empty bounding box".into(),
            ));
        }
        bounds = Some(b);
        units = Some(upi);
        r.seek(22)?;
    } else {
        r.seek(0)?;
    }
    let header_start = r.position();
    let file_type = r.u16()?;
    let wmf_type = match file_type {
        1 => WmfType::Memory,
        2 => WmfType::Disk,
        _ => {
            return Err(MetafileError::InvalidHeader(format!(
                "invalid WMF type {file_type}"
            )))
        }
    };
    let header_words = r.u16()?;
    if header_words != 9 {
        return Err(MetafileError::InvalidHeader(format!(
            "METAHEADER size is {header_words}, expected 9"
        )));
    }
    let version = r.u16()?;
    if !matches!(version, 0x0100 | 0x0300) {
        return Err(MetafileError::InvalidHeader(format!(
            "unsupported WMF version {version:#06x}"
        )));
    }
    let size_words = r.u32()?;
    let object_count = r.u16()?;
    let max_record = r.u32()?;
    let no_params = r.u16()?;
    if no_params != 0 {
        warnings.push(Diagnostic::warning(
            "nonzero_header_parameter_count",
            format!("METAHEADER parameter count is {no_params}"),
        ));
    }
    if usize::from(object_count) > limits.max_objects {
        return Err(MetafileError::ResourceLimitExceeded {
            resource: "object table entries",
            actual: u64::from(object_count),
            limit: limits.max_objects as u64,
        });
    }
    let declared_bytes = usize::try_from(size_words)
        .ok()
        .and_then(|w| w.checked_mul(2))
        .ok_or_else(|| MetafileError::InvalidHeader("declared file size overflow".into()))?;
    let declared_end = header_start
        .checked_add(declared_bytes)
        .ok_or_else(|| MetafileError::InvalidHeader("declared file end overflow".into()))?;
    if declared_bytes < 18 {
        return Err(MetafileError::InvalidHeader(
            "declared file is smaller than METAHEADER".into(),
        ));
    }
    if declared_end > bytes.len() {
        return Err(MetafileError::TruncatedInput {
            offset: header_start,
            needed: declared_bytes,
            available: bytes.len() - header_start,
        });
    }
    let (records, eof) = parse_records(bytes, header_start + 18, declared_end, limits)?;
    if !eof {
        warnings.push(Diagnostic::warning(
            "missing_eof",
            "WMF ended without META_EOF",
        ));
    }
    let info = MetafileInfo {
        format: "wmf".into(),
        wmf_type,
        placeable,
        bounds,
        units_per_inch: units,
        estimated_width: bounds
            .zip(units)
            .map(|(b, u)| b.width() * 96.0 / f64::from(u)),
        estimated_height: bounds
            .zip(units)
            .map(|(b, u)| b.height() * 96.0 / f64::from(u)),
        version,
        declared_size_bytes: declared_bytes as u64,
        object_count,
        maximum_record_words: max_record,
        parsed_record_count: records.len() as u32,
        has_eof: eof,
        warnings,
    };
    Ok(Parsed {
        info,
        records,
        placeable_bounds: bounds,
        units_per_inch: units,
    })
}

struct Player {
    dc: DeviceContext,
    stack: Vec<DeviceContext>,
    objects: Vec<Option<GdiObject>>,
    limits: ResourceLimits,
    warning_counts: BTreeMap<String, u32>,
}
impl Player {
    fn new(objects: u16, limits: ResourceLimits) -> Self {
        Self {
            dc: DeviceContext::default(),
            stack: Vec::new(),
            objects: vec![None; usize::from(objects)],
            limits,
            warning_counts: BTreeMap::new(),
        }
    }
    fn play(
        &mut self,
        e: &RecordEntry<'_>,
        r: &mut dyn Renderer,
        d: &mut Vec<Diagnostic>,
        strict: bool,
    ) -> Result<()> {
        match &e.record {
            Record::Eof => {}
            Record::NoArgs(f) if *f == META_SAVEDC => {
                if self.stack.len() >= self.limits.max_dc_depth {
                    return Err(MetafileError::ResourceLimitExceeded {
                        resource: "SaveDC depth",
                        actual: (self.stack.len() + 1) as u64,
                        limit: self.limits.max_dc_depth as u64,
                    });
                }
                self.stack.push(self.dc.clone());
            }
            Record::OneI16(f, v) => match *f {
                META_SETBKMODE => self.dc.background_opaque = *v == 2,
                META_SETMAPMODE => self.dc.map_mode = *v,
                META_SETROP2 => self.dc.raster_op = *v as u16,
                META_SETPOLYFILLMODE => self.dc.polygon_fill_mode = *v as u16,
                META_SETSTRETCHBLTMODE => self.dc.stretch_mode = *v as u16,
                META_RESTOREDC => self.restore(*v, e.index)?,
                _ => {}
            },
            Record::OneU16(f, v) => match *f {
                META_SETTEXTALIGN => self.dc.text_align = *v,
                META_SELECTOBJECT => self.select(*v, e.index)?,
                META_DELETEOBJECT => self.delete(*v, e.index)?,
                _ => {}
            },
            Record::OneU32(f, v) => match *f {
                META_SETBKCOLOR => self.dc.background_color = color(*v),
                META_SETTEXTCOLOR => self.dc.text_color = color(*v),
                META_SETMAPPERFLAGS => self.dc.mapper_flags = *v,
                _ => {}
            },
            Record::Point(f, p) => match *f {
                META_MOVETO => self.dc.current_position = *p,
                META_LINETO => {
                    let a = self.map(self.dc.current_position);
                    let b = self.map(*p);
                    r.line(a, b, &self.dc.pen, self.clip())?;
                    self.dc.current_position = *p;
                }
                META_SETWINDOWORG => self.dc.mapping.window_origin = *p,
                META_SETWINDOWEXT => self.dc.mapping.window_extent = *p,
                META_SETVIEWPORTORG => self.dc.mapping.viewport_origin = *p,
                META_SETVIEWPORTEXT => self.dc.mapping.viewport_extent = *p,
                META_OFFSETWINDOWORG => {
                    self.dc.mapping.window_origin.x += p.x;
                    self.dc.mapping.window_origin.y += p.y;
                }
                META_OFFSETVIEWPORTORG => {
                    self.dc.mapping.viewport_origin.x += p.x;
                    self.dc.mapping.viewport_origin.y += p.y;
                }
                _ => {}
            },
            Record::Scale(f, v) => {
                let target = if *f == META_SCALEWINDOWEXT {
                    &mut self.dc.mapping.window_extent
                } else {
                    &mut self.dc.mapping.viewport_extent
                };
                if v[0] == 0 || v[2] == 0 {
                    return Err(MetafileError::InvalidHeader(
                        "mapping scale denominator is zero".into(),
                    ));
                }
                target.y *= f64::from(v[1]) / f64::from(v[0]);
                target.x *= f64::from(v[3]) / f64::from(v[2]);
            }
            Record::Rect(f, rect) => {
                let mr = self.map_rect(*rect);
                match *f{META_RECTANGLE=>r.rectangle(mr,None,&self.dc.pen,&self.dc.brush,self.clip())?,META_ELLIPSE=>r.ellipse(mr,&self.dc.pen,&self.dc.brush,self.clip())?,META_INTERSECTCLIPRECT=>self.intersect_clip(mr),META_EXCLUDECLIPRECT=>self.warn(d,e,"approximate_exclude_clip","META_EXCLUDECLIPRECT cannot be represented by the rectangular clip model and was skipped"),_=>{}}
            }
            Record::RoundRect(rect, rad) => {
                let scale = self.map(*rad);
                r.rectangle(
                    self.map_rect(*rect),
                    Some(Point::new(
                        scale.x - self.dc.mapping.viewport_origin.x,
                        scale.y - self.dc.mapping.viewport_origin.y,
                    )),
                    &self.dc.pen,
                    &self.dc.brush,
                    self.clip(),
                )?
            }
            Record::Arc(f, rect, start, end) => {
                let kind = if *f == META_ARC {
                    ArcKind::Arc
                } else if *f == META_PIE {
                    ArcKind::Pie
                } else {
                    ArcKind::Chord
                };
                r.arc(
                    self.map_rect(*rect),
                    self.map(*start),
                    self.map(*end),
                    kind,
                    &self.dc.pen,
                    &self.dc.brush,
                    self.clip(),
                )?
            }
            Record::SetPixel(p, c) => r.pixel(self.map(*p), color(*c), self.clip())?,
            Record::Pen {
                style,
                width,
                color: c,
            } => self.insert(
                GdiObject::Pen(Pen {
                    style: pen_style(*style),
                    width: f64::from(width.abs()).max(1.0),
                    color: color(*c),
                }),
                e.index,
            )?,
            Record::Brush {
                style,
                color: c,
                hatch,
            } => {
                let brush = Brush {
                    style: brush_style(*style, *hatch),
                    color: color(*c),
                };
                if !matches!(brush.style, BrushStyle::Solid | BrushStyle::Null) {
                    self.warn(d, e, "unsupported_brush_style", "hatch and pattern brushes are not rendered; brush is treated as transparent");
                }
                self.insert(GdiObject::Brush(brush), e.index)?;
            }
            Record::Font(raw) => self.insert(GdiObject::Font(parse_font(raw)), e.index)?,
            Record::Points(f, pts) => {
                let p: Vec<_> = pts.iter().map(|p| self.map(*p)).collect();
                if *f == META_POLYLINE {
                    r.polyline(&p, &self.dc.pen, self.clip())?
                } else {
                    r.polygon(
                        &p,
                        &self.dc.pen,
                        &self.dc.brush,
                        self.dc.polygon_fill_mode,
                        self.clip(),
                    )?
                }
            }
            Record::PolyPolygon { counts, points } => {
                let mut at = 0;
                for c in counts {
                    let end = at + usize::from(*c);
                    let p: Vec<_> = points[at..end].iter().map(|p| self.map(*p)).collect();
                    r.polygon(
                        &p,
                        &self.dc.pen,
                        &self.dc.brush,
                        self.dc.polygon_fill_mode,
                        self.clip(),
                    )?;
                    at = end;
                }
            }
            Record::Text {
                extended,
                position,
                options,
                clip,
                bytes,
                dx,
            } => {
                let (enc, had) = decode_text(bytes, self.dc.font.charset);
                if had {
                    self.warn(
                        d,
                        e,
                        "text_decoding_replacement",
                        "text contained bytes not representable in the selected Windows encoding",
                    );
                }
                self.warn(
                    d,
                    e,
                    "approximate_text_metrics",
                    "text positioning uses SVG font metrics and may differ from Windows GDI",
                );
                let run = metafile_core::TextRun {
                    position: self.map(*position),
                    text: enc,
                    font: self.dc.font.clone(),
                    color: self.dc.text_color,
                    background: if self.dc.background_opaque {
                        Some(self.dc.background_color)
                    } else {
                        None
                    },
                    align: self.dc.text_align,
                    clip: clip.map(|c| self.map_rect(c)).or(self.clip()),
                    dx: dx.iter().map(|x| f64::from(*x)).collect(),
                };
                r.text(&run)?;
                if *extended && options & 0x0002 != 0 { /* clipping carried in run */ }
            }
            Record::Bitmap { function, params } => self.bitmap(*function, params, e, r, d)?,
            Record::Escape { function, data } => self.warn(
                d,
                e,
                "unsupported_escape",
                &format!(
                    "escape function {function:#06x} with {} data bytes skipped",
                    data.len()
                ),
            ),
            Record::Unknown(f, p) => {
                if strict {
                    return Err(MetafileError::UnsupportedCriticalFeature(format!(
                        "record {} ({f:#06x})",
                        name(*f)
                    )));
                }
                self.warn(
                    d,
                    e,
                    "unsupported_record",
                    &format!(
                        "{} ({f:#06x}, {} parameter bytes) skipped",
                        name(*f),
                        p.len()
                    ),
                );
            }
            Record::NoArgs(_) => {}
        }
        Ok(())
    }
    fn map(&self, p: Point) -> Point {
        self.dc.mapping.transform(p)
    }
    fn map_rect(&self, r: Rect) -> Rect {
        self.dc.mapping.transform_rect(r)
    }
    fn clip(&self) -> Option<Rect> {
        self.dc.clip
    }
    fn insert(&mut self, o: GdiObject, _index: usize) -> Result<()> {
        if let Some(slot) = self.objects.iter_mut().find(|x| x.is_none()) {
            *slot = Some(o);
            Ok(())
        } else {
            Err(MetafileError::ResourceLimitExceeded {
                resource: "WMF object table",
                actual: (self.objects.len() + 1) as u64,
                limit: self.objects.len() as u64,
            })
        }
    }
    fn select(&mut self, h: u16, index: usize) -> Result<()> {
        if h & 0x8000 != 0 {
            let object = stock_object(h & 0x7fff).ok_or(MetafileError::InvalidObjectHandle {
                handle: h,
                record_index: index,
            })?;
            self.apply_selected(h, object);
            return Ok(());
        }
        let o = self
            .objects
            .get(usize::from(h))
            .and_then(Clone::clone)
            .ok_or(MetafileError::InvalidObjectHandle {
                handle: h,
                record_index: index,
            })?;
        self.apply_selected(h, o);
        Ok(())
    }
    fn apply_selected(&mut self, h: u16, o: GdiObject) {
        match o {
            GdiObject::Pen(v) => {
                self.dc.pen = v;
                self.dc.selected_pen = Some(h)
            }
            GdiObject::Brush(v) => {
                self.dc.brush = v;
                self.dc.selected_brush = Some(h)
            }
            GdiObject::Font(v) => {
                self.dc.font = v;
                self.dc.selected_font = Some(h)
            }
        }
    }
    fn delete(&mut self, h: u16, index: usize) -> Result<()> {
        let slot =
            self.objects
                .get_mut(usize::from(h))
                .ok_or(MetafileError::InvalidObjectHandle {
                    handle: h,
                    record_index: index,
                })?;
        if slot.is_none() {
            return Err(MetafileError::InvalidObjectHandle {
                handle: h,
                record_index: index,
            });
        }
        *slot = None;
        Ok(())
    }
    fn restore(&mut self, n: i16, index: usize) -> Result<()> {
        let target = if n < 0 {
            self.stack.len().checked_sub(usize::from(n.unsigned_abs()))
        } else {
            usize::try_from(n).ok().and_then(|v| v.checked_sub(1))
        };
        let target = target.ok_or(MetafileError::InvalidObjectHandle {
            handle: n as u16,
            record_index: index,
        })?;
        if target >= self.stack.len() {
            return Err(MetafileError::InvalidObjectHandle {
                handle: n as u16,
                record_index: index,
            });
        }
        self.dc = self.stack[target].clone();
        self.stack.truncate(target);
        Ok(())
    }
    fn intersect_clip(&mut self, b: Rect) {
        self.dc.clip = Some(match self.dc.clip {
            None => b,
            Some(a) => Rect {
                left: a.left.max(b.left),
                top: a.top.max(b.top),
                right: a.right.min(b.right),
                bottom: a.bottom.min(b.bottom),
            },
        })
    }
    fn warn(&mut self, d: &mut Vec<Diagnostic>, e: &RecordEntry<'_>, code: &str, msg: &str) {
        let aggregation_key = format!("{code}:{:#06x}", e.function);
        let count = self.warning_counts.entry(aggregation_key).or_default();
        *count += 1;
        if let Some(existing) = d
            .iter_mut()
            .find(|x| x.code == code && x.record_type == Some(e.function))
        {
            existing.occurrences = *count;
            return;
        }
        if d.len() < self.limits.max_diagnostics {
            d.push(Diagnostic::warning(code, msg).at_record(
                e.index,
                e.function,
                name(e.function),
                e.offset,
            ));
        }
    }
    fn bitmap(
        &mut self,
        f: u16,
        p: &[u8],
        e: &RecordEntry<'_>,
        r: &mut dyn Renderer,
        d: &mut Vec<Diagnostic>,
    ) -> Result<()> {
        if f != META_STRETCHDIB {
            self.warn(
                d,
                e,
                "unsupported_bitmap_record",
                &format!(
                    "{} is parsed but not rendered; META_STRETCHDIB is supported",
                    name(f)
                ),
            );
            return Ok(());
        }
        let mut q = Reader::new(p);
        let _rop = q.u32()?;
        let _usage = q.u16()?;
        let _src_h = q.i16()?;
        let _src_w = q.i16()?;
        let _src_y = q.i16()?;
        let _src_x = q.i16()?;
        let dst_h = q.i16()?;
        let dst_w = q.i16()?;
        let dst_y = q.i16()?;
        let dst_x = q.i16()?;
        let dib = &p[q.position()..];
        match dib::decode_dib(dib, e.index, &self.limits) {
            Ok(bitmap) => r.bitmap(
                self.map_rect(Rect {
                    left: f64::from(dst_x),
                    top: f64::from(dst_y),
                    right: f64::from(dst_x + dst_w),
                    bottom: f64::from(dst_y + dst_h),
                }),
                &bitmap,
                self.clip(),
            ),
            Err(err) => {
                if matches!(err, MetafileError::InvalidBitmap { .. }) {
                    self.warn(d, e, "unsupported_or_invalid_dib", &err.to_string());
                    Ok(())
                } else {
                    Err(err)
                }
            }
        }
    }
}

fn color(v: u32) -> Color {
    Color::rgb(
        (v & 255) as u8,
        ((v >> 8) & 255) as u8,
        ((v >> 16) & 255) as u8,
    )
}
fn pen_style(v: u16) -> PenStyle {
    match v & 15 {
        0 => PenStyle::Solid,
        1 => PenStyle::Dash,
        2 => PenStyle::Dot,
        3 => PenStyle::DashDot,
        4 => PenStyle::DashDotDot,
        5 => PenStyle::Null,
        6 => PenStyle::InsideFrame,
        x => PenStyle::Unknown(x),
    }
}
fn brush_style(v: u16, h: u16) -> BrushStyle {
    match v {
        0 => BrushStyle::Solid,
        1 => BrushStyle::Null,
        2 => BrushStyle::Hatched(h),
        3 => BrushStyle::Pattern,
        x => BrushStyle::Unknown(x),
    }
}
fn stock_object(index: u16) -> Option<GdiObject> {
    Some(match index {
        0 => GdiObject::Brush(Brush {
            style: BrushStyle::Solid,
            color: Color::WHITE,
        }),
        1 => GdiObject::Brush(Brush {
            style: BrushStyle::Solid,
            color: Color::rgb(192, 192, 192),
        }),
        2 => GdiObject::Brush(Brush {
            style: BrushStyle::Solid,
            color: Color::rgb(128, 128, 128),
        }),
        3 => GdiObject::Brush(Brush {
            style: BrushStyle::Solid,
            color: Color::rgb(64, 64, 64),
        }),
        4 => GdiObject::Brush(Brush {
            style: BrushStyle::Solid,
            color: Color::BLACK,
        }),
        5 => GdiObject::Brush(Brush {
            style: BrushStyle::Null,
            color: Color::WHITE,
        }),
        6 => GdiObject::Pen(Pen {
            style: PenStyle::Solid,
            width: 1.0,
            color: Color::WHITE,
        }),
        7 => GdiObject::Pen(Pen::default()),
        8 => GdiObject::Pen(Pen {
            style: PenStyle::Null,
            width: 1.0,
            color: Color::BLACK,
        }),
        10..=17 => GdiObject::Font(Font::default()),
        _ => return None,
    })
}
fn parse_font(p: &[u8]) -> Font {
    let i16at = |o| i16::from_le_bytes([p[o], p[o + 1]]);
    let weight = i16at(8).max(0) as u16;
    let charset = p.get(13).copied().unwrap_or(0);
    let face = &p[18..p.len().min(50)];
    let end = face.iter().position(|b| *b == 0).unwrap_or(face.len());
    let (text, _) = decode_text(&face[..end], charset);
    Font {
        height: f64::from(i16at(0)),
        width: f64::from(i16at(2)),
        escapement_tenths: i16at(4),
        weight,
        italic: p[10] != 0,
        underline: p[11] != 0,
        strike_out: p[12] != 0,
        charset,
        family: if text.is_empty() {
            "sans-serif".into()
        } else {
            text
        },
    }
}
fn decode_text(b: &[u8], charset: u8) -> (String, bool) {
    let enc = match charset {
        0 | 1 | 2 | 77 | 255 => WINDOWS_1252,
        128 => Encoding::for_label(b"shift_jis").unwrap_or(WINDOWS_1252),
        129 => Encoding::for_label(b"euc-kr").unwrap_or(WINDOWS_1252),
        134 => Encoding::for_label(b"gbk").unwrap_or(WINDOWS_1252),
        136 => Encoding::for_label(b"big5").unwrap_or(WINDOWS_1252),
        161 => Encoding::for_label(b"windows-1253").unwrap_or(WINDOWS_1252),
        162 => Encoding::for_label(b"windows-1254").unwrap_or(WINDOWS_1252),
        177 => Encoding::for_label(b"windows-1255").unwrap_or(WINDOWS_1252),
        178 => Encoding::for_label(b"windows-1256").unwrap_or(WINDOWS_1252),
        186 => Encoding::for_label(b"windows-1257").unwrap_or(WINDOWS_1252),
        204 => Encoding::for_label(b"windows-1251").unwrap_or(WINDOWS_1252),
        222 => Encoding::for_label(b"windows-874").unwrap_or(WINDOWS_1252),
        238 => Encoding::for_label(b"windows-1250").unwrap_or(WINDOWS_1252),
        _ => WINDOWS_1252,
    };
    let (cow, _, had) = enc.decode(b);
    (cow.into_owned(), had)
}

#[cfg(test)]
mod tests;
