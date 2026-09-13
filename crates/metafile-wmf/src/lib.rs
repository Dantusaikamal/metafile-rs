//! Safe, first-party WMF parsing, inspection, and renderer-independent playback.

mod reader;
mod record;

use encoding_rs::{Encoding, WINDOWS_1252};
pub use metafile_core::RenderOptions;
use metafile_core::{
    ArcKind, BitmapSampling, Brush, BrushStyle, Color, DeviceContext, Diagnostic, Font, GdiObject,
    HorizontalTextAlignment, MetafileError, MetafileFormat, Pen, PenStyle, Point, Rect, Renderer,
    ResourceLimits, Result, VerticalTextAlignment,
};
use metafile_dib::{crop_bitmap, decode_dib};
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
#[serde(rename_all = "camelCase")]
pub struct WmfInfo {
    pub format: MetafileFormat,
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
pub struct WmfPlaybackResult {
    pub metadata: WmfInfo,
    pub diagnostics: Vec<Diagnostic>,
    pub output_bounds: Option<Rect>,
    pub physical_size: Option<(f64, f64)>,
}

struct Parsed<'a> {
    info: WmfInfo,
    records: Vec<RecordEntry<'a>>,
    placeable_bounds: Option<Rect>,
    units_per_inch: Option<u16>,
}

pub fn inspect(bytes: &[u8]) -> Result<WmfInfo> {
    inspect_with_options(bytes, &RenderOptions::default())
}
pub fn inspect_with_options(bytes: &[u8], options: &RenderOptions) -> Result<WmfInfo> {
    Ok(parse(bytes, &options.limits)?.info)
}
pub fn playback(
    bytes: &[u8],
    options: &RenderOptions,
    renderer: &mut dyn Renderer,
) -> Result<WmfPlaybackResult> {
    let parsed = parse(bytes, &options.limits)?;
    let mut diagnostics = parsed.info.warnings.clone();
    let mut player = Player::new(parsed.info.object_count, options.limits.clone());
    for entry in &parsed.records {
        player.play(entry, renderer, &mut diagnostics, options.strict)?;
    }
    if player.dc.clip.is_some() { /* clip state is emitted per operation */ }
    let drawn_bounds = renderer.drawing_bounds();
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
    let mut metadata = parsed.info;
    metadata.bounds = output_bounds;
    metadata.estimated_width = physical.map(|p| p.0);
    metadata.estimated_height = physical.map(|p| p.1);
    Ok(WmfPlaybackResult {
        metadata,
        diagnostics,
        output_bounds,
        physical_size: physical,
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
    let actual_max = records
        .iter()
        .map(|record| record.size_words)
        .max()
        .unwrap_or(3);
    if max_record < actual_max {
        return Err(MetafileError::InvalidHeader(format!(
            "declared maximum record size {max_record} words is smaller than actual {actual_max}"
        )));
    }
    if max_record > actual_max {
        warnings.push(Diagnostic::warning(
            "maximum_record_size_mismatch",
            format!("declared maximum record size is {max_record} words; actual maximum is {actual_max}"),
        ));
    }
    if let Some(last) = records.last() {
        let record_end = last
            .offset
            .saturating_add((last.size_words as usize).saturating_mul(2));
        if eof && record_end < declared_end {
            warnings.push(Diagnostic::warning(
                "data_after_eof",
                format!(
                    "{} bytes follow META_EOF inside the declared file",
                    declared_end - record_end
                ),
            ));
        }
    }
    if !eof {
        warnings.push(Diagnostic::warning(
            "missing_eof",
            "WMF ended without META_EOF",
        ));
    }
    let info = WmfInfo {
        format: MetafileFormat::Wmf,
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
                META_SETMAPMODE => self.set_map_mode(*v, e, d, strict)?,
                META_SETROP2 => {
                    self.dc.raster_op = *v as u16;
                    if *v != 13 {
                        self.unsupported(
                            strict,
                            d,
                            e,
                            "unsupported_rop2",
                            "non-copy vector raster operations are not representable in SVG",
                        )?;
                    }
                }
                META_SETPOLYFILLMODE => self.dc.polygon_fill_mode = *v as u16,
                META_SETSTRETCHBLTMODE => self.dc.stretch_mode = *v as u16,
                META_RESTOREDC => self.restore(*v, e.index)?,
                _ => {}
            },
            Record::OneU16(f, v) => match *f {
                META_SETTEXTALIGN => self.dc.text_align = *v,
                META_SELECTOBJECT => self.select(*v, e.index)?,
                META_DELETEOBJECT => self.delete(*v, e, d, strict)?,
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
                    r.line(a, b, &self.mapped_pen(), self.clip())?;
                    self.dc.current_position = *p;
                }
                META_SETWINDOWORG => self.dc.mapping.window_origin = *p,
                META_SETWINDOWEXT if matches!(self.dc.map_mode, 7 | 8) => {
                    if p.x == 0.0 || p.y == 0.0 {
                        return Err(MetafileError::InvalidHeader(
                            "window extent contains zero".into(),
                        ));
                    }
                    self.dc.mapping.window_extent = *p;
                    self.adjust_isotropic();
                }
                META_SETVIEWPORTORG => self.dc.mapping.viewport_origin = *p,
                META_SETVIEWPORTEXT if matches!(self.dc.map_mode, 7 | 8) => {
                    self.dc.mapping.viewport_extent = *p;
                    self.adjust_isotropic();
                }
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
                if matches!(self.dc.map_mode, 7 | 8) {
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
                    if !self.dc.mapping.is_finite() {
                        return Err(MetafileError::ResourceLimitExceeded {
                            resource: "coordinate transform",
                            actual: u64::MAX,
                            limit: u64::MAX - 1,
                        });
                    }
                    self.adjust_isotropic();
                }
            }
            Record::Rect(f, rect) => {
                let mr = self.map_rect(*rect);
                let pen = self.mapped_pen();
                match *f {
                    META_RECTANGLE => r.rectangle(mr, None, &pen, &self.dc.brush, self.clip())?,
                    META_ELLIPSE => r.ellipse(mr, &pen, &self.dc.brush, self.clip())?,
                    META_INTERSECTCLIPRECT => self.intersect_clip(mr),
                    META_EXCLUDECLIPRECT => self.unsupported(
                        strict,
                        d,
                        e,
                        "unsupported_exclude_clip",
                        "META_EXCLUDECLIPRECT cannot be represented by the rectangular clip model",
                    )?,
                    _ => {}
                }
            }
            Record::RoundRect(rect, rad) => {
                let scale = self
                    .dc
                    .mapping
                    .transform_vector(metafile_core::Vector::new(rad.x, rad.y));
                let pen = self.mapped_pen();
                r.rectangle(
                    self.map_rect(*rect),
                    Some(Point::new(scale.x, scale.y)),
                    &pen,
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
                let pen = self.mapped_pen();
                r.arc(
                    self.map_rect(*rect),
                    self.map(*start),
                    self.map(*end),
                    kind,
                    false,
                    &pen,
                    &self.dc.brush,
                    self.clip(),
                )?
            }
            Record::SetPixel(p, c) => r.pixel(self.map(*p), color(*c), self.clip())?,
            Record::Pen {
                style,
                width,
                color: c,
            } => {
                let style = pen_style(*style);
                match style {
                    PenStyle::InsideFrame => self.unsupported(
                        strict,
                        d,
                        e,
                        "approximate_inside_frame_pen",
                        "PS_INSIDEFRAME is rendered as a centered solid SVG stroke",
                    )?,
                    PenStyle::Unknown(value) => self.unsupported(
                        strict,
                        d,
                        e,
                        "unsupported_pen_style",
                        &format!("pen style {value} is not reproduced"),
                    )?,
                    PenStyle::Dash | PenStyle::Dot | PenStyle::DashDot | PenStyle::DashDotDot => {
                        self.unsupported(
                            strict,
                            d,
                            e,
                            "approximate_pen_dash",
                            "GDI cosmetic dash patterns are approximated with SVG dash arrays",
                        )?;
                    }
                    PenStyle::Solid | PenStyle::Null => {}
                }
                self.insert(
                    GdiObject::Pen(Pen {
                        style,
                        width: f64::from(width.unsigned_abs()),
                        cosmetic: *width == 0,
                        color: color(*c),
                    }),
                    e.index,
                )?;
            }
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
                    self.unsupported(strict, d, e, "unsupported_brush_style", "hatch and pattern brushes are not rendered; brush is treated as transparent")?;
                }
                self.insert(GdiObject::Brush(brush), e.index)?;
            }
            Record::Font(raw) => self.insert(GdiObject::Font(parse_font(raw)), e.index)?,
            Record::Points(f, pts) => {
                let p: Vec<_> = pts.iter().map(|p| self.map(*p)).collect();
                let pen = self.mapped_pen();
                if *f == META_POLYLINE {
                    r.polyline(&p, &pen, self.clip())?
                } else {
                    r.polygon(
                        &p,
                        &pen,
                        &self.dc.brush,
                        self.dc.polygon_fill_mode,
                        self.clip(),
                    )?
                }
            }
            Record::PolyPolygon { counts, points } => {
                let mut at = 0;
                let mut polygons = Vec::with_capacity(counts.len());
                for c in counts {
                    let end = at + usize::from(*c);
                    let p: Vec<_> = points[at..end].iter().map(|p| self.map(*p)).collect();
                    polygons.push(p);
                    at = end;
                }
                r.poly_polygon(
                    &polygons,
                    &self.mapped_pen(),
                    &self.dc.brush,
                    self.dc.polygon_fill_mode,
                    self.clip(),
                )?;
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
                if self.dc.mapper_flags != 0 {
                    self.unsupported(
                        strict,
                        d,
                        e,
                        "unsupported_mapper_flags",
                        "font matching requested by META_SETMAPPERFLAGS cannot be reproduced without host font APIs",
                    )?;
                }
                if self.dc.text_align & 0x0100 != 0 {
                    self.unsupported(
                        strict,
                        d,
                        e,
                        "unsupported_rtl_text",
                        "TA_RTLREADING is not reproduced by the current SVG text renderer",
                    )?;
                }
                let logical_position = if self.dc.text_align & 0x0001 != 0 {
                    self.dc.current_position
                } else {
                    *position
                };
                let mapped_rect = clip.map(|c| self.map_rect(c).normalized());
                let run = metafile_core::TextRun {
                    position: self.map(logical_position),
                    text: enc.clone(),
                    font: self.mapped_font(),
                    color: self.dc.text_color,
                    background: if self.dc.background_opaque || (*extended && options & 0x0002 != 0)
                    {
                        Some(self.dc.background_color)
                    } else {
                        None
                    },
                    background_rect: if *extended && options & 0x0002 != 0 {
                        mapped_rect
                    } else {
                        None
                    },
                    horizontal_align: horizontal_text_alignment(self.dc.text_align),
                    vertical_align: vertical_text_alignment(self.dc.text_align),
                    clip: if *extended && options & 0x0004 != 0 {
                        mapped_rect
                    } else {
                        self.clip()
                    },
                    dx: dx
                        .iter()
                        .map(|x| {
                            self.dc
                                .mapping
                                .transform_vector(metafile_core::Vector::new(f64::from(*x), 0.0))
                                .x
                        })
                        .collect(),
                };
                r.text(&run)?;
                if self.dc.text_align & 0x0001 != 0 {
                    let advance = if dx.is_empty() {
                        self.warn(
                            d,
                            e,
                            "approximate_update_cp",
                            "TA_UPDATECP advance is estimated because Windows GDI font metrics are unavailable",
                        );
                        approximate_text_advance(&enc, &self.dc.font)
                    } else {
                        dx.iter().map(|x| f64::from(*x)).sum::<f64>()
                    };
                    let angle = f64::from(self.dc.font.escapement_tenths).to_radians() / 10.0;
                    self.dc.current_position.x += advance * angle.cos();
                    self.dc.current_position.y -= advance * angle.sin();
                }
            }
            Record::Bitmap { function, params } => {
                self.bitmap(*function, params, e, r, d, strict)?
            }
            Record::Escape { function, data } => self.unsupported(
                strict,
                d,
                e,
                "unsupported_escape",
                &format!(
                    "escape function {function:#06x} with {} data bytes skipped",
                    data.len()
                ),
            )?,
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
        self.dc.mapping.transform_point(p)
    }
    fn map_rect(&self, r: Rect) -> Rect {
        self.dc.mapping.transform_rect(r)
    }
    fn clip(&self) -> Option<Rect> {
        self.dc.clip
    }
    fn mapped_pen(&self) -> Pen {
        let mut pen = self.dc.pen.clone();
        pen.width = if pen.cosmetic {
            1.0
        } else {
            self.dc
                .mapping
                .transform_vector(metafile_core::Vector::new(pen.width, 0.0))
                .x
                .abs()
                .max(1.0)
        };
        pen
    }
    fn mapped_font(&self) -> Font {
        let mut font = self.dc.font.clone();
        let extent = self
            .dc
            .mapping
            .transform_vector(metafile_core::Vector::new(font.width, font.height));
        font.width = extent.x.abs();
        font.height = extent.y.abs();
        font
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
                handle: u32::from(h),
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
                handle: u32::from(h),
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
    fn delete(
        &mut self,
        h: u16,
        entry: &RecordEntry<'_>,
        diagnostics: &mut Vec<Diagnostic>,
        strict: bool,
    ) -> Result<()> {
        let index = entry.index;
        let selected = |dc: &DeviceContext| {
            dc.selected_pen == Some(h)
                || dc.selected_brush == Some(h)
                || dc.selected_font == Some(h)
        };
        if selected(&self.dc) || self.stack.iter().any(selected) {
            if strict {
                return Err(MetafileError::ObjectInUse {
                    handle: u32::from(h),
                    record_index: index,
                });
            }
            self.warn(
                diagnostics,
                entry,
                "delete_selected_object",
                "selected object was not deleted",
            );
            return Ok(());
        }
        let slot =
            self.objects
                .get_mut(usize::from(h))
                .ok_or(MetafileError::InvalidObjectHandle {
                    handle: u32::from(h),
                    record_index: index,
                })?;
        if slot.is_none() {
            return Err(MetafileError::InvalidObjectHandle {
                handle: u32::from(h),
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
        let invalid = || MetafileError::InvalidRestoreDc {
            value: n,
            record_index: index,
            stack_depth: self.stack.len(),
        };
        let target = target.ok_or_else(invalid)?;
        if target >= self.stack.len() {
            return Err(invalid());
        }
        self.dc = self.stack[target].clone();
        self.stack.truncate(target);
        Ok(())
    }
    fn intersect_clip(&mut self, b: Rect) {
        let b = b.normalized();
        self.dc.clip = Some(match self.dc.clip {
            None => b,
            Some(a) => a.intersection(b).unwrap_or(Rect {
                left: b.left,
                top: b.top,
                right: b.left,
                bottom: b.top,
            }),
        })
    }
    fn warn(&mut self, d: &mut Vec<Diagnostic>, e: &RecordEntry<'_>, code: &str, msg: &str) {
        let aggregation_key = format!("{code}:{:#06x}", e.function);
        let count = self.warning_counts.entry(aggregation_key).or_default();
        *count += 1;
        if let Some(existing) = d
            .iter_mut()
            .find(|x| x.code == code && x.record_type == Some(u32::from(e.function)))
        {
            existing.occurrences = *count;
            return;
        }
        if d.len() < self.limits.max_diagnostics {
            d.push(Diagnostic::warning(code, msg).at_record(
                e.index,
                u32::from(e.function),
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
        strict: bool,
    ) -> Result<()> {
        if f != META_STRETCHDIB {
            self.unsupported(
                strict,
                d,
                e,
                "unsupported_bitmap_record",
                &format!(
                    "{} is parsed but not rendered; META_STRETCHDIB is supported",
                    name(f)
                ),
            )?;
            return Ok(());
        }
        let mut q = Reader::new(p);
        let rop = q.u32()?;
        let usage = q.u16()?;
        let src_h = q.i16()?;
        let src_w = q.i16()?;
        let src_y = q.i16()?;
        let src_x = q.i16()?;
        let dst_h = q.i16()?;
        let dst_w = q.i16()?;
        let dst_y = q.i16()?;
        let dst_x = q.i16()?;
        let dib = &p[q.position()..];
        if rop != 0x00cc_0020 {
            self.unsupported(
                strict,
                d,
                e,
                "unsupported_bitmap_rop",
                &format!("raster operation {rop:#010x} is not SRCCOPY; bitmap skipped"),
            )?;
            return Ok(());
        }
        if usage != 0 {
            self.unsupported(
                strict,
                d,
                e,
                "unsupported_dib_color_usage",
                "DIB_PAL_COLORS is not supported; bitmap skipped",
            )?;
            return Ok(());
        }
        match decode_dib(dib, e.index, &self.limits) {
            Ok(decoded) => {
                let bitmap = crop_bitmap(
                    decoded.bitmap,
                    decoded.top_down,
                    i32::from(src_x),
                    i32::from(src_y),
                    i32::from(src_w),
                    i32::from(src_h),
                    e.index,
                )?;
                let mapped_dest = self.map_rect(Rect {
                    left: f64::from(dst_x),
                    top: f64::from(dst_y),
                    right: f64::from(dst_x) + f64::from(dst_w),
                    bottom: f64::from(dst_y) + f64::from(dst_h),
                });
                let scaled = (mapped_dest.width() - f64::from(bitmap.width)).abs() > f64::EPSILON
                    || (mapped_dest.height() - f64::from(bitmap.height)).abs() > f64::EPSILON;
                let sampling = if scaled {
                    match self.dc.stretch_mode {
                        3 => BitmapSampling::Pixelated,
                        4 => {
                            self.unsupported(
                                strict,
                                d,
                                e,
                                "approximate_stretch_mode",
                                "HALFTONE is approximated with smooth SVG image interpolation",
                            )?;
                            BitmapSampling::Smooth
                        }
                        1 | 2 => {
                            self.unsupported(
                                strict,
                                d,
                                e,
                                "unsupported_stretch_mode",
                                "monochrome stretch modes are not reproduced by SVG image interpolation",
                            )?;
                            BitmapSampling::Auto
                        }
                        mode => {
                            self.unsupported(
                                strict,
                                d,
                                e,
                                "unsupported_stretch_mode",
                                &format!("invalid or unsupported stretch mode {mode}"),
                            )?;
                            BitmapSampling::Auto
                        }
                    }
                } else {
                    BitmapSampling::Auto
                };
                r.bitmap(mapped_dest, &bitmap, sampling, self.clip())
            }
            Err(err) => {
                if matches!(err, MetafileError::UnsupportedBitmap { .. }) {
                    self.unsupported(strict, d, e, "unsupported_dib", &err.to_string())?;
                    Ok(())
                } else {
                    Err(err)
                }
            }
        }
    }
    fn unsupported(
        &mut self,
        strict: bool,
        diagnostics: &mut Vec<Diagnostic>,
        entry: &RecordEntry<'_>,
        code: &str,
        message: &str,
    ) -> Result<()> {
        if strict {
            return Err(MetafileError::UnsupportedCriticalFeature(format!(
                "{} at record {}: {message}",
                name(entry.function),
                entry.index
            )));
        }
        self.warn(diagnostics, entry, code, message);
        Ok(())
    }
    fn set_map_mode(
        &mut self,
        mode: i16,
        entry: &RecordEntry<'_>,
        diagnostics: &mut Vec<Diagnostic>,
        strict: bool,
    ) -> Result<()> {
        let scale = match mode {
            1 => Some((1.0, 1.0)),
            2 => Some((96.0 / 254.0, -96.0 / 254.0)),
            3 => Some((96.0 / 2540.0, -96.0 / 2540.0)),
            4 => Some((0.96, -0.96)),
            5 => Some((0.096, -0.096)),
            6 => Some((96.0 / 1440.0, -96.0 / 1440.0)),
            7 | 8 => None,
            _ => {
                self.unsupported(
                    strict,
                    diagnostics,
                    entry,
                    "unsupported_map_mode",
                    &format!("map mode {mode} is unknown"),
                )?;
                None
            }
        };
        self.dc.map_mode = mode;
        if let Some((x, y)) = scale {
            self.dc.mapping.window_extent = Point::new(1.0, 1.0);
            self.dc.mapping.viewport_extent = Point::new(x, y);
        }
        Ok(())
    }
    fn adjust_isotropic(&mut self) {
        if self.dc.map_mode != 7
            || self.dc.mapping.window_extent.x == 0.0
            || self.dc.mapping.window_extent.y == 0.0
        {
            return;
        }
        let sx = self.dc.mapping.viewport_extent.x / self.dc.mapping.window_extent.x;
        let sy = self.dc.mapping.viewport_extent.y / self.dc.mapping.window_extent.y;
        let magnitude = sx.abs().min(sy.abs());
        self.dc.mapping.viewport_extent.x =
            magnitude.copysign(sx) * self.dc.mapping.window_extent.x;
        self.dc.mapping.viewport_extent.y =
            magnitude.copysign(sy) * self.dc.mapping.window_extent.y;
    }
}

fn color(v: u32) -> Color {
    Color::rgb(
        (v & 255) as u8,
        ((v >> 8) & 255) as u8,
        ((v >> 16) & 255) as u8,
    )
}

fn horizontal_text_alignment(value: u16) -> HorizontalTextAlignment {
    match value & 0x0006 {
        0x0006 => HorizontalTextAlignment::Center,
        0x0002 => HorizontalTextAlignment::Right,
        _ => HorizontalTextAlignment::Left,
    }
}

fn vertical_text_alignment(value: u16) -> VerticalTextAlignment {
    match value & 0x0018 {
        0x0018 => VerticalTextAlignment::Baseline,
        0x0008 => VerticalTextAlignment::Bottom,
        _ => VerticalTextAlignment::Top,
    }
}

fn approximate_text_advance(text: &str, font: &Font) -> f64 {
    let per_character = if font.width.abs() >= 1.0 {
        font.width.abs()
    } else {
        font.height.abs().max(1.0) * 0.6
    };
    per_character * text.chars().count() as f64
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
            cosmetic: true,
            color: Color::WHITE,
        }),
        7 => GdiObject::Pen(Pen::default()),
        8 => GdiObject::Pen(Pen {
            style: PenStyle::Null,
            width: 1.0,
            cosmetic: true,
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
