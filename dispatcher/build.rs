use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

trait UnwrapOrPanic<T, E: std::fmt::Display> {
    fn unwrap_or_panic(self, op: &str, ctx: &str) -> T;
}

impl<T, E: std::fmt::Display> UnwrapOrPanic<T, E> for Result<T, E> {
    fn unwrap_or_panic(self, op: &str, ctx: &str) -> T {
        self.unwrap_or_else(|e| panic!("failed to {op} {ctx}: {e}"))
    }
}

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

fn rebuild_needed<O, I, P1, P2>(outputs: O, inputs: I) -> bool
where
    O: AsRef<[P1]>,
    P1: AsRef<Path>,
    I: AsRef<[P2]>,
    P2: AsRef<Path>,
{
    for input in inputs.as_ref() {
        println!("cargo:rerun-if-changed={}", input.as_ref().display());
    }
    let newest_input = inputs
        .as_ref()
        .iter()
        .filter_map(|p| fs::metadata(p).ok())
        .filter_map(|m| m.modified().ok())
        .max()
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let rebuild_needed = outputs.as_ref().iter().any(|output| {
        match fs::metadata(output).and_then(|m| m.modified()) {
            Ok(output_time) => output_time < newest_input,
            Err(_) => true,
        }
    });
    rebuild_needed
}

fn add_custom_command<O, I, P1, P2, F>(outputs: O, inputs: I, mut action: F)
where
    O: AsRef<[P1]>,
    P1: AsRef<Path>,
    I: AsRef<[P2]>,
    P2: AsRef<Path>,
    F: FnMut(&[P1], &[P2]),
{
    if rebuild_needed(&outputs, &inputs) {
        for output in outputs.as_ref() {
            if let Some(parent) = output.as_ref().parent() {
                let _ = fs::create_dir_all(parent);
            }
        }
        action(outputs.as_ref(), inputs.as_ref());
    }
}

fn create_tar_zstd_from_so<O, I, P2>(output: O, inputs: I)
where
    O: AsRef<Path>,
    I: AsRef<[P2]>,
    P2: AsRef<Path>,
{
    let mut tar_buf: Vec<u8> = Vec::new();
    let version_r = regex::Regex::new(r".*([0-9]+[._][0-9]+)[.]so$")
        .expect("hardcoded version regex must compile");

    let mut tar_builder = tar::Builder::new(&mut tar_buf);

    for path in inputs.as_ref() {
        println!("cargo:rerun-if-changed={}", path.as_ref().display());

        let path_str = path.as_ref().display().to_string();
        let mut version = if let Some(caps) = version_r.captures(&path_str) {
            caps.get(1)
                .unwrap_or_else(|| panic!("regex matched but capture group 1 missing in {}", path_str))
                .as_str()
                .to_string()
        } else {
            panic!(
                "Could not extract version from path: {}",
                path.as_ref().display()
            );
        };
        version = version.replace("_", ".");

        let data = std::fs::read(path)
            .unwrap_or_panic("read .so file", &format!("'{}'", path.as_ref().display()));

        let mut header = tar::Header::new_old();
        header
            .set_path(version.clone())
            .unwrap_or_panic("set tar header path", &format!("'{}'", version));
        header.set_size(data.len() as u64);
        header.set_mode(0o755);
        header.set_mtime(0);

        tar_builder
            .append_data(&mut header, &version, &data[..])
            .unwrap_or_panic("append tar entry", &format!("'{}'", version));
    }
    tar_builder
        .into_inner()
        .unwrap_or_panic("finalize tar builder", "to inner vec");

    let mut zstd_buf = Vec::new();
    let mut encoder = zstd::stream::write::Encoder::new(&mut zstd_buf, 19)
        .unwrap_or_panic("create zstd encoder", "with level 19");
    std::io::Write::write_all(&mut encoder, &tar_buf)
        .unwrap_or_panic("write into zstd encoder", "tar buffer");
    encoder.finish().unwrap_or_panic("finish zstd stream", "to inner vec");

    std::fs::write(output.as_ref(), &zstd_buf)
        .unwrap_or_panic("write output", &format!("'{}'", output.as_ref().display()));
}

fn main() {
    let so_files_str =
        std::env::var("L_DISPATCHER_SO_FILES").expect("L_DISPATCHER_SO_FILES must be set");
    println!("cargo:rerun-if-env-changed=L_DISPATCHER_SO_FILES");
    let out_dir = PathBuf::from(
        std::env::var("OUT_DIR").expect("Cargo must always set the OUT_DIR environment variable"),
    );
    println!("cargo:rerun-if-env-changed=OUT_DIR");
    let so_files: Vec<PathBuf> = so_files_str
        .split(' ')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect();
    let zstd_path = out_dir.join("embedded.tar.zst");
    println!(
        "cargo:rustc-env=EMBEDDED_TAR_ZST_PATH={}",
        zstd_path.display()
    );
    add_custom_command([&zstd_path], &so_files, |outputs, inputs| {
        create_tar_zstd_from_so(outputs[0], inputs);
    });
    let manifest_parent = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();

    let get_relative = |p: &Path| p.strip_prefix(manifest_parent).unwrap_or(p).to_path_buf();

    let zstd_size = std::fs::metadata(&zstd_path).map(|m| m.len()).unwrap_or(0);
    let mut so_files_size = 0u64;

    for path in &so_files {
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        so_files_size += size;
        println!(
            "cargo:warning={} ({} bytes / {})",
            get_relative(path).display(),
            size,
            int_to_human(size)
        );
    }

    println!(
        "cargo:warning={} ({} bytes / {}). Compression ratio: {:.2}%",
        get_relative(&zstd_path).display(),
        zstd_size,
        int_to_human(zstd_size),
        (zstd_size as f64 / so_files_size as f64) * 100.0
    );
}
