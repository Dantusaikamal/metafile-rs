#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let _ = metafile_wmf::inspect(data);
    let _ = metafile_emf::inspect(data);
    let _ = metafile::inspect(data);
    let _ = metafile::to_svg(data, Default::default());
    let _ = metafile::to_svg(
        data,
        metafile_core::RenderOptions {
            strict: true,
            ..Default::default()
        },
    );
});
