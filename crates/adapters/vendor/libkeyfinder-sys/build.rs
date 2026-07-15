//! Vendored from libkeyfinder-sys 0.1.0. Fork change #4 (see Cargo.toml): upstream's
//! `.expect("Could not find libkeyfinder")` prints a bare pkg-config dump, which is the first thing
//! a fresh checkout hits and says nothing about how to fix it. The probe now explains the install.

/// The minimum libKeyFinder the C++ bridge is known to compile against (upstream's bound).
const MIN_LIBKEYFINDER_VERSION: &str = "2.2";

/// Points at the documented setup rather than at pkg-config's internals.
const SETUP_HINT: &str = "\
libkeyfinder was not found via pkg-config.

  macOS (Apple Silicon):  brew install libkeyfinder   # pulls fftw; ships the .pc file

If it is installed but still not found, its .pc directory is not on PKG_CONFIG_PATH:

  export PKG_CONFIG_PATH=\"$(brew --prefix libkeyfinder)/lib/pkgconfig:$PKG_CONFIG_PATH\"

Run scripts/check-system-libs.sh to verify every native prerequisite at once.";

fn main() {
    // Locate the system-installed libkeyfinder using pkg-config
    let lib = match pkg_config::Config::new()
        .atleast_version(MIN_LIBKEYFINDER_VERSION)
        .probe("libkeyfinder")
    {
        Ok(lib) => lib,
        Err(error) => panic!("{SETUP_HINT}\n\npkg-config reported: {error}"),
    };

    // Build the C++ bridge with cxx
    let mut build = cxx_build::bridge("src/lib.rs");

    build.file("src/bridge.cpp").flag("-std=c++11");

    // Add include paths from libkeyfinder
    for path in &lib.include_paths {
        build.include(path);
    }

    build.compile("libkeyfinder-sys");

    // Tell cargo to rerun if bridge files change
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/bridge.cpp");
}
