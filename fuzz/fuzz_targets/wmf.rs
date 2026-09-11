#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let _ = metafile_wmf::inspect(data);
    let _ = metafile_wmf::to_svg(data, Default::default());
});
