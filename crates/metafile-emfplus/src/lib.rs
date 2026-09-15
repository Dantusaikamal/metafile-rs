//! Safe, first-party EMF+ stream parsing and renderer-independent playback.

use metafile_core::{
    Bitmap, BitmapPlacement, BitmapSampling, ClipOperation, ClipRegion, Color, Diagnostic, Font,
    GradientStop, HorizontalTextAlignment, LineCap, LineJoin, MetafileError, Paint, Path,
    PathFigure, PathSegment, Point, Rect, RenderOptions, Renderer, ResourceLimits, Result, Stroke,
    TextRun, Transform, Vector, VerticalTextAlignment,
};
use serde::{Deserialize, Serialize};

const HEADER: u16 = 0x4001;
const END_OF_FILE: u16 = 0x4002;
const OBJECT: u16 = 0x4008;
const CLEAR: u16 = 0x4009;
const FILL_RECTS: u16 = 0x400a;
const DRAW_RECTS: u16 = 0x400b;
const FILL_POLYGON: u16 = 0x400c;
const DRAW_LINES: u16 = 0x400d;
const FILL_ELLIPSE: u16 = 0x400e;
const DRAW_ELLIPSE: u16 = 0x400f;
const FILL_PIE: u16 = 0x4010;
const DRAW_PIE: u16 = 0x4011;
const DRAW_ARC: u16 = 0x4012;
const FILL_REGION: u16 = 0x4013;
const FILL_PATH: u16 = 0x4014;
const DRAW_PATH: u16 = 0x4015;
const FILL_CLOSED_CURVE: u16 = 0x4016;
const DRAW_CLOSED_CURVE: u16 = 0x4017;
const DRAW_CURVE: u16 = 0x4018;
const DRAW_BEZIERS: u16 = 0x4019;
const DRAW_IMAGE: u16 = 0x401a;
const DRAW_IMAGE_POINTS: u16 = 0x401b;
const DRAW_STRING: u16 = 0x401c;
const SAVE: u16 = 0x4025;
const RESTORE: u16 = 0x4026;
const BEGIN_CONTAINER: u16 = 0x4027;
const BEGIN_CONTAINER_NO_PARAMS: u16 = 0x4028;
const END_CONTAINER: u16 = 0x4029;
const SET_WORLD_TRANSFORM: u16 = 0x402a;
const RESET_WORLD_TRANSFORM: u16 = 0x402b;
const MULTIPLY_WORLD_TRANSFORM: u16 = 0x402c;
const TRANSLATE_WORLD_TRANSFORM: u16 = 0x402d;
const SCALE_WORLD_TRANSFORM: u16 = 0x402e;
const ROTATE_WORLD_TRANSFORM: u16 = 0x402f;
const SET_PAGE_TRANSFORM: u16 = 0x4030;
const RESET_CLIP: u16 = 0x4031;
const SET_CLIP_RECT: u16 = 0x4032;
const SET_CLIP_PATH: u16 = 0x4033;
const SET_CLIP_REGION: u16 = 0x4034;
const OFFSET_CLIP: u16 = 0x4035;
const SET_RENDERING_ORIGIN: u16 = 0x401d;
const SET_SMOOTHING_MODE: u16 = 0x401e;
const SET_TEXT_RENDERING_HINT: u16 = 0x401f;
const SET_TEXT_CONTRAST: u16 = 0x4020;
const SET_INTERPOLATION_MODE: u16 = 0x4021;
const SET_PIXEL_OFFSET_MODE: u16 = 0x4022;
const SET_COMPOSITING_MODE: u16 = 0x4023;
const SET_COMPOSITING_QUALITY: u16 = 0x4024;

#[derive(Debug, Clone, Copy)]
pub struct Comment<'a> {
    pub outer_record_index: usize,
    pub offset: usize,
    /// Bytes following the four-byte `EMF+` signature.
    pub data: &'a [u8],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmfPlusInfo {
    pub dual: bool,
    pub graphics_version: u32,
    pub logical_dpi_x: u32,
    pub logical_dpi_y: u32,
    pub record_count: u32,
    pub object_count: u32,
    pub has_eof: bool,
    pub warnings: Vec<Diagnostic>,
}

#[derive(Debug, Clone)]
pub struct PlaybackResult {
    pub info: EmfPlusInfo,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Copy)]
struct Origin {
    logical_start: usize,
    logical_end: usize,
    outer_record_index: usize,
    absolute_start: usize,
}

struct Stream {
    bytes: Vec<u8>,
    origins: Vec<Origin>,
}

#[derive(Clone, Copy)]
struct Record<'a> {
    index: usize,
    kind: u16,
    flags: u16,
    offset: usize,
    origin: Origin,
    data: &'a [u8],
}

fn assemble(comments: &[Comment<'_>], limits: &ResourceLimits) -> Result<Stream> {
    let total = comments.iter().try_fold(0usize, |total, comment| {
        total
            .checked_add(comment.data.len())
            .ok_or(MetafileError::ResourceLimitExceeded {
                resource: "EMF+ comment bytes",
                actual: u64::MAX,
                limit: limits.max_comment_bytes as u64,
            })
    })?;
    if total > limits.max_comment_bytes {
        return Err(MetafileError::ResourceLimitExceeded {
            resource: "EMF+ comment bytes",
            actual: total as u64,
            limit: limits.max_comment_bytes as u64,
        });
    }
    let mut bytes = Vec::with_capacity(total);
    let mut origins = Vec::with_capacity(comments.len());
    for comment in comments {
        let logical_start = bytes.len();
        bytes.extend_from_slice(comment.data);
        origins.push(Origin {
            logical_start,
            logical_end: bytes.len(),
            outer_record_index: comment.outer_record_index,
            absolute_start: comment.offset,
        });
    }
    Ok(Stream { bytes, origins })
}

fn records<'a>(stream: &'a Stream, limits: &ResourceLimits) -> Result<Vec<Record<'a>>> {
    let mut records = Vec::new();
    let mut offset = 0usize;
    while offset < stream.bytes.len() {
        if records.len() >= limits.max_records {
            return Err(MetafileError::ResourceLimitExceeded {
                resource: "EMF+ records",
                actual: (records.len() + 1) as u64,
                limit: limits.max_records as u64,
            });
        }
        let origin = origin_at(stream, offset);
        if stream.bytes.len() - offset < 12 {
            return invalid(
                origin,
                records.len(),
                0,
                offset,
                "truncated 12-byte record header",
            );
        }
        let kind = u16_at(&stream.bytes, offset)?;
        let flags = u16_at(&stream.bytes, offset + 2)?;
        let size = usize::try_from(u32_at(&stream.bytes, offset + 4)?)
            .map_err(|_| malformed(origin, records.len(), kind, offset, "record size overflow"))?;
        let data_size = usize::try_from(u32_at(&stream.bytes, offset + 8)?)
            .map_err(|_| malformed(origin, records.len(), kind, offset, "data size overflow"))?;
        if size < 12 || size % 4 != 0 || data_size > size - 12 {
            return invalid(
                origin,
                records.len(),
                kind,
                offset,
                "invalid size, alignment, or data size",
            );
        }
        let end = offset
            .checked_add(size)
            .ok_or_else(|| malformed(origin, records.len(), kind, offset, "record end overflow"))?;
        if end > stream.bytes.len() {
            return invalid(
                origin,
                records.len(),
                kind,
                offset,
                "record extends past EMF+ stream",
            );
        }
        let data_end = offset + 12 + data_size;
        records.push(Record {
            index: records.len(),
            kind,
            flags,
            offset,
            origin,
            data: &stream.bytes[offset + 12..data_end],
        });
        offset = end;
    }
    Ok(records)
}

fn origin_at(stream: &Stream, offset: usize) -> Origin {
    stream
        .origins
        .iter()
        .copied()
        .find(|origin| offset >= origin.logical_start && offset < origin.logical_end)
        .unwrap_or(Origin {
            logical_start: 0,
            logical_end: stream.bytes.len(),
            outer_record_index: 0,
            absolute_start: 0,
        })
}

fn malformed(
    origin: Origin,
    index: usize,
    kind: u16,
    offset: usize,
    message: &str,
) -> MetafileError {
    MetafileError::InvalidEmfPlus {
        outer_record_index: origin.outer_record_index,
        inner_record_index: index,
        record_type: kind,
        offset: origin.absolute_start + offset.saturating_sub(origin.logical_start),
        message: message.into(),
    }
}

fn invalid<T>(origin: Origin, index: usize, kind: u16, offset: usize, message: &str) -> Result<T> {
    Err(malformed(origin, index, kind, offset, message))
}

pub fn inspect(comments: &[Comment<'_>], limits: &ResourceLimits) -> Result<EmfPlusInfo> {
    let stream = assemble(comments, limits)?;
    let records = records(&stream, limits)?;
    inspect_records(&records)
}

fn inspect_records(records: &[Record<'_>]) -> Result<EmfPlusInfo> {
    let header = records
        .first()
        .ok_or_else(|| MetafileError::InvalidHeader("empty EMF+ stream".into()))?;
    if header.kind != HEADER || header.data.len() < 16 {
        return invalid(
            header.origin,
            header.index,
            header.kind,
            header.offset,
            "stream does not begin with a complete EmfPlusHeader",
        );
    }
    let graphics_version = u32_at(header.data, 0)?;
    let _emf_plus_flags = u32_at(header.data, 4)?;
    let logical_dpi_x = u32_at(header.data, 8)?;
    let logical_dpi_y = u32_at(header.data, 12)?;
    let object_count = records
        .iter()
        .filter(|record| record.kind == OBJECT)
        .count() as u32;
    let has_eof = records.iter().any(|record| record.kind == END_OF_FILE);
    let mut warnings = Vec::new();
    if !has_eof {
        warnings.push(Diagnostic::warning(
            "emfplus_missing_eof",
            "EMF+ stream has no EndOfFile record",
        ));
    }
    Ok(EmfPlusInfo {
        // The record-header D bit classifies Dual versus Only. The similarly
        // named EmfPlusFlags field in EmfPlusHeaderData describes the display
        // device and must not be used for stream classification.
        dual: header.flags & 1 != 0,
        graphics_version,
        logical_dpi_x,
        logical_dpi_y,
        record_count: records.len() as u32,
        object_count,
        has_eof,
        warnings,
    })
}

#[derive(Debug, Clone)]
enum Object {
    Brush(Paint),
    Pen(Stroke),
    Path(Path),
    Region(ClipRegion),
    Image(Bitmap),
    Font(EmfPlusFont),
    StringFormat(StringFormat),
    ImageAttributes(ImageAttributes),
    Unsupported(String),
}

#[derive(Debug, Clone, Copy)]
struct StringFormat {
    horizontal: HorizontalTextAlignment,
    vertical: VerticalTextAlignment,
    flags: u32,
    trimming: u32,
}

#[derive(Debug, Clone, Copy)]
struct ImageAttributes {
    wrap_mode: u32,
    clamp_color: Color,
    object_clamp: u32,
}

#[derive(Debug, Clone)]
struct EmfPlusFont {
    font: Font,
    unit: u32,
}

#[derive(Clone)]
struct GraphicsState {
    transform: Transform,
    clip: Option<ClipRegion>,
    page_unit: u32,
    page_scale: f64,
    compositing_mode: u16,
    compositing_quality: u16,
    smoothing_mode: u16,
    interpolation_mode: u16,
    pixel_offset_mode: u16,
    text_rendering_hint: u16,
}

impl Default for GraphicsState {
    fn default() -> Self {
        Self {
            transform: Transform::IDENTITY,
            clip: None,
            page_unit: 2,
            page_scale: 1.0,
            compositing_mode: 0,
            compositing_quality: 0,
            smoothing_mode: 0,
            interpolation_mode: 0,
            pixel_offset_mode: 0,
            text_rendering_hint: 0,
        }
    }
}

struct PendingObject {
    id: usize,
    kind: u8,
    total: usize,
    data: Vec<u8>,
}

struct Player<'a> {
    objects: Vec<Option<Object>>,
    state: GraphicsState,
    saved: Vec<(u32, GraphicsState)>,
    containers: Vec<(u32, GraphicsState)>,
    pending: Option<PendingObject>,
    limits: &'a ResourceLimits,
    strict: bool,
    dpi_x: f64,
    dpi_y: f64,
    canvas: Rect,
}

pub fn playback(
    comments: &[Comment<'_>],
    options: &RenderOptions,
    canvas: Rect,
    renderer: &mut dyn Renderer,
) -> Result<PlaybackResult> {
    let stream = assemble(comments, &options.limits)?;
    let records = records(&stream, &options.limits)?;
    let info = inspect_records(&records)?;
    let mut player = Player {
        objects: vec![None; 64.min(options.limits.max_objects)],
        state: GraphicsState::default(),
        saved: Vec::new(),
        containers: Vec::new(),
        pending: None,
        limits: &options.limits,
        strict: options.strict,
        dpi_x: f64::from(info.logical_dpi_x.max(1)),
        dpi_y: f64::from(info.logical_dpi_y.max(1)),
        canvas,
    };
    let mut diagnostics = info.warnings.clone();
    for record in records {
        if let Err(error) = player.play(record, renderer, &mut diagnostics) {
            match error {
                MetafileError::UnsupportedCriticalFeature(message) if !options.strict => {
                    if diagnostics.len() < options.limits.max_diagnostics {
                        diagnostics.push(
                            Diagnostic::warning("emfplus_unsupported_semantics", message)
                                .at_record(
                                    record.origin.outer_record_index,
                                    u32::from(record.kind),
                                    record_name(record.kind),
                                    record.origin.absolute_start
                                        + record.offset.saturating_sub(record.origin.logical_start),
                                ),
                        );
                    }
                }
                error => return Err(error),
            }
        }
    }
    if player.pending.is_some() {
        return Err(MetafileError::InvalidHeader(
            "unterminated EMF+ continued object".into(),
        ));
    }
    Ok(PlaybackResult { info, diagnostics })
}

impl Player<'_> {
    fn play(
        &mut self,
        record: Record<'_>,
        renderer: &mut dyn Renderer,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<()> {
        match record.kind {
            HEADER | END_OF_FILE => Ok(()),
            OBJECT => self.object(record),
            CLEAR => {
                require_len(record, 4)?;
                let color = argb(u32_at(record.data, 0)?);
                let path = rectangle_path(self.canvas);
                renderer.styled_path(&path, None, Some(&Paint::Solid(color)), 1, None)
            }
            FILL_RECTS => self.rects(record, true, renderer),
            DRAW_RECTS => self.rects(record, false, renderer),
            FILL_ELLIPSE | DRAW_ELLIPSE => self.ellipse(record, renderer),
            FILL_POLYGON | DRAW_LINES | DRAW_BEZIERS => self.points(record, renderer),
            FILL_PIE | DRAW_PIE | DRAW_ARC => self.arc(record, renderer),
            FILL_PATH | DRAW_PATH => self.path(record, renderer),
            FILL_REGION => self.fill_region(record, renderer),
            FILL_CLOSED_CURVE | DRAW_CLOSED_CURVE | DRAW_CURVE => self.curve(record, renderer),
            DRAW_STRING => self.draw_string(record, renderer, diagnostics),
            DRAW_IMAGE | DRAW_IMAGE_POINTS => self.draw_image(record, renderer, diagnostics),
            SAVE => self.save(record),
            RESTORE => self.restore(record),
            BEGIN_CONTAINER | BEGIN_CONTAINER_NO_PARAMS => self.begin_container(record),
            END_CONTAINER => self.end_container(record),
            SET_WORLD_TRANSFORM => {
                self.state.transform = read_transform(record.data, record)?;
                Ok(())
            }
            RESET_WORLD_TRANSFORM => {
                self.state.transform = Transform::IDENTITY;
                Ok(())
            }
            MULTIPLY_WORLD_TRANSFORM => self.multiply_transform(record),
            TRANSLATE_WORLD_TRANSFORM => self.simple_transform(record, 0),
            SCALE_WORLD_TRANSFORM => self.simple_transform(record, 1),
            ROTATE_WORLD_TRANSFORM => self.simple_transform(record, 2),
            SET_PAGE_TRANSFORM => self.set_page_transform(record, diagnostics),
            RESET_CLIP => {
                self.state.clip = None;
                Ok(())
            }
            SET_CLIP_RECT => self.set_clip_rect(record),
            SET_CLIP_PATH | SET_CLIP_REGION => self.set_clip_object(record),
            OFFSET_CLIP => self.offset_clip(record, diagnostics),
            SET_COMPOSITING_MODE => {
                self.state.compositing_mode = record.flags & 0xff;
                self.quality_diagnostic(record, diagnostics, "compositing_mode")
            }
            SET_COMPOSITING_QUALITY => {
                self.state.compositing_quality = record.flags & 0xff;
                self.quality_diagnostic(record, diagnostics, "compositing_quality")
            }
            SET_SMOOTHING_MODE => {
                self.state.smoothing_mode = record.flags & 0xff;
                self.quality_diagnostic(record, diagnostics, "smoothing_mode")
            }
            SET_INTERPOLATION_MODE => {
                self.state.interpolation_mode = record.flags & 0xff;
                Ok(())
            }
            SET_PIXEL_OFFSET_MODE => {
                self.state.pixel_offset_mode = record.flags & 0xff;
                self.quality_diagnostic(record, diagnostics, "pixel_offset_mode")
            }
            SET_TEXT_RENDERING_HINT => {
                self.state.text_rendering_hint = record.flags & 0xff;
                self.quality_diagnostic(record, diagnostics, "text_rendering_hint")
            }
            SET_RENDERING_ORIGIN | SET_TEXT_CONTRAST => {
                self.quality_diagnostic(record, diagnostics, record_name(record.kind))
            }
            _ => self.unsupported(record, diagnostics),
        }
    }

    fn object(&mut self, record: Record<'_>) -> Result<()> {
        let id = usize::from(record.flags & 0xff);
        let kind = ((record.flags >> 8) & 0x7f) as u8;
        if id >= self.objects.len() {
            return invalid(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "object id exceeds configured table",
            );
        }
        let continued = record.flags & 0x8000 != 0;
        if continued || self.pending.is_some() {
            require_len(record, 4)?;
            let total = usize::try_from(u32_at(record.data, 0)?).map_err(|_| {
                malformed(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "continued object size overflow",
                )
            })?;
            if total > self.limits.max_object_bytes {
                return Err(MetafileError::ResourceLimitExceeded {
                    resource: "EMF+ object bytes",
                    actual: total as u64,
                    limit: self.limits.max_object_bytes as u64,
                });
            }
            let fragment = &record.data[4..];
            let pending = self.pending.get_or_insert_with(|| PendingObject {
                id,
                kind,
                total,
                data: Vec::with_capacity(total),
            });
            if pending.id != id || pending.kind != kind || pending.total != total {
                return invalid(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "continued object sequence changed id, type, or total size",
                );
            }
            let next = pending
                .data
                .len()
                .checked_add(fragment.len())
                .ok_or_else(|| {
                    malformed(
                        record.origin,
                        record.index,
                        record.kind,
                        record.offset,
                        "continued object length overflow",
                    )
                })?;
            if next > total {
                return invalid(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "continued object exceeds declared total size",
                );
            }
            pending.data.extend_from_slice(fragment);
            if !continued {
                if pending.data.len() != total {
                    return invalid(
                        record.origin,
                        record.index,
                        record.kind,
                        record.offset,
                        "final object fragment does not reach declared total size",
                    );
                }
                let Some(pending) = self.pending.take() else {
                    return invalid(
                        record.origin,
                        record.index,
                        record.kind,
                        record.offset,
                        "continued object state was unexpectedly absent",
                    );
                };
                self.objects[id] = Some(parse_object_or_unsupported(
                    kind,
                    &pending.data,
                    self.limits,
                    record,
                )?);
            }
            return Ok(());
        }
        if record.data.len() > self.limits.max_object_bytes {
            return Err(MetafileError::ResourceLimitExceeded {
                resource: "EMF+ object bytes",
                actual: record.data.len() as u64,
                limit: self.limits.max_object_bytes as u64,
            });
        }
        self.objects[id] = Some(parse_object_or_unsupported(
            kind,
            record.data,
            self.limits,
            record,
        )?);
        Ok(())
    }

    fn rects(&self, record: Record<'_>, fill: bool, renderer: &mut dyn Renderer) -> Result<()> {
        let (paint, stroke, offset) = if fill {
            (Some(self.paint(record, 0)?), None, 4)
        } else {
            (
                None,
                Some(self.pen(usize::from(record.flags & 0xff), record)?),
                0,
            )
        };
        require_len(record, offset + 4)?;
        let count = usize::try_from(u32_at(record.data, offset)?).map_err(|_| {
            malformed(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "rectangle count overflow",
            )
        })?;
        self.check_points(count, "EMF+ rectangles")?;
        let mut cursor = offset + 4;
        for _ in 0..count {
            let (rect, used) = read_rect(record.data, cursor, record.flags & 0x4000 != 0, record)?;
            cursor += used;
            let path = self.transform_path(&rectangle_path(rect));
            let paint = paint.as_ref().map(|paint| self.transform_paint(paint));
            let stroke = stroke.as_ref().map(|stroke| self.transform_stroke(stroke));
            renderer.styled_path(
                &path,
                stroke.as_ref(),
                paint.as_ref(),
                1,
                self.state.clip.as_ref(),
            )?;
        }
        Ok(())
    }

    fn ellipse(&self, record: Record<'_>, renderer: &mut dyn Renderer) -> Result<()> {
        let fill = record.kind == FILL_ELLIPSE;
        let (paint, stroke, offset) = if fill {
            (Some(self.paint(record, 0)?), None, 4)
        } else {
            (
                None,
                Some(self.pen(usize::from(record.flags & 0xff), record)?),
                0,
            )
        };
        let (rect, _) = read_rect(record.data, offset, record.flags & 0x4000 != 0, record)?;
        let path = self.transform_path(&ellipse_path(rect));
        let paint = paint.as_ref().map(|paint| self.transform_paint(paint));
        let stroke = stroke.as_ref().map(|stroke| self.transform_stroke(stroke));
        renderer.styled_path(
            &path,
            stroke.as_ref(),
            paint.as_ref(),
            1,
            self.state.clip.as_ref(),
        )
    }

    fn points(&self, record: Record<'_>, renderer: &mut dyn Renderer) -> Result<()> {
        let fill = record.kind == FILL_POLYGON;
        let (paint, stroke, offset) = if fill {
            (Some(self.paint(record, 0)?), None, 4)
        } else {
            (
                None,
                Some(self.pen(usize::from(record.flags & 0xff), record)?),
                0,
            )
        };
        require_len(record, offset + 4)?;
        let count = usize::try_from(u32_at(record.data, offset)?).map_err(|_| {
            malformed(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "point count overflow",
            )
        })?;
        self.check_points(count, "EMF+ points")?;
        let points = if record.flags & 0x0800 != 0 {
            read_relative_points(record.data, offset + 4, count, record)?
        } else {
            read_points(
                record.data,
                offset + 4,
                count,
                record.flags & 0x4000 != 0,
                record,
            )?
        };
        let closed = fill || (record.kind == DRAW_LINES && record.flags & 0x2000 != 0);
        let path = points_path(&points, record.kind == DRAW_BEZIERS, closed, record)?;
        let path = self.transform_path(&path);
        let paint = paint.as_ref().map(|paint| self.transform_paint(paint));
        let stroke = stroke.as_ref().map(|stroke| self.transform_stroke(stroke));
        renderer.styled_path(
            &path,
            stroke.as_ref(),
            paint.as_ref(),
            1,
            self.state.clip.as_ref(),
        )
    }

    fn arc(&self, record: Record<'_>, renderer: &mut dyn Renderer) -> Result<()> {
        let fill = record.kind == FILL_PIE;
        let (paint, stroke, offset) = if fill {
            (Some(self.paint(record, 0)?), None, 4)
        } else {
            (
                None,
                Some(self.pen(usize::from(record.flags & 0xff), record)?),
                0,
            )
        };
        require_len(record, offset + 8)?;
        let start = f64::from(f32_at(record.data, offset)?);
        let sweep = f64::from(f32_at(record.data, offset + 4)?);
        let (rect, _) = read_rect(record.data, offset + 8, record.flags & 0x4000 != 0, record)?;
        let close = matches!(record.kind, FILL_PIE | DRAW_PIE);
        let path = self.transform_path(&arc_path(rect, start, sweep, close));
        let paint = paint.as_ref().map(|paint| self.transform_paint(paint));
        let stroke = stroke.as_ref().map(|stroke| self.transform_stroke(stroke));
        renderer.styled_path(
            &path,
            stroke.as_ref(),
            paint.as_ref(),
            1,
            self.state.clip.as_ref(),
        )
    }

    fn path(&self, record: Record<'_>, renderer: &mut dyn Renderer) -> Result<()> {
        let id = usize::from(record.flags & 0xff);
        let Object::Path(path) = self.object_ref(id, record)? else {
            return invalid(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "path record references a non-path object",
            );
        };
        let path = self.transform_path(path);
        if record.kind == FILL_PATH {
            let paint = self.transform_paint(&self.paint(record, 0)?);
            renderer.styled_path(&path, None, Some(&paint), 1, self.state.clip.as_ref())
        } else {
            require_len(record, 4)?;
            let stroke =
                self.transform_stroke(&self.pen(u32_at(record.data, 0)? as usize, record)?);
            renderer.styled_path(&path, Some(&stroke), None, 1, self.state.clip.as_ref())
        }
    }

    fn fill_region(&self, record: Record<'_>, renderer: &mut dyn Renderer) -> Result<()> {
        require_len(record, 4)?;
        let paint = self.transform_paint(&self.paint(record, 0)?);
        let id = usize::from(record.flags & 0xff);
        let Object::Region(region) = self.object_ref(id, record)? else {
            return invalid(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "FillRegion references a non-region object",
            );
        };
        let region = transform_clip(region, self.effective_transform());
        let clip = match &self.state.clip {
            Some(current) => ClipRegion::Intersection(vec![current.clone(), region]),
            None => region,
        };
        renderer.styled_path(
            &rectangle_path(self.canvas),
            None,
            Some(&paint),
            1,
            Some(&clip),
        )
    }

    fn curve(&self, record: Record<'_>, renderer: &mut dyn Renderer) -> Result<()> {
        let fill = record.kind == FILL_CLOSED_CURVE;
        let (paint, stroke, mut cursor) = if fill {
            (Some(self.paint(record, 0)?), None, 4usize)
        } else {
            (
                None,
                Some(self.pen(usize::from(record.flags & 0xff), record)?),
                0usize,
            )
        };
        require_len(
            record,
            cursor + if record.kind == DRAW_CURVE { 16 } else { 8 },
        )?;
        let tension = f64::from(f32_at(record.data, cursor)?);
        if !tension.is_finite() || tension < 0.0 {
            return invalid(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "invalid curve tension",
            );
        }
        cursor += 4;
        let (offset, segments) = if record.kind == DRAW_CURVE {
            let offset = usize::try_from(u32_at(record.data, cursor)?).unwrap_or(usize::MAX);
            let segments = usize::try_from(u32_at(record.data, cursor + 4)?).unwrap_or(usize::MAX);
            cursor += 8;
            (offset, Some(segments))
        } else {
            (0, None)
        };
        let count = usize::try_from(u32_at(record.data, cursor)?).unwrap_or(usize::MAX);
        cursor += 4;
        self.check_points(count, "EMF+ curve points")?;
        let points = read_points(
            record.data,
            cursor,
            count,
            record.flags & 0x4000 != 0,
            record,
        )?;
        let closed = record.kind != DRAW_CURVE;
        let path = cardinal_spline_path(&points, tension, closed, offset, segments, record)?;
        let path = self.transform_path(&path);
        let paint = paint.as_ref().map(|value| self.transform_paint(value));
        let stroke = stroke.as_ref().map(|value| self.transform_stroke(value));
        renderer.styled_path(
            &path,
            stroke.as_ref(),
            paint.as_ref(),
            1,
            self.state.clip.as_ref(),
        )
    }

    fn draw_string(
        &self,
        record: Record<'_>,
        renderer: &mut dyn Renderer,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<()> {
        require_len(record, 28)?;
        let paint = self.paint(record, 0)?;
        let color = if let Paint::Solid(color) = paint {
            color
        } else {
            self.note(
                record,
                diagnostics,
                "emfplus_text_paint_approximate",
                "non-solid DrawString paint cannot be represented by the shared text run",
            )?;
            match paint {
                Paint::Hatch { foreground, .. } => foreground,
                Paint::LinearGradient { stops, .. } => {
                    stops.first().map_or(Color::BLACK, |stop| stop.color)
                }
                Paint::Texture { .. } => Color::BLACK,
                Paint::Solid(color) => color,
            }
        };
        let format_id = u32_at(record.data, 4)?;
        let length = usize::try_from(u32_at(record.data, 8)?).map_err(|_| {
            malformed(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "string length overflow",
            )
        })?;
        if length > self.limits.max_string_chars {
            return Err(MetafileError::ResourceLimitExceeded {
                resource: "EMF+ string characters",
                actual: length as u64,
                limit: self.limits.max_string_chars as u64,
            });
        }
        let layout = rect_f(record.data, 12, record)?;
        let bytes = length.checked_mul(2).ok_or_else(|| {
            malformed(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "string byte length overflow",
            )
        })?;
        if record.data.len() < 28 + bytes {
            return invalid(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "truncated UTF-16 string",
            );
        }
        let encoded = &record.data[28..28 + bytes];
        let units = (0..length)
            .map(|index| u16::from_le_bytes([encoded[index * 2], encoded[index * 2 + 1]]))
            .collect::<Vec<_>>();
        let mut text = String::from_utf16_lossy(&units);
        let font_id = usize::from(record.flags & 0xff);
        let Object::Font(font) = self.object_ref(font_id, record)? else {
            return invalid(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "DrawString references a non-font object",
            );
        };
        let format = if format_id == u32::MAX {
            StringFormat {
                horizontal: HorizontalTextAlignment::Left,
                vertical: VerticalTextAlignment::Top,
                flags: 0,
                trimming: 0,
            }
        } else {
            let id = usize::try_from(format_id).map_err(|_| {
                malformed(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "string format id overflow",
                )
            })?;
            let Object::StringFormat(format) = self.object_ref(id, record)? else {
                return invalid(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "DrawString references a non-string-format object",
                );
            };
            *format
        };
        let unsupported_flags =
            format.flags & !(0x0000_0001 | 0x0000_1000 | 0x0000_2000 | 0x0000_4000);
        if unsupported_flags != 0 || format.trimming != 0 {
            self.note(
                record,
                diagnostics,
                "emfplus_string_format_approximate",
                "StringFormat trimming/direction flags are not fully reproduced by SVG text",
            )?;
        }
        self.note(
            record,
            diagnostics,
            "emfplus_text_metrics_approximate",
            "DrawString uses host SVG font metrics; exact GDI+ layout is not available",
        )?;
        let mut run_font = font.font.clone();
        run_font.height *= self.unit_scale(font.unit, true);
        if format.flags & 0x1000 == 0 && layout.width() > 0.0 {
            text = wrap_and_trim_text(
                &text,
                layout.width(),
                run_font.height.abs(),
                format.trimming,
            );
        }
        let page = Transform {
            m11: self.unit_scale(self.state.page_unit, false) * self.state.page_scale,
            m22: self.unit_scale(self.state.page_unit, true) * self.state.page_scale,
            ..Transform::IDENTITY
        };
        let position =
            page.transform_point(layout_anchor(layout, format.horizontal, format.vertical));
        let layout_clip = (format.flags & 0x4000 == 0
            && layout.width() > 0.0
            && layout.height() > 0.0)
            .then(|| ClipRegion::Path {
                path: self.transform_path(&rectangle_path(layout)),
                fill_mode: 1,
            });
        let clip = combine_clip_regions(self.state.clip.as_ref(), layout_clip);
        renderer.text(&TextRun {
            position,
            text,
            font: run_font,
            color,
            background: None,
            background_rect: None,
            background_path: None,
            horizontal_align: format.horizontal,
            vertical_align: format.vertical,
            clip,
            dx: Vec::new(),
            transform: self.state.transform,
            right_to_left: format.flags & 1 != 0,
        })
    }

    fn draw_image(
        &self,
        record: Record<'_>,
        renderer: &mut dyn Renderer,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<()> {
        let id = usize::from(record.flags & 0xff);
        let bitmap = match self.object_ref(id, record)? {
            Object::Image(bitmap) => bitmap,
            Object::Unsupported(message) => {
                return Err(MetafileError::UnsupportedCriticalFeature(message.clone()))
            }
            _ => {
                return invalid(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "image record references a non-image object",
                )
            }
        };
        let attributes_id = u32_at(record.data, 0)?;
        let attributes = if attributes_id == u32::MAX {
            None
        } else {
            let id = usize::try_from(attributes_id).map_err(|_| {
                malformed(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "image attributes id overflow",
                )
            })?;
            match self.object_ref(id, record)? {
                Object::ImageAttributes(attributes) => Some(*attributes),
                Object::Unsupported(message) => {
                    return Err(MetafileError::UnsupportedCriticalFeature(message.clone()))
                }
                _ => {
                    return invalid(
                        record.origin,
                        record.index,
                        record.kind,
                        record.offset,
                        "DrawImage references a non-image-attributes object",
                    )
                }
            }
        };
        if i32_at(record.data, 4)? != 2 {
            return invalid(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "DrawImage source unit must be Pixel",
            );
        }
        let mut source = rect_f(record.data, 8, record)?;
        if [source.left, source.top, source.right, source.bottom]
            .iter()
            .any(|value| value.fract() != 0.0)
        {
            self.note(
                record,
                diagnostics,
                "emfplus_fractional_image_crop_approximate",
                "fractional image source rectangle was expanded to whole pixels",
            )?;
            source.left = source.left.floor();
            source.top = source.top.floor();
            source.right = source.right.ceil();
            source.bottom = source.bottom.ceil();
        }
        let source_is_full_bitmap = source.left <= 0.0
            && source.top <= 0.0
            && source.right >= f64::from(bitmap.width)
            && source.bottom >= f64::from(bitmap.height);
        let mut cropped = crop_bitmap(bitmap, source, record)?;
        let cropped_dimensions = (cropped.width, cropped.height);
        let has_clamp_border = attributes.is_some_and(|attributes| attributes.wrap_mode == 4);
        if let Some(attributes) = attributes {
            if attributes.wrap_mode == 4 {
                cropped =
                    add_half_pixel_clamp_border(&cropped, attributes.clamp_color, self.limits)?;
            }
        }
        // Both records begin with ImageAttributesId, SrcUnit and SrcRectF.
        // DrawImage then stores DstRectF; DrawImagePoints stores Count and a
        // three-point destination parallelogram (MS-EMFPLUS 2.3.4.8/2.3.4.9).
        let destination = if record.kind == DRAW_IMAGE {
            require_len(record, 40)?;
            rect_f(record.data, 24, record)?
        } else {
            require_len(record, 28)?;
            if u32_at(record.data, 24)? != 3 {
                return invalid(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "DrawImagePoints destination point count must be three",
                );
            }
            let points = if record.flags & 0x0800 != 0 {
                read_relative_points(record.data, 28, 3, record)?
            } else {
                let point_size = if record.flags & 0x4000 != 0 { 4 } else { 8 };
                require_len(record, 28 + 3 * point_size)?;
                read_points(record.data, 28, 3, record.flags & 0x4000 != 0, record)?
            };
            let origin = points[0];
            let x_axis = Vector::new(points[1].x - origin.x, points[1].y - origin.y);
            let y_axis = Vector::new(points[2].x - origin.x, points[2].y - origin.y);
            let mut placement = BitmapPlacement {
                origin,
                x_axis,
                y_axis,
            };
            // For an unqualified full-image DrawImagePoints, GDI+ maps the
            // outer pixel centres into the destination parallelogram whereas
            // SVG maps the image's outer edges. Inset by half a source pixel
            // to preserve the observed GDI+ sampling geometry. Crops can
            // sample adjacent source pixels and ImageAttributes define their
            // own edge behavior, so neither uses this default full-image rule.
            if attributes.is_none() && source_is_full_bitmap && !has_clamp_border {
                placement =
                    inset_bitmap_placement(placement, cropped_dimensions.0, cropped_dimensions.1);
            }
            let placement = self.transform_placement(placement);
            self.validate_image_attributes(attributes, record, diagnostics)?;
            return renderer.bitmap(
                placement,
                &cropped,
                self.sampling(),
                self.state.clip.as_ref(),
            );
        };
        let placement = self.transform_placement(BitmapPlacement::from_rect(destination));
        self.validate_image_attributes(attributes, record, diagnostics)?;
        renderer.bitmap(
            placement,
            &cropped,
            self.sampling(),
            self.state.clip.as_ref(),
        )
    }

    fn save(&mut self, record: Record<'_>) -> Result<()> {
        require_len(record, 4)?;
        if self.saved.len() >= self.limits.max_dc_depth {
            return Err(MetafileError::ResourceLimitExceeded {
                resource: "EMF+ saved state depth",
                actual: (self.saved.len() + 1) as u64,
                limit: self.limits.max_dc_depth as u64,
            });
        }
        self.saved
            .push((u32_at(record.data, 0)?, self.state.clone()));
        Ok(())
    }

    fn restore(&mut self, record: Record<'_>) -> Result<()> {
        require_len(record, 4)?;
        let id = u32_at(record.data, 0)?;
        let position = self
            .saved
            .iter()
            .rposition(|(saved_id, _)| *saved_id == id)
            .ok_or_else(|| {
                malformed(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "unknown saved-state index",
                )
            })?;
        self.state = self.saved[position].1.clone();
        self.saved.truncate(position);
        Ok(())
    }

    fn begin_container(&mut self, record: Record<'_>) -> Result<()> {
        if self.containers.len() >= self.limits.max_container_depth {
            return Err(MetafileError::ResourceLimitExceeded {
                resource: "EMF+ container depth",
                actual: (self.containers.len() + 1) as u64,
                limit: self.limits.max_container_depth as u64,
            });
        }
        require_len(
            record,
            if record.kind == BEGIN_CONTAINER {
                36
            } else {
                4
            },
        )?;
        let id_offset = if record.kind == BEGIN_CONTAINER {
            32
        } else {
            0
        };
        let id = u32_at(record.data, id_offset)?;
        self.containers.push((id, self.state.clone()));
        if record.kind == BEGIN_CONTAINER {
            let destination = rect_f(record.data, 0, record)?;
            let mut source = rect_f(record.data, 16, record)?;
            let source_unit = u32::from(record.flags & 0xff);
            if source_unit > 6 {
                return invalid(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "invalid container source unit",
                );
            }
            let source_scale_x = self.unit_scale(source_unit, false);
            let source_scale_y = self.unit_scale(source_unit, true);
            source.left *= source_scale_x;
            source.right *= source_scale_x;
            source.top *= source_scale_y;
            source.bottom *= source_scale_y;
            if source.width() == 0.0 || source.height() == 0.0 {
                return invalid(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "container source rectangle has zero extent",
                );
            }
            let map = Transform {
                m11: destination.width() / source.width(),
                m12: 0.0,
                m21: 0.0,
                m22: destination.height() / source.height(),
                dx: destination.left - source.left * destination.width() / source.width(),
                dy: destination.top - source.top * destination.height() / source.height(),
            };
            self.state.transform = self.state.transform.compose(map);
        }
        Ok(())
    }

    fn end_container(&mut self, record: Record<'_>) -> Result<()> {
        require_len(record, 4)?;
        let id = u32_at(record.data, 0)?;
        let position = self
            .containers
            .iter()
            .rposition(|(container_id, _)| *container_id == id)
            .ok_or_else(|| {
                malformed(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "unknown container index",
                )
            })?;
        self.state = self.containers[position].1.clone();
        self.containers.truncate(position);
        Ok(())
    }

    fn multiply_transform(&mut self, record: Record<'_>) -> Result<()> {
        let transform = read_transform(record.data, record)?;
        self.state.transform = if record.flags & 0x2000 != 0 {
            transform.compose(self.state.transform)
        } else {
            self.state.transform.compose(transform)
        };
        Ok(())
    }

    fn simple_transform(&mut self, record: Record<'_>, kind: u8) -> Result<()> {
        let transform = match kind {
            0 => {
                require_len(record, 8)?;
                Transform {
                    dx: f64::from(f32_at(record.data, 0)?),
                    dy: f64::from(f32_at(record.data, 4)?),
                    ..Transform::IDENTITY
                }
            }
            1 => {
                require_len(record, 8)?;
                Transform {
                    m11: f64::from(f32_at(record.data, 0)?),
                    m22: f64::from(f32_at(record.data, 4)?),
                    ..Transform::IDENTITY
                }
            }
            _ => {
                require_len(record, 4)?;
                let angle = f64::from(f32_at(record.data, 0)?).to_radians();
                Transform {
                    m11: angle.cos(),
                    m12: angle.sin(),
                    m21: -angle.sin(),
                    m22: angle.cos(),
                    dx: 0.0,
                    dy: 0.0,
                }
            }
        };
        self.state.transform = if record.flags & 0x2000 != 0 {
            transform.compose(self.state.transform)
        } else {
            self.state.transform.compose(transform)
        };
        Ok(())
    }

    fn set_page_transform(
        &mut self,
        record: Record<'_>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<()> {
        require_len(record, 4)?;
        let scale = f64::from(f32_at(record.data, 0)?);
        if !scale.is_finite() || scale <= 0.0 {
            return invalid(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "invalid page scale",
            );
        }
        let unit = u32::from(record.flags & 0xff);
        if unit > 6 {
            return invalid(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "invalid page unit",
            );
        }
        if unit <= 1 {
            self.note(
                record,
                diagnostics,
                "emfplus_unusual_page_unit",
                "World/Display units are discouraged for SetPageTransform and may be device-dependent",
            )?;
        }
        self.state.page_unit = unit;
        self.state.page_scale = scale;
        Ok(())
    }

    fn set_clip_rect(&mut self, record: Record<'_>) -> Result<()> {
        let rect = self.transform_path(&rectangle_path(rect_f(record.data, 0, record)?));
        self.combine_clip(
            record,
            ClipRegion::Path {
                path: rect,
                fill_mode: 1,
            },
        )
    }

    fn set_clip_object(&mut self, record: Record<'_>) -> Result<()> {
        let id = usize::from(record.flags & 0xff);
        let clip = match self.object_ref(id, record)? {
            Object::Path(path) => ClipRegion::Path {
                path: self.transform_path(path),
                fill_mode: 1,
            },
            Object::Region(region) => transform_clip(region, self.effective_transform()),
            _ => {
                return invalid(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "clip record references an incompatible object",
                )
            }
        };
        self.combine_clip(record, clip)
    }

    fn combine_clip(&mut self, record: Record<'_>, clip: ClipRegion) -> Result<()> {
        let mode = (record.flags >> 8) & 0x0f;
        self.state.clip = match mode {
            0 => Some(clip),
            1 => Some(match self.state.clip.take() {
                Some(current) => ClipRegion::Intersection(vec![current, clip]),
                None => clip,
            }),
            2..=5 => {
                let current = self.state.clip.take().unwrap_or_else(|| {
                    ClipRegion::Rect(Rect {
                        left: -1.0e9,
                        top: -1.0e9,
                        right: 1.0e9,
                        bottom: 1.0e9,
                    })
                });
                Some(ClipRegion::Combine {
                    operation: match mode {
                        2 => ClipOperation::Union,
                        3 => ClipOperation::Xor,
                        4 => ClipOperation::Exclude,
                        _ => ClipOperation::Complement,
                    },
                    left: Box::new(current),
                    right: Box::new(clip),
                })
            }
            _ => {
                return invalid(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "invalid clip combine mode",
                )
            }
        };
        Ok(())
    }

    fn offset_clip(&mut self, record: Record<'_>, diagnostics: &mut Vec<Diagnostic>) -> Result<()> {
        require_len(record, 8)?;
        let dx = f64::from(f32_at(record.data, 0)?);
        let dy = f64::from(f32_at(record.data, 4)?);
        if let Some(clip) = self.state.clip.take() {
            self.state.clip = Some(transform_clip(
                &clip,
                Transform {
                    dx,
                    dy,
                    ..Transform::IDENTITY
                },
            ));
        } else {
            self.note(
                record,
                diagnostics,
                "emfplus_offset_infinite_clip",
                "OffsetClip on an infinite clip has no visible effect",
            )?;
        }
        Ok(())
    }

    fn paint(&self, record: Record<'_>, offset: usize) -> Result<Paint> {
        require_len(record, offset + 4)?;
        let value = u32_at(record.data, offset)?;
        if record.flags & 0x8000 != 0 {
            Ok(Paint::Solid(argb(value)))
        } else {
            let id = usize::try_from(value).map_err(|_| {
                malformed(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "brush id overflow",
                )
            })?;
            match self.object_ref(id, record)? {
                Object::Brush(paint) => Ok(paint.clone()),
                Object::Unsupported(message) => {
                    Err(MetafileError::UnsupportedCriticalFeature(message.clone()))
                }
                _ => invalid(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "drawing record references a non-brush object",
                ),
            }
        }
    }

    fn pen(&self, id: usize, record: Record<'_>) -> Result<Stroke> {
        match self.object_ref(id, record)? {
            Object::Pen(stroke) => Ok(stroke.clone()),
            Object::Unsupported(message) => {
                Err(MetafileError::UnsupportedCriticalFeature(message.clone()))
            }
            _ => invalid(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "drawing record references a non-pen object",
            ),
        }
    }

    fn object_ref(&self, id: usize, record: Record<'_>) -> Result<&Object> {
        self.objects
            .get(id)
            .and_then(Option::as_ref)
            .ok_or_else(|| {
                malformed(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "invalid or empty EMF+ object id",
                )
            })
    }

    fn effective_transform(&self) -> Transform {
        let scale = Transform {
            m11: self.unit_scale(self.state.page_unit, false) * self.state.page_scale,
            m22: self.unit_scale(self.state.page_unit, true) * self.state.page_scale,
            ..Transform::IDENTITY
        };
        self.state.transform.compose(scale)
    }

    fn unit_scale(&self, unit: u32, y: bool) -> f64 {
        let dpi = if y { self.dpi_y } else { self.dpi_x };
        match unit {
            0 | 2 => 1.0,
            1 => dpi / 75.0,
            3 => dpi / 72.0,
            4 => dpi,
            5 => dpi / 300.0,
            6 => dpi / 25.4,
            _ => 1.0,
        }
    }

    fn transform_path(&self, path: &Path) -> Path {
        transform_path(path, self.effective_transform())
    }

    fn transform_paint(&self, paint: &Paint) -> Paint {
        match paint {
            Paint::LinearGradient {
                start,
                end,
                stops,
                wrap_mode,
                transform,
                gamma_corrected,
            } => Paint::LinearGradient {
                start: *start,
                end: *end,
                stops: stops.clone(),
                wrap_mode: *wrap_mode,
                transform: self.effective_transform().compose(*transform),
                gamma_corrected: *gamma_corrected,
            },
            Paint::Texture {
                bitmap,
                transform,
                wrap_mode,
                opacity,
                do_not_transform,
            } => Paint::Texture {
                bitmap: bitmap.clone(),
                transform: if *do_not_transform {
                    *transform
                } else {
                    self.effective_transform().compose(*transform)
                },
                wrap_mode: *wrap_mode,
                opacity: *opacity,
                do_not_transform: *do_not_transform,
            },
            paint => paint.clone(),
        }
    }

    fn transform_stroke(&self, stroke: &Stroke) -> Stroke {
        let (logical_width, transform) = if stroke.unit == 0 {
            (stroke.width, self.effective_transform())
        } else {
            (
                stroke.width * self.unit_scale(stroke.unit, false),
                self.state.transform,
            )
        };
        let vector = transform.transform_vector(Vector::new(logical_width, 0.0));
        let mut result = stroke.clone();
        result.width = vector.x.hypot(vector.y).max(0.0);
        result.paint = self.transform_paint(&result.paint);
        result.dash_pattern = result
            .dash_pattern
            .iter()
            .map(|value| value * result.width)
            .collect();
        result.dash_offset *= result.width;
        result
    }

    fn transform_placement(&self, placement: BitmapPlacement) -> BitmapPlacement {
        let transform = self.effective_transform();
        BitmapPlacement {
            origin: transform.transform_point(placement.origin),
            x_axis: transform.transform_vector(placement.x_axis),
            y_axis: transform.transform_vector(placement.y_axis),
        }
    }

    fn sampling(&self) -> BitmapSampling {
        match self.state.interpolation_mode {
            5 => BitmapSampling::Pixelated,
            3 | 4 | 6 | 7 => BitmapSampling::Smooth,
            _ => BitmapSampling::Auto,
        }
    }

    fn validate_image_attributes(
        &self,
        attributes: Option<ImageAttributes>,
        record: Record<'_>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<()> {
        if let Some(attributes) = attributes {
            if attributes.wrap_mode > 4 || attributes.object_clamp > 1 {
                return invalid(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "invalid image-attributes wrap or clamp mode",
                );
            }
            if attributes.wrap_mode != 4 || attributes.clamp_color.a != 0 {
                self.note(
                    record,
                    diagnostics,
                    "emfplus_image_attributes_sampling",
                    "image wrap/clamp attributes affect only samples outside the source rectangle",
                )?;
            }
        }
        Ok(())
    }

    fn check_points(&self, count: usize, resource: &'static str) -> Result<()> {
        if count > self.limits.max_points_per_record {
            return Err(MetafileError::ResourceLimitExceeded {
                resource,
                actual: count as u64,
                limit: self.limits.max_points_per_record as u64,
            });
        }
        Ok(())
    }

    fn quality_diagnostic(
        &self,
        record: Record<'_>,
        diagnostics: &mut Vec<Diagnostic>,
        name: &str,
    ) -> Result<()> {
        self.note(
            record,
            diagnostics,
            "emfplus_quality_approximate",
            format!("{name} is retained but has no exact SVG equivalent"),
        )
    }

    fn unsupported(&self, record: Record<'_>, diagnostics: &mut Vec<Diagnostic>) -> Result<()> {
        self.note(
            record,
            diagnostics,
            "emfplus_unsupported_record",
            format!("{} is not rendered", record_name(record.kind)),
        )
    }

    fn note(
        &self,
        record: Record<'_>,
        diagnostics: &mut Vec<Diagnostic>,
        code: &str,
        message: impl Into<String>,
    ) -> Result<()> {
        let message = message.into();
        if self.strict {
            return Err(MetafileError::UnsupportedCriticalFeature(format!(
                "{} at EMF+ record {}: {}",
                record_name(record.kind),
                record.index,
                message
            )));
        }
        if diagnostics.len() < self.limits.max_diagnostics {
            diagnostics.push(Diagnostic::warning(code, message).at_record(
                record.origin.outer_record_index,
                u32::from(record.kind),
                record_name(record.kind),
                record.origin.absolute_start
                    + record.offset.saturating_sub(record.origin.logical_start),
            ));
        }
        Ok(())
    }
}

fn parse_object(
    kind: u8,
    data: &[u8],
    limits: &ResourceLimits,
    record: Record<'_>,
) -> Result<Object> {
    match kind {
        1 => parse_brush(data, limits, record).map(Object::Brush),
        2 => parse_pen(data, limits, record).map(Object::Pen),
        3 => parse_path(data, limits, record).map(Object::Path),
        4 => parse_region(data, limits, record).map(Object::Region),
        5 => parse_image(data, limits, record).map(Object::Image),
        6 => parse_font(data, limits, record).map(Object::Font),
        7 => parse_string_format(data, limits, record).map(Object::StringFormat),
        8 => parse_image_attributes(data, record).map(Object::ImageAttributes),
        _ => Ok(Object::Unsupported(format!(
            "EMF+ object type {kind} is not implemented"
        ))),
    }
}

fn parse_object_or_unsupported(
    kind: u8,
    data: &[u8],
    limits: &ResourceLimits,
    record: Record<'_>,
) -> Result<Object> {
    match parse_object(kind, data, limits, record) {
        Err(MetafileError::UnsupportedCriticalFeature(message)) => Ok(Object::Unsupported(message)),
        result => result,
    }
}

fn parse_brush(data: &[u8], limits: &ResourceLimits, record: Record<'_>) -> Result<Paint> {
    if data.len() < 8 {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "truncated brush object",
        );
    }
    match u32_at(data, 4)? {
        0 => {
            if data.len() < 12 {
                return invalid(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "truncated solid brush",
                );
            }
            Ok(Paint::Solid(argb(u32_at(data, 8)?)))
        }
        1 => {
            if data.len() < 20 {
                return invalid(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "truncated hatch brush",
                );
            }
            Ok(Paint::Hatch {
                style: u32_at(data, 8)?,
                foreground: argb(u32_at(data, 12)?),
                background: argb(u32_at(data, 16)?),
            })
        }
        2 => parse_texture_brush(&data[8..], limits, record),
        4 => parse_linear_gradient(&data[8..], limits, record),
        brush_type => Err(MetafileError::UnsupportedCriticalFeature(format!(
            "EMF+ brush type {brush_type} is not implemented"
        ))),
    }
}

fn parse_linear_gradient(
    data: &[u8],
    limits: &ResourceLimits,
    record: Record<'_>,
) -> Result<Paint> {
    if data.len() < 40 {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "truncated linear gradient brush",
        );
    }
    let flags = u32_at(data, 0)?;
    let wrap_mode = u32_at(data, 4)?;
    let rect = rect_f(data, 8, record)?;
    let start_color = argb(u32_at(data, 24)?);
    let end_color = argb(u32_at(data, 28)?);
    let mut cursor = 40;
    let mut brush_transform = Transform::IDENTITY;
    if flags & 2 != 0 {
        if data.len() < cursor + 24 {
            return invalid(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "truncated gradient transform",
            );
        }
        brush_transform = read_transform(&data[cursor..cursor + 24], record)?;
        cursor += 24;
    }
    let mut stops = vec![
        GradientStop {
            offset: 0.0,
            color: start_color,
        },
        GradientStop {
            offset: 1.0,
            color: end_color,
        },
    ];
    if flags & 4 != 0 {
        if data.len() < cursor + 4 {
            return invalid(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "truncated preset-color count",
            );
        }
        let count = usize::try_from(u32_at(data, cursor)?).map_err(|_| {
            malformed(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "gradient stop count overflow",
            )
        })?;
        if count > limits.max_gradient_stops {
            return Err(MetafileError::ResourceLimitExceeded {
                resource: "EMF+ gradient stops",
                actual: count as u64,
                limit: limits.max_gradient_stops as u64,
            });
        }
        let positions_start = cursor + 4;
        let colors_start = positions_start
            .checked_add(count.checked_mul(4).ok_or_else(|| {
                malformed(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "gradient positions overflow",
                )
            })?)
            .ok_or_else(|| {
                malformed(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "gradient positions overflow",
                )
            })?;
        let end = colors_start
            .checked_add(count.checked_mul(4).ok_or_else(|| {
                malformed(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "gradient colors overflow",
                )
            })?)
            .ok_or_else(|| {
                malformed(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "gradient colors overflow",
                )
            })?;
        if end > data.len() {
            return invalid(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "truncated preset gradient colors",
            );
        }
        stops.clear();
        for index in 0..count {
            let offset = f64::from(f32_at(data, positions_start + index * 4)?);
            if !offset.is_finite() || !(0.0..=1.0).contains(&offset) {
                return invalid(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "gradient stop is non-finite or outside 0..1",
                );
            }
            stops.push(GradientStop {
                offset,
                color: argb(u32_at(data, colors_start + index * 4)?),
            });
        }
    } else if flags & (8 | 16) != 0 {
        if flags & 8 != 0 && flags & 16 != 0 {
            return Err(MetafileError::UnsupportedCriticalFeature(
                "simultaneous horizontal and vertical EMF+ blend factors require a two-dimensional gradient".into(),
            ));
        }
        let (positions, factors) = parse_blend_factors(data, cursor, limits, record)?;
        stops = positions
            .into_iter()
            .zip(factors)
            .map(|(offset, factor)| GradientStop {
                offset,
                color: mix_color(end_color, start_color, factor),
            })
            .collect();
    }
    Ok(Paint::LinearGradient {
        start: Point::new(rect.left, rect.top),
        end: Point::new(rect.right, rect.bottom),
        stops,
        wrap_mode,
        transform: brush_transform,
        gamma_corrected: flags & 0x80 != 0,
    })
}

fn parse_blend_factors(
    data: &[u8],
    cursor: usize,
    limits: &ResourceLimits,
    record: Record<'_>,
) -> Result<(Vec<f64>, Vec<f64>)> {
    if data.len() < cursor + 4 {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "truncated gradient blend count",
        );
    }
    let count = usize::try_from(u32_at(data, cursor)?).unwrap_or(usize::MAX);
    if count < 2 || count > limits.max_gradient_stops {
        return Err(MetafileError::ResourceLimitExceeded {
            resource: "EMF+ gradient stops",
            actual: count as u64,
            limit: limits.max_gradient_stops as u64,
        });
    }
    let bytes = count.checked_mul(8).ok_or_else(|| {
        malformed(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "gradient blend overflow",
        )
    })?;
    if data.len() < cursor + 4 + bytes {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "truncated gradient blend data",
        );
    }
    let mut positions = Vec::with_capacity(count);
    let mut factors = Vec::with_capacity(count);
    for index in 0..count {
        let position = f64::from(f32_at(data, cursor + 4 + index * 4)?);
        let factor = f64::from(f32_at(data, cursor + 4 + count * 4 + index * 4)?);
        if !position.is_finite()
            || !factor.is_finite()
            || !(0.0..=1.0).contains(&position)
            || !(0.0..=1.0).contains(&factor)
            || positions
                .last()
                .is_some_and(|previous| position < *previous)
        {
            return invalid(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "invalid gradient blend position or factor",
            );
        }
        positions.push(position);
        factors.push(factor);
    }
    Ok((positions, factors))
}

fn mix_color(a: Color, b: Color, amount: f64) -> Color {
    let channel = |left: u8, right: u8| {
        (f64::from(left) + (f64::from(right) - f64::from(left)) * amount)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color {
        r: channel(a.r, b.r),
        g: channel(a.g, b.g),
        b: channel(a.b, b.b),
        a: channel(a.a, b.a),
    }
}

fn parse_texture_brush(data: &[u8], limits: &ResourceLimits, record: Record<'_>) -> Result<Paint> {
    if data.len() < 8 {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "truncated texture brush",
        );
    }
    let flags = u32_at(data, 0)?;
    let wrap_mode = u32_at(data, 4)?;
    if wrap_mode > 4 {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "invalid texture wrap mode",
        );
    }
    if wrap_mode == 4 {
        return Err(MetafileError::UnsupportedCriticalFeature(
            "EMF+ texture Clamp edge sampling is not implemented".into(),
        ));
    }
    let mut cursor = 8;
    let transform = if flags & 2 != 0 {
        let end = checked_advance(cursor, 24, data.len(), record, "texture transform")?;
        let value = read_transform(&data[cursor..end], record)?;
        cursor = end;
        value
    } else {
        Transform::IDENTITY
    };
    if flags & !(2 | 0x80 | 0x100) != 0 {
        return Err(MetafileError::UnsupportedCriticalFeature(format!(
            "texture brush flags {flags:#x} are not implemented"
        )));
    }
    let bitmap = parse_image(&data[cursor..], limits, record)?;
    Ok(Paint::Texture {
        bitmap,
        transform,
        wrap_mode,
        opacity: 1.0,
        do_not_transform: flags & 0x100 != 0,
    })
}

fn parse_pen(data: &[u8], limits: &ResourceLimits, record: Record<'_>) -> Result<Stroke> {
    if data.len() < 20 {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "truncated pen object",
        );
    }
    let flags = u32_at(data, 8)?;
    let unit = u32_at(data, 12)?;
    if unit > 6 {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "invalid pen unit",
        );
    }
    let width = f64::from(f32_at(data, 16)?);
    if !width.is_finite() || width < 0.0 {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "invalid pen width",
        );
    }
    let mut cursor = 20usize;
    let mut cap = LineCap::Butt;
    let mut end_cap = LineCap::Butt;
    let mut join = LineJoin::Miter;
    let mut miter_limit = 10.0;
    let mut dash_style = 0u32;
    let mut dash_offset = 0.0;
    if flags & 1 != 0 {
        return Err(MetafileError::UnsupportedCriticalFeature(
            "EMF+ pen transforms are not faithfully representable by the current stroke vocabulary"
                .into(),
        ));
    }
    if flags & 2 != 0 {
        cap = line_cap(u32_at(data, cursor)?);
        cursor = checked_advance(cursor, 4, data.len(), record, "start cap")?;
    }
    if flags & 4 != 0 {
        end_cap = line_cap(u32_at(data, cursor)?);
        cursor = checked_advance(cursor, 4, data.len(), record, "end cap")?;
    }
    if flags & 8 != 0 {
        join = line_join(u32_at(data, cursor)?);
        cursor = checked_advance(cursor, 4, data.len(), record, "line join")?;
    }
    if flags & 0x10 != 0 {
        miter_limit = f64::from(f32_at(data, cursor)?);
        if !miter_limit.is_finite() || miter_limit <= 0.0 {
            return invalid(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "invalid pen miter limit",
            );
        }
        cursor = checked_advance(cursor, 4, data.len(), record, "miter limit")?;
    }
    if flags & 0x20 != 0 {
        dash_style = u32_at(data, cursor)?;
        cursor = checked_advance(cursor, 4, data.len(), record, "dash style")?;
    }
    if flags & 0x40 != 0 {
        let dash_cap = u32_at(data, cursor)?;
        cursor = checked_advance(cursor, 4, data.len(), record, "dashed cap")?;
        if dash_cap != 0 {
            return Err(MetafileError::UnsupportedCriticalFeature(
                "non-flat EMF+ dash caps are not implemented".into(),
            ));
        }
    }
    if flags & 0x80 != 0 {
        dash_offset = f64::from(f32_at(data, cursor)?);
        if !dash_offset.is_finite() {
            return invalid(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "non-finite pen dash offset",
            );
        }
        cursor = checked_advance(cursor, 4, data.len(), record, "dash offset")?;
    }
    let mut dash_pattern = default_dash(dash_style);
    if flags & 0x100 != 0 {
        let count = usize::try_from(u32_at(data, cursor)?).map_err(|_| {
            malformed(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "dash count overflow",
            )
        })?;
        cursor = checked_advance(cursor, 4, data.len(), record, "dash count")?;
        if count > limits.max_dash_entries {
            return Err(MetafileError::ResourceLimitExceeded {
                resource: "EMF+ dash entries",
                actual: count as u64,
                limit: limits.max_dash_entries as u64,
            });
        }
        let end = checked_advance(
            cursor,
            count.checked_mul(4).ok_or_else(|| {
                malformed(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "dash bytes overflow",
                )
            })?,
            data.len(),
            record,
            "dash pattern",
        )?;
        dash_pattern.clear();
        dash_pattern.reserve(count);
        for index in 0..count {
            let entry = f64::from(f32_at(data, cursor + index * 4)?);
            if !entry.is_finite() || entry < 0.0 {
                return invalid(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "invalid pen dash entry",
                );
            }
            dash_pattern.push(entry);
        }
        cursor = end;
    }
    if flags & !0x1ff != 0 {
        return Err(MetafileError::UnsupportedCriticalFeature(
            "advanced EMF+ pen optional data is not implemented".into(),
        ));
    }
    let paint = parse_brush(&data[cursor..], limits, record)?;
    if cap != end_cap {
        return Err(MetafileError::UnsupportedCriticalFeature(
            "different EMF+ start/end caps are not representable by the current stroke vocabulary"
                .into(),
        ));
    }
    Ok(Stroke {
        paint,
        width,
        unit,
        line_cap: cap,
        line_join: join,
        miter_limit,
        dash_pattern,
        dash_offset,
    })
}

fn parse_path(data: &[u8], limits: &ResourceLimits, record: Record<'_>) -> Result<Path> {
    if data.len() < 12 {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "truncated path object",
        );
    }
    let count = usize::try_from(u32_at(data, 4)?).map_err(|_| {
        malformed(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "path point count overflow",
        )
    })?;
    if count > limits.max_points_per_record {
        return Err(MetafileError::ResourceLimitExceeded {
            resource: "EMF+ path points",
            actual: count as u64,
            limit: limits.max_points_per_record as u64,
        });
    }
    let flags = u32_at(data, 8)?;
    if flags & 0x0800 != 0 {
        let (points, types_start) = read_relative_points_with_cursor(data, 12, count, record)?;
        if data.len() < types_start + count {
            return invalid(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "truncated relative path point types",
            );
        }
        return path_from_types(&points, &data[types_start..types_start + count], record);
    }
    let compressed = flags & 0x4000 != 0;
    let point_size = if compressed { 4 } else { 8 };
    let points_bytes = count.checked_mul(point_size).ok_or_else(|| {
        malformed(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "path points overflow",
        )
    })?;
    let types_start = 12usize.checked_add(points_bytes).ok_or_else(|| {
        malformed(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "path types offset overflow",
        )
    })?;
    if data.len() < types_start + count {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "truncated path points or types",
        );
    }
    let points = read_points(data, 12, count, compressed, record)?;
    path_from_types(&points, &data[types_start..types_start + count], record)
}

fn parse_region(data: &[u8], limits: &ResourceLimits, record: Record<'_>) -> Result<ClipRegion> {
    if data.len() < 12 {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "truncated region object",
        );
    }
    let count = usize::try_from(u32_at(data, 4)?).map_err(|_| {
        malformed(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "region node count overflow",
        )
    })?;
    if count > limits.max_region_nodes {
        return Err(MetafileError::ResourceLimitExceeded {
            resource: "EMF+ region nodes",
            actual: count as u64,
            limit: limits.max_region_nodes as u64,
        });
    }
    let mut cursor = 8;
    let mut parsed_nodes = 0usize;
    let region = parse_region_node(data, &mut cursor, &mut parsed_nodes, 0, limits, record)?;
    let expected = count.saturating_add(1);
    if parsed_nodes != expected {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "region node count does not match its binary tree",
        );
    }
    Ok(region)
}

fn parse_region_node(
    data: &[u8],
    cursor: &mut usize,
    parsed_nodes: &mut usize,
    depth: usize,
    limits: &ResourceLimits,
    record: Record<'_>,
) -> Result<ClipRegion> {
    if depth > limits.max_container_depth || *parsed_nodes >= limits.max_region_nodes {
        return Err(MetafileError::ResourceLimitExceeded {
            resource: "EMF+ region tree complexity",
            actual: (*parsed_nodes + 1) as u64,
            limit: limits.max_region_nodes as u64,
        });
    }
    let node_type = u32_at(data, *cursor)?;
    *cursor = checked_advance(*cursor, 4, data.len(), record, "region node")?;
    *parsed_nodes += 1;
    match node_type {
        1..=5 => {
            let left = parse_region_node(data, cursor, parsed_nodes, depth + 1, limits, record)?;
            let right = parse_region_node(data, cursor, parsed_nodes, depth + 1, limits, record)?;
            Ok(match node_type {
                1 => ClipRegion::Intersection(vec![left, right]),
                2 => ClipRegion::Combine {
                    operation: ClipOperation::Union,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                3 => ClipRegion::Combine {
                    operation: ClipOperation::Xor,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                4 => ClipRegion::Combine {
                    operation: ClipOperation::Exclude,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                _ => ClipRegion::Combine {
                    operation: ClipOperation::Complement,
                    left: Box::new(left),
                    right: Box::new(right),
                },
            })
        }
        0x1000_0000 => {
            let rect = rect_f(data, *cursor, record)?;
            *cursor = checked_advance(*cursor, 16, data.len(), record, "region rectangle")?;
            Ok(ClipRegion::Rect(rect))
        }
        0x1000_0001 => {
            let size = usize::try_from(u32_at(data, *cursor)?).unwrap_or(usize::MAX);
            *cursor = checked_advance(*cursor, 4, data.len(), record, "region path size")?;
            let end = checked_advance(*cursor, size, data.len(), record, "region path")?;
            let path = parse_path(&data[*cursor..end], limits, record)?;
            *cursor = end;
            Ok(ClipRegion::Path { path, fill_mode: 1 })
        }
        0x1000_0002 => Ok(ClipRegion::Polygon(Vec::new())),
        0x1000_0003 => Ok(ClipRegion::Rect(Rect {
            left: -1.0e9,
            top: -1.0e9,
            right: 1.0e9,
            bottom: 1.0e9,
        })),
        _ => Err(MetafileError::UnsupportedCriticalFeature(format!(
            "EMF+ region node type {node_type:#x} is not implemented"
        ))),
    }
}

fn parse_image(data: &[u8], limits: &ResourceLimits, record: Record<'_>) -> Result<Bitmap> {
    if data.len() < 28 || u32_at(data, 4)? != 1 {
        return Err(MetafileError::UnsupportedCriticalFeature(
            "EMF+ metafile or truncated image objects are not implemented".into(),
        ));
    }
    let width = i32_at(data, 8)?;
    let height = i32_at(data, 12)?;
    let stride = i32_at(data, 16)?;
    let pixel_format = u32_at(data, 20)?;
    let data_type = u32_at(data, 24)?;
    if data_type == 1 {
        return decode_compressed_image(&data[28..], limits, record);
    }
    if data_type != 0 {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "invalid EMF+ bitmap data type",
        );
    }
    if width <= 0 || height == 0 || stride == 0 || stride % 4 != 0 {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "invalid raw bitmap dimensions or stride",
        );
    }
    let width = u32::try_from(width).map_err(|_| {
        malformed(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "bitmap width overflow",
        )
    })?;
    let height_abs = height.unsigned_abs();
    if width > limits.max_dimension || height_abs > limits.max_dimension {
        return Err(MetafileError::ResourceLimitExceeded {
            resource: "EMF+ bitmap dimension",
            actual: u64::from(width.max(height_abs)),
            limit: u64::from(limits.max_dimension),
        });
    }
    let pixels = u64::from(width).checked_mul(u64::from(height_abs)).ok_or(
        MetafileError::ResourceLimitExceeded {
            resource: "EMF+ decoded pixels",
            actual: u64::MAX,
            limit: limits.max_pixels,
        },
    )?;
    if pixels > limits.max_pixels {
        return Err(MetafileError::ResourceLimitExceeded {
            resource: "EMF+ decoded pixels",
            actual: pixels,
            limit: limits.max_pixels,
        });
    }
    let bits = (pixel_format >> 8) & 0xff;
    if bits != 24 && bits != 32 {
        return Err(MetafileError::UnsupportedCriticalFeature(format!(
            "EMF+ raw bitmap pixel format {pixel_format:#x} is not a supported 24/32-bit format"
        )));
    }
    let bytes_per_pixel = usize::try_from(bits / 8).unwrap_or(usize::MAX);
    let row_bytes = stride.unsigned_abs() as usize;
    if row_bytes < width as usize * bytes_per_pixel {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "bitmap stride is shorter than a pixel row",
        );
    }
    let needed = row_bytes.checked_mul(height_abs as usize).ok_or_else(|| {
        malformed(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "bitmap byte length overflow",
        )
    })?;
    if data.len() < 28 + needed {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "truncated raw bitmap pixels",
        );
    }
    let premultiplied = pixel_format & 0x0008_0000 != 0;
    let has_alpha = pixel_format & 0x0004_0000 != 0;
    let mut rgba = vec![0; width as usize * height_abs as usize * 4];
    for y in 0..height_abs as usize {
        let source_y = if stride < 0 {
            y
        } else {
            height_abs as usize - 1 - y
        };
        let source = &data[28 + source_y * row_bytes
            ..28 + source_y * row_bytes + width as usize * bytes_per_pixel];
        for x in 0..width as usize {
            let offset = x * bytes_per_pixel;
            let bgra = &source[offset..offset + bytes_per_pixel];
            let a = if has_alpha && bytes_per_pixel == 4 {
                bgra[3]
            } else {
                255
            };
            let unpremultiply = |value: u8| {
                if premultiplied && a != 0 {
                    ((u16::from(value) * 255) / u16::from(a)).min(255) as u8
                } else {
                    value
                }
            };
            let target = (y * width as usize + x) * 4;
            rgba[target] = unpremultiply(bgra[2]);
            rgba[target + 1] = unpremultiply(bgra[1]);
            rgba[target + 2] = unpremultiply(bgra[0]);
            rgba[target + 3] = a;
        }
    }
    Ok(Bitmap {
        width,
        height: height_abs,
        rgba,
    })
}

fn decode_compressed_image(
    encoded: &[u8],
    limits: &ResourceLimits,
    record: Record<'_>,
) -> Result<Bitmap> {
    if encoded.len() > limits.max_object_bytes {
        return Err(MetafileError::ResourceLimitExceeded {
            resource: "EMF+ encoded image bytes",
            actual: encoded.len() as u64,
            limit: limits.max_object_bytes as u64,
        });
    }
    if encoded.starts_with(b"\x89PNG\r\n\x1a\n") {
        decode_png(encoded, limits, record)
    } else if encoded.starts_with(&[0xff, 0xd8]) {
        decode_jpeg(encoded, limits, record)
    } else {
        Err(MetafileError::UnsupportedCriticalFeature(
            "compressed EMF+ bitmap is neither PNG nor JPEG".into(),
        ))
    }
}

fn check_decoded_dimensions(width: u32, height: u32, limits: &ResourceLimits) -> Result<usize> {
    if width == 0 || height == 0 || width > limits.max_dimension || height > limits.max_dimension {
        return Err(MetafileError::ResourceLimitExceeded {
            resource: "EMF+ bitmap dimension",
            actual: u64::from(width.max(height)),
            limit: u64::from(limits.max_dimension),
        });
    }
    let pixels = u64::from(width).checked_mul(u64::from(height)).ok_or(
        MetafileError::ResourceLimitExceeded {
            resource: "EMF+ decoded pixels",
            actual: u64::MAX,
            limit: limits.max_pixels,
        },
    )?;
    if pixels > limits.max_pixels {
        return Err(MetafileError::ResourceLimitExceeded {
            resource: "EMF+ decoded pixels",
            actual: pixels,
            limit: limits.max_pixels,
        });
    }
    usize::try_from(pixels.saturating_mul(4)).map_err(|_| MetafileError::ResourceLimitExceeded {
        resource: "EMF+ decoded image bytes",
        actual: u64::MAX,
        limit: limits.max_pixels.saturating_mul(4),
    })
}

fn decode_png(encoded: &[u8], limits: &ResourceLimits, record: Record<'_>) -> Result<Bitmap> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(encoded));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|error| {
        malformed(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            &format!("invalid PNG image: {error}"),
        )
    })?;
    let (width, height) = (reader.info().width, reader.info().height);
    let rgba_len = check_decoded_dimensions(width, height, limits)?;
    if reader.output_buffer_size() > rgba_len {
        return Err(MetafileError::ResourceLimitExceeded {
            resource: "EMF+ PNG decode buffer",
            actual: reader.output_buffer_size() as u64,
            limit: rgba_len as u64,
        });
    }
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).map_err(|error| {
        malformed(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            &format!("invalid PNG pixels: {error}"),
        )
    })?;
    pixels.truncate(info.buffer_size());
    let rgba = pixels_to_rgba(&pixels, info.color_type, width, height, record)?;
    Ok(Bitmap {
        width,
        height,
        rgba,
    })
}

fn pixels_to_rgba(
    decoded: &[u8],
    color_type: png::ColorType,
    width: u32,
    height: u32,
    record: Record<'_>,
) -> Result<Vec<u8>> {
    let channels = match color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => {
            return invalid(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "PNG palette was not expanded",
            )
        }
    };
    let count = usize::try_from(u64::from(width) * u64::from(height)).unwrap_or(usize::MAX);
    if decoded.len() != count.saturating_mul(channels) {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "PNG decoded byte count is inconsistent",
        );
    }
    let mut rgba = Vec::with_capacity(count * 4);
    for pixel in decoded.chunks_exact(channels) {
        match color_type {
            png::ColorType::Grayscale => rgba.extend([pixel[0], pixel[0], pixel[0], 255]),
            png::ColorType::Rgb => rgba.extend([pixel[0], pixel[1], pixel[2], 255]),
            png::ColorType::GrayscaleAlpha => rgba.extend([pixel[0], pixel[0], pixel[0], pixel[1]]),
            png::ColorType::Rgba => rgba.extend_from_slice(pixel),
            png::ColorType::Indexed => unreachable!(),
        }
    }
    Ok(rgba)
}

fn decode_jpeg(encoded: &[u8], limits: &ResourceLimits, record: Record<'_>) -> Result<Bitmap> {
    let mut decoder = jpeg_decoder::Decoder::new(std::io::Cursor::new(encoded));
    decoder.read_info().map_err(|error| {
        malformed(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            &format!("invalid JPEG header: {error}"),
        )
    })?;
    let info = decoder.info().ok_or_else(|| {
        malformed(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "JPEG has no image information",
        )
    })?;
    let width = u32::from(info.width);
    let height = u32::from(info.height);
    let rgba_len = check_decoded_dimensions(width, height, limits)?;
    let pixels = decoder.decode().map_err(|error| {
        malformed(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            &format!("invalid JPEG pixels: {error}"),
        )
    })?;
    let channels = match info.pixel_format {
        jpeg_decoder::PixelFormat::L8 => 1,
        jpeg_decoder::PixelFormat::RGB24 => 3,
        _ => {
            return Err(MetafileError::UnsupportedCriticalFeature(format!(
                "JPEG pixel format {:?} is not implemented",
                info.pixel_format
            )))
        }
    };
    if pixels.len().saturating_mul(4) / channels != rgba_len {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "JPEG decoded byte count is inconsistent",
        );
    }
    let mut rgba = Vec::with_capacity(rgba_len);
    for pixel in pixels.chunks_exact(channels) {
        if channels == 1 {
            rgba.extend([pixel[0], pixel[0], pixel[0], 255]);
        } else {
            rgba.extend([pixel[0], pixel[1], pixel[2], 255]);
        }
    }
    Ok(Bitmap {
        width,
        height,
        rgba,
    })
}

fn parse_font(data: &[u8], limits: &ResourceLimits, record: Record<'_>) -> Result<EmfPlusFont> {
    if data.len() < 24 {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "truncated font object",
        );
    }
    let size = f64::from(f32_at(data, 4)?);
    let unit = u32_at(data, 8)?;
    if unit > 6 {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "invalid font size unit",
        );
    }
    let style = u32_at(data, 12)?;
    let length = usize::try_from(u32_at(data, 20)?).map_err(|_| {
        malformed(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "font family length overflow",
        )
    })?;
    if length > limits.max_string_chars {
        return Err(MetafileError::ResourceLimitExceeded {
            resource: "EMF+ font family characters",
            actual: length as u64,
            limit: limits.max_string_chars as u64,
        });
    }
    let bytes = length.checked_mul(2).ok_or_else(|| {
        malformed(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "font family bytes overflow",
        )
    })?;
    if data.len() < 24 + bytes {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "truncated font family",
        );
    }
    let encoded = &data[24..24 + bytes];
    let units = (0..length)
        .map(|index| u16::from_le_bytes([encoded[index * 2], encoded[index * 2 + 1]]))
        .collect::<Vec<_>>();
    let family = String::from_utf16_lossy(&units);
    Ok(EmfPlusFont {
        font: Font {
            height: size,
            width: 0.0,
            escapement_tenths: 0,
            weight: if style & 1 != 0 { 700 } else { 400 },
            italic: style & 2 != 0,
            underline: style & 4 != 0,
            strike_out: style & 8 != 0,
            charset: 1,
            family,
        },
        unit,
    })
}

fn parse_string_format(
    data: &[u8],
    limits: &ResourceLimits,
    record: Record<'_>,
) -> Result<StringFormat> {
    if data.len() < 60 {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "truncated string-format object",
        );
    }
    let flags = u32_at(data, 4)?;
    let horizontal = alignment(u32_at(data, 12)?);
    let vertical = alignment_vertical(u32_at(data, 16)?);
    let tab_count = i32_at(data, 52)?;
    let range_count = i32_at(data, 56)?;
    if tab_count < 0 || range_count < 0 {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "negative string-format tab or range count",
        );
    }
    let entries = usize::try_from(tab_count)
        .unwrap_or(usize::MAX)
        .saturating_add(usize::try_from(range_count).unwrap_or(usize::MAX));
    if entries > limits.max_points_per_record {
        return Err(MetafileError::ResourceLimitExceeded {
            resource: "EMF+ string-format entries",
            actual: entries as u64,
            limit: limits.max_points_per_record as u64,
        });
    }
    Ok(StringFormat {
        horizontal,
        vertical,
        flags,
        trimming: u32_at(data, 36)?,
    })
}

fn parse_image_attributes(data: &[u8], record: Record<'_>) -> Result<ImageAttributes> {
    if data.len() < 24 {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "truncated image-attributes object",
        );
    }
    let wrap_mode = u32_at(data, 8)?;
    let object_clamp = u32_at(data, 16)?;
    if wrap_mode > 4 || object_clamp > 1 {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "invalid image-attributes wrap or clamp mode",
        );
    }
    if data.len() > 24 && data[24..].iter().any(|byte| *byte != 0) {
        return Err(MetafileError::UnsupportedCriticalFeature(
            "extended EMF+ image adjustments (including color matrices) are not implemented".into(),
        ));
    }
    Ok(ImageAttributes {
        wrap_mode,
        clamp_color: argb(u32_at(data, 12)?),
        object_clamp,
    })
}

fn require_len(record: Record<'_>, needed: usize) -> Result<()> {
    if record.data.len() < needed {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "truncated record data",
        );
    }
    Ok(())
}

fn checked_advance(
    cursor: usize,
    amount: usize,
    length: usize,
    record: Record<'_>,
    field: &str,
) -> Result<usize> {
    let end = cursor.checked_add(amount).ok_or_else(|| {
        malformed(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "field offset overflow",
        )
    })?;
    if end > length {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            &format!("truncated {field}"),
        );
    }
    Ok(end)
}

fn u16_at(data: &[u8], offset: usize) -> Result<u16> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or(MetafileError::TruncatedInput {
            offset,
            needed: 2,
            available: data.len().saturating_sub(offset),
        })?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn u32_at(data: &[u8], offset: usize) -> Result<u32> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or(MetafileError::TruncatedInput {
            offset,
            needed: 4,
            available: data.len().saturating_sub(offset),
        })?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn i32_at(data: &[u8], offset: usize) -> Result<i32> {
    let bytes =
        data.get(offset..offset.saturating_add(4))
            .ok_or(MetafileError::TruncatedInput {
                offset,
                needed: 4,
                available: data.len().saturating_sub(offset),
            })?;
    Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn f32_at(data: &[u8], offset: usize) -> Result<f32> {
    Ok(f32::from_bits(u32_at(data, offset)?))
}

fn read_transform(data: &[u8], record: Record<'_>) -> Result<Transform> {
    if data.len() < 24 {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "truncated transform matrix",
        );
    }
    let values = [
        f64::from(f32_at(data, 0)?),
        f64::from(f32_at(data, 4)?),
        f64::from(f32_at(data, 8)?),
        f64::from(f32_at(data, 12)?),
        f64::from(f32_at(data, 16)?),
        f64::from(f32_at(data, 20)?),
    ];
    if !values.iter().all(|value| value.is_finite()) {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "non-finite transform matrix",
        );
    }
    Ok(Transform {
        m11: values[0],
        m12: values[1],
        m21: values[2],
        m22: values[3],
        dx: values[4],
        dy: values[5],
    })
}

fn rect_f(data: &[u8], offset: usize, record: Record<'_>) -> Result<Rect> {
    if data.len() < offset + 16 {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "truncated RectF",
        );
    }
    let x = f64::from(f32_at(data, offset)?);
    let y = f64::from(f32_at(data, offset + 4)?);
    let width = f64::from(f32_at(data, offset + 8)?);
    let height = f64::from(f32_at(data, offset + 12)?);
    if ![x, y, width, height].iter().all(|value| value.is_finite()) {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "non-finite RectF",
        );
    }
    Ok(Rect {
        left: x,
        top: y,
        right: x + width,
        bottom: y + height,
    })
}

fn read_rect(
    data: &[u8],
    offset: usize,
    compressed: bool,
    record: Record<'_>,
) -> Result<(Rect, usize)> {
    if !compressed {
        return Ok((rect_f(data, offset, record)?, 16));
    }
    if data.len() < offset + 8 {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "truncated compressed rectangle",
        );
    }
    let i = |at| i16::from_le_bytes([data[offset + at], data[offset + at + 1]]);
    let x = f64::from(i(0));
    let y = f64::from(i(2));
    Ok((
        Rect {
            left: x,
            top: y,
            right: x + f64::from(i(4)),
            bottom: y + f64::from(i(6)),
        },
        8,
    ))
}

fn read_points(
    data: &[u8],
    offset: usize,
    count: usize,
    compressed: bool,
    record: Record<'_>,
) -> Result<Vec<Point>> {
    let size = if compressed { 4 } else { 8 };
    let bytes = count.checked_mul(size).ok_or_else(|| {
        malformed(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "point byte length overflow",
        )
    })?;
    if data.len() < offset + bytes {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "truncated point array",
        );
    }
    let mut points = Vec::with_capacity(count);
    for index in 0..count {
        let at = offset + index * size;
        let point = if compressed {
            Point::new(
                f64::from(i16::from_le_bytes([data[at], data[at + 1]])),
                f64::from(i16::from_le_bytes([data[at + 2], data[at + 3]])),
            )
        } else {
            Point::new(
                f64::from(f32_at(data, at)?),
                f64::from(f32_at(data, at + 4)?),
            )
        };
        if !point.x.is_finite() || !point.y.is_finite() {
            return invalid(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "non-finite point",
            );
        }
        points.push(point);
    }
    Ok(points)
}

fn read_relative_points(
    data: &[u8],
    offset: usize,
    count: usize,
    record: Record<'_>,
) -> Result<Vec<Point>> {
    read_relative_points_with_cursor(data, offset, count, record).map(|(points, _)| points)
}

fn read_relative_points_with_cursor(
    data: &[u8],
    offset: usize,
    count: usize,
    record: Record<'_>,
) -> Result<(Vec<Point>, usize)> {
    let mut cursor = offset;
    let mut current = Point::new(0.0, 0.0);
    let mut points = Vec::with_capacity(count);
    for _ in 0..count {
        let x = read_relative_integer(data, &mut cursor, record)?;
        let y = read_relative_integer(data, &mut cursor, record)?;
        current = Point::new(current.x + f64::from(x), current.y + f64::from(y));
        points.push(current);
    }
    Ok((points, cursor))
}

fn read_relative_integer(data: &[u8], cursor: &mut usize, record: Record<'_>) -> Result<i16> {
    let first = *data.get(*cursor).ok_or_else(|| {
        malformed(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "truncated relative point coordinate",
        )
    })?;
    *cursor += 1;
    if first & 0x80 == 0 {
        let value = i16::from(first & 0x7f);
        Ok(if value & 0x40 != 0 {
            value - 0x80
        } else {
            value
        })
    } else {
        let second = *data.get(*cursor).ok_or_else(|| {
            malformed(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "truncated 15-bit relative point coordinate",
            )
        })?;
        *cursor += 1;
        let value = (i16::from(first & 0x7f) << 8) | i16::from(second);
        Ok(if value & 0x4000 != 0 {
            value | i16::MIN
        } else {
            value
        })
    }
}

fn crop_bitmap(bitmap: &Bitmap, source: Rect, record: Record<'_>) -> Result<Bitmap> {
    let left = source.left as i64;
    let top = source.top as i64;
    let right = source.right as i64;
    let bottom = source.bottom as i64;
    if left < 0
        || top < 0
        || right <= left
        || bottom <= top
        || right > i64::from(bitmap.width)
        || bottom > i64::from(bitmap.height)
    {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "image source rectangle is empty or outside bitmap bounds",
        );
    }
    let width = u32::try_from(right - left).map_err(|_| {
        malformed(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "image crop width overflow",
        )
    })?;
    let height = u32::try_from(bottom - top).map_err(|_| {
        malformed(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "image crop height overflow",
        )
    })?;
    let bytes = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| {
            malformed(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "image crop allocation overflow",
            )
        })?;
    let mut rgba = Vec::with_capacity(bytes);
    let source_stride = usize::try_from(bitmap.width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or_else(|| {
            malformed(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "image stride overflow",
            )
        })?;
    let row_bytes = usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or_else(|| {
            malformed(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "image crop stride overflow",
            )
        })?;
    for y in top..bottom {
        let start = usize::try_from(y)
            .ok()
            .and_then(|y| y.checked_mul(source_stride))
            .and_then(|offset| {
                usize::try_from(left)
                    .ok()
                    .and_then(|left| left.checked_mul(4))
                    .and_then(|left| offset.checked_add(left))
            })
            .ok_or_else(|| {
                malformed(
                    record.origin,
                    record.index,
                    record.kind,
                    record.offset,
                    "image crop offset overflow",
                )
            })?;
        let end = start.checked_add(row_bytes).ok_or_else(|| {
            malformed(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "image crop offset overflow",
            )
        })?;
        let row = bitmap.rgba.get(start..end).ok_or_else(|| {
            malformed(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "image crop exceeds decoded pixels",
            )
        })?;
        rgba.extend_from_slice(row);
    }
    Ok(Bitmap {
        width,
        height,
        rgba,
    })
}

fn add_half_pixel_clamp_border(
    bitmap: &Bitmap,
    clamp: Color,
    limits: &ResourceLimits,
) -> Result<Bitmap> {
    let width = bitmap
        .width
        .checked_mul(2)
        .and_then(|value| value.checked_add(2))
        .ok_or(MetafileError::ResourceLimitExceeded {
            resource: "EMF+ clamped bitmap dimension",
            actual: u64::MAX,
            limit: u64::from(limits.max_dimension),
        })?;
    let height = bitmap
        .height
        .checked_mul(2)
        .and_then(|value| value.checked_add(2))
        .ok_or(MetafileError::ResourceLimitExceeded {
            resource: "EMF+ clamped bitmap dimension",
            actual: u64::MAX,
            limit: u64::from(limits.max_dimension),
        })?;
    let bytes = check_decoded_dimensions(width, height, limits)?;
    let mut rgba = vec![0; bytes];
    for pixel in rgba.as_chunks_mut::<4>().0 {
        *pixel = [clamp.r, clamp.g, clamp.b, clamp.a];
    }
    for y in 0..bitmap.height as usize {
        for x in 0..bitmap.width as usize {
            let source = (y * bitmap.width as usize + x) * 4;
            for target_y in [y * 2 + 1, y * 2 + 2] {
                for target_x in [x * 2 + 1, x * 2 + 2] {
                    let target = (target_y * width as usize + target_x) * 4;
                    rgba[target..target + 4].copy_from_slice(&bitmap.rgba[source..source + 4]);
                }
            }
        }
    }
    Ok(Bitmap {
        width,
        height,
        rgba,
    })
}

fn inset_bitmap_placement(
    placement: BitmapPlacement,
    source_width: u32,
    source_height: u32,
) -> BitmapPlacement {
    let x_inset = 0.5 / f64::from(source_width.max(1));
    let y_inset = 0.5 / f64::from(source_height.max(1));
    BitmapPlacement {
        origin: Point::new(
            placement.origin.x + placement.x_axis.x * x_inset + placement.y_axis.x * y_inset,
            placement.origin.y + placement.x_axis.y * x_inset + placement.y_axis.y * y_inset,
        ),
        x_axis: Vector::new(
            placement.x_axis.x * (1.0 - 2.0 * x_inset),
            placement.x_axis.y * (1.0 - 2.0 * x_inset),
        ),
        y_axis: Vector::new(
            placement.y_axis.x * (1.0 - 2.0 * y_inset),
            placement.y_axis.y * (1.0 - 2.0 * y_inset),
        ),
    }
}

fn layout_anchor(
    layout: Rect,
    horizontal: HorizontalTextAlignment,
    vertical: VerticalTextAlignment,
) -> Point {
    let x = match horizontal {
        HorizontalTextAlignment::Left => layout.left,
        HorizontalTextAlignment::Center => (layout.left + layout.right) / 2.0,
        HorizontalTextAlignment::Right => layout.right,
    };
    let y = match vertical {
        VerticalTextAlignment::Top | VerticalTextAlignment::Baseline => layout.top,
        VerticalTextAlignment::Center => (layout.top + layout.bottom) / 2.0,
        VerticalTextAlignment::Bottom => layout.bottom,
    };
    Point::new(x, y)
}

fn combine_clip_regions(
    current: Option<&ClipRegion>,
    incoming: Option<ClipRegion>,
) -> Option<ClipRegion> {
    match (current, incoming) {
        (Some(current), Some(incoming)) => {
            Some(ClipRegion::Intersection(vec![current.clone(), incoming]))
        }
        (Some(current), None) => Some(current.clone()),
        (None, incoming) => incoming,
    }
}

fn wrap_and_trim_text(text: &str, width: f64, font_size: f64, trimming: u32) -> String {
    let capacity = (width / (font_size.max(1.0) * 0.6)).floor().max(1.0) as usize;
    let mut lines = Vec::new();
    for source_line in text.lines() {
        let mut line = String::new();
        for word in source_line.split_inclusive(char::is_whitespace) {
            if !line.is_empty() && line.chars().count() + word.chars().count() > capacity {
                lines.push(std::mem::take(&mut line));
            }
            if word.chars().count() > capacity {
                for ch in word.chars() {
                    if line.chars().count() == capacity {
                        lines.push(std::mem::take(&mut line));
                    }
                    line.push(ch);
                }
            } else {
                line.push_str(word);
            }
        }
        lines.push(line);
    }
    if lines.is_empty() {
        return String::new();
    }
    if trimming >= 3 {
        for line in &mut lines {
            if line.chars().count() > capacity {
                *line = line
                    .chars()
                    .take(capacity.saturating_sub(1))
                    .collect::<String>()
                    + "…";
            }
        }
    }
    lines.join("\n")
}

fn argb(value: u32) -> Color {
    Color {
        a: (value >> 24) as u8,
        r: (value >> 16) as u8,
        g: (value >> 8) as u8,
        b: value as u8,
    }
}

fn line_cap(value: u32) -> LineCap {
    match value {
        2 => LineCap::Square,
        1 | 5 => LineCap::Round,
        _ => LineCap::Butt,
    }
}

fn line_join(value: u32) -> LineJoin {
    match value {
        2 => LineJoin::Bevel,
        1 => LineJoin::Round,
        _ => LineJoin::Miter,
    }
}

fn default_dash(style: u32) -> Vec<f64> {
    match style {
        1 => vec![3.0, 1.0],
        2 => vec![1.0, 1.0],
        3 => vec![3.0, 1.0, 1.0, 1.0],
        4 => vec![3.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        _ => Vec::new(),
    }
}

fn alignment(value: u32) -> HorizontalTextAlignment {
    match value {
        1 => HorizontalTextAlignment::Center,
        2 => HorizontalTextAlignment::Right,
        _ => HorizontalTextAlignment::Left,
    }
}

fn alignment_vertical(value: u32) -> VerticalTextAlignment {
    match value {
        1 => VerticalTextAlignment::Center,
        2 => VerticalTextAlignment::Bottom,
        _ => VerticalTextAlignment::Top,
    }
}

fn rectangle_path(rect: Rect) -> Path {
    Path {
        figures: vec![PathFigure {
            start: Point::new(rect.left, rect.top),
            segments: vec![
                PathSegment::Line(Point::new(rect.right, rect.top)),
                PathSegment::Line(Point::new(rect.right, rect.bottom)),
                PathSegment::Line(Point::new(rect.left, rect.bottom)),
            ],
            closed: true,
        }],
    }
}

fn ellipse_path(rect: Rect) -> Path {
    let rect = rect.normalized();
    let cx = rect.left.midpoint(rect.right);
    let cy = rect.top.midpoint(rect.bottom);
    let rx = rect.width() / 2.0;
    let ry = rect.height() / 2.0;
    let kx = rx * 0.552_284_749_830_793_6;
    let ky = ry * 0.552_284_749_830_793_6;
    Path {
        figures: vec![PathFigure {
            start: Point::new(cx + rx, cy),
            segments: vec![
                PathSegment::Cubic {
                    control1: Point::new(cx + rx, cy + ky),
                    control2: Point::new(cx + kx, cy + ry),
                    to: Point::new(cx, cy + ry),
                },
                PathSegment::Cubic {
                    control1: Point::new(cx - kx, cy + ry),
                    control2: Point::new(cx - rx, cy + ky),
                    to: Point::new(cx - rx, cy),
                },
                PathSegment::Cubic {
                    control1: Point::new(cx - rx, cy - ky),
                    control2: Point::new(cx - kx, cy - ry),
                    to: Point::new(cx, cy - ry),
                },
                PathSegment::Cubic {
                    control1: Point::new(cx + kx, cy - ry),
                    control2: Point::new(cx + rx, cy - ky),
                    to: Point::new(cx + rx, cy),
                },
            ],
            closed: true,
        }],
    }
}

fn arc_path(rect: Rect, start_degrees: f64, sweep_degrees: f64, pie: bool) -> Path {
    let rect = rect.normalized();
    let cx = rect.left.midpoint(rect.right);
    let cy = rect.top.midpoint(rect.bottom);
    let rx = rect.width() / 2.0;
    let ry = rect.height() / 2.0;
    let segments = (sweep_degrees.abs() / 90.0).ceil().max(1.0) as usize;
    let step = sweep_degrees.to_radians() / segments as f64;
    let start_angle = start_degrees.to_radians();
    let start = Point::new(cx + rx * start_angle.cos(), cy + ry * start_angle.sin());
    let mut path_segments = Vec::with_capacity(segments + usize::from(pie));
    let figure_start = if pie { Point::new(cx, cy) } else { start };
    if pie {
        path_segments.push(PathSegment::Line(start));
    }
    for index in 0..segments {
        let a0 = start_angle + step * index as f64;
        let a1 = a0 + step;
        let factor = 4.0 / 3.0 * ((a1 - a0) / 4.0).tan();
        let p0 = Point::new(cx + rx * a0.cos(), cy + ry * a0.sin());
        let p1 = Point::new(cx + rx * a1.cos(), cy + ry * a1.sin());
        path_segments.push(PathSegment::Cubic {
            control1: Point::new(p0.x - factor * rx * a0.sin(), p0.y + factor * ry * a0.cos()),
            control2: Point::new(p1.x + factor * rx * a1.sin(), p1.y - factor * ry * a1.cos()),
            to: p1,
        });
    }
    Path {
        figures: vec![PathFigure {
            start: figure_start,
            segments: path_segments,
            closed: pie,
        }],
    }
}

fn points_path(points: &[Point], bezier: bool, closed: bool, record: Record<'_>) -> Result<Path> {
    let Some(first) = points.first().copied() else {
        return Ok(Path {
            figures: Vec::new(),
        });
    };
    let mut segments = Vec::new();
    if bezier {
        if points.len() < 4 || (points.len() - 1).rem_euclid(3) != 0 {
            return invalid(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "Bezier point count is not 1 + 3n",
            );
        }
        for index in (1..points.len()).step_by(3) {
            segments.push(PathSegment::Cubic {
                control1: points[index],
                control2: points[index + 1],
                to: points[index + 2],
            });
        }
    } else {
        segments.extend(points[1..].iter().copied().map(PathSegment::Line));
    }
    Ok(Path {
        figures: vec![PathFigure {
            start: first,
            segments,
            closed,
        }],
    })
}

fn cardinal_spline_path(
    points: &[Point],
    tension: f64,
    closed: bool,
    offset: usize,
    segments: Option<usize>,
    record: Record<'_>,
) -> Result<Path> {
    if points.len() < if closed { 3 } else { 2 } {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "cardinal spline has too few points",
        );
    }
    let segment_count = segments.unwrap_or_else(|| points.len().saturating_sub(1));
    if !closed && (offset >= points.len() - 1 || segment_count > points.len() - 1 - offset) {
        return invalid(
            record.origin,
            record.index,
            record.kind,
            record.offset,
            "cardinal spline offset or segment count is out of range",
        );
    }
    let start_index = if closed { 0 } else { offset };
    let mut path = PathFigure {
        start: points[start_index],
        segments: Vec::with_capacity(if closed { points.len() } else { segment_count }),
        closed,
    };
    let count = if closed { points.len() } else { segment_count };
    for step in 0..count {
        let i = start_index + step;
        let p1 = points[i % points.len()];
        let p0 = if i == 0 {
            if closed {
                points[points.len() - 1]
            } else {
                p1
            }
        } else {
            points[i - 1]
        };
        let p2 = if closed {
            points[(i + 1) % points.len()]
        } else {
            points[i + 1]
        };
        let p3 = if closed {
            points[(i + 2) % points.len()]
        } else {
            points.get(i + 2).copied().unwrap_or(p2)
        };
        let factor = tension / 3.0;
        path.segments.push(PathSegment::Cubic {
            control1: Point::new(p1.x + (p2.x - p0.x) * factor, p1.y + (p2.y - p0.y) * factor),
            control2: Point::new(p2.x - (p3.x - p1.x) * factor, p2.y - (p3.y - p1.y) * factor),
            to: p2,
        });
    }
    Ok(Path {
        figures: vec![path],
    })
}

fn path_from_types(points: &[Point], types: &[u8], record: Record<'_>) -> Result<Path> {
    let mut figures: Vec<PathFigure> = Vec::new();
    let mut index = 0usize;
    while index < points.len() {
        let kind = types[index] & 0x07;
        if kind != 0 {
            return invalid(
                record.origin,
                record.index,
                record.kind,
                record.offset,
                "path figure does not begin with Start point type",
            );
        }
        figures.push(PathFigure {
            start: points[index],
            segments: Vec::new(),
            closed: types[index] & 0x80 != 0,
        });
        index += 1;
        while index < points.len() && types[index] & 0x07 != 0 {
            match types[index] & 0x07 {
                1 => {
                    let figure = figures.last_mut().ok_or_else(|| {
                        malformed(
                            record.origin,
                            record.index,
                            record.kind,
                            record.offset,
                            "path figure state was unexpectedly absent",
                        )
                    })?;
                    figure.segments.push(PathSegment::Line(points[index]));
                    figure.closed |= types[index] & 0x80 != 0;
                    index += 1;
                }
                3 => {
                    if index + 2 >= points.len()
                        || types[index + 1] & 0x07 != 3
                        || types[index + 2] & 0x07 != 3
                    {
                        return invalid(
                            record.origin,
                            record.index,
                            record.kind,
                            record.offset,
                            "incomplete path Bezier triplet",
                        );
                    }
                    let figure = figures.last_mut().ok_or_else(|| {
                        malformed(
                            record.origin,
                            record.index,
                            record.kind,
                            record.offset,
                            "path figure state was unexpectedly absent",
                        )
                    })?;
                    figure.segments.push(PathSegment::Cubic {
                        control1: points[index],
                        control2: points[index + 1],
                        to: points[index + 2],
                    });
                    figure.closed |= types[index + 2] & 0x80 != 0;
                    index += 3;
                }
                _ => {
                    return invalid(
                        record.origin,
                        record.index,
                        record.kind,
                        record.offset,
                        "unknown path point type",
                    )
                }
            }
        }
    }
    Ok(Path { figures })
}

fn transform_path(path: &Path, transform: Transform) -> Path {
    Path {
        figures: path
            .figures
            .iter()
            .map(|figure| PathFigure {
                start: transform.transform_point(figure.start),
                segments: figure
                    .segments
                    .iter()
                    .map(|segment| match *segment {
                        PathSegment::Line(point) => {
                            PathSegment::Line(transform.transform_point(point))
                        }
                        PathSegment::Cubic {
                            control1,
                            control2,
                            to,
                        } => PathSegment::Cubic {
                            control1: transform.transform_point(control1),
                            control2: transform.transform_point(control2),
                            to: transform.transform_point(to),
                        },
                    })
                    .collect(),
                closed: figure.closed,
            })
            .collect(),
    }
}

fn transform_clip(clip: &ClipRegion, transform: Transform) -> ClipRegion {
    match clip {
        ClipRegion::Rect(rect) => ClipRegion::Path {
            path: transform_path(&rectangle_path(*rect), transform),
            fill_mode: 1,
        },
        ClipRegion::Polygon(points) => ClipRegion::Polygon(
            points
                .iter()
                .map(|point| transform.transform_point(*point))
                .collect(),
        ),
        ClipRegion::Path { path, fill_mode } => ClipRegion::Path {
            path: transform_path(path, transform),
            fill_mode: *fill_mode,
        },
        ClipRegion::Intersection(regions) => ClipRegion::Intersection(
            regions
                .iter()
                .map(|region| transform_clip(region, transform))
                .collect(),
        ),
        ClipRegion::Combine {
            operation,
            left,
            right,
        } => ClipRegion::Combine {
            operation: *operation,
            left: Box::new(transform_clip(left, transform)),
            right: Box::new(transform_clip(right, transform)),
        },
    }
}

fn record_name(kind: u16) -> &'static str {
    match kind {
        HEADER => "EmfPlusHeader",
        END_OF_FILE => "EmfPlusEndOfFile",
        OBJECT => "EmfPlusObject",
        CLEAR => "EmfPlusClear",
        FILL_RECTS => "EmfPlusFillRects",
        DRAW_RECTS => "EmfPlusDrawRects",
        FILL_POLYGON => "EmfPlusFillPolygon",
        DRAW_LINES => "EmfPlusDrawLines",
        FILL_ELLIPSE => "EmfPlusFillEllipse",
        DRAW_ELLIPSE => "EmfPlusDrawEllipse",
        FILL_PIE => "EmfPlusFillPie",
        DRAW_PIE => "EmfPlusDrawPie",
        DRAW_ARC => "EmfPlusDrawArc",
        FILL_REGION => "EmfPlusFillRegion",
        FILL_PATH => "EmfPlusFillPath",
        DRAW_PATH => "EmfPlusDrawPath",
        FILL_CLOSED_CURVE => "EmfPlusFillClosedCurve",
        DRAW_CLOSED_CURVE => "EmfPlusDrawClosedCurve",
        DRAW_CURVE => "EmfPlusDrawCurve",
        DRAW_BEZIERS => "EmfPlusDrawBeziers",
        DRAW_IMAGE => "EmfPlusDrawImage",
        DRAW_IMAGE_POINTS => "EmfPlusDrawImagePoints",
        DRAW_STRING => "EmfPlusDrawString",
        SAVE => "EmfPlusSave",
        RESTORE => "EmfPlusRestore",
        BEGIN_CONTAINER => "EmfPlusBeginContainer",
        BEGIN_CONTAINER_NO_PARAMS => "EmfPlusBeginContainerNoParams",
        END_CONTAINER => "EmfPlusEndContainer",
        SET_WORLD_TRANSFORM => "EmfPlusSetWorldTransform",
        RESET_WORLD_TRANSFORM => "EmfPlusResetWorldTransform",
        MULTIPLY_WORLD_TRANSFORM => "EmfPlusMultiplyWorldTransform",
        TRANSLATE_WORLD_TRANSFORM => "EmfPlusTranslateWorldTransform",
        SCALE_WORLD_TRANSFORM => "EmfPlusScaleWorldTransform",
        ROTATE_WORLD_TRANSFORM => "EmfPlusRotateWorldTransform",
        SET_PAGE_TRANSFORM => "EmfPlusSetPageTransform",
        RESET_CLIP => "EmfPlusResetClip",
        SET_CLIP_RECT => "EmfPlusSetClipRect",
        SET_CLIP_PATH => "EmfPlusSetClipPath",
        SET_CLIP_REGION => "EmfPlusSetClipRegion",
        OFFSET_CLIP => "EmfPlusOffsetClip",
        SET_COMPOSITING_MODE => "EmfPlusSetCompositingMode",
        SET_COMPOSITING_QUALITY => "EmfPlusSetCompositingQuality",
        SET_RENDERING_ORIGIN => "EmfPlusSetRenderingOrigin",
        SET_TEXT_RENDERING_HINT => "EmfPlusSetTextRenderingHint",
        SET_TEXT_CONTRAST => "EmfPlusSetTextContrast",
        SET_INTERPOLATION_MODE => "EmfPlusSetInterpolationMode",
        SET_PIXEL_OFFSET_MODE => "EmfPlusSetPixelOffsetMode",
        SET_SMOOTHING_MODE => "EmfPlusSetSmoothingMode",
        _ => "UnknownEmfPlusRecord",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use metafile_svg::SvgRenderer;

    fn bytes_u32(values: &[u32]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    fn floats(values: &[f32]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    fn record(kind: u16, flags: u16, mut data: Vec<u8>) -> Vec<u8> {
        while data.len().rem_euclid(4) != 0 {
            data.push(0);
        }
        let mut output = Vec::new();
        output.extend_from_slice(&kind.to_le_bytes());
        output.extend_from_slice(&flags.to_le_bytes());
        output.extend_from_slice(&(12u32 + data.len() as u32).to_le_bytes());
        output.extend_from_slice(&(data.len() as u32).to_le_bytes());
        output.extend_from_slice(&data);
        output
    }

    fn stream(dual: bool, records: &[Vec<u8>]) -> Vec<u8> {
        let mut header = bytes_u32(&[0xdbc0_1002, u32::from(dual), 96, 96]);
        let mut output = record(HEADER, u16::from(dual), std::mem::take(&mut header));
        for record in records {
            output.extend_from_slice(record);
        }
        output.extend_from_slice(&record(END_OF_FILE, 0, Vec::new()));
        output
    }

    fn comment(data: &[u8]) -> Comment<'_> {
        Comment {
            outer_record_index: 7,
            offset: 100,
            data,
        }
    }

    fn render(data: &[u8], strict: bool) -> Result<(String, Vec<Diagnostic>)> {
        let options = RenderOptions {
            strict,
            ..RenderOptions::default()
        };
        let mut renderer = SvgRenderer::new(options.limits.clone());
        let result = playback(
            &[comment(data)],
            &options,
            Rect {
                left: 0.0,
                top: 0.0,
                right: 400.0,
                bottom: 300.0,
            },
            &mut renderer,
        )?;
        Ok((
            renderer.finish(
                Some(Rect {
                    left: 0.0,
                    top: 0.0,
                    right: 400.0,
                    bottom: 300.0,
                }),
                None,
            )?,
            result.diagnostics,
        ))
    }

    fn solid_brush(color: u32) -> Vec<u8> {
        bytes_u32(&[0xdbc0_1002, 0, color])
    }

    fn pen(color: u32, width: f32, flags: u32, optional: &[u8]) -> Vec<u8> {
        let mut data = bytes_u32(&[0xdbc0_1002, 0, flags, 2]);
        data.extend_from_slice(&width.to_le_bytes());
        data.extend_from_slice(optional);
        data.extend_from_slice(&solid_brush(color));
        data
    }

    #[test]
    fn parses_split_stream_and_dual_metadata() {
        let data = stream(true, &[]);
        let split = 9;
        let comments = [
            Comment {
                outer_record_index: 1,
                offset: 20,
                data: &data[..split],
            },
            Comment {
                outer_record_index: 2,
                offset: 80,
                data: &data[split..],
            },
        ];
        let info = inspect(&comments, &ResourceLimits::default()).unwrap();
        assert!(info.dual);
        assert!(info.has_eof);
        assert_eq!(info.record_count, 2);
    }

    #[test]
    fn only_or_dual_comes_from_the_header_record_flag() {
        let mut only = stream(false, &[]);
        only[16..20].copy_from_slice(&1u32.to_le_bytes());
        assert!(
            !inspect(&[comment(&only)], &ResourceLimits::default())
                .unwrap()
                .dual
        );

        let dual = stream(true, &[]);
        assert!(
            inspect(&[comment(&dual)], &ResourceLimits::default())
                .unwrap()
                .dual
        );
    }

    #[test]
    fn rejects_truncated_and_misaligned_records() {
        let mut truncated = stream(false, &[]);
        truncated.pop();
        assert!(matches!(
            inspect(&[comment(&truncated)], &ResourceLimits::default()),
            Err(MetafileError::InvalidEmfPlus { .. })
        ));
        let mut invalid = stream(false, &[]);
        invalid[4..8].copy_from_slice(&13u32.to_le_bytes());
        assert!(matches!(
            inspect(&[comment(&invalid)], &ResourceLimits::default()),
            Err(MetafileError::InvalidEmfPlus { .. })
        ));
    }

    #[test]
    fn renders_inline_solid_rectangles() {
        let mut data = bytes_u32(&[0xffff_0000, 1]);
        data.extend_from_slice(&floats(&[10.0, 20.0, 30.0, 40.0]));
        let (svg, _) = render(&stream(false, &[record(FILL_RECTS, 0x8000, data)]), false).unwrap();
        assert!(
            svg.contains("fill=\"#ff0000\"") && svg.contains("M 10 20 L 40 20"),
            "{svg}"
        );
    }

    #[test]
    fn renders_pen_dash_and_ellipse() {
        let pen = record(
            OBJECT,
            0x0200,
            pen(0xff00_00ff, 3.0, 0x20, &1u32.to_le_bytes()),
        );
        let (svg, _) = render(
            &stream(
                false,
                &[
                    pen,
                    record(
                        DRAW_ELLIPSE,
                        0x4000,
                        [
                            10i16.to_le_bytes(),
                            20i16.to_le_bytes(),
                            30i16.to_le_bytes(),
                            40i16.to_le_bytes(),
                        ]
                        .concat(),
                    ),
                ],
            ),
            false,
        )
        .unwrap();
        assert!(
            svg.contains("stroke=\"#0000ff\"") && svg.contains("stroke-dasharray"),
            "{svg}"
        );
    }

    #[test]
    fn renders_path_object_with_multiple_segments() {
        let mut path = bytes_u32(&[0xdbc0_1002, 4, 0x4000]);
        for point in [(0i16, 0i16), (20, 0), (20, 20), (0, 20)] {
            path.extend_from_slice(&point.0.to_le_bytes());
            path.extend_from_slice(&point.1.to_le_bytes());
        }
        path.extend_from_slice(&[0, 1, 1, 0x81]);
        let object = record(OBJECT, 0x0301, path);
        let fill = record(FILL_PATH, 0x8001, 0xff00_ff00u32.to_le_bytes().to_vec());
        let (svg, _) = render(&stream(false, &[object, fill]), false).unwrap();
        assert!(svg.contains("M 0 0 L 20 0 L 20 20 L 0 20 Z"), "{svg}");
    }

    #[test]
    fn world_transform_and_save_restore_are_ordered() {
        let brush = record(OBJECT, 0x0100, solid_brush(0xff00_0000));
        let save = record(SAVE, 0, 9u32.to_le_bytes().to_vec());
        let translate = record(TRANSLATE_WORLD_TRANSFORM, 0, floats(&[50.0, 0.0]));
        let mut rect = bytes_u32(&[0, 1]);
        rect.extend_from_slice(&floats(&[0.0, 0.0, 10.0, 10.0]));
        let restore = record(RESTORE, 0, 9u32.to_le_bytes().to_vec());
        let (svg, _) = render(
            &stream(
                false,
                &[
                    brush,
                    save,
                    translate,
                    record(FILL_RECTS, 0, rect.clone()),
                    restore,
                    record(FILL_RECTS, 0, rect),
                ],
            ),
            false,
        )
        .unwrap();
        assert!(
            svg.contains("M 50 0 L 60 0") && svg.contains("M 0 0 L 10 0"),
            "{svg}"
        );
    }

    #[test]
    fn rectangle_clip_is_applied() {
        let clip = record(SET_CLIP_RECT, 0, floats(&[0.0, 0.0, 20.0, 20.0]));
        let mut rect = bytes_u32(&[0xffff_0000, 1]);
        rect.extend_from_slice(&floats(&[0.0, 0.0, 100.0, 100.0]));
        let (svg, _) = render(
            &stream(false, &[clip, record(FILL_RECTS, 0x8000, rect)]),
            false,
        )
        .unwrap();
        assert!(
            svg.contains("<clipPath") && svg.contains("clip-path="),
            "{svg}"
        );
    }

    #[test]
    fn hatch_and_linear_gradient_are_deterministic_resources() {
        let hatch = record(
            OBJECT,
            0x0100,
            bytes_u32(&[0xdbc0_1002, 1, 4, 0xffff_0000, 0xff00_00ff]),
        );
        let mut gradient_data = bytes_u32(&[0xdbc0_1002, 4, 0, 0]);
        gradient_data.extend_from_slice(&floats(&[0.0, 0.0, 100.0, 100.0]));
        gradient_data.extend_from_slice(&bytes_u32(&[0xff00_0000, 0xffff_ffff, 0, 0]));
        let gradient = record(OBJECT, 0x0101, gradient_data);
        let mut first = bytes_u32(&[0, 1]);
        first.extend_from_slice(&floats(&[0.0, 0.0, 20.0, 20.0]));
        let mut second = bytes_u32(&[1, 1]);
        second.extend_from_slice(&floats(&[30.0, 0.0, 20.0, 20.0]));
        let data = stream(
            false,
            &[
                hatch,
                gradient,
                record(FILL_RECTS, 0, first),
                record(FILL_RECTS, 0, second),
            ],
        );
        let (svg, _) = render(&data, false).unwrap();
        assert!(
            svg.contains("<pattern id=\"paint0\"") && svg.contains("<linearGradient id=\"paint1\""),
            "{svg}"
        );
        assert_eq!(svg, render(&data, false).unwrap().0);
    }

    #[test]
    fn font_and_draw_string_preserve_unicode() {
        let family: Vec<u8> = "Arial".encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut font = bytes_u32(&[0xdbc0_1002]);
        font.extend_from_slice(&12f32.to_le_bytes());
        font.extend_from_slice(&bytes_u32(&[3, 3, 0, 5]));
        font.extend_from_slice(&family);
        let object = record(OBJECT, 0x0601, font);
        let text: Vec<u8> = "AΩ".encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut draw = bytes_u32(&[0xff00_0000, u32::MAX, 2]);
        draw.extend_from_slice(&floats(&[10.0, 20.0, 100.0, 30.0]));
        draw.extend_from_slice(&text);
        let (svg, diagnostics) = render(
            &stream(false, &[object, record(DRAW_STRING, 0x8001, draw)]),
            false,
        )
        .unwrap();
        assert!(
            svg.contains("AΩ")
                && svg.contains("font-weight=\"700\"")
                && svg.contains("font-style=\"italic\""),
            "{svg}"
        );
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "emfplus_text_metrics_approximate"));
    }

    #[test]
    fn draw_string_anchors_are_relative_to_layout_rect_and_transform() {
        let family: Vec<u8> = "Arial".encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut font = bytes_u32(&[0xdbc0_1002]);
        font.extend_from_slice(&12f32.to_le_bytes());
        font.extend_from_slice(&bytes_u32(&[3, 0, 0, 5]));
        font.extend_from_slice(&family);
        let mut format = vec![0; 60];
        format[12..16].copy_from_slice(&1u32.to_le_bytes());
        format[16..20].copy_from_slice(&1u32.to_le_bytes());
        let text: Vec<u8> = "Ω".encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut draw = bytes_u32(&[0xff00_0000, 2, 1]);
        draw.extend_from_slice(&floats(&[10.0, 20.0, 100.0, 50.0]));
        draw.extend_from_slice(&text);
        let translated = record(TRANSLATE_WORLD_TRANSFORM, 0, floats(&[5.0, 7.0]));
        let (svg, _) = render(
            &stream(
                false,
                &[
                    record(OBJECT, 0x0601, font),
                    record(OBJECT, 0x0702, format),
                    translated,
                    record(DRAW_STRING, 0x8001, draw),
                ],
            ),
            false,
        )
        .unwrap();
        assert!(
            svg.contains("x=\"60\" y=\"45\"")
                && svg.contains("matrix(1 0 0 1 5 7)")
                && svg.contains("Ω"),
            "{svg}"
        );
    }

    #[test]
    fn layout_anchor_covers_near_center_far_and_zero_size() {
        let layout = Rect {
            left: 10.0,
            top: 20.0,
            right: 110.0,
            bottom: 70.0,
        };
        assert_eq!(
            layout_anchor(
                layout,
                HorizontalTextAlignment::Left,
                VerticalTextAlignment::Top
            ),
            Point::new(10.0, 20.0)
        );
        assert_eq!(
            layout_anchor(
                layout,
                HorizontalTextAlignment::Center,
                VerticalTextAlignment::Center
            ),
            Point::new(60.0, 45.0)
        );
        assert_eq!(
            layout_anchor(
                layout,
                HorizontalTextAlignment::Right,
                VerticalTextAlignment::Bottom
            ),
            Point::new(110.0, 70.0)
        );
        let zero = Rect {
            left: 7.0,
            top: 9.0,
            right: 7.0,
            bottom: 9.0,
        };
        assert_eq!(
            layout_anchor(
                zero,
                HorizontalTextAlignment::Center,
                VerticalTextAlignment::Center
            ),
            Point::new(7.0, 9.0)
        );
    }

    #[test]
    fn image_source_crop_selects_exact_pixels() {
        let bitmap = Bitmap {
            width: 3,
            height: 2,
            rgba: (0u8..24).collect(),
        };
        let dummy = Record {
            index: 0,
            kind: DRAW_IMAGE,
            flags: 0,
            offset: 0,
            origin: Origin {
                logical_start: 0,
                logical_end: 0,
                outer_record_index: 0,
                absolute_start: 0,
            },
            data: &[],
        };
        let cropped = crop_bitmap(
            &bitmap,
            Rect {
                left: 1.0,
                top: 0.0,
                right: 3.0,
                bottom: 1.0,
            },
            dummy,
        )
        .unwrap();
        assert_eq!((cropped.width, cropped.height), (2, 1));
        assert_eq!(cropped.rgba, (4u8..12).collect::<Vec<_>>());
    }

    #[test]
    fn draw_lines_supports_relative_and_closed_flags() {
        let pen = record(OBJECT, 0x0200, pen(0xff00_00ff, 1.0, 0, &[]));
        let mut relative = bytes_u32(&[3]);
        relative.extend_from_slice(&[10, 10, 10, 0, 0, 10]);
        relative.extend_from_slice(&[0, 0]);
        let (svg, _) = render(
            &stream(false, &[pen, record(DRAW_LINES, 0x2800, relative)]),
            false,
        )
        .unwrap();
        assert!(svg.contains("M 10 10 L 20 10 L 20 20 Z"), "{svg}");

        let malformed = stream(false, &[record(DRAW_LINES, 0x0800, bytes_u32(&[2]))]);
        assert!(matches!(
            render(&malformed, false),
            Err(MetafileError::InvalidEmfPlus { .. })
        ));
    }

    #[test]
    fn linear_gradient_applies_brush_then_world_transform() {
        let mut gradient = bytes_u32(&[0xdbc0_1002, 4, 2, 0]);
        gradient.extend_from_slice(&floats(&[0.0, 0.0, 10.0, 20.0]));
        gradient.extend_from_slice(&bytes_u32(&[0xff00_0000, 0xffff_ffff, 0, 0]));
        gradient.extend_from_slice(&floats(&[2.0, 0.0, 0.0, 3.0, 5.0, 7.0]));
        let mut rect = bytes_u32(&[0, 1]);
        rect.extend_from_slice(&floats(&[0.0, 0.0, 20.0, 20.0]));
        let (svg, _) = render(
            &stream(
                false,
                &[
                    record(OBJECT, 0x0100, gradient),
                    record(TRANSLATE_WORLD_TRANSFORM, 0, floats(&[11.0, 13.0])),
                    record(FILL_RECTS, 0, rect),
                ],
            ),
            false,
        )
        .unwrap();
        assert!(
            svg.contains("x1=\"0\" y1=\"0\" x2=\"10\" y2=\"20\"")
                && svg.contains("gradientTransform=\"matrix(2 0 0 3 16 20)\""),
            "{svg}"
        );
    }

    #[test]
    fn raw_argb_image_preserves_alpha() {
        let mut image = bytes_u32(&[0xdbc0_1002, 1, 1, 1, 4, 0x0026_200a, 0]);
        image.extend_from_slice(&[10, 20, 30, 128]);
        let object = record(OBJECT, 0x0500, image);
        let mut draw = bytes_u32(&[u32::MAX, 2]);
        draw.extend_from_slice(&floats(&[0.0, 0.0, 1.0, 1.0, 10.0, 20.0, 30.0, 40.0]));
        let (svg, diagnostics) = render(
            &stream(false, &[object, record(DRAW_IMAGE, 0, draw)]),
            false,
        )
        .unwrap();
        assert!(
            svg.contains("data:image/png;base64") && svg.contains("matrix(30 0 0 40 10 20)"),
            "{svg}"
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn unknown_record_is_diagnostic_or_strict_error() {
        let data = stream(false, &[record(0x40ff, 0, Vec::new())]);
        assert!(render(&data, false)
            .unwrap()
            .1
            .iter()
            .any(|diagnostic| diagnostic.code == "emfplus_unsupported_record"));
        assert!(matches!(
            render(&data, true),
            Err(MetafileError::UnsupportedCriticalFeature(_))
        ));
    }

    #[test]
    fn unsupported_objects_fail_only_when_materially_used() {
        let path_gradient = record(OBJECT, 0x0100, bytes_u32(&[0xdbc0_1002, 3]));
        assert!(render(&stream(false, std::slice::from_ref(&path_gradient)), false).is_ok());
        let mut rect = bytes_u32(&[0, 1]);
        rect.extend_from_slice(&floats(&[0.0, 0.0, 10.0, 10.0]));
        let used = stream(false, &[path_gradient, record(FILL_RECTS, 0, rect)]);
        let (_, diagnostics) = render(&used, false).unwrap();
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "emfplus_unsupported_semantics"));
        assert!(matches!(
            render(&used, true),
            Err(MetafileError::UnsupportedCriticalFeature(_))
        ));
    }

    #[test]
    fn object_continuation_and_replacement_are_bounded() {
        let brush = solid_brush(0xffff_0000);
        let mut first = (brush.len() as u32).to_le_bytes().to_vec();
        first.extend_from_slice(&brush[..4]);
        let mut last = (brush.len() as u32).to_le_bytes().to_vec();
        last.extend_from_slice(&brush[4..]);
        let data = stream(
            false,
            &[
                record(OBJECT, 0x8100, first),
                record(OBJECT, 0x0100, last),
                record(OBJECT, 0x0100, solid_brush(0xff00_ff00)),
            ],
        );
        assert!(render(&data, false).is_ok());
        let mut options = RenderOptions::default();
        options.limits.max_object_bytes = 4;
        let mut renderer = SvgRenderer::new(options.limits.clone());
        assert!(matches!(
            playback(
                &[comment(&data)],
                &options,
                Rect {
                    left: 0.0,
                    top: 0.0,
                    right: 1.0,
                    bottom: 1.0
                },
                &mut renderer
            ),
            Err(MetafileError::ResourceLimitExceeded {
                resource: "EMF+ object bytes",
                ..
            })
        ));
    }

    #[test]
    fn non_finite_transform_is_rejected() {
        let data = stream(
            false,
            &[record(
                SET_WORLD_TRANSFORM,
                0,
                floats(&[f32::NAN, 0.0, 0.0, 1.0, 0.0, 0.0]),
            )],
        );
        assert!(matches!(
            render(&data, false),
            Err(MetafileError::InvalidEmfPlus { .. })
        ));
    }

    fn dummy_record() -> Record<'static> {
        Record {
            index: 0,
            kind: OBJECT,
            flags: 0,
            offset: 0,
            origin: Origin {
                logical_start: 0,
                logical_end: 0,
                outer_record_index: 0,
                absolute_start: 0,
            },
            data: &[],
        }
    }

    #[test]
    fn decodes_png_and_jpeg_images_with_bounded_dimensions() {
        let mut png_bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_bytes, 2, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer
                .write_image_data(&[255, 0, 0, 128, 0, 0, 255, 255])
                .unwrap();
        }
        let png = decode_compressed_image(&png_bytes, &ResourceLimits::default(), dummy_record())
            .unwrap();
        assert_eq!((png.width, png.height), (2, 1));
        assert_eq!(png.rgba, [255, 0, 0, 128, 0, 0, 255, 255]);

        let mut jpeg_bytes = Vec::new();
        jpeg_encoder::Encoder::new(&mut jpeg_bytes, 100)
            .encode(&[240, 20, 10], 1, 1, jpeg_encoder::ColorType::Rgb)
            .unwrap();
        let jpeg = decode_compressed_image(&jpeg_bytes, &ResourceLimits::default(), dummy_record())
            .unwrap();
        assert_eq!((jpeg.width, jpeg.height), (1, 1));
        assert!(jpeg.rgba[0] > 200 && jpeg.rgba[1] < 60 && jpeg.rgba[3] == 255);

        assert!(matches!(
            decode_compressed_image(b"not an image", &ResourceLimits::default(), dummy_record()),
            Err(MetafileError::UnsupportedCriticalFeature(_))
        ));
    }

    #[test]
    fn texture_brush_embeds_a_deterministic_transformed_pattern() {
        let mut image = bytes_u32(&[0xdbc0_1002, 1, 1, 1, 4, 0x0026_200a, 0]);
        image.extend_from_slice(&[0, 0, 255, 255]);
        let mut texture = bytes_u32(&[0xdbc0_1002, 2, 2, 1]);
        texture.extend_from_slice(&floats(&[1.0, 0.0, 0.0, 1.0, 3.0, 4.0]));
        texture.extend_from_slice(&image);
        let mut rect = bytes_u32(&[0, 1]);
        rect.extend_from_slice(&floats(&[0.0, 0.0, 20.0, 20.0]));
        let data = stream(
            false,
            &[record(OBJECT, 0x0100, texture), record(FILL_RECTS, 0, rect)],
        );
        let (svg, diagnostics) = render(&data, false).unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert!(
            svg.contains("<pattern id=\"paint0\"") && svg.contains("matrix(1 0 0 1 3 4)"),
            "{svg}"
        );
        assert_eq!(svg, render(&data, false).unwrap().0);
    }

    #[test]
    fn recursive_boolean_region_is_retained_as_a_mask() {
        let mut region = bytes_u32(&[0xdbc0_1002, 2, 2, 0x1000_0000]);
        region.extend_from_slice(&floats(&[0.0, 0.0, 20.0, 20.0]));
        region.extend_from_slice(&0x1000_0000u32.to_le_bytes());
        region.extend_from_slice(&floats(&[30.0, 0.0, 20.0, 20.0]));
        let mut rect = bytes_u32(&[0xffff_0000, 1]);
        rect.extend_from_slice(&floats(&[0.0, 0.0, 100.0, 100.0]));
        let data = stream(
            false,
            &[
                record(OBJECT, 0x0400, region),
                record(SET_CLIP_REGION, 0, Vec::new()),
                record(FILL_RECTS, 0x8000, rect),
            ],
        );
        let (svg, diagnostics) = render(&data, false).unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert!(
            svg.contains("<mask id=\"mask") && svg.contains("mask=\"url(#mask"),
            "{svg}"
        );
    }

    #[test]
    fn gradient_blend_factors_become_color_stops() {
        let mut gradient = bytes_u32(&[0xdbc0_1002, 4, 8, 0]);
        gradient.extend_from_slice(&floats(&[0.0, 0.0, 100.0, 0.0]));
        gradient.extend_from_slice(&bytes_u32(&[0xffff_0000, 0xff00_00ff, 0, 0, 3]));
        gradient.extend_from_slice(&floats(&[0.0, 0.5, 1.0, 0.0, 0.5, 1.0]));
        let paint = parse_brush(&gradient, &ResourceLimits::default(), dummy_record()).unwrap();
        let Paint::LinearGradient { stops, .. } = paint else {
            panic!("not a gradient")
        };
        assert_eq!(stops.len(), 3);
        assert_eq!(stops[0].color, argb(0xff00_00ff));
        assert_eq!(stops[2].color, argb(0xffff_0000));
    }

    #[test]
    fn curve_records_render_cardinal_splines_as_cubics() {
        let pen = record(OBJECT, 0x0200, pen(0xff00_0000, 1.0, 0, &[]));
        let mut curve = floats(&[0.5]);
        curve.extend_from_slice(&bytes_u32(&[0, 3, 4]));
        curve.extend_from_slice(&floats(&[0.0, 0.0, 20.0, 0.0, 20.0, 20.0, 40.0, 20.0]));
        let (svg, diagnostics) =
            render(&stream(false, &[pen, record(DRAW_CURVE, 0, curve)]), false).unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert!(
            svg.contains("M 0 0 C") && svg.matches(" C ").count() == 3,
            "{svg}"
        );
    }

    #[test]
    fn affine_draw_string_uses_a_full_text_matrix() {
        let family: Vec<u8> = "Arial".encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut font = bytes_u32(&[0xdbc0_1002]);
        font.extend_from_slice(&12f32.to_le_bytes());
        font.extend_from_slice(&bytes_u32(&[2, 0, 0, 5]));
        font.extend_from_slice(&family);
        let mut draw = bytes_u32(&[0xff00_0000, u32::MAX, 1]);
        draw.extend_from_slice(&floats(&[10.0, 20.0, 0.0, 0.0]));
        draw.extend_from_slice(&('A' as u16).to_le_bytes());
        let transform = record(
            SET_WORLD_TRANSFORM,
            0,
            floats(&[0.0, 1.0, -1.0, 0.0, 50.0, 5.0]),
        );
        let (svg, _) = render(
            &stream(
                false,
                &[
                    record(OBJECT, 0x0601, font),
                    transform,
                    record(DRAW_STRING, 0x8001, draw),
                ],
            ),
            false,
        )
        .unwrap();
        assert!(svg.contains("transform=\"matrix(0 1 -1 0 50 5)\""), "{svg}");
    }

    #[test]
    fn full_image_point_placement_uses_pixel_centres() {
        let placement = inset_bitmap_placement(
            BitmapPlacement {
                origin: Point::new(320.0, 35.0),
                x_axis: Vector::new(240.0, 35.0),
                y_axis: Vector::new(-30.0, 185.0),
            },
            8,
            6,
        );
        assert!((placement.origin.x - 332.5).abs() < 1e-9);
        assert!((placement.origin.y - 52.604_166_666_7).abs() < 1e-9);
        assert_eq!(placement.x_axis, Vector::new(210.0, 30.625));
        assert!((placement.y_axis.x + 25.0).abs() < 1e-9);
        assert!((placement.y_axis.y - 154.166_666_666_7).abs() < 1e-9);
    }
}
