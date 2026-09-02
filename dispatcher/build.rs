use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use tar::Builder;
use zstd::stream::Encoder;

fn int_to_human(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{:.2} {}", value, UNITS[unit])
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=L_DISPATCHER_SO_FILES");

    let so_files = env::var("L_DISPATCHER_SO_FILES")
        .expect("L_DISPATCHER_SO_FILES must be set to a space-separated list of .so paths");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Build a tar archive in memory. Each .so is added as a tar entry named
    // by the bash version.
    let mut tar_buf: Vec<u8> = Vec::new();
    let mut so_entries: Vec<(String, u64)> = Vec::new();
    {
        let mut tar_builder = Builder::new(&mut tar_buf);

        for entry in so_files.split(' ') {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }
            println!("cargo:rerun-if-changed={entry}");

            let path = PathBuf::from(entry);

            // Extract the bash version from the path using regex.
            // Expected path format: .../build/Release/5.2/L_builtin.so or .../build/Debug/5.2/l_builtin_embedded.so
            let version = {
                use regex::Regex;
                let re = Regex::new(r".*/([0-9]+\.[0-9]+)/").unwrap();
                let path_str = path.display().to_string();
                if let Some(caps) = re.captures(&path_str) {
                    caps.get(1).unwrap().as_str().to_string()
                } else {
                    panic!("Could not extract version from path: {}", path.display())
                }
            };

            let data = fs::read(&path).unwrap_or_else(|e| panic!("Failed to read .so: {}", e));

            so_entries.push((path.display().to_string(), data.len() as u64));

            // Append to tar with the version string as the entry name.
            tar_builder
                .append_data(
                    &mut {
                        let mut header = tar::Header::new_old();
                        header.set_path(version.clone()).unwrap();
                        header.set_size(data.len() as u64);
                        header.set_mode(0o755);
                        header.set_mtime(0);
                        header.set_mtime(0);
                        header
                    },
                    &version,
                    &data[..],
                )
                .unwrap_or_else(|e| panic!("Failed to append to tar: {}", e));

            println!(
                "cargo:warning=  archived {} as tar entry '{}'",
                path.display(),
                version
            );
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

    println!(
        "cargo:rustc-env=EMBEDDED_TAR_ZST_PATH={}",
        zstd_path.display()
    );
    println!(
        "cargo:warning=Embedded tar.zst: {} (tar: {})",
        int_to_human(zstd_buf.len() as u64),
        int_to_human(tar_buf.len() as u64)
    );

    // Write a size report artifact for inspection.
    let report_path = out_dir.join("embedded_sizes_report.txt");
    let mut report = String::new();
    report.push_str("L_builtin dispatcher embedded archive size report\n");
    report.push_str("=================================================\n\n");
    report.push_str("Used .so files:\n");
    for (path, size) in &so_entries {
        report.push_str(&format!(
            "  {}  ({} bytes / {})\n",
            path,
            size,
            int_to_human(*size)
        ));
    }
    report.push_str(&format!(
        "\nUncompressed tar archive size: {} bytes ({})\n",
        tar_buf.len(),
        int_to_human(tar_buf.len() as u64)
    ));
    report.push_str(&format!(
        "Compressed tar archive size: {} bytes ({})\n",
        zstd_buf.len(),
        int_to_human(zstd_buf.len() as u64)
    ));
    report.push_str(&format!(
        "Compression ratio: {:.2}%\n",
        (zstd_buf.len() as f64 / tar_buf.len() as f64) * 100.0
    ));
    fs::write(&report_path, &report)
        .unwrap_or_else(|e| panic!("Failed to write size report: {}", e));
    println!(
        "cargo:warning=Size report written to {}",
        report_path.display()
    );

    println!("cargo:rerun-if-env-changed=L_DISPATCHER_SO_FILES");
}
