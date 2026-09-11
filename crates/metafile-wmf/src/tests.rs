use super::*;

fn rec(function: u16, params: &[u8]) -> Vec<u8> {
    assert_eq!(params.len() % 2, 0);
    let words = (params.len() / 2 + 3) as u32;
    let mut v = Vec::new();
    v.extend(words.to_le_bytes());
    v.extend(function.to_le_bytes());
    v.extend(params);
    v
}
fn i16s(values: &[i16]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
fn standard(records: &[Vec<u8>], objects: u16) -> Vec<u8> {
    let body: Vec<u8> = records.iter().flatten().copied().collect();
    let words = ((18 + body.len()) / 2) as u32;
    let max = records
        .iter()
        .map(|r| (r.len() / 2) as u32)
        .max()
        .unwrap_or(3);
    let mut v = Vec::new();
    v.extend(1u16.to_le_bytes());
    v.extend(9u16.to_le_bytes());
    v.extend(0x0300u16.to_le_bytes());
    v.extend(words.to_le_bytes());
    v.extend(objects.to_le_bytes());
    v.extend(max.to_le_bytes());
    v.extend(0u16.to_le_bytes());
    v.extend(body);
    v
}
fn placeable(records: &[Vec<u8>], objects: u16) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend(PLACEABLE_MAGIC.to_le_bytes());
    p.extend(0u16.to_le_bytes());
    p.extend(0i16.to_le_bytes());
    p.extend(0i16.to_le_bytes());
    p.extend(100i16.to_le_bytes());
    p.extend(100i16.to_le_bytes());
    p.extend(1440u16.to_le_bytes());
    p.extend(0u32.to_le_bytes());
    let checksum = p
        .chunks_exact(2)
        .fold(0u16, |a, b| a ^ u16::from_le_bytes([b[0], b[1]]));
    p.extend(checksum.to_le_bytes());
    p.extend(standard(records, objects));
    p
}
fn eof() -> Vec<u8> {
    rec(META_EOF, &[])
}

#[test]
fn inspects_standard_wmf() {
    let b = standard(&[eof()], 0);
    let i = inspect(&b).unwrap();
    assert!(!i.placeable);
    assert_eq!(i.version, 0x300);
    assert!(i.has_eof);
}
#[test]
fn inspects_placeable_wmf() {
    let b = placeable(&[eof()], 0);
    let i = inspect(&b).unwrap();
    assert!(i.placeable);
    assert_eq!(i.units_per_inch, Some(1440));
    assert_eq!(i.bounds.unwrap().width(), 100.0);
}
#[test]
fn rejects_bad_checksum() {
    let mut b = placeable(&[eof()], 0);
    b[20] ^= 1;
    assert!(matches!(
        inspect(&b),
        Err(MetafileError::InvalidChecksum { .. })
    ));
}
#[test]
fn rejects_invalid_type() {
    let mut b = standard(&[eof()], 0);
    b[0] = 9;
    assert!(matches!(inspect(&b), Err(MetafileError::InvalidHeader(_))));
}
#[test]
fn rejects_invalid_header_size() {
    let mut b = standard(&[eof()], 0);
    b[2] = 8;
    assert!(inspect(&b).is_err());
}
#[test]
fn rejects_truncated_header() {
    assert!(matches!(
        inspect(&[0; 10]),
        Err(MetafileError::TruncatedInput { .. })
    ));
}
#[test]
fn rejects_zero_record_size() {
    let mut b = standard(&[eof()], 0);
    b[18..22].copy_from_slice(&0u32.to_le_bytes());
    assert!(matches!(
        inspect(&b),
        Err(MetafileError::InvalidRecordSize { .. })
    ));
}
#[test]
fn rejects_record_past_eof() {
    let mut b = standard(&[eof()], 0);
    b[18..22].copy_from_slice(&100u32.to_le_bytes());
    assert!(matches!(
        inspect(&b),
        Err(MetafileError::RecordOutOfBounds { .. })
    ));
}
#[test]
fn warns_for_missing_eof() {
    let b = standard(&[rec(META_SAVEDC, &[])], 0);
    let i = inspect(&b).unwrap();
    assert!(!i.has_eof);
    assert_eq!(i.warnings[0].code, "missing_eof");
}
#[test]
fn unknown_record_is_diagnostic() {
    let b = standard(&[rec(0x7777, &[]), eof()], 0);
    let r = to_svg(&b, RenderOptions::default()).unwrap();
    assert_eq!(r.diagnostics[0].code, "unsupported_record");
}
#[test]
fn strict_mode_rejects_unknown() {
    let b = standard(&[rec(0x7777, &[]), eof()], 0);
    assert!(matches!(
        to_svg(
            &b,
            RenderOptions {
                strict: true,
                ..Default::default()
            }
        ),
        Err(MetafileError::UnsupportedCriticalFeature(_))
    ));
}
#[test]
fn renders_line_and_tracks_position() {
    let b = standard(
        &[
            rec(META_MOVETO, &i16s(&[10, 5])),
            rec(META_LINETO, &i16s(&[20, 15])),
            eof(),
        ],
        0,
    );
    let r = to_svg(&b, Default::default()).unwrap();
    assert!(r.svg.contains("M 5 10 L 15 20"));
    assert!(r.svg.contains("viewBox=\"5 10 10 10\""));
}
#[test]
fn renders_pen_brush_polygon() {
    let mut pen = i16s(&[0, 2, 0]);
    pen.extend(0x0000ffu32.to_le_bytes());
    let mut brush = Vec::new();
    brush.extend(0u16.to_le_bytes());
    brush.extend(0x00ff00u32.to_le_bytes());
    brush.extend(0u16.to_le_bytes());
    let poly = {
        let mut p = Vec::new();
        p.extend(3u16.to_le_bytes());
        p.extend(i16s(&[0, 0, 10, 0, 0, 10]));
        p
    };
    let b = standard(
        &[
            rec(META_CREATEPENINDIRECT, &pen),
            rec(META_CREATEBRUSHINDIRECT, &brush),
            rec(META_SELECTOBJECT, &0u16.to_le_bytes()),
            rec(META_SELECTOBJECT, &1u16.to_le_bytes()),
            rec(META_POLYGON, &poly),
            eof(),
        ],
        2,
    );
    let r = to_svg(&b, Default::default()).unwrap();
    assert!(r.svg.contains("stroke=\"#ff0000\""));
    assert!(r.svg.contains("fill=\"#00ff00\""));
    assert!(r.svg.contains("0,0 10,0 0,10"));
}
#[test]
fn mapping_handles_origins_and_negative_extents() {
    let b = standard(
        &[
            rec(META_SETWINDOWORG, &i16s(&[10, 10])),
            rec(META_SETWINDOWEXT, &i16s(&[100, 100])),
            rec(META_SETVIEWPORTORG, &i16s(&[0, 0])),
            rec(META_SETVIEWPORTEXT, &i16s(&[-200, 200])),
            rec(META_MOVETO, &i16s(&[10, 10])),
            rec(META_LINETO, &i16s(&[60, 60])),
            eof(),
        ],
        0,
    );
    let r = to_svg(&b, Default::default()).unwrap();
    assert!(r.svg.contains("M 0 0 L 100 -100"));
}
#[test]
fn save_restore_restores_mapping_and_position() {
    let b = standard(
        &[
            rec(META_MOVETO, &i16s(&[1, 1])),
            rec(META_SAVEDC, &[]),
            rec(META_MOVETO, &i16s(&[9, 9])),
            rec(META_RESTOREDC, &(-1i16).to_le_bytes()),
            rec(META_LINETO, &i16s(&[2, 2])),
            eof(),
        ],
        0,
    );
    let r = to_svg(&b, Default::default()).unwrap();
    assert!(r.svg.contains("M 1 1 L 2 2"));
}
#[test]
fn invalid_handle_is_error() {
    let b = standard(&[rec(META_SELECTOBJECT, &3u16.to_le_bytes()), eof()], 1);
    assert!(matches!(
        to_svg(&b, Default::default()),
        Err(MetafileError::InvalidObjectHandle { .. })
    ));
}
#[test]
fn save_depth_is_bounded() {
    let b = standard(&[rec(META_SAVEDC, &[]), rec(META_SAVEDC, &[]), eof()], 0);
    let mut o = RenderOptions::default();
    o.limits.max_dc_depth = 1;
    assert!(matches!(
        to_svg(&b, o),
        Err(MetafileError::ResourceLimitExceeded { .. })
    ));
}
#[test]
fn input_size_is_bounded() {
    let b = standard(&[eof()], 0);
    let mut o = RenderOptions::default();
    o.limits.max_input_bytes = 1;
    assert!(inspect_with_options(&b, &o).is_err());
}
#[test]
fn object_count_is_bounded() {
    let b = standard(&[eof()], 2);
    let mut o = RenderOptions::default();
    o.limits.max_objects = 1;
    assert!(inspect_with_options(&b, &o).is_err());
}
#[test]
fn renders_text_with_xml_escaping() {
    let mut p = Vec::new();
    p.extend(3u16.to_le_bytes());
    p.extend(b"<&x");
    p.push(0);
    p.extend(i16s(&[10, 5]));
    let b = standard(&[rec(META_TEXTOUT, &p), eof()], 0);
    let r = to_svg(&b, Default::default()).unwrap();
    assert!(r.svg.contains("&lt;&amp;x"));
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.code == "approximate_text_metrics"));
}
#[test]
fn renders_intersection_clip() {
    let b = standard(
        &[
            rec(META_INTERSECTCLIPRECT, &i16s(&[20, 20, 0, 0])),
            rec(META_RECTANGLE, &i16s(&[30, 30, 0, 0])),
            eof(),
        ],
        0,
    );
    let r = to_svg(&b, Default::default()).unwrap();
    assert!(r.svg.contains("<clipPath id=\"clip0\""));
    assert!(r.svg.contains("clip-path=\"url(#clip0)\""));
}
#[test]
fn renders_common_shapes() {
    let b = standard(
        &[
            rec(META_RECTANGLE, &i16s(&[20, 30, 1, 2])),
            rec(META_ELLIPSE, &i16s(&[20, 30, 1, 2])),
            rec(META_ROUNDRECT, &i16s(&[4, 6, 20, 30, 1, 2])),
            rec(META_ARC, &i16s(&[10, 30, 10, 2, 20, 30, 0, 0])),
            rec(META_PIE, &i16s(&[10, 30, 10, 2, 20, 30, 0, 0])),
            rec(META_CHORD, &i16s(&[10, 30, 10, 2, 20, 30, 0, 0])),
            eof(),
        ],
        0,
    );
    let s = to_svg(&b, Default::default()).unwrap().svg;
    assert!(s.contains("<rect"));
    assert!(s.contains("<ellipse"));
    assert!(s.matches(" A ").count() >= 3);
}
#[test]
fn placeable_sets_physical_size() {
    let r = to_svg(&placeable(&[eof()], 0), Default::default()).unwrap();
    assert!(r.svg.contains("width=\"6.666667\""));
    assert!(r.svg.contains("viewBox=\"0 0 100 100\""));
}

#[test]
fn selects_stock_black_pen() {
    let b = standard(
        &[
            rec(META_SELECTOBJECT, &0x8007u16.to_le_bytes()),
            rec(META_MOVETO, &i16s(&[0, 0])),
            rec(META_LINETO, &i16s(&[5, 5])),
            eof(),
        ],
        0,
    );
    let svg = to_svg(&b, RenderOptions::default()).unwrap().svg;
    assert!(svg.contains("stroke=\"#000000\""));
}

#[test]
fn renders_stretchdib_as_embedded_png() {
    let mut dib = vec![0u8; 48];
    dib[0..4].copy_from_slice(&40u32.to_le_bytes());
    dib[4..8].copy_from_slice(&2i32.to_le_bytes());
    dib[8..12].copy_from_slice(&1i32.to_le_bytes());
    dib[12..14].copy_from_slice(&1u16.to_le_bytes());
    dib[14..16].copy_from_slice(&24u16.to_le_bytes());
    dib[40..46].copy_from_slice(&[0, 0, 255, 0, 255, 0]);
    let mut params = Vec::new();
    params.extend(0x00cc_0020u32.to_le_bytes());
    params.extend(0u16.to_le_bytes());
    params.extend(i16s(&[1, 2, 0, 0, 10, 20, 5, 6]));
    params.extend(dib);
    let b = standard(&[rec(META_STRETCHDIB, &params), eof()], 0);
    let svg = to_svg(&b, RenderOptions::default()).unwrap().svg;
    assert!(svg.contains("href=\"data:image/png;base64,"));
    assert!(svg.contains("x=\"6\" y=\"5\" width=\"20\" height=\"10\""));
}

#[test]
fn bitfields_dib_is_diagnostic_not_guessed() {
    let mut dib = vec![0u8; 56];
    dib[0..4].copy_from_slice(&40u32.to_le_bytes());
    dib[4..8].copy_from_slice(&1i32.to_le_bytes());
    dib[8..12].copy_from_slice(&1i32.to_le_bytes());
    dib[12..14].copy_from_slice(&1u16.to_le_bytes());
    dib[14..16].copy_from_slice(&32u16.to_le_bytes());
    dib[16..20].copy_from_slice(&3u32.to_le_bytes());
    let mut params = Vec::new();
    params.extend(0x00cc_0020u32.to_le_bytes());
    params.extend(0u16.to_le_bytes());
    params.extend(i16s(&[1, 1, 0, 0, 1, 1, 0, 0]));
    params.extend(dib);
    let b = standard(&[rec(META_STRETCHDIB, &params), eof()], 0);
    let result = to_svg(&b, RenderOptions::default()).unwrap();
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.code == "unsupported_or_invalid_dib"));
    assert!(!result.svg.contains("data:image/png"));
}
