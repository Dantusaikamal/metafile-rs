//! Safe, first-party EMF parsing, inspection, and renderer-independent playback.

mod reader;

use encoding_rs::{Encoding, WINDOWS_1252};
pub use metafile_core::RenderOptions;
use metafile_core::{
    ArcKind, BitmapSampling, Brush, BrushStyle, Color, DeviceContext, Diagnostic, Font, GdiObject,
    HorizontalTextAlignment, MetafileError, MetafileFormat, Path, PathFigure, PathSegment, Pen,
    PenStyle, Point, Rect, Renderer, ResourceLimits, Result, Transform, Vector,
    VerticalTextAlignment,
};
use metafile_dib::{crop_bitmap, decode_dib};
use reader::Reader;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const EMF_SIGNATURE: u32 = 0x464d_4520;
const EMR_HEADER: u32 = 1;
const EMR_EOF: u32 = 14;
const EMR_GDICOMMENT: u32 = 70;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmfInfo {
    pub format: MetafileFormat,
    pub bounds: Rect,
    pub frame: Rect,
    pub version: u32,
    pub declared_size_bytes: u32,
    pub declared_record_count: u32,
    pub parsed_record_count: u32,
    pub handle_count: u16,
    pub description: Option<String>,
    pub palette_entries: u32,
    pub device_width: i32,
    pub device_height: i32,
    pub millimeters_width: i32,
    pub millimeters_height: i32,
    pub contains_emf_plus: bool,
    pub has_eof: bool,
    pub warnings: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmfPlaybackResult {
    pub metadata: EmfInfo,
    pub diagnostics: Vec<Diagnostic>,
    pub output_bounds: Option<Rect>,
    pub physical_size: Option<(f64, f64)>,
}

#[derive(Clone, Copy)]
struct Record<'a> {
    index: usize,
    kind: u32,
    offset: usize,
    bytes: &'a [u8],
}

struct Parsed<'a> {
    info: EmfInfo,
    records: Vec<Record<'a>>,
}

pub fn inspect(bytes: &[u8]) -> Result<EmfInfo> {
    inspect_with_options(bytes, &RenderOptions::default())
}

pub fn inspect_with_options(bytes: &[u8], options: &RenderOptions) -> Result<EmfInfo> {
    Ok(parse(bytes, &options.limits)?.info)
}

pub fn playback(
    bytes: &[u8],
    options: &RenderOptions,
    renderer: &mut dyn Renderer,
) -> Result<EmfPlaybackResult> {
    let parsed = parse(bytes, &options.limits)?;
    if parsed.info.contains_emf_plus {
        return Err(MetafileError::UnsupportedCriticalFeature(
            "EMF+ playback is not implemented; refusing to ignore embedded EMF+ content".into(),
        ));
    }
    let mut diagnostics = parsed.info.warnings.clone();
    let mut player = Player::new(options.limits.clone());
    for record in &parsed.records {
        player.play(*record, renderer, &mut diagnostics, options.strict)?;
    }
    let output_bounds = Some(parsed.info.bounds)
        .filter(|bounds| bounds.width() > 0.0 && bounds.height() > 0.0)
        .or_else(|| renderer.drawing_bounds());
    let frame = parsed.info.frame;
    let physical_size = if frame.width() > 0.0 && frame.height() > 0.0 {
        Some((
            frame.width() * 96.0 / 2540.0,
            frame.height() * 96.0 / 2540.0,
        ))
    } else {
        output_bounds.map(|bounds| (bounds.width().max(1.0), bounds.height().max(1.0)))
    };
    Ok(EmfPlaybackResult {
        metadata: parsed.info,
        diagnostics,
        output_bounds,
        physical_size,
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
    if bytes.len() < 88 {
        return Err(MetafileError::TruncatedInput {
            offset: 0,
            needed: 88,
            available: bytes.len(),
        });
    }
    let mut reader = Reader::new(bytes);
    if reader.u32()? != EMR_HEADER {
        return Err(MetafileError::InvalidHeader(
            "first record is not EMR_HEADER".into(),
        ));
    }
    let header_size = reader.u32()?;
    if header_size < 88 || header_size % 4 != 0 {
        return Err(MetafileError::InvalidHeader(format!(
            "invalid EMR_HEADER size {header_size}"
        )));
    }
    let header_len = usize::try_from(header_size)
        .map_err(|_| MetafileError::InvalidHeader("EMR_HEADER size overflow".into()))?;
    if header_len > bytes.len() {
        return Err(MetafileError::TruncatedInput {
            offset: 0,
            needed: header_len,
            available: bytes.len(),
        });
    }
    let bounds = read_rect(&mut reader)?;
    let frame = read_rect(&mut reader)?;
    if reader.u32()? != EMF_SIGNATURE {
        return Err(MetafileError::InvalidHeader("invalid EMF signature".into()));
    }
    let version = reader.u32()?;
    let declared_size = reader.u32()?;
    let declared_records = reader.u32()?;
    let handles = reader.u16()?;
    let reserved = reader.u16()?;
    let description_chars = reader.u32()?;
    let description_offset = reader.u32()?;
    let palette_entries = reader.u32()?;
    let device_width = reader.i32()?;
    let device_height = reader.i32()?;
    let millimeters_width = reader.i32()?;
    let millimeters_height = reader.i32()?;
    if reserved != 0 {
        return Err(MetafileError::InvalidHeader(
            "EMR_HEADER reserved field is nonzero".into(),
        ));
    }
    if handles as usize > limits.max_objects {
        return Err(MetafileError::ResourceLimitExceeded {
            resource: "object table entries",
            actual: u64::from(handles),
            limit: limits.max_objects as u64,
        });
    }
    let declared_len = usize::try_from(declared_size)
        .map_err(|_| MetafileError::InvalidHeader("declared EMF size overflow".into()))?;
    if declared_len < header_len || declared_len > bytes.len() {
        return Err(MetafileError::InvalidHeader(format!(
            "declared EMF size {declared_size} is outside the input"
        )));
    }
    if declared_records == 0 {
        return Err(MetafileError::InvalidHeader(
            "declared EMF record count is zero".into(),
        ));
    }
    if declared_records as usize > limits.max_records {
        return Err(MetafileError::ResourceLimitExceeded {
            resource: "record count",
            actual: u64::from(declared_records),
            limit: limits.max_records as u64,
        });
    }
    let description = read_description(bytes, header_len, description_offset, description_chars)?;
    let mut records = Vec::new();
    let mut offset = header_len;
    let mut has_eof = false;
    let mut contains_emf_plus = false;
    while offset < declared_len {
        if records.len() + 1 >= limits.max_records {
            return Err(MetafileError::ResourceLimitExceeded {
                resource: "record count",
                actual: (records.len() + 2) as u64,
                limit: limits.max_records as u64,
            });
        }
        let prefix_end = offset
            .checked_add(8)
            .ok_or(MetafileError::RecordOutOfBounds {
                offset,
                end: usize::MAX,
                input_len: declared_len,
            })?;
        if prefix_end > declared_len {
            return Err(MetafileError::TruncatedInput {
                offset,
                needed: 8,
                available: declared_len - offset,
            });
        }
        let kind = le_u32(bytes, offset)?;
        let size = le_u32(bytes, offset + 4)?;
        if size < 8 || size % 4 != 0 {
            return Err(MetafileError::InvalidRecordSize {
                offset,
                words: size / 4,
            });
        }
        let size = usize::try_from(size).map_err(|_| MetafileError::RecordOutOfBounds {
            offset,
            end: usize::MAX,
            input_len: declared_len,
        })?;
        let end = offset
            .checked_add(size)
            .ok_or(MetafileError::RecordOutOfBounds {
                offset,
                end: usize::MAX,
                input_len: declared_len,
            })?;
        if end > declared_len {
            return Err(MetafileError::RecordOutOfBounds {
                offset,
                end,
                input_len: declared_len,
            });
        }
        let record = Record {
            index: records.len() + 1,
            kind,
            offset,
            bytes: &bytes[offset..end],
        };
        if kind == EMR_GDICOMMENT && is_emf_plus_comment(record.bytes)? {
            contains_emf_plus = true;
        }
        records.push(record);
        offset = end;
        if kind == EMR_EOF {
            has_eof = true;
            break;
        }
    }
    let actual_records = records.len() + 1;
    let mut warnings = Vec::new();
    if !has_eof {
        warnings.push(Diagnostic::warning(
            "missing_eof",
            "EMF has no EMR_EOF record",
        ));
    }
    if declared_records as usize != actual_records {
        warnings.push(Diagnostic::warning(
            "record_count_mismatch",
            format!("header declares {declared_records} records; parsed {actual_records}"),
        ));
    }
    if contains_emf_plus {
        warnings.push(Diagnostic::warning(
            "emf_plus_present",
            "EMR_GDICOMMENT contains EMF+ records; EMF+ playback is not implemented",
        ));
    }
    Ok(Parsed {
        info: EmfInfo {
            format: if contains_emf_plus {
                MetafileFormat::EmfPlus
            } else {
                MetafileFormat::Emf
            },
            bounds,
            frame,
            version,
            declared_size_bytes: declared_size,
            declared_record_count: declared_records,
            parsed_record_count: actual_records as u32,
            handle_count: handles,
            description,
            palette_entries,
            device_width,
            device_height,
            millimeters_width,
            millimeters_height,
            contains_emf_plus,
            has_eof,
            warnings,
        },
        records,
    })
}

fn read_description(
    bytes: &[u8],
    header_len: usize,
    offset: u32,
    characters: u32,
) -> Result<Option<String>> {
    if characters == 0 {
        return Ok(None);
    }
    let start = usize::try_from(offset)
        .map_err(|_| MetafileError::InvalidHeader("description offset overflow".into()))?;
    let byte_len = usize::try_from(characters)
        .ok()
        .and_then(|count| count.checked_mul(2))
        .ok_or_else(|| MetafileError::InvalidHeader("description length overflow".into()))?;
    let end = start
        .checked_add(byte_len)
        .ok_or_else(|| MetafileError::InvalidHeader("description end overflow".into()))?;
    if start < 88 || end > header_len || end > bytes.len() {
        return Err(MetafileError::InvalidHeader(
            "description is outside EMR_HEADER".into(),
        ));
    }
    let units: Vec<u16> = bytes[start..end]
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .take_while(|unit| *unit != 0)
        .collect();
    Ok(Some(String::from_utf16_lossy(&units)))
}

fn is_emf_plus_comment(bytes: &[u8]) -> Result<bool> {
    if bytes.len() < 12 {
        return Ok(false);
    }
    let length = usize::try_from(le_u32(bytes, 8)?)
        .map_err(|_| MetafileError::InvalidHeader("comment length overflow".into()))?;
    let end = 12usize
        .checked_add(length)
        .ok_or_else(|| MetafileError::InvalidHeader("comment length overflow".into()))?;
    if end > bytes.len() {
        return Err(MetafileError::TruncatedInput {
            offset: 12,
            needed: length,
            available: bytes.len() - 12,
        });
    }
    Ok(length >= 4 && &bytes[12..16] == b"EMF+")
}

#[derive(Clone)]
struct EmfDc {
    core: DeviceContext,
    selected_pen: Option<u32>,
    selected_brush: Option<u32>,
    selected_font: Option<u32>,
    arc_clockwise: bool,
}

impl Default for EmfDc {
    fn default() -> Self {
        Self {
            core: DeviceContext::default(),
            selected_pen: None,
            selected_brush: None,
            selected_font: None,
            arc_clockwise: false,
        }
    }
}

struct PathBuilder {
    path: Path,
    current: Option<usize>,
}

impl PathBuilder {
    fn new() -> Self {
        Self {
            path: Path {
                figures: Vec::new(),
            },
            current: None,
        }
    }

    fn move_to(&mut self, point: Point) {
        self.path.figures.push(PathFigure {
            start: point,
            segments: Vec::new(),
            closed: false,
        });
        self.current = Some(self.path.figures.len() - 1);
    }

    fn ensure(&mut self, point: Point) -> usize {
        if let Some(current) = self.current {
            current
        } else {
            self.move_to(point);
            self.current.unwrap_or(0)
        }
    }

    fn line_to(&mut self, from: Point, to: Point) {
        let index = self.ensure(from);
        self.path.figures[index]
            .segments
            .push(PathSegment::Line(to));
    }

    fn bezier_to(&mut self, from: Point, control1: Point, control2: Point, to: Point) {
        let index = self.ensure(from);
        self.path.figures[index].segments.push(PathSegment::Cubic {
            control1,
            control2,
            to,
        });
    }

    fn close(&mut self) {
        if let Some(index) = self.current {
            self.path.figures[index].closed = true;
        }
    }
}

struct Player {
    dc: EmfDc,
    stack: Vec<EmfDc>,
    objects: BTreeMap<u32, GdiObject>,
    limits: ResourceLimits,
    active_path: Option<PathBuilder>,
    completed_path: Option<Path>,
    path_points: usize,
}

impl Player {
    fn new(limits: ResourceLimits) -> Self {
        Self {
            dc: EmfDc::default(),
            stack: Vec::new(),
            objects: BTreeMap::new(),
            limits,
            active_path: None,
            completed_path: None,
            path_points: 0,
        }
    }

    fn play(
        &mut self,
        record: Record<'_>,
        renderer: &mut dyn Renderer,
        diagnostics: &mut Vec<Diagnostic>,
        strict: bool,
    ) -> Result<()> {
        let payload = &record.bytes[8..];
        match record.kind {
            EMR_EOF => Ok(()),
            9 => self.set_extent(payload, true),
            10 => self.set_origin(payload, true),
            11 => self.set_extent(payload, false),
            12 => self.set_origin(payload, false),
            15 => {
                let point = self.transform(read_point(payload, 0, record.offset + 8)?);
                let color = read_color(payload, 8, record.offset + 8)?;
                renderer.pixel(point, color, self.clip())
            }
            16 => {
                self.dc.core.mapper_flags = read_u32(payload, 0, record.offset + 8)?;
                Ok(())
            }
            17 => self.set_map_mode(
                read_i32(payload, 0, record.offset + 8)?,
                record,
                diagnostics,
                strict,
            ),
            18 => {
                self.dc.core.background_opaque = read_u32(payload, 0, record.offset + 8)? == 2;
                Ok(())
            }
            19 => {
                self.dc.core.polygon_fill_mode = read_u32(payload, 0, record.offset + 8)? as u16;
                Ok(())
            }
            20 => {
                self.dc.core.raster_op = read_u32(payload, 0, record.offset + 8)? as u16;
                Ok(())
            }
            21 => {
                self.dc.core.stretch_mode = read_u32(payload, 0, record.offset + 8)? as u16;
                Ok(())
            }
            22 => {
                self.dc.core.text_align = read_u32(payload, 0, record.offset + 8)? as u16;
                Ok(())
            }
            24 => {
                self.dc.core.text_color = read_color(payload, 0, record.offset + 8)?;
                Ok(())
            }
            25 => {
                self.dc.core.background_color = read_color(payload, 0, record.offset + 8)?;
                Ok(())
            }
            27 => {
                self.dc.core.current_position = read_point(payload, 0, record.offset + 8)?;
                let point = self.transform(self.dc.core.current_position);
                if self.active_path.is_some() {
                    self.add_path_points(1)?;
                }
                if let Some(path) = &mut self.active_path {
                    path.move_to(point);
                }
                Ok(())
            }
            29 => self.unsupported(
                record,
                diagnostics,
                strict,
                "exclude_clip_rect",
                "EMR_EXCLUDECLIPRECT cannot be represented by rectangular clipping",
            ),
            30 => self.intersect_clip(payload, record.offset + 8),
            31 => self.scale_extent(payload, false),
            32 => self.scale_extent(payload, true),
            33 => self.save_dc(),
            34 => self.restore_dc(payload, record.index, record.offset + 8),
            35 => self.set_world_transform(payload, record.offset + 8),
            36 => self.modify_world_transform(payload, record.offset + 8),
            37 => self.select_object(payload, record.index, record.offset + 8),
            38 => self.create_pen(payload, record.index, record.offset + 8),
            39 => self.create_brush(
                payload,
                record.index,
                record.offset + 8,
                record,
                diagnostics,
                strict,
            ),
            40 => self.delete_object(payload, record.index, record.offset + 8),
            42..=47 => self.draw_box_record(record, renderer),
            54 => self.line_to(payload, record.offset + 8, renderer),
            55 => self.draw_box_record(record, renderer),
            57 => {
                self.dc.arc_clockwise = read_u32(payload, 0, record.offset + 8)? == 2;
                Ok(())
            }
            58 => self.unsupported(
                record,
                diagnostics,
                strict,
                "miter_limit",
                "custom miter limit is not represented by the renderer",
            ),
            59 => {
                self.active_path = Some(PathBuilder::new());
                self.completed_path = None;
                self.path_points = 0;
                Ok(())
            }
            60 => self.end_path(record, diagnostics, strict),
            61 => {
                if let Some(path) = &mut self.active_path {
                    path.close();
                    Ok(())
                } else {
                    self.unsupported(
                        record,
                        diagnostics,
                        strict,
                        "close_without_path",
                        "EMR_CLOSEFIGURE outside an active path",
                    )
                }
            }
            62 => self.render_path(record, renderer, false, true, diagnostics, strict),
            63 => self.render_path(record, renderer, true, true, diagnostics, strict),
            64 => self.render_path(record, renderer, true, false, diagnostics, strict),
            68 => {
                self.active_path = None;
                self.completed_path = None;
                self.path_points = 0;
                Ok(())
            }
            EMR_GDICOMMENT => self.unsupported(
                record,
                diagnostics,
                strict,
                "gdi_comment",
                "non-EMF+ GDI comment skipped",
            ),
            80 => self.unsupported(
                record,
                diagnostics,
                strict,
                "set_dibits_to_device",
                "EMR_SETDIBITSTODEVICE is not yet rendered",
            ),
            81 => self.stretch_dibits(record, renderer, diagnostics, strict),
            82 => self.create_font(payload, record.index, record.offset + 8),
            83 | 84 => self.ext_text_out(record, renderer, diagnostics, strict),
            2..=8 | 85..=91 => self.poly_record(record, renderer),
            95 => self.create_ext_pen(
                payload,
                record.index,
                record.offset + 8,
                diagnostics,
                strict,
                record,
            ),
            98 => {
                self.diagnostic(
                    diagnostics,
                    record,
                    "icm_mode",
                    "ICM color management state is not applied to SVG colors",
                );
                Ok(())
            }
            _ => self.unsupported(
                record,
                diagnostics,
                strict,
                "unsupported_record",
                &format!("unsupported {} skipped", record_name(record.kind)),
            ),
        }
    }

    fn transform(&self, point: Point) -> Point {
        self.dc
            .core
            .mapping
            .transform_point(self.dc.core.world_transform.transform_point(point))
    }

    fn vector(&self, vector: Vector) -> Vector {
        self.dc
            .core
            .mapping
            .transform_vector(self.dc.core.world_transform.transform_vector(vector))
    }

    fn transform_rect(&self, rect: Rect) -> Rect {
        let a = self.transform(Point::new(rect.left, rect.top));
        let b = self.transform(Point::new(rect.right, rect.bottom));
        Rect {
            left: a.x,
            top: a.y,
            right: b.x,
            bottom: b.y,
        }
    }

    fn clip(&self) -> Option<Rect> {
        self.dc.core.clip
    }

    fn effective_pen(&self) -> Pen {
        let mut pen = self.dc.core.pen.clone();
        if !pen.cosmetic {
            let width = self.vector(Vector::new(pen.width, 0.0));
            pen.width = width.x.hypot(width.y).max(1.0);
        }
        pen
    }

    fn orientation_reversed(&self) -> bool {
        let world = self.dc.core.world_transform;
        let world_determinant = world.m11 * world.m22 - world.m12 * world.m21;
        let mapping = self.dc.core.mapping;
        let scale_x = if mapping.window_extent.x == 0.0 {
            1.0
        } else {
            mapping.viewport_extent.x / mapping.window_extent.x
        };
        let scale_y = if mapping.window_extent.y == 0.0 {
            1.0
        } else {
            mapping.viewport_extent.y / mapping.window_extent.y
        };
        world_determinant * scale_x * scale_y < 0.0
    }

    fn set_origin(&mut self, payload: &[u8], window: bool) -> Result<()> {
        let point = read_point(payload, 0, 0)?;
        if window {
            self.dc.core.mapping.window_origin = point;
        } else {
            self.dc.core.mapping.viewport_origin = point;
        }
        Ok(())
    }

    fn set_extent(&mut self, payload: &[u8], window: bool) -> Result<()> {
        let point = read_point(payload, 0, 0)?;
        if point.x == 0.0 || point.y == 0.0 {
            return Err(MetafileError::InvalidHeader("zero mapping extent".into()));
        }
        if window {
            self.dc.core.mapping.window_extent = point;
        } else {
            self.dc.core.mapping.viewport_extent = point;
        }
        self.adjust_isotropic();
        Ok(())
    }

    fn scale_extent(&mut self, payload: &[u8], window: bool) -> Result<()> {
        let x_num = f64::from(read_i32(payload, 0, 0)?);
        let x_den = f64::from(read_i32(payload, 4, 0)?);
        let y_num = f64::from(read_i32(payload, 8, 0)?);
        let y_den = f64::from(read_i32(payload, 12, 0)?);
        if x_den == 0.0 || y_den == 0.0 {
            return Err(MetafileError::InvalidHeader(
                "zero mapping scale denominator".into(),
            ));
        }
        let extent = if window {
            &mut self.dc.core.mapping.window_extent
        } else {
            &mut self.dc.core.mapping.viewport_extent
        };
        extent.x *= x_num / x_den;
        extent.y *= y_num / y_den;
        self.adjust_isotropic();
        Ok(())
    }

    fn set_map_mode(
        &mut self,
        mode: i32,
        record: Record<'_>,
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
                    record,
                    diagnostics,
                    strict,
                    "unsupported_map_mode",
                    &format!("map mode {mode} is unknown"),
                )?;
                None
            }
        };
        self.dc.core.map_mode = mode.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        if let Some((x, y)) = scale {
            self.dc.core.mapping.window_extent = Point::new(1.0, 1.0);
            self.dc.core.mapping.viewport_extent = Point::new(x, y);
        }
        Ok(())
    }

    fn adjust_isotropic(&mut self) {
        if self.dc.core.map_mode != 7
            || self.dc.core.mapping.window_extent.x == 0.0
            || self.dc.core.mapping.window_extent.y == 0.0
        {
            return;
        }
        let scale_x = self.dc.core.mapping.viewport_extent.x / self.dc.core.mapping.window_extent.x;
        let scale_y = self.dc.core.mapping.viewport_extent.y / self.dc.core.mapping.window_extent.y;
        let magnitude = scale_x.abs().min(scale_y.abs());
        self.dc.core.mapping.viewport_extent.x =
            magnitude.copysign(scale_x) * self.dc.core.mapping.window_extent.x;
        self.dc.core.mapping.viewport_extent.y =
            magnitude.copysign(scale_y) * self.dc.core.mapping.window_extent.y;
    }

    fn set_world_transform(&mut self, payload: &[u8], base: usize) -> Result<()> {
        let transform = read_transform(payload, 0, base)?;
        if !transform.is_finite() {
            return Err(MetafileError::InvalidHeader(
                "non-finite world transform".into(),
            ));
        }
        self.dc.core.world_transform = transform;
        Ok(())
    }

    fn modify_world_transform(&mut self, payload: &[u8], base: usize) -> Result<()> {
        let transform = read_transform(payload, 0, base)?;
        let mode = read_u32(payload, 24, base)?;
        self.dc.core.world_transform = match mode {
            1 => Transform::IDENTITY,
            2 => self.dc.core.world_transform.compose(transform),
            3 => transform.compose(self.dc.core.world_transform),
            4 => transform,
            _ => {
                return Err(MetafileError::InvalidHeader(format!(
                    "invalid ModifyWorldTransform mode {mode}"
                )))
            }
        };
        Ok(())
    }

    fn save_dc(&mut self) -> Result<()> {
        if self.stack.len() >= self.limits.max_dc_depth {
            return Err(MetafileError::ResourceLimitExceeded {
                resource: "SaveDC depth",
                actual: (self.stack.len() + 1) as u64,
                limit: self.limits.max_dc_depth as u64,
            });
        }
        self.stack.push(self.dc.clone());
        Ok(())
    }

    fn restore_dc(&mut self, payload: &[u8], record_index: usize, base: usize) -> Result<()> {
        let value = read_i32(payload, 0, base)?;
        let target = match value.cmp(&0) {
            std::cmp::Ordering::Less => self.stack.len().checked_sub(value.unsigned_abs() as usize),
            std::cmp::Ordering::Greater => usize::try_from(value - 1)
                .ok()
                .filter(|index| *index < self.stack.len()),
            std::cmp::Ordering::Equal => None,
        };
        let target = target.ok_or(MetafileError::InvalidRestoreDc {
            value: value.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
            record_index,
            stack_depth: self.stack.len(),
        })?;
        self.dc = self.stack[target].clone();
        self.stack.truncate(target);
        Ok(())
    }

    fn add_object(&mut self, handle: u32, object: GdiObject, record_index: usize) -> Result<()> {
        if handle == 0 || handle & 0x8000_0000 != 0 || self.objects.contains_key(&handle) {
            return Err(MetafileError::InvalidObjectHandle {
                handle,
                record_index,
            });
        }
        if self.objects.len() >= self.limits.max_objects {
            return Err(MetafileError::ResourceLimitExceeded {
                resource: "object table entries",
                actual: (self.objects.len() + 1) as u64,
                limit: self.limits.max_objects as u64,
            });
        }
        self.objects.insert(handle, object);
        Ok(())
    }

    fn create_pen(&mut self, payload: &[u8], record_index: usize, base: usize) -> Result<()> {
        let handle = read_u32(payload, 0, base)?;
        let style = read_u32(payload, 4, base)?;
        let width = read_i32(payload, 8, base)?;
        let color = read_color(payload, 16, base)?;
        self.add_object(
            handle,
            GdiObject::Pen(Pen {
                style: pen_style(style),
                width: f64::from(width.unsigned_abs().max(1)),
                cosmetic: true,
                color,
            }),
            record_index,
        )
    }

    fn create_ext_pen(
        &mut self,
        payload: &[u8],
        record_index: usize,
        base: usize,
        diagnostics: &mut Vec<Diagnostic>,
        strict: bool,
        record: Record<'_>,
    ) -> Result<()> {
        let handle = read_u32(payload, 0, base)?;
        let style = read_u32(payload, 20, base)?;
        let width = read_u32(payload, 24, base)?;
        let brush_style = read_u32(payload, 28, base)?;
        let color = read_color(payload, 32, base)?;
        if brush_style != 0 {
            self.unsupported(
                record,
                diagnostics,
                strict,
                "extended_pen_brush",
                "extended pens with non-solid brushes are not supported",
            )?;
        }
        self.add_object(
            handle,
            GdiObject::Pen(Pen {
                style: pen_style(style),
                width: f64::from(width.max(1)),
                cosmetic: style & 0x0001_0000 == 0,
                color,
            }),
            record_index,
        )
    }

    fn create_brush(
        &mut self,
        payload: &[u8],
        record_index: usize,
        base: usize,
        record: Record<'_>,
        diagnostics: &mut Vec<Diagnostic>,
        strict: bool,
    ) -> Result<()> {
        let handle = read_u32(payload, 0, base)?;
        let style = read_u32(payload, 4, base)?;
        let color = read_color(payload, 8, base)?;
        let hatch = read_u32(payload, 12, base)? as u16;
        let style = match style {
            0 => BrushStyle::Solid,
            1 => BrushStyle::Null,
            2 => BrushStyle::Hatched(hatch),
            3 => BrushStyle::Pattern,
            other => BrushStyle::Unknown(other as u16),
        };
        if !matches!(style, BrushStyle::Solid | BrushStyle::Null) {
            self.unsupported(
                record,
                diagnostics,
                strict,
                "unsupported_brush_style",
                "hatched and pattern brush fills are not represented by the SVG renderer",
            )?;
        }
        self.add_object(
            handle,
            GdiObject::Brush(Brush { style, color }),
            record_index,
        )
    }

    fn create_font(&mut self, payload: &[u8], record_index: usize, base: usize) -> Result<()> {
        let handle = read_u32(payload, 0, base)?;
        require(payload, 4, 92, base)?;
        let font_data = &payload[4..];
        let mut units = Vec::new();
        for pair in font_data[28..92].chunks_exact(2) {
            let unit = u16::from_le_bytes([pair[0], pair[1]]);
            if unit == 0 {
                break;
            }
            units.push(unit);
        }
        let font = Font {
            height: f64::from(read_i32(font_data, 0, base + 4)?),
            width: f64::from(read_i32(font_data, 4, base + 4)?),
            escapement_tenths: read_i32(font_data, 8, base + 4)?
                .clamp(i32::from(i16::MIN), i32::from(i16::MAX))
                as i16,
            weight: read_i32(font_data, 16, base + 4)?.clamp(0, i32::from(u16::MAX)) as u16,
            italic: font_data[20] != 0,
            underline: font_data[21] != 0,
            strike_out: font_data[22] != 0,
            charset: font_data[23],
            family: if units.is_empty() {
                "sans-serif".into()
            } else {
                String::from_utf16_lossy(&units)
            },
        };
        self.add_object(handle, GdiObject::Font(font), record_index)
    }

    fn select_object(&mut self, payload: &[u8], record_index: usize, base: usize) -> Result<()> {
        let handle = read_u32(payload, 0, base)?;
        let object = if handle & 0x8000_0000 != 0 {
            stock_object(handle & 0x7fff_ffff).ok_or(MetafileError::InvalidObjectHandle {
                handle,
                record_index,
            })?
        } else {
            self.objects
                .get(&handle)
                .cloned()
                .ok_or(MetafileError::InvalidObjectHandle {
                    handle,
                    record_index,
                })?
        };
        match object {
            GdiObject::Pen(pen) => {
                self.dc.core.pen = pen;
                self.dc.selected_pen = (handle & 0x8000_0000 == 0).then_some(handle);
            }
            GdiObject::Brush(brush) => {
                self.dc.core.brush = brush;
                self.dc.selected_brush = (handle & 0x8000_0000 == 0).then_some(handle);
            }
            GdiObject::Font(font) => {
                self.dc.core.font = font;
                self.dc.selected_font = (handle & 0x8000_0000 == 0).then_some(handle);
            }
        }
        Ok(())
    }

    fn delete_object(&mut self, payload: &[u8], record_index: usize, base: usize) -> Result<()> {
        let handle = read_u32(payload, 0, base)?;
        let used = std::iter::once(&self.dc)
            .chain(self.stack.iter())
            .any(|dc| {
                dc.selected_pen == Some(handle)
                    || dc.selected_brush == Some(handle)
                    || dc.selected_font == Some(handle)
            });
        if used {
            return Err(MetafileError::ObjectInUse {
                handle,
                record_index,
            });
        }
        if self.objects.remove(&handle).is_none() {
            return Err(MetafileError::InvalidObjectHandle {
                handle,
                record_index,
            });
        }
        Ok(())
    }

    fn intersect_clip(&mut self, payload: &[u8], base: usize) -> Result<()> {
        let rect = self
            .transform_rect(read_rect_at(payload, 0, base)?)
            .normalized();
        self.dc.core.clip = Some(match self.dc.core.clip {
            Some(current) => current.intersection(rect).unwrap_or(Rect {
                left: rect.left,
                top: rect.top,
                right: rect.left,
                bottom: rect.top,
            }),
            None => rect,
        });
        Ok(())
    }

    fn line_to(&mut self, payload: &[u8], base: usize, renderer: &mut dyn Renderer) -> Result<()> {
        let logical_to = read_point(payload, 0, base)?;
        let from = self.transform(self.dc.core.current_position);
        let to = self.transform(logical_to);
        if self.active_path.is_some() {
            self.add_path_points(1)?;
        }
        if let Some(path) = &mut self.active_path {
            path.line_to(from, to);
        } else {
            renderer.line(from, to, &self.effective_pen(), self.clip())?;
        }
        self.dc.core.current_position = logical_to;
        Ok(())
    }

    fn draw_box_record(&mut self, record: Record<'_>, renderer: &mut dyn Renderer) -> Result<()> {
        let payload = &record.bytes[8..];
        let pen = self.effective_pen();
        match record.kind {
            42 => renderer.ellipse(
                self.transform_rect(read_rect_at(payload, 0, record.offset + 8)?),
                &pen,
                &self.dc.core.brush,
                self.clip(),
            ),
            43 => renderer.rectangle(
                self.transform_rect(read_rect_at(payload, 0, record.offset + 8)?),
                None,
                &pen,
                &self.dc.core.brush,
                self.clip(),
            ),
            44 => {
                let rect = self.transform_rect(read_rect_at(payload, 0, record.offset + 8)?);
                let size = self.vector(Vector::new(
                    f64::from(read_i32(payload, 16, record.offset + 8)?),
                    f64::from(read_i32(payload, 20, record.offset + 8)?),
                ));
                renderer.rectangle(
                    rect,
                    Some(Point::new(size.x, size.y)),
                    &pen,
                    &self.dc.core.brush,
                    self.clip(),
                )
            }
            45 | 46 | 47 | 55 => {
                let rect = self.transform_rect(read_rect_at(payload, 0, record.offset + 8)?);
                let start = self.transform(read_point(payload, 16, record.offset + 8)?);
                let end = self.transform(read_point(payload, 24, record.offset + 8)?);
                let clockwise = self.dc.arc_clockwise ^ self.orientation_reversed();
                let kind = match record.kind {
                    46 => ArcKind::Chord,
                    47 => ArcKind::Pie,
                    _ => ArcKind::Arc,
                };
                let (arc_start, segments) = ellipse_arc_segments(rect, start, end, clockwise);
                if self.active_path.is_some() {
                    self.add_path_points(segments.len().saturating_mul(3).saturating_add(2))?;
                    let current = self.transform(self.dc.core.current_position);
                    let center = Point::new(
                        rect.left.midpoint(rect.right),
                        rect.top.midpoint(rect.bottom),
                    );
                    let Some(path) = self.active_path.as_mut() else {
                        return Ok(());
                    };
                    match kind {
                        ArcKind::Pie => {
                            path.move_to(center);
                            path.line_to(center, arc_start);
                        }
                        ArcKind::Chord | ArcKind::Arc => {
                            if record.kind == 55 {
                                path.line_to(current, arc_start);
                            } else {
                                path.move_to(arc_start);
                            }
                        }
                    }
                    let mut cursor = arc_start;
                    for (control1, control2, to) in segments {
                        path.bezier_to(cursor, control1, control2, to);
                        cursor = to;
                    }
                    if matches!(kind, ArcKind::Pie | ArcKind::Chord) {
                        path.close();
                    }
                    if record.kind == 55 {
                        self.dc.core.current_position = read_point(payload, 24, record.offset + 8)?;
                    }
                    return Ok(());
                }
                if record.kind == 55 {
                    renderer.line(
                        self.transform(self.dc.core.current_position),
                        arc_start,
                        &pen,
                        self.clip(),
                    )?;
                }
                renderer.arc(
                    rect,
                    start,
                    end,
                    kind,
                    clockwise,
                    &pen,
                    &self.dc.core.brush,
                    self.clip(),
                )?;
                if record.kind == 55 {
                    self.dc.core.current_position = read_point(payload, 24, record.offset + 8)?;
                }
                Ok(())
            }
            _ => unreachable!(),
        }
    }

    fn poly_record(&mut self, record: Record<'_>, renderer: &mut dyn Renderer) -> Result<()> {
        let sixteen = record.kind >= 85;
        let normalized_kind = if sixteen {
            record.kind - 83
        } else {
            record.kind
        };
        if matches!(normalized_kind, 7 | 8) {
            return self.poly_poly(record, renderer, sixteen, normalized_kind == 8);
        }
        let payload = &record.bytes[8..];
        let count = usize::try_from(read_u32(payload, 16, record.offset + 8)?)
            .map_err(|_| MetafileError::InvalidHeader("point count overflow".into()))?;
        self.check_points(count)?;
        let logical_points = read_points(payload, 20, count, sixteen, record.offset + 8)?;
        let last_logical = logical_points.last().copied();
        let points: Vec<Point> = logical_points
            .into_iter()
            .map(|point| self.transform(point))
            .collect();
        let pen = self.effective_pen();
        match normalized_kind {
            2 => self.bezier(points, false, renderer),
            3 => renderer.polygon(
                &points,
                &pen,
                &self.dc.core.brush,
                self.dc.core.polygon_fill_mode,
                self.clip(),
            ),
            4 => renderer.polyline(&points, &pen, self.clip()),
            5 => {
                self.bezier(points, true, renderer)?;
                if let Some(last) = last_logical {
                    self.dc.core.current_position = last;
                }
                Ok(())
            }
            6 => {
                let from = self.transform(self.dc.core.current_position);
                if self.active_path.is_some() {
                    self.add_path_points(points.len())?;
                }
                if let Some(path) = &mut self.active_path {
                    let mut previous = from;
                    for point in &points {
                        path.line_to(previous, *point);
                        previous = *point;
                    }
                } else {
                    let mut all = Vec::with_capacity(points.len() + 1);
                    all.push(from);
                    all.extend_from_slice(&points);
                    renderer.polyline(&all, &pen, self.clip())?;
                }
                if let Some(last) = last_logical {
                    self.dc.core.current_position = last;
                }
                Ok(())
            }
            _ => unreachable!(),
        }
    }

    fn poly_poly(
        &mut self,
        record: Record<'_>,
        renderer: &mut dyn Renderer,
        sixteen: bool,
        polygon: bool,
    ) -> Result<()> {
        let payload = &record.bytes[8..];
        let groups = usize::try_from(read_u32(payload, 16, record.offset + 8)?)
            .map_err(|_| MetafileError::InvalidHeader("polygon count overflow".into()))?;
        let total = usize::try_from(read_u32(payload, 20, record.offset + 8)?)
            .map_err(|_| MetafileError::InvalidHeader("point count overflow".into()))?;
        self.check_points(total)?;
        if groups > self.limits.max_points_per_record {
            return Err(MetafileError::ResourceLimitExceeded {
                resource: "polygons per record",
                actual: groups as u64,
                limit: self.limits.max_points_per_record as u64,
            });
        }
        let counts_bytes = groups
            .checked_mul(4)
            .ok_or_else(|| MetafileError::InvalidHeader("polygon counts overflow".into()))?;
        require(payload, 24, counts_bytes, record.offset + 8)?;
        let mut counts = Vec::with_capacity(groups);
        let mut sum = 0usize;
        for index in 0..groups {
            let count = usize::try_from(read_u32(payload, 24 + index * 4, record.offset + 8)?)
                .map_err(|_| MetafileError::InvalidHeader("polygon count overflow".into()))?;
            sum = sum
                .checked_add(count)
                .ok_or_else(|| MetafileError::InvalidHeader("point count overflow".into()))?;
            counts.push(count);
        }
        if sum != total {
            return Err(MetafileError::InvalidHeader(
                "PolyPoly point counts do not match total".into(),
            ));
        }
        let points = read_points(
            payload,
            24 + counts_bytes,
            total,
            sixteen,
            record.offset + 8,
        )?;
        let mut cursor = 0;
        let mut output = Vec::with_capacity(groups);
        for count in counts {
            output.push(
                points[cursor..cursor + count]
                    .iter()
                    .map(|point| self.transform(*point))
                    .collect(),
            );
            cursor += count;
        }
        let pen = self.effective_pen();
        if polygon {
            renderer.poly_polygon(
                &output,
                &pen,
                &self.dc.core.brush,
                self.dc.core.polygon_fill_mode,
                self.clip(),
            )
        } else {
            for line in output {
                renderer.polyline(&line, &pen, self.clip())?;
            }
            Ok(())
        }
    }

    fn bezier(&mut self, points: Vec<Point>, to: bool, renderer: &mut dyn Renderer) -> Result<()> {
        let mut cursor = if to {
            self.transform(self.dc.core.current_position)
        } else {
            *points
                .first()
                .ok_or_else(|| MetafileError::InvalidHeader("empty PolyBezier".into()))?
        };
        let controls = if to { &points[..] } else { &points[1..] };
        if controls.len() % 3 != 0 {
            return Err(MetafileError::InvalidHeader(
                "PolyBezier point count is not 3n+1/3n".into(),
            ));
        }
        let mut path = PathBuilder::new();
        path.move_to(cursor);
        for chunk in controls.chunks_exact(3) {
            path.bezier_to(cursor, chunk[0], chunk[1], chunk[2]);
            cursor = chunk[2];
        }
        if self.active_path.is_some() {
            // Each cubic contributes three stored points.
            let starts_figure = usize::from(
                self.active_path
                    .as_ref()
                    .is_some_and(|active| active.current.is_none()),
            );
            self.add_path_points(controls.len() + starts_figure)?;
        }
        if let Some(active) = &mut self.active_path {
            for figure in path.path.figures {
                if active.current.is_none() {
                    active.move_to(figure.start);
                }
                for segment in figure.segments {
                    if let PathSegment::Cubic {
                        control1,
                        control2,
                        to,
                    } = segment
                    {
                        active.bezier_to(figure.start, control1, control2, to);
                    }
                }
            }
        } else {
            renderer.path(
                &path.path,
                &self.effective_pen(),
                &self.dc.core.brush,
                self.dc.core.polygon_fill_mode,
                true,
                false,
                self.clip(),
            )?;
        }
        Ok(())
    }

    fn end_path(
        &mut self,
        record: Record<'_>,
        diagnostics: &mut Vec<Diagnostic>,
        strict: bool,
    ) -> Result<()> {
        if let Some(path) = self.active_path.take() {
            self.completed_path = Some(path.path);
            Ok(())
        } else {
            self.unsupported(
                record,
                diagnostics,
                strict,
                "end_without_path",
                "EMR_ENDPATH outside an active path",
            )
        }
    }

    fn render_path(
        &mut self,
        record: Record<'_>,
        renderer: &mut dyn Renderer,
        stroke: bool,
        fill: bool,
        diagnostics: &mut Vec<Diagnostic>,
        strict: bool,
    ) -> Result<()> {
        let Some(path) = self.completed_path.take() else {
            return self.unsupported(
                record,
                diagnostics,
                strict,
                "render_without_path",
                "path paint record has no completed path",
            );
        };
        self.path_points = 0;
        renderer.path(
            &path,
            &self.effective_pen(),
            &self.dc.core.brush,
            self.dc.core.polygon_fill_mode,
            stroke,
            fill,
            self.clip(),
        )
    }

    fn ext_text_out(
        &mut self,
        record: Record<'_>,
        renderer: &mut dyn Renderer,
        diagnostics: &mut Vec<Diagnostic>,
        strict: bool,
    ) -> Result<()> {
        let bytes = record.bytes;
        require(bytes, 0, 76, record.offset)?;
        let reference = read_point(bytes, 36, record.offset)?;
        let count = usize::try_from(read_u32(bytes, 44, record.offset)?)
            .map_err(|_| MetafileError::InvalidHeader("text count overflow".into()))?;
        if count > self.limits.max_points_per_record {
            return Err(MetafileError::ResourceLimitExceeded {
                resource: "text characters",
                actual: count as u64,
                limit: self.limits.max_points_per_record as u64,
            });
        }
        let string_offset = usize::try_from(read_u32(bytes, 48, record.offset)?)
            .map_err(|_| MetafileError::InvalidHeader("text offset overflow".into()))?;
        let options = read_u32(bytes, 52, record.offset)?;
        if options & 0x10 != 0 {
            return self.unsupported(
                record,
                diagnostics,
                strict,
                "glyph_index_text",
                "ETO_GLYPH_INDEX text cannot be decoded without the source font glyph map",
            );
        }
        if options & 0x80 != 0 {
            self.unsupported(
                record,
                diagnostics,
                strict,
                "rtl_text",
                "ETO_RTLREADING layout is not represented",
            )?;
        }
        let opaque_rect = read_rect_at(bytes, 56, record.offset)?;
        let dx_offset = usize::try_from(read_u32(bytes, 72, record.offset)?)
            .map_err(|_| MetafileError::InvalidHeader("dx offset overflow".into()))?;
        let text = if record.kind == 84 {
            let byte_count = count
                .checked_mul(2)
                .ok_or_else(|| MetafileError::InvalidHeader("UTF-16 length overflow".into()))?;
            require(bytes, string_offset, byte_count, record.offset)?;
            let units: Vec<u16> = bytes[string_offset..string_offset + byte_count]
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            if let Ok(text) = String::from_utf16(&units) {
                text
            } else {
                self.unsupported(
                    record,
                    diagnostics,
                    strict,
                    "text_decode_replacement",
                    "invalid UTF-16 text was replaced",
                )?;
                String::from_utf16_lossy(&units)
            }
        } else {
            require(bytes, string_offset, count, record.offset)?;
            let (decoded, fallback_charset, malformed) = decode_ansi_text(
                &bytes[string_offset..string_offset + count],
                self.dc.core.font.charset,
            );
            if fallback_charset {
                self.unsupported(
                    record,
                    diagnostics,
                    strict,
                    "unknown_charset",
                    "unknown font charset was decoded as Windows-1252",
                )?;
            }
            if malformed {
                self.unsupported(
                    record,
                    diagnostics,
                    strict,
                    "text_decode_replacement",
                    "invalid ANSI text bytes were replaced",
                )?;
            }
            decoded
        };
        let mut dx = Vec::new();
        let mut logical_advance = 0.0;
        if dx_offset != 0 {
            let dx_count = if options & 0x2000 != 0 {
                count
                    .checked_mul(2)
                    .ok_or_else(|| MetafileError::InvalidHeader("PDY count overflow".into()))?
            } else {
                count
            };
            require(
                bytes,
                dx_offset,
                dx_count
                    .checked_mul(4)
                    .ok_or_else(|| MetafileError::InvalidHeader("dx length overflow".into()))?,
                record.offset,
            )?;
            for index in 0..count {
                let advance = f64::from(read_i32(bytes, dx_offset + index * 4, record.offset)?);
                logical_advance += advance;
                dx.push(self.vector(Vector::new(advance, 0.0)).x);
            }
            if options & 0x2000 != 0 {
                self.unsupported(
                    record,
                    diagnostics,
                    strict,
                    "pdy_approximate",
                    "ETO_PDY vertical advances are not represented by SVG text",
                )?;
            }
        } else {
            self.unsupported(
                record,
                diagnostics,
                strict,
                "text_metrics_approximate",
                "text has no explicit advances; host SVG font metrics are approximate",
            )?;
        }
        let position = if self.dc.core.text_align & 1 != 0 {
            self.transform(self.dc.core.current_position)
        } else {
            self.transform(reference)
        };
        let clip_rect = (options & 4 != 0).then(|| self.transform_rect(opaque_rect));
        let run = metafile_core::TextRun {
            position,
            text,
            font: {
                let mut font = self.dc.core.font.clone();
                font.height = self.vector(Vector::new(0.0, font.height)).y.abs().max(1.0);
                font
            },
            color: self.dc.core.text_color,
            background: (options & 2 != 0 || self.dc.core.background_opaque)
                .then_some(self.dc.core.background_color),
            background_rect: (options & 2 != 0).then(|| self.transform_rect(opaque_rect)),
            horizontal_align: match self.dc.core.text_align & 6 {
                6 => HorizontalTextAlignment::Center,
                2 => HorizontalTextAlignment::Right,
                _ => HorizontalTextAlignment::Left,
            },
            vertical_align: if self.dc.core.text_align & 24 == 24 {
                VerticalTextAlignment::Baseline
            } else if self.dc.core.text_align & 8 != 0 {
                VerticalTextAlignment::Bottom
            } else {
                VerticalTextAlignment::Top
            },
            clip: combine_clip(self.clip(), clip_rect),
            dx: dx.clone(),
        };
        renderer.text(&run)?;
        if self.dc.core.text_align & 1 != 0 {
            let advance = if dx.is_empty() {
                self.dc.core.font.height.abs().max(1.0) * run.text.chars().count() as f64 * 0.6
            } else {
                logical_advance
            };
            let angle = f64::from(self.dc.core.font.escapement_tenths).to_radians() / 10.0;
            self.dc.core.current_position.x += advance * angle.cos();
            self.dc.core.current_position.y -= advance * angle.sin();
        }
        Ok(())
    }

    fn stretch_dibits(
        &mut self,
        record: Record<'_>,
        renderer: &mut dyn Renderer,
        diagnostics: &mut Vec<Diagnostic>,
        strict: bool,
    ) -> Result<()> {
        let bytes = record.bytes;
        require(bytes, 0, 80, record.offset)?;
        let destination_x = read_i32(bytes, 24, record.offset)?;
        let destination_y = read_i32(bytes, 28, record.offset)?;
        let source_x = read_i32(bytes, 32, record.offset)?;
        let source_y = read_i32(bytes, 36, record.offset)?;
        let source_width = read_i32(bytes, 40, record.offset)?;
        let source_height = read_i32(bytes, 44, record.offset)?;
        let bmi_offset = usize::try_from(read_u32(bytes, 48, record.offset)?).map_err(|_| {
            MetafileError::InvalidBitmap {
                record_index: record.index,
                message: "BMI offset overflow".into(),
            }
        })?;
        let bmi_size = usize::try_from(read_u32(bytes, 52, record.offset)?).map_err(|_| {
            MetafileError::InvalidBitmap {
                record_index: record.index,
                message: "BMI size overflow".into(),
            }
        })?;
        let bits_offset = usize::try_from(read_u32(bytes, 56, record.offset)?).map_err(|_| {
            MetafileError::InvalidBitmap {
                record_index: record.index,
                message: "bits offset overflow".into(),
            }
        })?;
        let bits_size = usize::try_from(read_u32(bytes, 60, record.offset)?).map_err(|_| {
            MetafileError::InvalidBitmap {
                record_index: record.index,
                message: "bits size overflow".into(),
            }
        })?;
        let usage = read_u32(bytes, 64, record.offset)?;
        let rop = read_u32(bytes, 68, record.offset)?;
        let destination_width = read_i32(bytes, 72, record.offset)?;
        let destination_height = read_i32(bytes, 76, record.offset)?;
        if usage != 0 {
            return self.unsupported(
                record,
                diagnostics,
                strict,
                "dib_palette_usage",
                "only DIB_RGB_COLORS is supported",
            );
        }
        if rop != 0x00cc_0020 {
            return self.unsupported(
                record,
                diagnostics,
                strict,
                "bitmap_rop",
                &format!("bitmap ROP {rop:#010x} is not SRCCOPY"),
            );
        }
        require(bytes, bmi_offset, bmi_size, record.offset)?;
        require(bytes, bits_offset, bits_size, record.offset)?;
        let dib_len =
            bmi_size
                .checked_add(bits_size)
                .ok_or_else(|| MetafileError::InvalidBitmap {
                    record_index: record.index,
                    message: "DIB length overflow".into(),
                })?;
        let mut dib = Vec::with_capacity(dib_len);
        dib.extend_from_slice(&bytes[bmi_offset..bmi_offset + bmi_size]);
        dib.extend_from_slice(&bytes[bits_offset..bits_offset + bits_size]);
        match decode_dib(&dib, record.index, &self.limits).and_then(|decoded| {
            crop_bitmap(
                decoded.bitmap,
                decoded.top_down,
                source_x,
                source_y,
                source_width,
                source_height,
                record.index,
            )
        }) {
            Ok(bitmap) => {
                let top_left = self.transform(Point::new(
                    f64::from(destination_x),
                    f64::from(destination_y),
                ));
                let bottom_right = self.transform(Point::new(
                    f64::from(destination_x) + f64::from(destination_width),
                    f64::from(destination_y) + f64::from(destination_height),
                ));
                let sampling = match self.dc.core.stretch_mode {
                    3 => BitmapSampling::Pixelated,
                    4 => {
                        self.unsupported(
                            record,
                            diagnostics,
                            strict,
                            "approximate_stretch_mode",
                            "HALFTONE is approximated with smooth SVG interpolation",
                        )?;
                        BitmapSampling::Smooth
                    }
                    1 | 2 => {
                        self.unsupported(
                            record,
                            diagnostics,
                            strict,
                            "unsupported_stretch_mode",
                            "monochrome stretch modes are not reproduced by SVG interpolation",
                        )?;
                        BitmapSampling::Auto
                    }
                    _ => BitmapSampling::Auto,
                };
                renderer.bitmap(
                    Rect {
                        left: top_left.x,
                        top: top_left.y,
                        right: bottom_right.x,
                        bottom: bottom_right.y,
                    },
                    &bitmap,
                    sampling,
                    self.clip(),
                )
            }
            Err(MetafileError::UnsupportedBitmap { message, .. }) if !strict => {
                self.diagnostic(diagnostics, record, "unsupported_bitmap", &message);
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn check_points(&self, count: usize) -> Result<()> {
        if count > self.limits.max_points_per_record {
            Err(MetafileError::ResourceLimitExceeded {
                resource: "points per record",
                actual: count as u64,
                limit: self.limits.max_points_per_record as u64,
            })
        } else {
            Ok(())
        }
    }

    fn add_path_points(&mut self, count: usize) -> Result<()> {
        self.path_points =
            self.path_points
                .checked_add(count)
                .ok_or(MetafileError::ResourceLimitExceeded {
                    resource: "path points",
                    actual: u64::MAX,
                    limit: self.limits.max_points_per_record as u64,
                })?;
        if self.path_points > self.limits.max_points_per_record {
            return Err(MetafileError::ResourceLimitExceeded {
                resource: "path points",
                actual: self.path_points as u64,
                limit: self.limits.max_points_per_record as u64,
            });
        }
        Ok(())
    }

    fn unsupported(
        &self,
        record: Record<'_>,
        diagnostics: &mut Vec<Diagnostic>,
        strict: bool,
        code: &str,
        message: &str,
    ) -> Result<()> {
        if strict {
            return Err(MetafileError::UnsupportedCriticalFeature(format!(
                "{} at record {}: {message}",
                record_name(record.kind),
                record.index
            )));
        }
        self.diagnostic(diagnostics, record, code, message);
        Ok(())
    }

    fn diagnostic(
        &self,
        diagnostics: &mut Vec<Diagnostic>,
        record: Record<'_>,
        code: &str,
        message: &str,
    ) {
        if let Some(existing) = diagnostics.iter_mut().find(|diagnostic| {
            diagnostic.code == code && diagnostic.record_type == Some(record.kind)
        }) {
            existing.occurrences = existing.occurrences.saturating_add(1);
        } else if diagnostics.len() < self.limits.max_diagnostics {
            diagnostics.push(Diagnostic::warning(code, message).at_record(
                record.index,
                record.kind,
                record_name(record.kind),
                record.offset,
            ));
        }
    }
}

fn read_points(
    data: &[u8],
    offset: usize,
    count: usize,
    sixteen: bool,
    base: usize,
) -> Result<Vec<Point>> {
    let point_size = if sixteen { 4 } else { 8 };
    let bytes = count
        .checked_mul(point_size)
        .ok_or_else(|| MetafileError::InvalidHeader("point data overflow".into()))?;
    require(data, offset, bytes, base)?;
    let mut points = Vec::with_capacity(count);
    for index in 0..count {
        let start = offset + index * point_size;
        points.push(if sixteen {
            Point::new(
                f64::from(read_i16(data, start, base)?),
                f64::from(read_i16(data, start + 2, base)?),
            )
        } else {
            read_point(data, start, base)?
        });
    }
    Ok(points)
}

fn read_rect(reader: &mut Reader<'_>) -> Result<Rect> {
    Ok(Rect {
        left: f64::from(reader.i32()?),
        top: f64::from(reader.i32()?),
        right: f64::from(reader.i32()?),
        bottom: f64::from(reader.i32()?),
    })
}

fn read_rect_at(data: &[u8], offset: usize, base: usize) -> Result<Rect> {
    require(data, offset, 16, base)?;
    Ok(Rect {
        left: f64::from(read_i32(data, offset, base)?),
        top: f64::from(read_i32(data, offset + 4, base)?),
        right: f64::from(read_i32(data, offset + 8, base)?),
        bottom: f64::from(read_i32(data, offset + 12, base)?),
    })
}

fn read_point(data: &[u8], offset: usize, base: usize) -> Result<Point> {
    Ok(Point::new(
        f64::from(read_i32(data, offset, base)?),
        f64::from(read_i32(data, offset + 4, base)?),
    ))
}

fn read_transform(data: &[u8], offset: usize, base: usize) -> Result<Transform> {
    Ok(Transform {
        m11: f64::from(read_f32(data, offset, base)?),
        m12: f64::from(read_f32(data, offset + 4, base)?),
        m21: f64::from(read_f32(data, offset + 8, base)?),
        m22: f64::from(read_f32(data, offset + 12, base)?),
        dx: f64::from(read_f32(data, offset + 16, base)?),
        dy: f64::from(read_f32(data, offset + 20, base)?),
    })
}

fn read_color(data: &[u8], offset: usize, base: usize) -> Result<Color> {
    let color = read_u32(data, offset, base)?;
    Ok(Color::rgb(
        color as u8,
        (color >> 8) as u8,
        (color >> 16) as u8,
    ))
}

fn read_u32(data: &[u8], offset: usize, base: usize) -> Result<u32> {
    require(data, offset, 4, base)?;
    Ok(u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]))
}
fn read_i32(data: &[u8], offset: usize, base: usize) -> Result<i32> {
    require(data, offset, 4, base)?;
    Ok(i32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]))
}
fn read_i16(data: &[u8], offset: usize, base: usize) -> Result<i16> {
    require(data, offset, 2, base)?;
    Ok(i16::from_le_bytes([data[offset], data[offset + 1]]))
}
fn read_f32(data: &[u8], offset: usize, base: usize) -> Result<f32> {
    Ok(f32::from_bits(read_u32(data, offset, base)?))
}
fn le_u32(data: &[u8], offset: usize) -> Result<u32> {
    read_u32(data, offset, 0)
}

fn require(data: &[u8], offset: usize, length: usize, base: usize) -> Result<()> {
    let end = offset
        .checked_add(length)
        .ok_or(MetafileError::TruncatedInput {
            offset: base.saturating_add(offset),
            needed: length,
            available: 0,
        })?;
    if end > data.len() {
        return Err(MetafileError::TruncatedInput {
            offset: base.saturating_add(offset),
            needed: length,
            available: data.len().saturating_sub(offset),
        });
    }
    Ok(())
}

fn pen_style(style: u32) -> PenStyle {
    match style & 0x0f {
        0 => PenStyle::Solid,
        1 => PenStyle::Dash,
        2 => PenStyle::Dot,
        3 => PenStyle::DashDot,
        4 => PenStyle::DashDotDot,
        5 => PenStyle::Null,
        6 => PenStyle::InsideFrame,
        other => PenStyle::Unknown(other as u16),
    }
}

fn stock_object(index: u32) -> Option<GdiObject> {
    Some(match index {
        0 => GdiObject::Brush(Brush {
            style: BrushStyle::Solid,
            color: Color::WHITE,
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
            color: Color::WHITE,
            ..Pen::default()
        }),
        7 => GdiObject::Pen(Pen::default()),
        8 => GdiObject::Pen(Pen {
            style: PenStyle::Null,
            ..Pen::default()
        }),
        10..=17 => GdiObject::Font(Font::default()),
        18 => GdiObject::Brush(Brush::default()),
        19 => GdiObject::Pen(Pen::default()),
        _ => return None,
    })
}

fn combine_clip(a: Option<Rect>, b: Option<Rect>) -> Option<Rect> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.intersection(b).unwrap_or(Rect {
            left: b.left,
            top: b.top,
            right: b.left,
            bottom: b.top,
        })),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

fn ellipse_arc_segments(
    rect: Rect,
    start: Point,
    end: Point,
    clockwise: bool,
) -> (Point, Vec<(Point, Point, Point)>) {
    let rect = rect.normalized();
    let center_x = rect.left.midpoint(rect.right);
    let center_y = rect.top.midpoint(rect.bottom);
    let radius_x = rect.width() / 2.0;
    let radius_y = rect.height() / 2.0;
    let angle = |point: Point| {
        ((point.y - center_y) / radius_y.max(f64::EPSILON))
            .atan2((point.x - center_x) / radius_x.max(f64::EPSILON))
    };
    let start_angle = angle(start);
    let end_angle = angle(end);
    let mut sweep = end_angle - start_angle;
    if clockwise {
        if sweep <= 0.0 {
            sweep += std::f64::consts::TAU;
        }
    } else if sweep >= 0.0 {
        sweep -= std::f64::consts::TAU;
    }
    let count = (sweep.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = sweep / count as f64;
    let point = |angle: f64| {
        Point::new(
            center_x + radius_x * angle.cos(),
            center_y + radius_y * angle.sin(),
        )
    };
    let first = point(start_angle);
    let mut segments = Vec::with_capacity(count);
    for index in 0..count {
        let from_angle = start_angle + index as f64 * step;
        let to_angle = from_angle + step;
        let from = point(from_angle);
        let to = point(to_angle);
        let alpha = 4.0 / 3.0 * (step / 4.0).tan();
        let control1 = Point::new(
            from.x - alpha * radius_x * from_angle.sin(),
            from.y + alpha * radius_y * from_angle.cos(),
        );
        let control2 = Point::new(
            to.x + alpha * radius_x * to_angle.sin(),
            to.y - alpha * radius_y * to_angle.cos(),
        );
        segments.push((control1, control2, to));
    }
    (first, segments)
}

fn decode_ansi_text(bytes: &[u8], charset: u8) -> (String, bool, bool) {
    let (encoding, fallback) = match charset {
        0 | 1 | 2 | 77 | 255 => (WINDOWS_1252, false),
        128 => (
            Encoding::for_label(b"shift_jis").unwrap_or(WINDOWS_1252),
            false,
        ),
        129 => (
            Encoding::for_label(b"euc-kr").unwrap_or(WINDOWS_1252),
            false,
        ),
        134 => (Encoding::for_label(b"gbk").unwrap_or(WINDOWS_1252), false),
        136 => (Encoding::for_label(b"big5").unwrap_or(WINDOWS_1252), false),
        161 => (
            Encoding::for_label(b"windows-1253").unwrap_or(WINDOWS_1252),
            false,
        ),
        162 => (
            Encoding::for_label(b"windows-1254").unwrap_or(WINDOWS_1252),
            false,
        ),
        177 => (
            Encoding::for_label(b"windows-1255").unwrap_or(WINDOWS_1252),
            false,
        ),
        178 => (
            Encoding::for_label(b"windows-1256").unwrap_or(WINDOWS_1252),
            false,
        ),
        186 => (
            Encoding::for_label(b"windows-1257").unwrap_or(WINDOWS_1252),
            false,
        ),
        204 => (
            Encoding::for_label(b"windows-1251").unwrap_or(WINDOWS_1252),
            false,
        ),
        222 => (
            Encoding::for_label(b"windows-874").unwrap_or(WINDOWS_1252),
            false,
        ),
        238 => (
            Encoding::for_label(b"windows-1250").unwrap_or(WINDOWS_1252),
            false,
        ),
        _ => (WINDOWS_1252, true),
    };
    let (text, _, malformed) = encoding.decode(bytes);
    (text.into_owned(), fallback, malformed)
}

fn record_name(kind: u32) -> &'static str {
    match kind {
        1 => "EMR_HEADER",
        2 => "EMR_POLYBEZIER",
        3 => "EMR_POLYGON",
        4 => "EMR_POLYLINE",
        5 => "EMR_POLYBEZIERTO",
        6 => "EMR_POLYLINETO",
        7 => "EMR_POLYPOLYLINE",
        8 => "EMR_POLYPOLYGON",
        9 => "EMR_SETWINDOWEXTEX",
        10 => "EMR_SETWINDOWORGEX",
        11 => "EMR_SETVIEWPORTEXTEX",
        12 => "EMR_SETVIEWPORTORGEX",
        14 => "EMR_EOF",
        15 => "EMR_SETPIXELV",
        16 => "EMR_SETMAPPERFLAGS",
        17 => "EMR_SETMAPMODE",
        18 => "EMR_SETBKMODE",
        19 => "EMR_SETPOLYFILLMODE",
        20 => "EMR_SETROP2",
        21 => "EMR_SETSTRETCHBLTMODE",
        22 => "EMR_SETTEXTALIGN",
        24 => "EMR_SETTEXTCOLOR",
        25 => "EMR_SETBKCOLOR",
        27 => "EMR_MOVETOEX",
        29 => "EMR_EXCLUDECLIPRECT",
        30 => "EMR_INTERSECTCLIPRECT",
        31 => "EMR_SCALEVIEWPORTEXTEX",
        32 => "EMR_SCALEWINDOWEXTEX",
        33 => "EMR_SAVEDC",
        34 => "EMR_RESTOREDC",
        35 => "EMR_SETWORLDTRANSFORM",
        36 => "EMR_MODIFYWORLDTRANSFORM",
        37 => "EMR_SELECTOBJECT",
        38 => "EMR_CREATEPEN",
        39 => "EMR_CREATEBRUSHINDIRECT",
        40 => "EMR_DELETEOBJECT",
        42 => "EMR_ELLIPSE",
        43 => "EMR_RECTANGLE",
        44 => "EMR_ROUNDRECT",
        45 => "EMR_ARC",
        46 => "EMR_CHORD",
        47 => "EMR_PIE",
        54 => "EMR_LINETO",
        55 => "EMR_ARCTO",
        57 => "EMR_SETARCDIRECTION",
        58 => "EMR_SETMITERLIMIT",
        59 => "EMR_BEGINPATH",
        60 => "EMR_ENDPATH",
        61 => "EMR_CLOSEFIGURE",
        62 => "EMR_FILLPATH",
        63 => "EMR_STROKEANDFILLPATH",
        64 => "EMR_STROKEPATH",
        68 => "EMR_ABORTPATH",
        70 => "EMR_GDICOMMENT",
        80 => "EMR_SETDIBITSTODEVICE",
        81 => "EMR_STRETCHDIBITS",
        82 => "EMR_EXTCREATEFONTINDIRECTW",
        83 => "EMR_EXTTEXTOUTA",
        84 => "EMR_EXTTEXTOUTW",
        85 => "EMR_POLYBEZIER16",
        86 => "EMR_POLYGON16",
        87 => "EMR_POLYLINE16",
        88 => "EMR_POLYBEZIERTO16",
        89 => "EMR_POLYLINETO16",
        90 => "EMR_POLYPOLYLINE16",
        91 => "EMR_POLYPOLYGON16",
        95 => "EMR_EXTCREATEPEN",
        98 => "EMR_SETICMMODE",
        _ => "EMR_UNKNOWN",
    }
}

#[cfg(test)]
mod tests;
