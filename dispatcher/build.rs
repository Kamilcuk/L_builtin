use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use tar::Builder;
use zstd::stream::Encoder;

fn main() {
    println!("cargo:rerun-if-env-changed=L_DISPATCHER_SO_FILES");

    let so_files = env::var("L_DISPATCHER_SO_FILES")
        .expect("L_DISPATCHER_SO_FILES must be set to a semicolon-separated list of .so paths");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Build a tar archive in memory. Each .so is objcopy'd first to localize
    // L_builtin_struct, then added as a tar entry named by the bash version.
    let mut tar_buf: Vec<u8> = Vec::new();
    {
        let mut tar_builder = Builder::new(&mut tar_buf);

        for entry in so_files.split(';') {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }

            let path = PathBuf::from(entry);

            // Extract the bash version from the parent directory name.
            // Expected path format: .../build/Release/5.2/L_builtin.so
            let version = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|f| f.to_str())
                .unwrap_or_else(|| {
                    panic!("Could not extract version from path: {}", path.display())
                });

            // Run objcopy to localize L_builtin_struct in the .so.
            let objcopy_path = out_dir.join(format!("{}.objcopy", path.file_name().unwrap().to_str().unwrap()));
            let status = std::process::Command::new("objcopy")
                .arg("--localize-symbol=L_builtin_struct")
                .arg(&path)
                .arg(&objcopy_path)
                .status()
                .unwrap_or_else(|e| panic!("Failed to run objcopy: {}", e));

            if !status.success() {
                panic!("objcopy failed for {}", path.display());
            }

            let data = fs::read(&objcopy_path)
                .unwrap_or_else(|e| panic!("Failed to read objcopy'd .so: {}", e));

            // Append to tar with the version string as the entry name.
            tar_builder
                .append_data(
                    &mut {
                        let mut header = tar::Header::new_old();
                        header.set_path(version).unwrap();
                        header.set_size(data.len() as u64);
                        header.set_mode(0o755);
                        header.set_mtime(0);
                        header.set_mtime(0);
                        header
                    },
                    version,
                    &data[..],
                )
                .unwrap_or_else(|e| panic!("Failed to append to tar: {}", e));

            println!("cargo:warning=  archived {} as tar entry '{}'", path.display(), version);
        }

        tar_builder
            .into_inner()
            .unwrap_or_else(|e| panic!("Failed to finalize tar: {}", e));
    }

    // Zstd-compress the tar archive at level 19.
    let mut zstd_buf: Vec<u8> = Vec::new();
    {
        let mut encoder = Encoder::new(&mut zstd_buf, 19)
            .unwrap_or_else(|e| panic!("Failed to create zstd encoder: {}", e));
        encoder
            .write_all(&tar_buf)
            .unwrap_or_else(|e| panic!("Failed to write to zstd encoder: {}", e));
        encoder
            .finish()
            .unwrap_or_else(|e| panic!("Failed to finish zstd encoding: {}", e));
    }

    // Write the compressed tar to OUT_DIR.
    let zstd_path = out_dir.join("embedded.tar.zst");
    fs::write(&zstd_path, &zstd_buf)
        .unwrap_or_else(|e| panic!("Failed to write embedded.tar.zst: {}", e));

    println!("cargo:rustc-env=EMBEDDED_TAR_ZST_PATH={}", zstd_path.display());
    println!(
        "cargo:warning=Embedded tar.zst: {} bytes (tar: {} bytes)",
        zstd_buf.len(),
        tar_buf.len()
    );
    println!("cargo:rerun-if-env-changed=L_DISPATCHER_SO_FILES");
}
