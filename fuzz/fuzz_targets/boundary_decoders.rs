#![no_main]

use ca_engine::{repository::RelativeSourcePath, retrieval::validate_opaque_cursor_syntax};
use ca_mcp::validate_resource_uri_syntax;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let bounded = &data[..data.len().min(8 * 1_024)];
    if let Ok(value) = std::str::from_utf8(bounded) {
        let _ = RelativeSourcePath::new(value.to_owned());
        let _ = validate_opaque_cursor_syntax(value);
        let _ = validate_resource_uri_syntax(value);
    }
});
