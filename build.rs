use std::io::{self, Write};
use std::path::PathBuf;
use std::{env, fs};

use flate2::Compression;
use flate2::write::GzEncoder;

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-changed=src/manifest.yaml");

    let manifest = fs::read("src/manifest.yaml")?;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(&manifest)?;
    let compressed = encoder.finish()?;

    let out_dir = env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("OUT_DIR is not set"))?;
    fs::write(out_dir.join("manifest.yaml.gz"), compressed)
}
