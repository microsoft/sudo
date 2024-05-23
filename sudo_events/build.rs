use std::{io, path::Path};

// BODGY
//
// * As a part of the build process, we need to replace the fake GUID in our
//   tracing lib with the real one.
// * This build script here will take the value out of the env var
//   MAGIC_TRACING_GUID, and replace the fake GUID in the events_template.rs
//   file with that one.
// * We'll write that file out to %OUT_DIR%/mangled_events.rs, and then include
//   _that mangled file_ in our lib.rs file.
fn main() -> io::Result<()> {
    let input = std::fs::read_to_string("src/events_template.rs")?;
    // Is the MAGIC_TRACING_GUID env var set? If it is...
    let output = match std::env::var("MAGIC_TRACING_GUID") {
        Ok(guid) => {
            println!("MAGIC_TRACING_GUID: {}", guid);

            // Replace the fake guid (ffffffff-ffff-ffff-ffff-ffffffffffff) with this one.

            input.replace("ffffffff-ffff-ffff-ffff-ffffffffffff", &guid)
        }
        Err(_) => input,
    };
    let path = Path::new(&std::env::var("OUT_DIR").unwrap()).join("mangled_events.rs");
    println!(
        "cargo:rerun-if-changed={}",
        path.as_path().to_str().unwrap()
    );
    std::fs::write(path.as_path(), output)
}
