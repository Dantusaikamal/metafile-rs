use serde::Deserialize;
use std::{fs, path::PathBuf};

#[derive(Deserialize)]
struct Manifest {
    fixtures: Vec<Fixture>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    file: String,
    expected_format: String,
    expected_support: String,
}

#[test]
fn committed_corpus_matches_declared_support_contract() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest: Manifest = serde_json::from_slice(
        &fs::read(root.join("fixtures/manifest.json")).expect("read fixture manifest"),
    )
    .expect("parse fixture manifest");
    assert!(
        manifest
            .fixtures
            .iter()
            .filter(|f| f.expected_format == "wmf")
            .count()
            >= 5
    );
    assert!(manifest.fixtures.iter().any(|f| f.expected_format == "emf"));
    assert!(manifest
        .fixtures
        .iter()
        .any(|f| f.expected_format == "emfplus"));
    for fixture in manifest.fixtures {
        let bytes =
            fs::read(root.join("fixtures").join(&fixture.file)).expect("read declared fixture");
        if fixture.expected_support == "render" {
            let first = metafile::to_svg(&bytes, Default::default()).expect("render fixture");
            let second = metafile::to_svg(&bytes, Default::default()).expect("repeat fixture");
            assert_eq!(
                first.svg, second.svg,
                "{} was not deterministic",
                fixture.file
            );
            assert!(first.svg.contains("<svg"));
            assert!(first.svg.contains("</svg>"));
        } else {
            let info = metafile::inspect(&bytes).expect("inspect unsupported playback fixture");
            assert_eq!(info.format, metafile_core::MetafileFormat::EmfPlus);
            assert!(matches!(
                metafile::to_svg(&bytes, Default::default()),
                Err(metafile_core::MetafileError::UnsupportedCriticalFeature(_))
            ));
        }
    }
}
