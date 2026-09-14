use super::*;
use metafile_svg::SvgRenderer;

fn record(kind: u32, payload: &[u8]) -> Vec<u8> {
    let size = 8 + payload.len();
    assert_eq!(size % 4, 0);
    let mut output = Vec::with_capacity(size);
    output.extend_from_slice(&kind.to_le_bytes());
    output.extend_from_slice(&(size as u32).to_le_bytes());
    output.extend_from_slice(payload);
    output
}

fn i32s(values: &[i32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

fn transform(values: [f32; 6]) -> Vec<u8> {
    values.into_iter().flat_map(f32::to_le_bytes).collect()
}

fn modify_transform(values: [f32; 6], mode: u32) -> Vec<u8> {
    let mut payload = transform(values);
    payload.extend_from_slice(&mode.to_le_bytes());
    payload
}

fn emf(records: &[Vec<u8>]) -> Vec<u8> {
    let mut output = vec![0; 88];
    output[0..4].copy_from_slice(&1u32.to_le_bytes());
    output[4..8].copy_from_slice(&88u32.to_le_bytes());
    output[8..24].copy_from_slice(&i32s(&[0, 0, 200, 100]));
    output[24..40].copy_from_slice(&i32s(&[0, 0, 5292, 2646]));
    output[40..44].copy_from_slice(&EMF_SIGNATURE.to_le_bytes());
    output[44..48].copy_from_slice(&0x0001_0000u32.to_le_bytes());
    output[56..58].copy_from_slice(&32u16.to_le_bytes());
    output[72..80].copy_from_slice(&i32s(&[1920, 1080]));
    output[80..88].copy_from_slice(&i32s(&[508, 286]));
    for item in records {
        output.extend_from_slice(item);
    }
    output.extend_from_slice(&record(EMR_EOF, &[0; 12]));
    let length = output.len() as u32;
    output[48..52].copy_from_slice(&length.to_le_bytes());
    output[52..56].copy_from_slice(&((records.len() + 2) as u32).to_le_bytes());
    output
}

fn render(bytes: &[u8], strict: bool) -> Result<(String, Vec<Diagnostic>)> {
    let options = RenderOptions {
        strict,
        ..RenderOptions::default()
    };
    let mut renderer = SvgRenderer::new(options.limits.clone());
    let result = playback(bytes, &options, &mut renderer)?;
    Ok((
        renderer.finish(result.output_bounds, result.physical_size)?,
        result.diagnostics,
    ))
}

#[test]
fn inspects_valid_header() {
    let info = inspect(&emf(&[])).unwrap();
    assert_eq!(info.format, MetafileFormat::Emf);
    assert_eq!(info.bounds.width(), 200.0);
    assert_eq!(info.parsed_record_count, 2);
    assert!(info.has_eof);
}

#[test]
fn physical_frame_preserves_canvas_beyond_ink_bounds() {
    let mut input = emf(&[record(27, &i32s(&[60, 40])), record(54, &i32s(&[80, 60]))]);
    input[8..24].copy_from_slice(&i32s(&[50, 30, 90, 70]));
    let (svg, _) = render(&input, false).unwrap();
    assert!(svg.contains("viewBox=\"0 0 200."), "{svg}");
    assert!(!svg.contains("viewBox=\"50 30 40 40\""), "{svg}");
}

#[test]
fn rejects_truncated_and_invalid_headers() {
    assert!(matches!(
        inspect(&[0; 20]),
        Err(MetafileError::TruncatedInput { .. })
    ));
    let mut input = emf(&[]);
    input[40] = 0;
    assert!(matches!(
        inspect(&input),
        Err(MetafileError::InvalidHeader(_))
    ));
}

#[test]
fn rejects_zero_and_out_of_bounds_records() {
    let mut zero = emf(&[]);
    zero[88..92].copy_from_slice(&54u32.to_le_bytes());
    zero[92..96].copy_from_slice(&0u32.to_le_bytes());
    assert!(matches!(
        inspect(&zero),
        Err(MetafileError::InvalidRecordSize { .. })
    ));
    let mut huge = emf(&[]);
    huge[92..96].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        inspect(&huge),
        Err(MetafileError::InvalidRecordSize { .. } | MetafileError::RecordOutOfBounds { .. })
    ));
}

#[test]
fn renders_line_and_mapping() {
    let input = emf(&[
        record(10, &i32s(&[10, 20])),
        record(9, &i32s(&[100, 50])),
        record(12, &i32s(&[0, 0])),
        record(11, &i32s(&[200, -100])),
        record(27, &i32s(&[10, 20])),
        record(54, &i32s(&[60, 45])),
    ]);
    let (svg, _) = render(&input, false).unwrap();
    assert!(svg.contains("M 0 0 L 100 -50"), "{svg}");
}

#[test]
fn world_transform_is_applied() {
    let mut transform = Vec::new();
    for value in [2.0f32, 0.0, 0.0, 3.0, 5.0, 7.0] {
        transform.extend_from_slice(&value.to_le_bytes());
    }
    let input = emf(&[
        record(35, &transform),
        record(27, &i32s(&[0, 0])),
        record(54, &i32s(&[10, 10])),
    ]);
    let (svg, _) = render(&input, false).unwrap();
    assert!(svg.contains("M 5 7 L 25 37"), "{svg}");
}

#[test]
fn creates_selects_and_deletes_objects() {
    let mut pen = Vec::new();
    pen.extend_from_slice(&1u32.to_le_bytes());
    pen.extend_from_slice(&0u32.to_le_bytes());
    pen.extend_from_slice(&i32s(&[3, 0]));
    pen.extend_from_slice(&0x0000_00ffu32.to_le_bytes());
    let input = emf(&[
        record(38, &pen),
        record(37, &1u32.to_le_bytes()),
        record(27, &i32s(&[0, 0])),
        record(54, &i32s(&[20, 10])),
    ]);
    let (svg, _) = render(&input, false).unwrap();
    assert!(svg.contains("stroke=\"#ff0000\""), "{svg}");
}

#[test]
fn extended_pen_preserves_derived_cap_and_join() {
    let mut pen = vec![0; 44];
    pen[0..4].copy_from_slice(&1u32.to_le_bytes());
    pen[20..24].copy_from_slice(&0x0001_1100u32.to_le_bytes());
    pen[24..28].copy_from_slice(&5u32.to_le_bytes());
    pen[28..32].copy_from_slice(&0u32.to_le_bytes());
    pen[32..36].copy_from_slice(&0x0000_00ffu32.to_le_bytes());
    let input = emf(&[
        record(95, &pen),
        record(37, &1u32.to_le_bytes()),
        record(27, &i32s(&[0, 0])),
        record(54, &i32s(&[10, 0])),
    ]);
    let (svg, diagnostics) = render(&input, false).unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(svg.contains("stroke-linecap=\"square\""), "{svg}");
    assert!(svg.contains("stroke-linejoin=\"bevel\""), "{svg}");
}

#[test]
fn save_restore_restores_transform() {
    let translate = |x: f32| {
        let mut payload = Vec::new();
        for value in [1.0f32, 0.0, 0.0, 1.0, x, 0.0] {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        payload
    };
    let input = emf(&[
        record(35, &translate(5.0)),
        record(33, &[]),
        record(35, &translate(50.0)),
        record(34, &(-1i32).to_le_bytes()),
        record(27, &i32s(&[0, 0])),
        record(54, &i32s(&[10, 0])),
    ]);
    let (svg, _) = render(&input, false).unwrap();
    assert!(svg.contains("M 5 0 L 15 0"), "{svg}");
}

#[test]
fn invalid_restore_dc_preserves_full_i32_value() {
    let input = emf(&[record(34, &70_000i32.to_le_bytes())]);
    assert!(matches!(
        render(&input, false),
        Err(MetafileError::InvalidRestoreDc {
            value: 70_000,
            record_index: 1,
            stack_depth: 0
        })
    ));
}

#[test]
fn renders_polygon_and_polypolygon() {
    let mut polygon = i32s(&[0, 0, 20, 20]);
    polygon.extend_from_slice(&3u32.to_le_bytes());
    polygon.extend_from_slice(&i32s(&[0, 0, 20, 0, 10, 20]));
    let (svg, _) = render(&emf(&[record(3, &polygon)]), false).unwrap();
    assert!(svg.contains("<polygon points=\"0,0 20,0 10,20\""), "{svg}");
}

#[test]
fn renders_bezier_and_completed_path() {
    let mut bezier = i32s(&[0, 0, 30, 20]);
    bezier.extend_from_slice(&4u32.to_le_bytes());
    bezier.extend_from_slice(&i32s(&[0, 0, 10, 20, 20, 20, 30, 0]));
    let (svg, _) = render(&emf(&[record(2, &bezier)]), false).unwrap();
    assert!(svg.contains("C 10 20 20 20 30 0"), "{svg}");

    let path_input = emf(&[
        record(59, &[]),
        record(27, &i32s(&[1, 2])),
        record(54, &i32s(&[10, 20])),
        record(61, &[]),
        record(60, &[]),
        record(64, &i32s(&[0, 0, 20, 30])),
    ]);
    let (svg, _) = render(&path_input, false).unwrap();
    assert!(svg.contains("M 1 2 L 10 20 Z"), "{svg}");
}

#[test]
fn rectangular_clip_is_emitted() {
    let input = emf(&[
        record(30, &i32s(&[0, 0, 20, 20])),
        record(27, &i32s(&[0, 0])),
        record(54, &i32s(&[30, 30])),
    ]);
    let (svg, _) = render(&input, false).unwrap();
    assert!(svg.contains("<clipPath id=\"clip0\""), "{svg}");
}

#[test]
fn renders_unicode_ext_text_out_with_dx() {
    let text: Vec<u8> = "AΩ".encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut payload = vec![0; 68];
    payload[28..36].copy_from_slice(&i32s(&[10, 20]));
    payload[36..40].copy_from_slice(&2u32.to_le_bytes());
    payload[40..44].copy_from_slice(&76u32.to_le_bytes());
    payload[64..68].copy_from_slice(&80u32.to_le_bytes());
    payload.extend_from_slice(&text);
    payload.extend_from_slice(&i32s(&[8, 9]));
    let (svg, _) = render(&emf(&[record(84, &payload)]), false).unwrap();
    assert!(
        svg.contains("A</tspan><tspan") && svg.contains("Ω"),
        "{svg}"
    );
}

#[test]
fn eto_pdy_preserves_horizontal_pair_advances_and_reports_vertical_limitation() {
    let text: Vec<u8> = "ABC".encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut payload = vec![0; 68];
    payload[28..36].copy_from_slice(&i32s(&[10, 20]));
    payload[36..40].copy_from_slice(&3u32.to_le_bytes());
    payload[40..44].copy_from_slice(&76u32.to_le_bytes());
    payload[44..48].copy_from_slice(&0x2000u32.to_le_bytes());
    payload[64..68].copy_from_slice(&84u32.to_le_bytes());
    payload.extend_from_slice(&text);
    payload.extend_from_slice(&[0, 0]);
    payload.extend_from_slice(&i32s(&[8, 100, 9, 200, 10, 300]));
    let (svg, diagnostics) = render(&emf(&[record(84, &payload)]), false).unwrap();
    assert!(svg.contains("<tspan x=\"27\" y=\"20\">C</tspan>"), "{svg}");
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "pdy_approximate"));
}

#[test]
fn renders_ansi_text_and_independent_opaque_clipped_options() {
    let mut payload = vec![0; 68];
    payload[28..36].copy_from_slice(&i32s(&[10, 20]));
    payload[36..40].copy_from_slice(&1u32.to_le_bytes());
    payload[40..44].copy_from_slice(&76u32.to_le_bytes());
    payload[44..48].copy_from_slice(&6u32.to_le_bytes());
    payload[48..64].copy_from_slice(&i32s(&[2, 3, 30, 40]));
    payload[64..68].copy_from_slice(&80u32.to_le_bytes());
    payload.extend_from_slice(&[0xe9, 0, 0, 0]);
    payload.extend_from_slice(&8i32.to_le_bytes());
    let (svg, diagnostics) = render(&emf(&[record(83, &payload)]), false).unwrap();
    assert!(svg.contains('\u{e9}'), "{svg}");
    assert!(svg.contains("<clipPath"), "{svg}");
    assert!(svg.contains("fill=\"#ffffff\" stroke=\"none\""), "{svg}");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn arbitrary_affine_text_is_explicitly_approximate_or_strictly_rejected() {
    let text: Vec<u8> = "A".encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut payload = vec![0; 68];
    payload[28..36].copy_from_slice(&i32s(&[10, 20]));
    payload[36..40].copy_from_slice(&1u32.to_le_bytes());
    payload[40..44].copy_from_slice(&76u32.to_le_bytes());
    payload.extend_from_slice(&text);
    payload.extend_from_slice(&[0, 0]);
    let input = emf(&[
        record(35, &transform([0.0, 1.0, -1.0, 0.0, 0.0, 0.0])),
        record(84, &payload),
    ]);
    let (_, diagnostics) = render(&input, false).unwrap();
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "affine_text_approximate"));
    assert!(matches!(
        render(&input, true),
        Err(MetafileError::UnsupportedCriticalFeature(_))
    ));
}

#[test]
fn emf_plus_is_classified_and_not_rendered() {
    let mut comment = 4u32.to_le_bytes().to_vec();
    comment.extend_from_slice(b"EMF+");
    let input = emf(&[record(70, &comment)]);
    let info = inspect(&input).unwrap();
    assert_eq!(info.format, MetafileFormat::EmfPlus);
    assert!(matches!(
        render(&input, false),
        Err(MetafileError::UnsupportedCriticalFeature(_))
    ));
}

#[test]
fn emf_plus_comment_validation_is_bounded_and_deterministic() {
    let mut truncated = 16u32.to_le_bytes().to_vec();
    truncated.extend_from_slice(b"EMF+");
    let input = emf(&[record(70, &truncated)]);
    assert!(matches!(
        inspect(&input),
        Err(MetafileError::TruncatedInput { .. })
    ));

    let short = emf(&[record(70, &0u32.to_le_bytes())]);
    assert_eq!(inspect(&short).unwrap().format, MetafileFormat::Emf);

    let mut comment = 4u32.to_le_bytes().to_vec();
    comment.extend_from_slice(b"EMF+");
    let valid = emf(&[record(70, &comment)]);
    assert_eq!(inspect(&valid).unwrap().format, MetafileFormat::EmfPlus);
    assert_eq!(inspect(&valid).unwrap().format, MetafileFormat::EmfPlus);

    let mut unknown = 16u32.to_le_bytes().to_vec();
    unknown.extend_from_slice(b"EMF+");
    unknown.extend_from_slice(&0x4fffu16.to_le_bytes());
    unknown.extend_from_slice(&0u16.to_le_bytes());
    unknown.extend_from_slice(&12u32.to_le_bytes());
    unknown.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(
        inspect(&emf(&[record(70, &unknown)])).unwrap().format,
        MetafileFormat::EmfPlus
    );

    let mut bad_record = 16u32.to_le_bytes().to_vec();
    bad_record.extend_from_slice(b"EMF+");
    bad_record.extend_from_slice(&0x4001u16.to_le_bytes());
    bad_record.extend_from_slice(&0u16.to_le_bytes());
    bad_record.extend_from_slice(&20u32.to_le_bytes());
    bad_record.extend_from_slice(&8u32.to_le_bytes());
    assert!(matches!(
        inspect(&emf(&[record(70, &bad_record)])),
        Err(MetafileError::RecordOutOfBounds { .. })
    ));

    let mut options = RenderOptions::default();
    options.limits.max_input_bytes = valid.len() - 1;
    assert!(matches!(
        inspect_with_options(&valid, &options),
        Err(MetafileError::ResourceLimitExceeded {
            resource: "input bytes",
            ..
        })
    ));
}

#[test]
fn non_finite_world_transforms_are_rejected_but_identity_ignores_xform() {
    let nan = f32::NAN;
    let input = emf(&[record(35, &transform([nan, 0.0, 0.0, 1.0, 0.0, 0.0]))]);
    assert!(matches!(
        render(&input, false),
        Err(MetafileError::InvalidHeader(_))
    ));

    let identity = emf(&[record(
        36,
        &modify_transform([nan, nan, nan, nan, nan, nan], 1),
    )]);
    assert!(render(&identity, false).is_ok());
}

#[test]
fn unknown_is_diagnostic_or_strict_error() {
    let input = emf(&[record(122, &[])]);
    let (_, diagnostics) = render(&input, false).unwrap();
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "unsupported_record"));
    assert!(matches!(
        render(&input, true),
        Err(MetafileError::UnsupportedCriticalFeature(_))
    ));
}

#[test]
fn repeated_render_is_deterministic() {
    let input = emf(&[record(43, &i32s(&[1, 2, 30, 40]))]);
    assert_eq!(
        render(&input, false).unwrap().0,
        render(&input, false).unwrap().0
    );
}

#[test]
fn resource_limits_bound_record_and_object_counts() {
    let input = emf(&[]);
    let mut options = RenderOptions::default();
    options.limits.max_records = 1;
    assert!(matches!(
        inspect_with_options(&input, &options),
        Err(MetafileError::ResourceLimitExceeded { .. })
    ));
    options.limits.max_records = 10;
    options.limits.max_objects = 1;
    assert!(matches!(
        inspect_with_options(&input, &options),
        Err(MetafileError::ResourceLimitExceeded { .. })
    ));
}

#[test]
fn renders_set_pixel_and_negative_viewport_extent() {
    let input = emf(&[
        record(11, &i32s(&[-2, 3])),
        record(
            15,
            &[i32s(&[4, 5]), 0x0000_00ffu32.to_le_bytes().to_vec()].concat(),
        ),
    ]);
    let (svg, _) = render(&input, false).unwrap();
    assert!(
        svg.contains("x=\"-8\" y=\"15\"") && svg.contains("fill=\"#ff0000\""),
        "{svg}"
    );
}

#[test]
fn modify_world_transform_composes_and_can_reset() {
    let input = emf(&[
        record(36, &modify_transform([1.0, 0.0, 0.0, 1.0, 5.0, 0.0], 2)),
        record(36, &modify_transform([2.0, 0.0, 0.0, 2.0, 0.0, 0.0], 2)),
        record(27, &i32s(&[0, 0])),
        record(54, &i32s(&[10, 0])),
    ]);
    let (svg, _) = render(&input, false).unwrap();
    assert!(svg.contains("M 5 0 L 25 0"), "{svg}");
}

fn object_player(handle: u32, object: GdiObject) -> Player {
    let mut player = Player::new(ResourceLimits::default());
    player.add_object(handle, object, 1).unwrap();
    player
}

#[test]
fn deleting_selected_pen_restores_default_pen() {
    let custom = Pen {
        color: Color::rgb(10, 20, 30),
        width: 7.0,
        ..Pen::default()
    };
    let mut player = object_player(1, GdiObject::Pen(custom));
    player.select_object(&1u32.to_le_bytes(), 2, 0).unwrap();
    player.delete_object(&1u32.to_le_bytes(), 3, 0).unwrap();
    assert_eq!(player.dc.core.pen, Pen::default());
    assert_eq!(player.dc.selected_pen, None);
}

#[test]
fn deleting_selected_brush_restores_default_brush() {
    let custom = Brush {
        style: BrushStyle::Solid,
        color: Color::rgb(10, 20, 30),
    };
    let mut player = object_player(1, GdiObject::Brush(custom));
    player.select_object(&1u32.to_le_bytes(), 2, 0).unwrap();
    player.delete_object(&1u32.to_le_bytes(), 3, 0).unwrap();
    assert_eq!(player.dc.core.brush, Brush::default());
    assert_eq!(player.dc.selected_brush, None);
}

#[test]
fn deleting_selected_font_restores_default_font() {
    let custom = Font {
        family: "Deleted Font".into(),
        height: 42.0,
        ..Font::default()
    };
    let mut player = object_player(1, GdiObject::Font(custom));
    player.select_object(&1u32.to_le_bytes(), 2, 0).unwrap();
    player.delete_object(&1u32.to_le_bytes(), 3, 0).unwrap();
    assert_eq!(player.dc.core.font, Font::default());
    assert_eq!(player.dc.selected_font, None);
}

#[test]
fn deleting_unselected_object_does_not_change_dc() {
    let mut player = object_player(
        1,
        GdiObject::Pen(Pen {
            color: Color::rgb(10, 20, 30),
            ..Pen::default()
        }),
    );
    let before = player.dc.clone();
    player.delete_object(&1u32.to_le_bytes(), 2, 0).unwrap();
    assert_eq!(player.dc.core, before.core);
    assert_eq!(player.dc.selected_pen, before.selected_pen);
}

#[test]
fn deleted_object_handle_can_be_reused() {
    let mut player = object_player(1, GdiObject::Pen(Pen::default()));
    player.delete_object(&1u32.to_le_bytes(), 2, 0).unwrap();
    assert!(player
        .add_object(1, GdiObject::Brush(Brush::default()), 3)
        .is_ok());
}

#[test]
fn save_select_delete_restore_keeps_default_object() {
    let mut player = object_player(1, GdiObject::Pen(Pen::default()));
    player.save_dc().unwrap();
    player.select_object(&1u32.to_le_bytes(), 2, 0).unwrap();
    player.delete_object(&1u32.to_le_bytes(), 3, 0).unwrap();
    player.restore_dc(&(-1i32).to_le_bytes(), 4, 0).unwrap();
    assert_eq!(player.dc.core.pen, Pen::default());
    assert_eq!(player.dc.selected_pen, None);
}

#[test]
fn deletion_sanitizes_an_older_saved_dc_reference() {
    let first = Pen {
        color: Color::rgb(10, 20, 30),
        ..Pen::default()
    };
    let second = Pen {
        color: Color::rgb(40, 50, 60),
        ..Pen::default()
    };
    let mut player = object_player(1, GdiObject::Pen(first));
    player
        .add_object(2, GdiObject::Pen(second.clone()), 1)
        .unwrap();
    player.select_object(&1u32.to_le_bytes(), 2, 0).unwrap();
    player.save_dc().unwrap();
    player.select_object(&2u32.to_le_bytes(), 3, 0).unwrap();
    player.delete_object(&1u32.to_le_bytes(), 4, 0).unwrap();
    assert_eq!(player.dc.core.pen, second);
    player.restore_dc(&(-1i32).to_le_bytes(), 5, 0).unwrap();
    assert_eq!(player.dc.core.pen, Pen::default());
    assert_eq!(player.dc.selected_pen, None);
}

#[test]
fn save_dc_depth_is_bounded() {
    let input = emf(&[record(33, &[]), record(33, &[])]);
    let mut options = RenderOptions::default();
    options.limits.max_dc_depth = 1;
    let mut renderer = SvgRenderer::new(options.limits.clone());
    assert!(matches!(
        playback(&input, &options, &mut renderer),
        Err(MetafileError::ResourceLimitExceeded {
            resource: "SaveDC depth",
            ..
        })
    ));
}

#[test]
fn non_overlapping_clips_remain_empty() {
    let input = emf(&[
        record(30, &i32s(&[0, 0, 10, 10])),
        record(30, &i32s(&[20, 20, 30, 30])),
        record(27, &i32s(&[0, 0])),
        record(54, &i32s(&[50, 50])),
    ]);
    let (svg, _) = render(&input, false).unwrap();
    assert!(svg.contains("<polygon points=\"\"/>"), "{svg}");
}

#[test]
fn renders_stretch_dibits_and_destination_mirroring() {
    let mut payload = vec![0; 72];
    payload[16..48].copy_from_slice(&i32s(&[10, 20, 0, 0, 1, 1, 80, 40]));
    payload[40..44].copy_from_slice(&80u32.to_le_bytes());
    payload[44..48].copy_from_slice(&40u32.to_le_bytes());
    payload[48..52].copy_from_slice(&120u32.to_le_bytes());
    payload[52..56].copy_from_slice(&4u32.to_le_bytes());
    payload[60..64].copy_from_slice(&0x00cc_0020u32.to_le_bytes());
    payload[64..72].copy_from_slice(&i32s(&[-20, 30]));
    let mut bmi = vec![0; 40];
    bmi[0..4].copy_from_slice(&40u32.to_le_bytes());
    bmi[4..8].copy_from_slice(&1i32.to_le_bytes());
    bmi[8..12].copy_from_slice(&1i32.to_le_bytes());
    bmi[12..14].copy_from_slice(&1u16.to_le_bytes());
    bmi[14..16].copy_from_slice(&24u16.to_le_bytes());
    payload.extend_from_slice(&bmi);
    payload.extend_from_slice(&[0, 0, 255, 0]);
    let (svg, _) = render(&emf(&[record(81, &payload)]), false).unwrap();
    assert!(
        svg.contains("data:image/png;base64")
            && svg.contains("transform=\"matrix(-20 0 0 30 10 20)\""),
        "{svg}"
    );
}

#[test]
fn unsupported_dib_compression_is_diagnostic_or_strict_error() {
    let mut payload = vec![0; 72];
    payload[16..48].copy_from_slice(&i32s(&[0, 0, 0, 0, 1, 1, 80, 40]));
    payload[40..44].copy_from_slice(&80u32.to_le_bytes());
    payload[44..48].copy_from_slice(&40u32.to_le_bytes());
    payload[48..52].copy_from_slice(&120u32.to_le_bytes());
    payload[52..56].copy_from_slice(&4u32.to_le_bytes());
    payload[60..64].copy_from_slice(&0x00cc_0020u32.to_le_bytes());
    payload[64..72].copy_from_slice(&i32s(&[10, 10]));
    let mut bmi = vec![0; 40];
    bmi[0..4].copy_from_slice(&40u32.to_le_bytes());
    bmi[4..8].copy_from_slice(&1i32.to_le_bytes());
    bmi[8..12].copy_from_slice(&1i32.to_le_bytes());
    bmi[12..14].copy_from_slice(&1u16.to_le_bytes());
    bmi[14..16].copy_from_slice(&24u16.to_le_bytes());
    bmi[16..20].copy_from_slice(&1u32.to_le_bytes());
    payload.extend_from_slice(&bmi);
    payload.extend_from_slice(&[0; 4]);
    let input = emf(&[record(81, &payload)]);
    assert!(render(&input, false)
        .unwrap()
        .1
        .iter()
        .any(|diagnostic| diagnostic.code == "unsupported_bitmap"));
    assert!(matches!(
        render(&input, true),
        Err(MetafileError::UnsupportedBitmap { .. })
    ));
}

#[test]
fn malformed_input_never_panics() {
    let mut state = 0x1234_5678u32;
    for length in 0..256usize {
        let mut bytes = vec![0; length];
        for byte in &mut bytes {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            *byte = state as u8;
        }
        assert!(std::panic::catch_unwind(|| {
            let _ = inspect(&bytes);
            let _ = render(&bytes, false);
            let _ = render(&bytes, true);
        })
        .is_ok());
    }
}

#[test]
fn arc_to_inside_path_connects_and_uses_cubic_segments() {
    let mut arc = i32s(&[0, 0, 100, 100]);
    arc.extend_from_slice(&i32s(&[100, 50, 50, 100]));
    let input = emf(&[
        record(59, &[]),
        record(27, &i32s(&[0, 50])),
        record(55, &arc),
        record(60, &[]),
        record(64, &i32s(&[0, 0, 100, 100])),
    ]);
    let (svg, _) = render(&input, false).unwrap();
    assert!(svg.contains("L 100 50 C"), "{svg}");
}

#[test]
fn glyph_index_text_is_never_misdecoded() {
    let mut payload = vec![0; 68];
    payload[36..40].copy_from_slice(&1u32.to_le_bytes());
    payload[40..44].copy_from_slice(&76u32.to_le_bytes());
    payload[44..48].copy_from_slice(&0x10u32.to_le_bytes());
    let input = emf(&[record(84, &payload)]);
    let (svg, diagnostics) = render(&input, false).unwrap();
    assert!(!svg.contains("<text"));
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "glyph_index_text"));
    assert!(matches!(
        render(&input, true),
        Err(MetafileError::UnsupportedCriticalFeature(_))
    ));
}

#[test]
fn affine_rectangles_preserve_rotation_and_shear() {
    let rotated = emf(&[
        record(35, &transform([0.0, 1.0, -1.0, 0.0, 0.0, 0.0])),
        record(43, &i32s(&[0, 0, 10, 20])),
    ]);
    let (svg, _) = render(&rotated, false).unwrap();
    assert!(svg.contains("M 0 0 L 0 10 L -20 10 L -20 0 Z"), "{svg}");
    assert!(!svg.contains("<rect"), "{svg}");

    let sheared = emf(&[
        record(35, &transform([1.0, 0.0, 0.5, 1.0, 0.0, 0.0])),
        record(43, &i32s(&[0, 0, 10, 20])),
    ]);
    let (svg, _) = render(&sheared, false).unwrap();
    assert!(svg.contains("M 0 0 L 10 0 L 20 20 L 10 20 Z"), "{svg}");
}

#[test]
fn affine_ellipses_transform_every_control_point() {
    let rotated = emf(&[
        record(35, &transform([0.0, 1.0, -1.0, 0.0, 0.0, 0.0])),
        record(42, &i32s(&[0, 0, 10, 20])),
    ]);
    let (svg, _) = render(&rotated, false).unwrap();
    assert!(svg.contains("M -10 10 C -15.522847 10"), "{svg}");

    let scaled_and_rotated = emf(&[
        record(35, &transform([0.0, 2.0, -3.0, 0.0, 0.0, 0.0])),
        record(42, &i32s(&[0, 0, 10, 20])),
    ]);
    let (svg, _) = render(&scaled_and_rotated, false).unwrap();
    assert!(svg.contains("M -30 20 C -46.568542 20"), "{svg}");
}

#[test]
fn affine_arcs_pies_and_chords_are_renderer_neutral_paths() {
    let mut arc = i32s(&[0, 0, 100, 50]);
    arc.extend_from_slice(&i32s(&[100, 25, 50, 50]));
    let input = emf(&[
        record(35, &transform([0.0, 1.0, -1.0, 0.0, 0.0, 0.0])),
        record(45, &arc),
        record(46, &arc),
        record(47, &arc),
    ]);
    let (svg, _) = render(&input, false).unwrap();
    assert!(svg.contains("M -25 100 C"), "{svg}");
    assert!(svg.matches(" Z\"").count() >= 2, "{svg}");
    assert!(!svg.contains(" A "), "{svg}");
}

#[test]
fn negative_mapping_and_world_transform_compose_without_box_collapse() {
    let input = emf(&[
        record(11, &i32s(&[-2, 3])),
        record(35, &transform([0.0, 1.0, -1.0, 0.0, 0.0, 0.0])),
        record(43, &i32s(&[0, 0, 10, 20])),
    ]);
    let (svg, _) = render(&input, false).unwrap();
    assert!(svg.contains("M 0 0 L 0 30 L 40 30 L 40 0 Z"), "{svg}");
}

#[test]
fn modify_world_transform_left_and_right_orders_differ() {
    let records = |mode| {
        vec![
            record(35, &transform([1.0, 0.0, 0.0, 1.0, 10.0, 0.0])),
            record(36, &modify_transform([2.0, 0.0, 0.0, 2.0, 0.0, 0.0], mode)),
            record(27, &i32s(&[0, 0])),
            record(54, &i32s(&[1, 0])),
        ]
    };
    let (left, _) = render(&emf(&records(2)), false).unwrap();
    let (right, _) = render(&emf(&records(3)), false).unwrap();
    assert!(left.contains("M 10 0 L 12 0"), "{left}");
    assert!(right.contains("M 20 0 L 22 0"), "{right}");
}

#[test]
fn transformed_clip_is_polygonal_and_survives_save_restore() {
    let input = emf(&[
        record(35, &transform([0.0, 1.0, -1.0, 0.0, 0.0, 0.0])),
        record(30, &i32s(&[0, 0, 10, 20])),
        record(33, &[]),
        record(30, &i32s(&[0, 0, 5, 5])),
        record(34, &(-1i32).to_le_bytes()),
        record(27, &i32s(&[0, 0])),
        record(54, &i32s(&[30, 30])),
    ]);
    let (svg, _) = render(&input, false).unwrap();
    assert!(
        svg.contains("<polygon points=\"0,0 0,10 -20,10 -20,0\"/>"),
        "{svg}"
    );
}

#[test]
fn completed_paths_can_become_deterministic_clips() {
    let input = emf(&[
        record(59, &[]),
        record(27, &i32s(&[0, 0])),
        record(54, &i32s(&[20, 0])),
        record(54, &i32s(&[20, 20])),
        record(61, &[]),
        record(60, &[]),
        record(67, &5u32.to_le_bytes()),
        record(27, &i32s(&[0, 10])),
        record(54, &i32s(&[30, 10])),
    ]);
    let (first, diagnostics) = render(&input, false).unwrap();
    let (second, _) = render(&input, false).unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(
        first.contains(
            "<clipPath id=\"clip0\"><path d=\"M 0 0 L 20 0 L 20 20 Z\" clip-rule=\"evenodd\"/>"
        ),
        "{first}"
    );
    assert_eq!(first, second);
}

#[test]
fn path_and_rectangular_clips_intersect_deterministically() {
    let input = emf(&[
        record(30, &i32s(&[0, 0, 30, 30])),
        record(59, &[]),
        record(27, &i32s(&[5, 5])),
        record(54, &i32s(&[25, 5])),
        record(54, &i32s(&[25, 25])),
        record(61, &[]),
        record(60, &[]),
        record(67, &1u32.to_le_bytes()),
        record(27, &i32s(&[0, 10])),
        record(54, &i32s(&[30, 10])),
    ]);
    let (svg, diagnostics) = render(&input, false).unwrap();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(svg.contains("<clipPath id=\"clip0\"><polygon"), "{svg}");
    assert!(
        svg.contains("<clipPath id=\"clip1\"><g clip-path=\"url(#clip0)\"><path"),
        "{svg}"
    );
}

#[test]
fn transformed_bitmap_uses_full_affine_placement() {
    let mut payload = vec![0; 72];
    payload[16..48].copy_from_slice(&i32s(&[10, 20, 0, 0, 1, 1, 80, 40]));
    payload[40..44].copy_from_slice(&80u32.to_le_bytes());
    payload[44..48].copy_from_slice(&40u32.to_le_bytes());
    payload[48..52].copy_from_slice(&120u32.to_le_bytes());
    payload[52..56].copy_from_slice(&4u32.to_le_bytes());
    payload[60..64].copy_from_slice(&0x00cc_0020u32.to_le_bytes());
    payload[64..72].copy_from_slice(&i32s(&[20, 30]));
    let mut bmi = vec![0; 40];
    bmi[0..4].copy_from_slice(&40u32.to_le_bytes());
    bmi[4..8].copy_from_slice(&1i32.to_le_bytes());
    bmi[8..12].copy_from_slice(&1i32.to_le_bytes());
    bmi[12..14].copy_from_slice(&1u16.to_le_bytes());
    bmi[14..16].copy_from_slice(&24u16.to_le_bytes());
    payload.extend_from_slice(&bmi);
    payload.extend_from_slice(&[0, 0, 255, 0]);
    let input = emf(&[
        record(35, &transform([1.0, 0.5, 0.25, 1.0, 3.0, 4.0])),
        record(81, &payload),
    ]);
    let (svg, _) = render(&input, false).unwrap();
    assert!(
        svg.contains("transform=\"matrix(20 10 7.5 30 18 29)\""),
        "{svg}"
    );
}
