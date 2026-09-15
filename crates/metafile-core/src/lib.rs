//! Shared, format-neutral graphics types and renderer events.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// A translation-independent displacement or extent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Vector {
    pub x: f64,
    pub y: f64,
}

impl Vector {
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

impl Point {
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl Rect {
    #[must_use]
    pub fn normalized(self) -> Self {
        Self {
            left: self.left.min(self.right),
            top: self.top.min(self.bottom),
            right: self.left.max(self.right),
            bottom: self.top.max(self.bottom),
        }
    }
    #[must_use]
    pub fn width(self) -> f64 {
        (self.right - self.left).abs()
    }
    #[must_use]
    pub fn height(self) -> f64 {
        (self.bottom - self.top).abs()
    }
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.left.is_finite()
            && self.top.is_finite()
            && self.right.is_finite()
            && self.bottom.is_finite()
    }
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        let a = self.normalized();
        let b = other.normalized();
        Self {
            left: a.left.min(b.left),
            top: a.top.min(b.top),
            right: a.right.max(b.right),
            bottom: a.bottom.max(b.bottom),
        }
    }
    #[must_use]
    pub fn intersection(self, other: Self) -> Option<Self> {
        let a = self.normalized();
        let b = other.normalized();
        let result = Self {
            left: a.left.max(b.left),
            top: a.top.max(b.top),
            right: a.right.min(b.right),
            bottom: a.bottom.min(b.bottom),
        };
        (result.left < result.right && result.top < result.bottom).then_some(result)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
    #[must_use]
    pub fn css(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!(
                "rgba({},{},{},{:.3})",
                self.r,
                self.g,
                self.b,
                f64::from(self.a) / 255.0
            )
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MetafileFormat {
    Wmf,
    Emf,
    EmfPlus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub record_index: Option<u32>,
    pub record_type: Option<u32>,
    pub record_name: Option<String>,
    pub offset: Option<usize>,
    pub occurrences: u32,
}

impl Diagnostic {
    #[must_use]
    pub fn warning(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            code: code.into(),
            message: message.into(),
            record_index: None,
            record_type: None,
            record_name: None,
            offset: None,
            occurrences: 1,
        }
    }
    #[must_use]
    pub fn at_record(mut self, index: usize, record_type: u32, name: &str, offset: usize) -> Self {
        self.record_index = Some(index as u32);
        self.record_type = Some(record_type);
        self.record_name = Some(name.into());
        self.offset = Some(offset);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[serde(rename_all = "camelCase")]
pub struct ResourceLimits {
    pub max_input_bytes: usize,
    pub max_records: usize,
    pub max_dimension: u32,
    pub max_pixels: u64,
    pub max_svg_bytes: usize,
    pub max_dc_depth: usize,
    pub max_objects: usize,
    pub max_points_per_record: usize,
    pub max_diagnostics: usize,
    pub max_object_bytes: usize,
    pub max_comment_bytes: usize,
    pub max_gradient_stops: usize,
    pub max_dash_entries: usize,
    pub max_string_chars: usize,
    pub max_region_nodes: usize,
    pub max_container_depth: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 64 * 1024 * 1024,
            max_records: 200_000,
            max_dimension: 8192,
            max_pixels: 8192 * 8192,
            max_svg_bytes: 64 * 1024 * 1024,
            max_dc_depth: 256,
            max_objects: 16_384,
            max_points_per_record: 1_000_000,
            max_diagnostics: 256,
            max_object_bytes: 16 * 1024 * 1024,
            max_comment_bytes: 32 * 1024 * 1024,
            max_gradient_stops: 16_384,
            max_dash_entries: 16_384,
            max_string_chars: 4 * 1024 * 1024,
            max_region_nodes: 100_000,
            max_container_depth: 256,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
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

#[derive(Debug, Error)]
pub enum MetafileError {
    #[error("unsupported metafile format")]
    UnsupportedFormat,
    #[error("metafile format mismatch: expected {expected:?}, received {actual:?}")]
    FormatMismatch {
        expected: MetafileFormat,
        actual: MetafileFormat,
    },
    #[error("invalid header: {0}")]
    InvalidHeader(String),
    #[error("invalid placeable header: {0}")]
    InvalidPlaceableHeader(String),
    #[error("placeable WMF checksum mismatch: expected {expected:#06x}, computed {actual:#06x}")]
    InvalidChecksum { expected: u16, actual: u16 },
    #[error("truncated input at byte {offset}: needed {needed} bytes, {available} available")]
    TruncatedInput {
        offset: usize,
        needed: usize,
        available: usize,
    },
    #[error("invalid record size {words} words at byte {offset}")]
    InvalidRecordSize { offset: usize, words: u32 },
    #[error("record at byte {offset} extends to {end}, past input length {input_len}")]
    RecordOutOfBounds {
        offset: usize,
        end: usize,
        input_len: usize,
    },
    #[error("resource limit exceeded ({resource}): {actual} > {limit}")]
    ResourceLimitExceeded {
        resource: &'static str,
        actual: u64,
        limit: u64,
    },
    #[error("invalid object handle {handle} at record {record_index}")]
    InvalidObjectHandle { handle: u32, record_index: usize },
    #[error("object handle {handle} is selected and cannot be deleted at record {record_index}")]
    ObjectInUse { handle: u32, record_index: usize },
    #[error(
        "invalid RestoreDC value {value} at record {record_index} with stack depth {stack_depth}"
    )]
    InvalidRestoreDc {
        value: i32,
        record_index: usize,
        stack_depth: usize,
    },
    #[error("invalid bitmap at record {record_index}: {message}")]
    InvalidBitmap {
        record_index: usize,
        message: String,
    },
    #[error("unsupported bitmap at record {record_index}: {message}")]
    UnsupportedBitmap {
        record_index: usize,
        message: String,
    },
    #[error("unsupported critical feature: {0}")]
    UnsupportedCriticalFeature(String),
    #[error(
        "invalid EMF+ record {inner_record_index} (type {record_type:#06x}) in outer EMF record {outer_record_index} at byte {offset}: {message}"
    )]
    InvalidEmfPlus {
        outer_record_index: usize,
        inner_record_index: usize,
        record_type: u16,
        offset: usize,
        message: String,
    },
    #[error("SVG generation failed: {0}")]
    SvgGeneration(String),
}

pub type Result<T> = std::result::Result<T, MetafileError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PenStyle {
    Solid,
    Dash,
    Dot,
    DashDot,
    DashDotDot,
    Null,
    InsideFrame,
    Unknown(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pen {
    pub style: PenStyle,
    pub width: f64,
    /// A cosmetic pen remains one output-device pixel wide regardless of mapping scale.
    pub cosmetic: bool,
    pub color: Color,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
}
impl Default for Pen {
    fn default() -> Self {
        Self {
            style: PenStyle::Solid,
            width: 1.0,
            cosmetic: true,
            color: Color::BLACK,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrushStyle {
    Solid,
    Null,
    Hatched(u16),
    Pattern,
    Unknown(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Brush {
    pub style: BrushStyle,
    pub color: Color,
}

/// Renderer-neutral paint used by formats with richer brushes than classic GDI.
#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
    Solid(Color),
    Hatch {
        style: u32,
        foreground: Color,
        background: Color,
    },
    LinearGradient {
        start: Point,
        end: Point,
        stops: Vec<GradientStop>,
        wrap_mode: u32,
        transform: Transform,
        gamma_corrected: bool,
    },
    Texture {
        bitmap: Bitmap,
        transform: Transform,
        wrap_mode: u32,
        opacity: f64,
        do_not_transform: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStop {
    pub offset: f64,
    pub color: Color,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub paint: Paint,
    pub width: f64,
    /// GDI+ `UnitType` for the pen width (Pixel by default).
    pub unit: u32,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
    pub miter_limit: f64,
    pub dash_pattern: Vec<f64>,
    pub dash_offset: f64,
}
impl Default for Brush {
    fn default() -> Self {
        Self {
            style: BrushStyle::Solid,
            color: Color::WHITE,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    pub height: f64,
    pub width: f64,
    pub escapement_tenths: i16,
    pub weight: u16,
    pub italic: bool,
    pub underline: bool,
    pub strike_out: bool,
    pub charset: u8,
    pub family: String,
}
impl Default for Font {
    fn default() -> Self {
        Self {
            height: 12.0,
            width: 0.0,
            escapement_tenths: 0,
            weight: 400,
            italic: false,
            underline: false,
            strike_out: false,
            charset: 0,
            family: "sans-serif".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum GdiObject {
    Pen(Pen),
    Brush(Brush),
    Font(Font),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mapping {
    pub window_origin: Point,
    pub window_extent: Point,
    pub viewport_origin: Point,
    pub viewport_extent: Point,
}

/// A format-neutral GDI affine world transform.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub m11: f64,
    pub m12: f64,
    pub m21: f64,
    pub m22: f64,
    pub dx: f64,
    pub dy: f64,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Self = Self {
        m11: 1.0,
        m12: 0.0,
        m21: 0.0,
        m22: 1.0,
        dx: 0.0,
        dy: 0.0,
    };

    #[must_use]
    pub fn transform_point(self, point: Point) -> Point {
        Point::new(
            point.x * self.m11 + point.y * self.m21 + self.dx,
            point.x * self.m12 + point.y * self.m22 + self.dy,
        )
    }

    #[must_use]
    pub fn transform_vector(self, vector: Vector) -> Vector {
        Vector::new(
            vector.x * self.m11 + vector.y * self.m21,
            vector.x * self.m12 + vector.y * self.m22,
        )
    }

    /// Returns the transform which applies `other` and then `self`.
    #[must_use]
    pub fn compose(self, other: Self) -> Self {
        Self {
            m11: other.m11 * self.m11 + other.m12 * self.m21,
            m12: other.m11 * self.m12 + other.m12 * self.m22,
            m21: other.m21 * self.m11 + other.m22 * self.m21,
            m22: other.m21 * self.m12 + other.m22 * self.m22,
            dx: other.dx * self.m11 + other.dy * self.m21 + self.dx,
            dy: other.dx * self.m12 + other.dy * self.m22 + self.dy,
        }
    }

    #[must_use]
    pub fn is_finite(self) -> bool {
        [self.m11, self.m12, self.m21, self.m22, self.dx, self.dy]
            .iter()
            .all(|value| value.is_finite())
    }
}
impl Default for Mapping {
    fn default() -> Self {
        Self {
            window_origin: Point::default(),
            window_extent: Point::new(1.0, 1.0),
            viewport_origin: Point::default(),
            viewport_extent: Point::new(1.0, 1.0),
        }
    }
}
impl Mapping {
    #[must_use]
    pub fn transform_point(self, point: Point) -> Point {
        let sx = if self.window_extent.x == 0.0 {
            1.0
        } else {
            self.viewport_extent.x / self.window_extent.x
        };
        let sy = if self.window_extent.y == 0.0 {
            1.0
        } else {
            self.viewport_extent.y / self.window_extent.y
        };
        Point::new(
            (point.x - self.window_origin.x) * sx + self.viewport_origin.x,
            (point.y - self.window_origin.y) * sy + self.viewport_origin.y,
        )
    }
    #[must_use]
    pub fn transform_vector(self, vector: Vector) -> Vector {
        let sx = if self.window_extent.x == 0.0 {
            1.0
        } else {
            self.viewport_extent.x / self.window_extent.x
        };
        let sy = if self.window_extent.y == 0.0 {
            1.0
        } else {
            self.viewport_extent.y / self.window_extent.y
        };
        Vector::new(vector.x * sx, vector.y * sy)
    }
    #[must_use]
    pub fn transform_rect(self, rect: Rect) -> Rect {
        Rect {
            left: self.transform_point(Point::new(rect.left, rect.top)).x,
            top: self.transform_point(Point::new(rect.left, rect.top)).y,
            right: self.transform_point(Point::new(rect.right, rect.bottom)).x,
            bottom: self.transform_point(Point::new(rect.right, rect.bottom)).y,
        }
    }
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.window_origin.x.is_finite()
            && self.window_origin.y.is_finite()
            && self.window_extent.x.is_finite()
            && self.window_extent.y.is_finite()
            && self.viewport_origin.x.is_finite()
            && self.viewport_origin.y.is_finite()
            && self.viewport_extent.x.is_finite()
            && self.viewport_extent.y.is_finite()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceContext {
    pub current_position: Point,
    pub pen: Pen,
    pub brush: Brush,
    pub font: Font,
    pub text_color: Color,
    pub background_color: Color,
    pub background_opaque: bool,
    pub text_align: u16,
    pub polygon_fill_mode: u16,
    pub map_mode: i16,
    pub raster_op: u16,
    pub stretch_mode: u16,
    pub mapper_flags: u32,
    pub mapping: Mapping,
    pub world_transform: Transform,
    pub clip: Option<ClipRegion>,
    pub selected_pen: Option<u16>,
    pub selected_brush: Option<u16>,
    pub selected_font: Option<u16>,
}
impl Default for DeviceContext {
    fn default() -> Self {
        Self {
            current_position: Point::default(),
            pen: Pen::default(),
            brush: Brush::default(),
            font: Font::default(),
            text_color: Color::BLACK,
            background_color: Color::WHITE,
            background_opaque: false,
            text_align: 0,
            polygon_fill_mode: 1,
            map_mode: 1,
            raster_op: 13,
            stretch_mode: 1,
            mapper_flags: 0,
            mapping: Mapping::default(),
            world_transform: Transform::default(),
            clip: None,
            selected_pen: None,
            selected_brush: None,
            selected_font: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub position: Point,
    pub text: String,
    pub font: Font,
    pub color: Color,
    pub background: Option<Color>,
    pub background_rect: Option<Rect>,
    pub background_path: Option<Path>,
    pub horizontal_align: HorizontalTextAlignment,
    pub vertical_align: VerticalTextAlignment,
    pub clip: Option<ClipRegion>,
    pub dx: Vec<f64>,
    /// Maps local text coordinates into output coordinates.
    pub transform: Transform,
    pub right_to_left: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HorizontalTextAlignment {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalTextAlignment {
    Top,
    Center,
    Bottom,
    Baseline,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// A renderer-neutral clipping region. `Polygon` vertices describe one closed,
/// convex region in output coordinates. An empty polygon clips all output.
#[derive(Debug, Clone, PartialEq)]
pub enum ClipRegion {
    Rect(Rect),
    Polygon(Vec<Point>),
    Path {
        path: Path,
        fill_mode: u16,
    },
    Intersection(Vec<ClipRegion>),
    Combine {
        operation: ClipOperation,
        left: Box<ClipRegion>,
        right: Box<ClipRegion>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipOperation {
    Union,
    Xor,
    Exclude,
    Complement,
}

impl ClipRegion {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Rect(rect) => rect.width() == 0.0 || rect.height() == 0.0,
            Self::Polygon(points) => points.len() < 3,
            Self::Path { path, .. } => path.figures.is_empty(),
            Self::Intersection(regions) => regions.is_empty() || regions.iter().any(Self::is_empty),
            Self::Combine {
                operation,
                left,
                right,
            } => match operation {
                ClipOperation::Union | ClipOperation::Xor => left.is_empty() && right.is_empty(),
                ClipOperation::Exclude => left.is_empty(),
                ClipOperation::Complement => right.is_empty(),
            },
        }
    }
}

/// Placement of a bitmap's normalized unit square in output coordinates.
/// This represents translation, scaling, mirroring, rotation, and shear
/// without coupling the core renderer contract to SVG matrices.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BitmapPlacement {
    pub origin: Point,
    pub x_axis: Vector,
    pub y_axis: Vector,
}

impl BitmapPlacement {
    #[must_use]
    pub fn from_rect(rect: Rect) -> Self {
        Self {
            origin: Point::new(rect.left, rect.top),
            x_axis: Vector::new(rect.right - rect.left, 0.0),
            y_axis: Vector::new(0.0, rect.bottom - rect.top),
        }
    }

    #[must_use]
    pub fn corners(self) -> [Point; 4] {
        let top_left = self.origin;
        let top_right = Point::new(self.origin.x + self.x_axis.x, self.origin.y + self.x_axis.y);
        let bottom_left = Point::new(self.origin.x + self.y_axis.x, self.origin.y + self.y_axis.y);
        let bottom_right = Point::new(top_right.x + self.y_axis.x, top_right.y + self.y_axis.y);
        [top_left, top_right, bottom_right, bottom_left]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitmapSampling {
    Auto,
    Pixelated,
    Smooth,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    pub figures: Vec<PathFigure>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PathFigure {
    pub start: Point,
    pub segments: Vec<PathSegment>,
    pub closed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathSegment {
    Line(Point),
    Cubic {
        control1: Point,
        control2: Point,
        to: Point,
    },
}

pub trait Renderer {
    fn drawing_bounds(&self) -> Option<Rect>;
    fn line(&mut self, from: Point, to: Point, pen: &Pen, clip: Option<&ClipRegion>) -> Result<()>;
    fn polyline(&mut self, points: &[Point], pen: &Pen, clip: Option<&ClipRegion>) -> Result<()>;
    fn polygon(
        &mut self,
        points: &[Point],
        pen: &Pen,
        brush: &Brush,
        fill_mode: u16,
        clip: Option<&ClipRegion>,
    ) -> Result<()>;
    fn poly_polygon(
        &mut self,
        polygons: &[Vec<Point>],
        pen: &Pen,
        brush: &Brush,
        fill_mode: u16,
        clip: Option<&ClipRegion>,
    ) -> Result<()>;
    fn path(
        &mut self,
        path: &Path,
        pen: &Pen,
        brush: &Brush,
        fill_mode: u16,
        stroke: bool,
        fill: bool,
        clip: Option<&ClipRegion>,
    ) -> Result<()>;
    fn styled_path(
        &mut self,
        path: &Path,
        stroke: Option<&Stroke>,
        fill: Option<&Paint>,
        fill_mode: u16,
        clip: Option<&ClipRegion>,
    ) -> Result<()>;
    fn rectangle(
        &mut self,
        rect: Rect,
        radius: Option<Point>,
        pen: &Pen,
        brush: &Brush,
        clip: Option<&ClipRegion>,
    ) -> Result<()>;
    fn ellipse(
        &mut self,
        rect: Rect,
        pen: &Pen,
        brush: &Brush,
        clip: Option<&ClipRegion>,
    ) -> Result<()>;
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
    ) -> Result<()>;
    fn pixel(&mut self, point: Point, color: Color, clip: Option<&ClipRegion>) -> Result<()>;
    fn text(&mut self, run: &TextRun) -> Result<()>;
    fn bitmap(
        &mut self,
        placement: BitmapPlacement,
        bitmap: &Bitmap,
        sampling: BitmapSampling,
        clip: Option<&ClipRegion>,
    ) -> Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArcKind {
    Arc,
    Pie,
    Chord,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_transform_ignores_translation() {
        let mapping = Mapping {
            window_origin: Point::new(100.0, 200.0),
            window_extent: Point::new(10.0, 20.0),
            viewport_origin: Point::new(1_000.0, 2_000.0),
            viewport_extent: Point::new(20.0, -40.0),
        };
        assert_eq!(
            mapping.transform_vector(Vector::new(3.0, 4.0)),
            Vector::new(6.0, -8.0)
        );
        assert_eq!(
            mapping.transform_point(Point::new(103.0, 204.0)),
            Point::new(1_006.0, 1_992.0)
        );
    }

    #[test]
    fn rectangle_intersection_normalizes_and_preserves_empty() {
        let a = Rect {
            left: 10.0,
            top: 10.0,
            right: 0.0,
            bottom: 0.0,
        };
        let b = Rect {
            left: 5.0,
            top: -5.0,
            right: 15.0,
            bottom: 5.0,
        };
        assert_eq!(
            a.intersection(b),
            Some(Rect {
                left: 5.0,
                top: 0.0,
                right: 10.0,
                bottom: 5.0
            })
        );
        assert!(a
            .intersection(Rect {
                left: 20.0,
                top: 20.0,
                right: 30.0,
                bottom: 30.0
            })
            .is_none());
    }

    #[test]
    fn affine_composition_and_vector_translation_are_correct() {
        let translate = Transform {
            dx: 5.0,
            dy: 7.0,
            ..Transform::IDENTITY
        };
        let scale = Transform {
            m11: 2.0,
            m22: 3.0,
            ..Transform::IDENTITY
        };
        assert_eq!(
            translate
                .compose(scale)
                .transform_point(Point::new(1.0, 1.0)),
            Point::new(7.0, 10.0)
        );
        assert_eq!(
            translate.transform_vector(Vector::new(2.0, 3.0)),
            Vector::new(2.0, 3.0)
        );
    }
}
