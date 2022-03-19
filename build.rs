//! This build script copies the `memory.x` file from the crate root into
//! a directory where the linker can always find it at build time.
//! For many projects this is optional, as the linker always searches the
//! project root directory -- wherever `Cargo.toml` is. However, if you
//! are using a workspace or have a more complicated build setup, this
//! build script becomes required. Additionally, by requesting that
//! Cargo re-run the build script whenever `memory.x` is changed,
//! updating `memory.x` ensures a rebuild of the application with the
//! new memory settings.

use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    #[cfg(not(any(feature = "stm32f407", feature = "gd32f307")))]
    compile_error!("No printer selected. Use make PRINTER=mono4k or cargo --features=mono4k");

    #[cfg(feature = "stm32f407")]
    let mcu = "stm32f407";

    #[cfg(feature = "gd32f307")]
    let mcu = "gd32f307";

    let linker_file = format!("linker/{}.x", mcu);
    let linker_file_content = std::fs::read(&linker_file)
        .unwrap_or_else(|_| panic!("Cannot read {}", &linker_file));

    // Put `memory.x` in our output directory and ensure it's
    // on the linker search path.
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    File::create(out.join("memory.x"))
        .unwrap()
        .write_all(&linker_file_content).unwrap();
    println!("cargo:rustc-link-search={}", out.display());

    // By default, Cargo will re-run a build script whenever
    // any file in the project changes. By specifying `memory.x`
    // here, we ensure the build script is only re-run when
    // `memory.x` is changed.
    println!("cargo:rerun-if-changed=memory.x");
}
