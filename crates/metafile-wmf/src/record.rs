use crate::reader::Reader;
use metafile_core::{MetafileError, Point, Rect, ResourceLimits, Result};

pub const META_EOF: u16 = 0x0000;
pub const META_SAVEDC: u16 = 0x001e;
pub const META_SETBKMODE: u16 = 0x0102;
pub const META_SETMAPMODE: u16 = 0x0103;
pub const META_SETROP2: u16 = 0x0104;
pub const META_SETPOLYFILLMODE: u16 = 0x0106;
pub const META_SETSTRETCHBLTMODE: u16 = 0x0107;
pub const META_RESTOREDC: u16 = 0x0127;
pub const META_SELECTOBJECT: u16 = 0x012d;
pub const META_SETTEXTALIGN: u16 = 0x012e;
pub const META_DELETEOBJECT: u16 = 0x01f0;
pub const META_SETBKCOLOR: u16 = 0x0201;
pub const META_SETTEXTCOLOR: u16 = 0x0209;
pub const META_SETWINDOWORG: u16 = 0x020b;
pub const META_SETWINDOWEXT: u16 = 0x020c;
pub const META_SETVIEWPORTORG: u16 = 0x020d;
pub const META_SETVIEWPORTEXT: u16 = 0x020e;
pub const META_OFFSETWINDOWORG: u16 = 0x020f;
pub const META_OFFSETVIEWPORTORG: u16 = 0x0211;
pub const META_LINETO: u16 = 0x0213;
pub const META_MOVETO: u16 = 0x0214;
pub const META_SETMAPPERFLAGS: u16 = 0x0231;
pub const META_CREATEPENINDIRECT: u16 = 0x02fa;
pub const META_CREATEFONTINDIRECT: u16 = 0x02fb;
pub const META_CREATEBRUSHINDIRECT: u16 = 0x02fc;
pub const META_POLYGON: u16 = 0x0324;
pub const META_POLYLINE: u16 = 0x0325;
pub const META_SCALEWINDOWEXT: u16 = 0x0410;
pub const META_SCALEVIEWPORTEXT: u16 = 0x0412;
pub const META_EXCLUDECLIPRECT: u16 = 0x0415;
pub const META_INTERSECTCLIPRECT: u16 = 0x0416;
pub const META_ELLIPSE: u16 = 0x0418;
pub const META_RECTANGLE: u16 = 0x041b;
pub const META_SETPIXEL: u16 = 0x041f;
pub const META_TEXTOUT: u16 = 0x0521;
pub const META_POLYPOLYGON: u16 = 0x0538;
pub const META_ESCAPE: u16 = 0x0626;
pub const META_ROUNDRECT: u16 = 0x061c;
pub const META_ARC: u16 = 0x0817;
pub const META_PIE: u16 = 0x081a;
pub const META_CHORD: u16 = 0x0830;
pub const META_EXTTEXTOUT: u16 = 0x0a32;
pub const META_DIBBITBLT: u16 = 0x0940;
pub const META_DIBSTRETCHBLT: u16 = 0x0b41;
pub const META_SETDIBTODEV: u16 = 0x0d33;
pub const META_STRETCHDIB: u16 = 0x0f43;

#[derive(Debug, Clone)]
pub enum Record<'a> {
    Eof,
    NoArgs(u16),
    OneI16(u16, i16),
    OneU16(u16, u16),
    OneU32(u16, u32),
    Point(u16, Point),
    Rect(u16, Rect),
    Scale(u16, [i16; 4]),
    Arc(u16, Rect, Point, Point),
    RoundRect(Rect, Point),
    SetPixel(Point, u32),
    Pen {
        style: u16,
        width: i16,
        color: u32,
    },
    Brush {
        style: u16,
        color: u32,
        hatch: u16,
    },
    Font(&'a [u8]),
    Points(u16, Vec<Point>),
    PolyPolygon {
        counts: Vec<u16>,
        points: Vec<Point>,
    },
    Text {
        extended: bool,
        position: Point,
        options: u16,
        clip: Option<Rect>,
        bytes: &'a [u8],
        dx: Vec<i16>,
    },
    Bitmap {
        function: u16,
        params: &'a [u8],
    },
    Escape {
        function: u16,
        data: &'a [u8],
    },
    Unknown(u16, &'a [u8]),
}

#[derive(Debug, Clone)]
pub struct RecordEntry<'a> {
    pub index: usize,
    pub offset: usize,
    pub function: u16,
    pub size_words: u32,
    pub record: Record<'a>,
}

pub fn parse_records<'a>(
    data: &'a [u8],
    start: usize,
    declared_end: usize,
    limits: &ResourceLimits,
) -> Result<(Vec<RecordEntry<'a>>, bool)> {
    let mut out = Vec::new();
    let mut off = start;
    let mut eof = false;
    while off < declared_end {
        if out.len() >= limits.max_records {
            return Err(MetafileError::ResourceLimitExceeded {
                resource: "record count",
                actual: (out.len() + 1) as u64,
                limit: limits.max_records as u64,
            });
        }
        if declared_end - off < 6 {
            return Err(MetafileError::TruncatedInput {
                offset: off,
                needed: 6,
                available: declared_end - off,
            });
        }
        let mut h = Reader::new(&data[off..declared_end]);
        let words = h.u32()?;
        let function = h.u16()?;
        if words < 3 {
            return Err(MetafileError::InvalidRecordSize { offset: off, words });
        }
        let bytes = usize::try_from(words)
            .ok()
            .and_then(|w| w.checked_mul(2))
            .ok_or(MetafileError::InvalidRecordSize { offset: off, words })?;
        let end = off
            .checked_add(bytes)
            .ok_or(MetafileError::RecordOutOfBounds {
                offset: off,
                end: usize::MAX,
                input_len: data.len(),
            })?;
        if end > declared_end {
            return Err(MetafileError::RecordOutOfBounds {
                offset: off,
                end,
                input_len: declared_end,
            });
        }
        let payload = &data[off + 6..end];
        let record = decode(function, payload, limits)?;
        let is_eof = matches!(record, Record::Eof);
        out.push(RecordEntry {
            index: out.len(),
            offset: off,
            function,
            size_words: words,
            record,
        });
        off = end;
        if is_eof {
            eof = true;
            break;
        }
    }
    Ok((out, eof))
}

fn decode<'a>(f: u16, p: &'a [u8], limits: &ResourceLimits) -> Result<Record<'a>> {
    let mut r = Reader::new(p);
    let rec = match f {
        META_EOF => Record::Eof,
        META_SAVEDC => Record::NoArgs(f),
        META_SETBKMODE
        | META_SETMAPMODE
        | META_SETROP2
        | META_SETPOLYFILLMODE
        | META_SETSTRETCHBLTMODE
        | META_RESTOREDC => Record::OneI16(f, r.i16()?),
        META_SELECTOBJECT | META_DELETEOBJECT => Record::OneU16(f, r.u16()?),
        META_SETMAPPERFLAGS => Record::OneU32(f, r.u32()?),
        META_SETBKCOLOR | META_SETTEXTCOLOR => Record::OneU32(f, r.u32()?),
        META_SETTEXTALIGN => Record::OneU16(f, r.u16()?),
        META_SETWINDOWORG
        | META_SETWINDOWEXT
        | META_SETVIEWPORTORG
        | META_SETVIEWPORTEXT
        | META_OFFSETWINDOWORG
        | META_OFFSETVIEWPORTORG
        | META_LINETO
        | META_MOVETO => Record::Point(f, rev_point(&mut r)?),
        META_SCALEWINDOWEXT | META_SCALEVIEWPORTEXT => {
            Record::Scale(f, [r.i16()?, r.i16()?, r.i16()?, r.i16()?])
        }
        META_EXCLUDECLIPRECT | META_INTERSECTCLIPRECT | META_ELLIPSE | META_RECTANGLE => {
            Record::Rect(f, rev_rect(&mut r)?)
        }
        META_ARC | META_PIE | META_CHORD => {
            let end = rev_point(&mut r)?;
            let start = rev_point(&mut r)?;
            let rect = rev_rect(&mut r)?;
            Record::Arc(f, rect, start, end)
        }
        META_ROUNDRECT => {
            let radius = rev_point(&mut r)?;
            let rect = rev_rect(&mut r)?;
            Record::RoundRect(rect, radius)
        }
        META_SETPIXEL => {
            let color = r.u32()?;
            let point = rev_point(&mut r)?;
            Record::SetPixel(point, color)
        }
        META_CREATEPENINDIRECT => Record::Pen {
            style: r.u16()?,
            width: r.i16()?,
            color: {
                r.i16()?;
                r.u32()?
            },
        },
        META_CREATEBRUSHINDIRECT => Record::Brush {
            style: r.u16()?,
            color: r.u32()?,
            hatch: r.u16()?,
        },
        META_CREATEFONTINDIRECT => {
            if p.len() < 18 {
                return Err(MetafileError::TruncatedInput {
                    offset: 0,
                    needed: 18,
                    available: p.len(),
                });
            }
            Record::Font(p)
        }
        META_POLYGON | META_POLYLINE => {
            let count = usize::from(r.u16()?);
            guard_points(count, limits)?;
            let mut pts = Vec::with_capacity(count);
            for _ in 0..count {
                pts.push(Point::new(f64::from(r.i16()?), f64::from(r.i16()?)));
            }
            Record::Points(f, pts)
        }
        META_POLYPOLYGON => {
            let polys = usize::from(r.u16()?);
            guard_points(polys, limits)?;
            let mut counts = Vec::with_capacity(polys);
            let mut total = 0usize;
            for _ in 0..polys {
                let c = r.u16()?;
                total = total.checked_add(usize::from(c)).ok_or(
                    MetafileError::ResourceLimitExceeded {
                        resource: "polygon points",
                        actual: u64::MAX,
                        limit: limits.max_points_per_record as u64,
                    },
                )?;
                counts.push(c);
            }
            guard_points(total, limits)?;
            let mut points = Vec::with_capacity(total);
            for _ in 0..total {
                points.push(Point::new(f64::from(r.i16()?), f64::from(r.i16()?)));
            }
            Record::PolyPolygon { counts, points }
        }
        META_TEXTOUT => {
            let count = usize::from(r.u16()?);
            let bytes = r.take(count)?;
            if count % 2 != 0 {
                r.skip(1)?;
            }
            let position = rev_point(&mut r)?;
            Record::Text {
                extended: false,
                position,
                options: 0,
                clip: None,
                bytes,
                dx: Vec::new(),
            }
        }
        META_EXTTEXTOUT => {
            let position = rev_point(&mut r)?;
            let count = usize::from(r.u16()?);
            let options = r.u16()?;
            let clip = if options & 0x0006 != 0 {
                Some(Rect {
                    left: f64::from(r.i16()?),
                    top: f64::from(r.i16()?),
                    right: f64::from(r.i16()?),
                    bottom: f64::from(r.i16()?),
                })
            } else {
                None
            };
            let bytes = r.take(count)?;
            if count % 2 != 0 && r.remaining() > 0 {
                r.skip(1)?;
            }
            let mut dx = Vec::new();
            while r.remaining() >= 2 && dx.len() < count {
                dx.push(r.i16()?);
            }
            Record::Text {
                extended: true,
                position,
                options,
                clip,
                bytes,
                dx,
            }
        }
        META_DIBBITBLT | META_DIBSTRETCHBLT | META_SETDIBTODEV | META_STRETCHDIB => {
            Record::Bitmap {
                function: f,
                params: p,
            }
        }
        META_ESCAPE => {
            let function = r.u16()?;
            let count = usize::from(r.u16()?);
            Record::Escape {
                function,
                data: r.take(count)?,
            }
        }
        _ => Record::Unknown(f, p),
    };
    Ok(rec)
}
fn rev_point(r: &mut Reader<'_>) -> Result<Point> {
    let y = r.i16()?;
    let x = r.i16()?;
    Ok(Point::new(f64::from(x), f64::from(y)))
}
fn rev_rect(r: &mut Reader<'_>) -> Result<Rect> {
    let bottom = r.i16()?;
    let right = r.i16()?;
    let top = r.i16()?;
    let left = r.i16()?;
    Ok(Rect {
        left: f64::from(left),
        top: f64::from(top),
        right: f64::from(right),
        bottom: f64::from(bottom),
    })
}
fn guard_points(n: usize, l: &ResourceLimits) -> Result<()> {
    if n > l.max_points_per_record {
        Err(MetafileError::ResourceLimitExceeded {
            resource: "points per record",
            actual: n as u64,
            limit: l.max_points_per_record as u64,
        })
    } else {
        Ok(())
    }
}

pub fn name(f: u16) -> &'static str {
    match f {
        META_EOF => "META_EOF",
        META_SAVEDC => "META_SAVEDC",
        META_SETBKMODE => "META_SETBKMODE",
        META_SETMAPMODE => "META_SETMAPMODE",
        META_SETROP2 => "META_SETROP2",
        META_SETPOLYFILLMODE => "META_SETPOLYFILLMODE",
        META_SETSTRETCHBLTMODE => "META_SETSTRETCHBLTMODE",
        META_RESTOREDC => "META_RESTOREDC",
        META_SELECTOBJECT => "META_SELECTOBJECT",
        META_SETTEXTALIGN => "META_SETTEXTALIGN",
        META_DELETEOBJECT => "META_DELETEOBJECT",
        META_SETBKCOLOR => "META_SETBKCOLOR",
        META_SETTEXTCOLOR => "META_SETTEXTCOLOR",
        META_SETWINDOWORG => "META_SETWINDOWORG",
        META_SETWINDOWEXT => "META_SETWINDOWEXT",
        META_SETVIEWPORTORG => "META_SETVIEWPORTORG",
        META_SETVIEWPORTEXT => "META_SETVIEWPORTEXT",
        META_OFFSETWINDOWORG => "META_OFFSETWINDOWORG",
        META_OFFSETVIEWPORTORG => "META_OFFSETVIEWPORTORG",
        META_LINETO => "META_LINETO",
        META_MOVETO => "META_MOVETO",
        META_SETMAPPERFLAGS => "META_SETMAPPERFLAGS",
        META_CREATEPENINDIRECT => "META_CREATEPENINDIRECT",
        META_CREATEFONTINDIRECT => "META_CREATEFONTINDIRECT",
        META_CREATEBRUSHINDIRECT => "META_CREATEBRUSHINDIRECT",
        META_POLYGON => "META_POLYGON",
        META_POLYLINE => "META_POLYLINE",
        META_POLYPOLYGON => "META_POLYPOLYGON",
        META_EXCLUDECLIPRECT => "META_EXCLUDECLIPRECT",
        META_INTERSECTCLIPRECT => "META_INTERSECTCLIPRECT",
        META_ELLIPSE => "META_ELLIPSE",
        META_RECTANGLE => "META_RECTANGLE",
        META_ROUNDRECT => "META_ROUNDRECT",
        META_ARC => "META_ARC",
        META_PIE => "META_PIE",
        META_CHORD => "META_CHORD",
        META_SETPIXEL => "META_SETPIXEL",
        META_TEXTOUT => "META_TEXTOUT",
        META_EXTTEXTOUT => "META_EXTTEXTOUT",
        META_ESCAPE => "META_ESCAPE",
        META_DIBBITBLT => "META_DIBBITBLT",
        META_DIBSTRETCHBLT => "META_DIBSTRETCHBLT",
        META_SETDIBTODEV => "META_SETDIBTODEV",
        META_STRETCHDIB => "META_STRETCHDIB",
        _ => "UNKNOWN",
    }
}
