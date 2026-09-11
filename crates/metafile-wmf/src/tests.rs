use super::*;
use metafile_svg::SvgRenderer;

#[derive(Debug)]
struct TestRenderResult {
    svg: String,
    diagnostics: Vec<Diagnostic>,
}

fn to_svg(bytes: &[u8], options: RenderOptions) -> Result<TestRenderResult> {
    let mut renderer = SvgRenderer::new(options.limits.clone());
    let result = playback(bytes, &options, &mut renderer)?;
    Ok(TestRenderResult {
        svg: renderer.finish(result.output_bounds, result.physical_size)?,
        diagnostics: result.diagnostics,
    })
}

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
        .chunks(2)
        .fold(0u16, |a, b| a ^ u16::from_le_bytes([b[0], b[1]]));
    p.extend(checksum.to_le_bytes());
    p.extend(standard(records, objects));
    p
}
fn eof() -> Vec<u8> {
    rec(META_EOF, &[])
}
fn ext_text(
    text: &[u8],
    options: u16,
    rect: Option<[i16; 4]>,
    position: [i16; 2],
    dx: &[i16],
) -> Vec<u8> {
    let mut params = Vec::new();
    params.extend(i16s(&position));
    params.extend((text.len() as u16).to_le_bytes());
    params.extend(options.to_le_bytes());
    if let Some(rect) = rect {
        params.extend(i16s(&rect));
    }
    params.extend(text);
    if text.len() & 1 == 1 {
        params.push(0);
    }
    params.extend(i16s(dx));
    rec(META_EXTTEXTOUT, &params)
}

fn create_pen(width: i16, color: u32) -> Vec<u8> {
    let mut params = i16s(&[0, width, 0]);
    params.extend(color.to_le_bytes());
    rec(META_CREATEPENINDIRECT, &params)
}

fn text_out(text: &[u8], position: [i16; 2]) -> Vec<u8> {
    let mut params = Vec::new();
    params.extend((text.len() as u16).to_le_bytes());
    params.extend(text);
    if text.len() % 2 != 0 {
        params.push(0);
    }
    params.extend(i16s(&position));
    rec(META_TEXTOUT, &params)
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
    assert!(r.svg.contains("viewBox=\"4.5 9.5 11 11\""));
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
            rec(META_SETMAPMODE, &8i16.to_le_bytes()),
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
fn cosmetic_and_geometric_pen_widths_map_differently() {
    let records = |width| {
        vec![
            create_pen(width, 0),
            rec(META_SELECTOBJECT, &0u16.to_le_bytes()),
            rec(META_SETMAPMODE, &8i16.to_le_bytes()),
            rec(META_SETWINDOWEXT, &i16s(&[10, -10])),
            rec(META_SETVIEWPORTEXT, &i16s(&[50, 50])),
            rec(META_MOVETO, &i16s(&[0, 0])),
            rec(META_LINETO, &i16s(&[1, 1])),
            eof(),
        ]
    };
    let cosmetic = to_svg(&standard(&records(0), 1), Default::default())
        .unwrap()
        .svg;
    let geometric = to_svg(&standard(&records(1), 1), Default::default())
        .unwrap()
        .svg;
    assert!(cosmetic.contains("stroke-width=\"1\""), "{cosmetic}");
    assert!(geometric.contains("stroke-width=\"5\""), "{geometric}");
}

#[test]
fn zero_width_pen_is_one_unit_in_mm_text() {
    let b = standard(
        &[
            create_pen(0, 0),
            rec(META_SELECTOBJECT, &0u16.to_le_bytes()),
            rec(META_MOVETO, &i16s(&[0, 0])),
            rec(META_LINETO, &i16s(&[5, 5])),
            eof(),
        ],
        1,
    );
    let svg = to_svg(&b, Default::default()).unwrap().svg;
    assert!(svg.contains("stroke-width=\"1\""));
}

#[test]
fn saved_dc_restores_cosmetic_pen_selection() {
    let b = standard(
        &[
            create_pen(0, 0),
            create_pen(2, 0xff),
            rec(META_SELECTOBJECT, &0u16.to_le_bytes()),
            rec(META_SAVEDC, &[]),
            rec(META_SELECTOBJECT, &1u16.to_le_bytes()),
            rec(META_RESTOREDC, &(-1i16).to_le_bytes()),
            rec(META_MOVETO, &i16s(&[0, 0])),
            rec(META_LINETO, &i16s(&[1, 1])),
            eof(),
        ],
        2,
    );
    let svg = to_svg(&b, Default::default()).unwrap().svg;
    assert!(svg.contains("stroke=\"#000000\" stroke-width=\"1\""));
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
fn text_alignment_modes_map_to_distinct_svg_semantics() {
    let horizontal = [(0x0000, "start"), (0x0006, "middle"), (0x0002, "end")];
    let vertical = [
        (0x0000, "text-before-edge"),
        (0x0018, "alphabetic"),
        (0x0008, "text-after-edge"),
    ];
    for (horizontal_bits, anchor) in horizontal {
        for (vertical_bits, baseline) in vertical {
            let align = horizontal_bits | vertical_bits;
            let b = standard(
                &[
                    rec(META_SETTEXTALIGN, &(align as u16).to_le_bytes()),
                    text_out(b"A", [20, 30]),
                    eof(),
                ],
                0,
            );
            let svg = to_svg(&b, Default::default()).unwrap().svg;
            assert!(svg.contains(&format!("text-anchor=\"{anchor}\"")), "{svg}");
            assert!(
                svg.contains(&format!("dominant-baseline=\"{baseline}\"")),
                "{svg}"
            );
        }
    }
}

#[test]
fn update_cp_uses_dx_and_text_escapement_in_logical_space() {
    let mut font = vec![0u8; 50];
    font[0..2].copy_from_slice(&12i16.to_le_bytes());
    font[4..6].copy_from_slice(&900i16.to_le_bytes());
    font[8..10].copy_from_slice(&400i16.to_le_bytes());
    font[18..24].copy_from_slice(b"Arial\0");
    let b = standard(
        &[
            rec(META_CREATEFONTINDIRECT, &font),
            rec(META_SELECTOBJECT, &0u16.to_le_bytes()),
            rec(META_SETTEXTALIGN, &1u16.to_le_bytes()),
            rec(META_MOVETO, &i16s(&[20, 10])),
            ext_text(b"AB", 0, None, [0, 0], &[3, 4]),
            rec(META_LINETO, &i16s(&[20, 20])),
            eof(),
        ],
        1,
    );
    let result = to_svg(&b, Default::default()).unwrap();
    assert!(result.svg.contains("rotate(-90 10 20)"), "{}", result.svg);
    assert!(result.svg.contains("M 10 13 L 20 20"), "{}", result.svg);
    assert!(!result
        .diagnostics
        .iter()
        .any(|d| d.code == "approximate_update_cp"));
}

#[test]
fn update_cp_without_dx_is_deterministic_and_diagnostic() {
    let b = standard(
        &[
            rec(META_SETTEXTALIGN, &1u16.to_le_bytes()),
            rec(META_MOVETO, &i16s(&[0, 0])),
            text_out(b"AB", [99, 99]),
            rec(META_LINETO, &i16s(&[0, 30])),
            eof(),
        ],
        0,
    );
    let result = to_svg(&b, Default::default()).unwrap();
    assert!(result.svg.contains("M 14.4 0 L 30 0"), "{}", result.svg);
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.code == "approximate_update_cp"));
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
    let result = to_svg(&b, RenderOptions::default()).unwrap();
    assert!(result.svg.contains("href=\"data:image/png;base64,"));
    assert!(result
        .svg
        .contains("x=\"6\" y=\"5\" width=\"20\" height=\"10\""));
    assert!(result
        .diagnostics
        .iter()
        .any(|d| d.code == "unsupported_stretch_mode"));
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
    let mut mapped_params = params;
    mapped_params[14..16].copy_from_slice(&1i16.to_le_bytes());
    mapped_params[16..18].copy_from_slice(&2i16.to_le_bytes());
    let mapped = standard(
        &[
            rec(META_SETMAPMODE, &8i16.to_le_bytes()),
            rec(META_SETWINDOWEXT, &i16s(&[2, 2])),
            rec(META_SETVIEWPORTEXT, &i16s(&[10, 10])),
            rec(META_STRETCHDIB, &mapped_params),
            eof(),
        ],
        0,
    );
    assert!(matches!(
        to_svg(
            &mapped,
            RenderOptions {
                strict: true,
                ..Default::default()
            }
        ),
        Err(MetafileError::UnsupportedCriticalFeature(_))
    ));
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
        .any(|d| d.code == "unsupported_dib"));
    assert!(!result.svg.contains("data:image/png"));
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
fn roundrect_radius_is_translation_independent() {
    let b = standard(
        &[
            rec(META_SETMAPMODE, &8i16.to_le_bytes()),
            rec(META_SETWINDOWORG, &i16s(&[20, 10])),
            rec(META_SETWINDOWEXT, &i16s(&[100, 100])),
            rec(META_SETVIEWPORTORG, &i16s(&[400, 300])),
            rec(META_SETVIEWPORTEXT, &i16s(&[200, 200])),
            rec(META_ROUNDRECT, &i16s(&[10, 20, 80, 80, 20, 20])),
            eof(),
        ],
        0,
    );
    let svg = to_svg(&b, RenderOptions::default()).unwrap().svg;
    assert!(svg.contains("rx=\"20\" ry=\"10\""));
}

#[test]
fn stretchdib_destination_addition_cannot_overflow_i16() {
    let mut dib = vec![0u8; 44];
    dib[0..4].copy_from_slice(&40u32.to_le_bytes());
    dib[4..8].copy_from_slice(&1i32.to_le_bytes());
    dib[8..12].copy_from_slice(&1i32.to_le_bytes());
    dib[12..14].copy_from_slice(&1u16.to_le_bytes());
    dib[14..16].copy_from_slice(&32u16.to_le_bytes());
    let mut params = Vec::new();
    params.extend(0x00cc_0020u32.to_le_bytes());
    params.extend(0u16.to_le_bytes());
    params.extend(i16s(&[1, 1, 0, 0, 32767, 32767, 32767, 32767]));
    params.extend(dib);
    let b = standard(&[rec(META_STRETCHDIB, &params), eof()], 0);
    let outcome = std::panic::catch_unwind(|| to_svg(&b, RenderOptions::default()));
    assert!(outcome.is_ok());
    let result = outcome.unwrap();
    assert!(
        matches!(result, Err(MetafileError::ResourceLimitExceeded { .. })),
        "unexpected result: {result:?}"
    );
}

#[test]
fn clipping_normalizes_inverted_axes_and_deduplicates_resources() {
    for viewport in [[100, -100], [-100, 100], [-100, -100]] {
        let b = standard(
            &[
                rec(META_SETMAPMODE, &8i16.to_le_bytes()),
                rec(META_SETWINDOWEXT, &i16s(&[100, 100])),
                rec(META_SETVIEWPORTEXT, &i16s(&viewport)),
                rec(META_INTERSECTCLIPRECT, &i16s(&[80, 80, 20, 20])),
                rec(META_RECTANGLE, &i16s(&[90, 90, 10, 10])),
                rec(META_RECTANGLE, &i16s(&[70, 70, 30, 30])),
                eof(),
            ],
            0,
        );
        let svg = to_svg(&b, RenderOptions::default()).unwrap().svg;
        assert_eq!(svg.matches("<clipPath").count(), 1);
        assert_eq!(svg.matches("clip-path=\"url(#clip0)\"").count(), 2);
    }
}

#[test]
fn non_overlapping_nested_clip_is_empty_and_restore_recovers() {
    let b = standard(
        &[
            rec(META_INTERSECTCLIPRECT, &i16s(&[10, 10, 0, 0])),
            rec(META_SAVEDC, &[]),
            rec(META_INTERSECTCLIPRECT, &i16s(&[30, 30, 20, 20])),
            rec(META_RECTANGLE, &i16s(&[5, 5, 0, 0])),
            rec(META_RESTOREDC, &(-1i16).to_le_bytes()),
            rec(META_RECTANGLE, &i16s(&[5, 5, 0, 0])),
            eof(),
        ],
        0,
    );
    let svg = to_svg(&b, RenderOptions::default()).unwrap().svg;
    assert!(svg.contains("width=\"0\" height=\"0\""));
    assert_eq!(svg.matches("<clipPath").count(), 2);
}

#[test]
fn exttextout_opaque_and_clipped_are_independent() {
    let cases = [
        (0x0002, true, false),
        (0x0004, false, true),
        (0x0006, true, true),
        (0, false, false),
    ];
    for (flags, opaque, clipped) in cases {
        let b = standard(
            &[
                rec(META_SETBKCOLOR, &0x0000_ff00u32.to_le_bytes()),
                ext_text(
                    b"A",
                    flags,
                    (flags != 0).then_some([1, 2, 11, 12]),
                    [5, 6],
                    &[],
                ),
                eof(),
            ],
            0,
        );
        let svg = to_svg(&b, RenderOptions::default()).unwrap().svg;
        assert_eq!(
            svg.contains("<rect x=\"1\" y=\"2\" width=\"10\" height=\"10\" fill=\"#00ff00\""),
            opaque
        );
        assert_eq!(svg.contains("clip-path=\"url(#clip0)\""), clipped);
    }
}

#[test]
fn exttextout_dx_and_transformed_position_are_used() {
    let b = standard(
        &[
            rec(META_SETMAPMODE, &8i16.to_le_bytes()),
            rec(META_SETWINDOWORG, &i16s(&[10, 10])),
            rec(META_SETWINDOWEXT, &i16s(&[10, 10])),
            rec(META_SETVIEWPORTORG, &i16s(&[100, 100])),
            rec(META_SETVIEWPORTEXT, &i16s(&[20, 20])),
            ext_text(b"AB", 0, None, [12, 13], &[3, 4]),
            eof(),
        ],
        0,
    );
    let svg = to_svg(&b, RenderOptions::default()).unwrap().svg;
    assert!(
        svg.contains("<tspan x=\"106\" y=\"104\">A</tspan><tspan x=\"112\" y=\"104\">B</tspan>")
    );
    assert!(svg.contains("font-size=\"24\""));
}

#[test]
fn strict_mode_rejects_known_lossy_operations() {
    let escape = {
        let mut p = Vec::new();
        p.extend(1u16.to_le_bytes());
        p.extend(0u16.to_le_bytes());
        rec(META_ESCAPE, &p)
    };
    let exclude = rec(META_EXCLUDECLIPRECT, &i16s(&[10, 10, 0, 0]));
    let dib_transfer = rec(META_DIBBITBLT, &[]);
    let mut brush = Vec::new();
    brush.extend(2u16.to_le_bytes());
    brush.extend(0u32.to_le_bytes());
    brush.extend(1u16.to_le_bytes());
    let hatch = rec(META_CREATEBRUSHINDIRECT, &brush);
    for unsupported in [escape, exclude, dib_transfer, hatch] {
        let b = standard(&[unsupported, eof()], 1);
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
        assert!(to_svg(&b, RenderOptions::default())
            .unwrap()
            .diagnostics
            .iter()
            .any(|d| d.severity == metafile_core::Severity::Warning));
    }
}

#[test]
fn object_slot_is_reused_after_valid_delete() {
    let pen = {
        let mut p = i16s(&[0, 1, 0]);
        p.extend(0u32.to_le_bytes());
        rec(META_CREATEPENINDIRECT, &p)
    };
    let brush = {
        let mut p = Vec::new();
        p.extend(0u16.to_le_bytes());
        p.extend(0xffu32.to_le_bytes());
        p.extend(0u16.to_le_bytes());
        rec(META_CREATEBRUSHINDIRECT, &p)
    };
    let b = standard(
        &[
            pen,
            rec(META_SELECTOBJECT, &0x8007u16.to_le_bytes()),
            rec(META_DELETEOBJECT, &0u16.to_le_bytes()),
            brush,
            rec(META_SELECTOBJECT, &0u16.to_le_bytes()),
            rec(META_RECTANGLE, &i16s(&[5, 5, 0, 0])),
            eof(),
        ],
        1,
    );
    assert!(to_svg(&b, RenderOptions::default())
        .unwrap()
        .svg
        .contains("fill=\"#ff0000\""));
}

#[test]
fn deleting_object_selected_in_saved_dc_is_diagnostic_or_strict_error() {
    let pen = {
        let mut p = i16s(&[0, 1, 0]);
        p.extend(0u32.to_le_bytes());
        rec(META_CREATEPENINDIRECT, &p)
    };
    let records = [
        pen,
        rec(META_SELECTOBJECT, &0u16.to_le_bytes()),
        rec(META_SAVEDC, &[]),
        rec(META_SELECTOBJECT, &0x8007u16.to_le_bytes()),
        rec(META_DELETEOBJECT, &0u16.to_le_bytes()),
        eof(),
    ];
    let permissive = to_svg(&standard(&records, 1), RenderOptions::default()).unwrap();
    assert!(permissive
        .diagnostics
        .iter()
        .any(|d| d.code == "delete_selected_object"));
    assert!(matches!(
        to_svg(
            &standard(&records, 1),
            RenderOptions {
                strict: true,
                ..Default::default()
            }
        ),
        Err(MetafileError::ObjectInUse { .. })
    ));
}

#[test]
fn restoredc_relative_absolute_and_invalid_values() {
    let valid = standard(
        &[
            rec(META_SAVEDC, &[]),
            rec(META_SAVEDC, &[]),
            rec(META_MOVETO, &i16s(&[9, 9])),
            rec(META_RESTOREDC, &(-1i16).to_le_bytes()),
            rec(META_RESTOREDC, &1i16.to_le_bytes()),
            eof(),
        ],
        0,
    );
    assert!(to_svg(&valid, RenderOptions::default()).is_ok());
    for value in [0i16, -2, 2] {
        let b = standard(
            &[
                rec(META_SAVEDC, &[]),
                rec(META_RESTOREDC, &value.to_le_bytes()),
                eof(),
            ],
            0,
        );
        assert!(matches!(
            to_svg(&b, RenderOptions::default()),
            Err(MetafileError::InvalidRestoreDc { .. })
        ));
    }
}

#[test]
fn fixed_map_modes_have_specification_units_and_y_orientation() {
    let cases = [
        (1, 1.0, 1.0),
        (2, 96.0 / 254.0, -96.0 / 254.0),
        (3, 96.0 / 2540.0, -96.0 / 2540.0),
        (4, 0.96, -0.96),
        (5, 0.096, -0.096),
        (6, 96.0 / 1440.0, -96.0 / 1440.0),
    ];
    for (mode, sx, sy) in cases {
        let mut player = Player::new(0, ResourceLimits::default());
        let e = RecordEntry {
            index: 0,
            offset: 0,
            function: META_SETMAPMODE,
            size_words: 4,
            record: Record::OneI16(META_SETMAPMODE, mode),
        };
        player.set_map_mode(mode, &e, &mut vec![], false).unwrap();
        let p = player.map(Point::new(10.0, 10.0));
        assert!((p.x - 10.0 * sx).abs() < 1e-9);
        assert!((p.y - 10.0 * sy).abs() < 1e-9);
    }
}

#[test]
fn polypolygon_is_one_combined_path_with_active_fill_rule() {
    let mut p = Vec::new();
    p.extend(2u16.to_le_bytes());
    p.extend(4u16.to_le_bytes());
    p.extend(4u16.to_le_bytes());
    p.extend(i16s(&[
        0, 0, 20, 0, 20, 20, 0, 20, 5, 5, 5, 15, 15, 15, 15, 5,
    ]));
    let b = standard(&[rec(META_POLYPOLYGON, &p), eof()], 0);
    let svg = to_svg(&b, RenderOptions::default()).unwrap().svg;
    assert_eq!(svg.matches("<path").count(), 1);
    assert!(svg.contains("fill-rule=\"evenodd\""));
    assert_eq!(svg.matches(" Z").count(), 2);

    let winding = standard(
        &[
            rec(META_SETPOLYFILLMODE, &2i16.to_le_bytes()),
            rec(META_POLYPOLYGON, &p),
            eof(),
        ],
        0,
    );
    let winding_svg = to_svg(&winding, RenderOptions::default()).unwrap().svg;
    assert_eq!(winding_svg.matches("<path").count(), 1);
    assert!(winding_svg.contains("fill-rule=\"nonzero\""));
}

#[test]
fn repeated_render_is_byte_deterministic() {
    let b = standard(
        &[
            rec(META_INTERSECTCLIPRECT, &i16s(&[10, 10, 0, 0])),
            rec(META_RECTANGLE, &i16s(&[5, 5, 0, 0])),
            eof(),
        ],
        0,
    );
    assert_eq!(
        to_svg(&b, RenderOptions::default()).unwrap().svg,
        to_svg(&b, RenderOptions::default()).unwrap().svg
    );
}

#[test]
fn randomized_malformed_input_never_panics() {
    let mut state = 0x1234_5678_9abc_def0u64;
    for len in 0..512usize {
        let mut bytes = vec![0u8; len];
        for byte in &mut bytes {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *byte = state as u8;
        }
        assert!(std::panic::catch_unwind(|| {
            let _ = inspect(&bytes);
            let _ = to_svg(&bytes, RenderOptions::default());
            let _ = to_svg(
                &bytes,
                RenderOptions {
                    strict: true,
                    ..Default::default()
                },
            );
        })
        .is_ok());
    }
}

#[test]
fn bitmap_crop_selects_only_requested_pixels() {
    let bitmap = metafile_core::Bitmap {
        width: 3,
        height: 2,
        rgba: (0u8..24).collect(),
    };
    let cropped = crop_bitmap(bitmap, true, 1, 0, 2, 2, 0).unwrap();
    assert_eq!((cropped.width, cropped.height), (2, 2));
    assert_eq!(
        cropped.rgba,
        vec![4, 5, 6, 7, 8, 9, 10, 11, 16, 17, 18, 19, 20, 21, 22, 23]
    );
}

#[test]
fn bitmap_crop_honors_bottom_up_origin_and_negative_extents() {
    let bitmap = metafile_core::Bitmap {
        width: 2,
        height: 2,
        rgba: vec![1, 0, 0, 255, 2, 0, 0, 255, 3, 0, 0, 255, 4, 0, 0, 255],
    };
    let bottom = crop_bitmap(bitmap.clone(), false, 0, 0, 2, 1, 0).unwrap();
    assert_eq!(bottom.rgba, vec![3, 0, 0, 255, 4, 0, 0, 255]);
    let mirrored = crop_bitmap(bitmap, true, 2, 2, -2, -2, 0).unwrap();
    assert_eq!(
        mirrored.rgba,
        vec![4, 0, 0, 255, 3, 0, 0, 255, 2, 0, 0, 255, 1, 0, 0, 255]
    );
}

#[test]
fn stretch_and_mapper_state_only_warn_when_consumed() {
    let state_only = standard(
        &[
            rec(META_SETSTRETCHBLTMODE, &4i16.to_le_bytes()),
            rec(META_SETMAPPERFLAGS, &1u32.to_le_bytes()),
            eof(),
        ],
        0,
    );
    let state_diagnostics = to_svg(&state_only, Default::default()).unwrap().diagnostics;
    assert!(!state_diagnostics.iter().any(|d| {
        matches!(
            d.code.as_str(),
            "unsupported_stretch_mode" | "approximate_stretch_mode" | "unsupported_mapper_flags"
        )
    }));

    let mapped_text = standard(
        &[
            rec(META_SETMAPPERFLAGS, &1u32.to_le_bytes()),
            text_out(b"A", [0, 0]),
            eof(),
        ],
        0,
    );
    let permissive = to_svg(&mapped_text, Default::default()).unwrap();
    assert!(permissive
        .diagnostics
        .iter()
        .any(|d| d.code == "unsupported_mapper_flags"));
    assert!(matches!(
        to_svg(
            &mapped_text,
            RenderOptions {
                strict: true,
                ..Default::default()
            }
        ),
        Err(MetafileError::UnsupportedCriticalFeature(_))
    ));
}

#[test]
fn resource_limit_boundaries_are_enforced() {
    let b = standard(&[eof()], 0);
    for limit in [b.len() - 1, b.len()] {
        let mut options = RenderOptions::default();
        options.limits.max_input_bytes = limit;
        assert_eq!(inspect_with_options(&b, &options).is_ok(), limit == b.len());
    }
    let two = standard(&[rec(META_SAVEDC, &[]), eof()], 0);
    let mut options = RenderOptions::default();
    options.limits.max_records = 1;
    assert!(matches!(
        inspect_with_options(&two, &options),
        Err(MetafileError::ResourceLimitExceeded { .. })
    ));
}

#[test]
fn maximum_record_metadata_is_validated() {
    let mut too_small = standard(&[rec(META_SAVEDC, &[]), eof()], 0);
    too_small[12..16].copy_from_slice(&2u32.to_le_bytes());
    assert!(matches!(
        inspect(&too_small),
        Err(MetafileError::InvalidHeader(_))
    ));
    let mut suspicious = standard(&[eof()], 0);
    suspicious[12..16].copy_from_slice(&99u32.to_le_bytes());
    assert!(inspect(&suspicious)
        .unwrap()
        .warnings
        .iter()
        .any(|d| d.code == "maximum_record_size_mismatch"));
}

#[test]
fn arc_sweep_and_fill_follow_kind_and_axis_orientation() {
    let normal = standard(
        &[
            rec(META_ARC, &i16s(&[50, 50, 50, 100, 100, 100, 0, 0])),
            rec(META_PIE, &i16s(&[50, 50, 50, 100, 100, 100, 0, 0])),
            rec(META_CHORD, &i16s(&[50, 50, 50, 100, 100, 100, 0, 0])),
            eof(),
        ],
        0,
    );
    let svg = to_svg(&normal, RenderOptions::default()).unwrap().svg;
    assert!(svg.contains(" A 50 50 0 0 0"));
    assert_eq!(svg.matches(" Z").count(), 2);
    let inverted = standard(
        &[
            rec(META_SETMAPMODE, &8i16.to_le_bytes()),
            rec(META_SETWINDOWEXT, &i16s(&[100, 100])),
            rec(META_SETVIEWPORTEXT, &i16s(&[-100, 100])),
            rec(META_ARC, &i16s(&[50, 50, 50, 100, 100, 100, 0, 0])),
            eof(),
        ],
        0,
    );
    let inverted_svg = to_svg(&inverted, RenderOptions::default()).unwrap().svg;
    assert!(inverted_svg.contains(" A 50 50 0 0 0"), "{inverted_svg}");
}
