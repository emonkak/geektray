use pkg_config::probe_library;
use std::env;
use std::path::PathBuf;

fn main() {
    probe_library("xkbcommon").unwrap();
    probe_library("xkbcommon-x11").unwrap();

    let bindings = bindgen::Builder::default()
        .header_contents(
            "wrapper.h",
            r#"
#include <xkbcommon/xkbcommon.h>
#include <xkbcommon/xkbcommon-x11.h>
"#,
        )
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .prepend_enum_name(false)
        .size_t_is_usize(true)
        .allowlist_function("xkb_.*")
        .allowlist_type("xkb_.*")
        .allowlist_var("XKB_.*")
        .blocklist_type("FILE") // we use FILE from libc
        .blocklist_type("_IO_.*")
        .generate()
        .expect("Unable to generate bindings");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_dir.join("xkbcommon_sys.rs"))
        .expect("Couldn't write bindings!");
}
