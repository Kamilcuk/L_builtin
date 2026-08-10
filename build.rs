// build.rs - Cargo-only build system matching CMakeLists.txt exactly

use std::env;
use std::fs;
use std::hash::Hash;
use std::path::{Path, PathBuf};

fn get_bash_version(bash_source: &Path) -> i32 {
    let version_h =
        fs::read_to_string(bash_source.join("version.h")).expect("Failed to read version.h");
    let re_bash_version = regex::Regex::new(r#"version (\d+)\.(\d+)\.(\d+)"#).unwrap();
    let caps = re_bash_version.captures(&version_h).unwrap();
    let (major, minor, patch): (_, _, _) = (
        caps[1].parse::<i32>().unwrap(),
        caps[2].parse::<i32>().unwrap(),
        caps[3].parse::<i32>().unwrap(),
    );
    major * 10000 + minor * 100 + patch
}

fn main() {
    let bash_source =
        PathBuf::from(env::var("BASH_SOURCE_DIR").expect("BASH_SOURCE_DIR must be set to compile. Use `make build' to compile the project, which will clone and build bash source code automatically."));
    if !bash_source.join("version.h").exists() {
        panic!(
            "BASH_SOURCE_DIR must point to compiled bash source code directory containing version.h"
        );
    }
    println!("cargo:rustc-env=BASH_SOURCE_DIR={}", bash_source.display());
    let bash_version = get_bash_version(&bash_source);
    println!("cargo:rustc-env=L_BASH_VERSION={}", bash_version);
    println!("cargo:rustc-cfg=bash_version=\"{}\"", bash_version);
    if bash_version < 40300 {
        println!("cargo:rustc-cfg=feature=\"bash_lt_4_3\"");
    }
    let loadable_info = generate_loadables_array(&bash_source);
    generate_loadables_header(&loadable_info);
    generate_bash_bindings(&bash_source, bash_version);
    compile_c_sources(&bash_source, bash_version, &loadable_info);
    setup_version_script();
    if cfg!(feature = "dev") {
        println!("cargo:rustc-link-arg=-Wall");
        println!("cargo:rustc-link-arg=-Wextra");
        if env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default() == "gnu" {
            println!("cargo:rustc-link-arg=-fanalyzer");
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/L_builtin.map");
}

fn has_duplicates<T: Eq + Hash>(slice: &[T]) -> Option<&T> {
    let mut seen = std::collections::HashSet::new();
    slice.iter().find(|&item| !seen.insert(item))
}

fn glob_c_sorted_by_modification(dir: &Path) -> Vec<PathBuf> {
    let mut sources: Vec<_> = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension() == Some("c".as_ref()))
        .map(|e| {
            let path = e.into_path();
            let mtime = fs::metadata(&path).and_then(|m| m.modified()).ok();
            (mtime, path)
        })
        .collect();
    sources.sort_by(|(ma, _), (mb, _)| mb.cmp(ma));
    sources.into_iter().map(|(_, path)| path).collect()
}

fn generate_loadables_array(bash_source: &Path) -> (Vec<String>, Vec<PathBuf>) {
    let sources: Vec<PathBuf> =
        glob_c_sorted_by_modification(&bash_source.join("examples/loadables"));
    let mut loadable_names = Vec::new();
    let mut valid_sources = Vec::new();
    let excluded_sources = ["accept.c", "ocut.c", "bperl.c", "iperl.c"];
    let struct_re = regex::Regex::new(r#"struct\s+builtin\s+([A-Za-z0-9_]+)_struct"#).unwrap();
    for src in &sources {
        let src_name = src.file_name().unwrap().to_string_lossy();
        if excluded_sources.iter().any(|ex| *ex == src_name.as_ref()) {
            continue;
        }
        let content = fs::read_to_string(src).unwrap_or_default();
        let matches: Vec<_> = struct_re.captures_iter(&content).collect();
        if matches.is_empty() {
            println!(
                "cargo:warning=Skipping {}: no symbol matching '<name>_struct' found",
                src.display()
            );
            continue;
        }
        let mut file_struct_names = Vec::new();
        let mut file_has_conflict = false;
        for caps in &matches {
            let struct_name = caps[1].to_string();
            if loadable_names.contains(&struct_name) {
                file_has_conflict = true;
                break;
            }
            file_struct_names.push(struct_name);
        }
        if file_has_conflict {
            println!(
                "cargo:warning=Skipping older/duplicate source: {}",
                src.display()
            );
            continue;
        }
        valid_sources.push(src.clone());
        loadable_names.extend(file_struct_names);
    }
    if let Some(name) = has_duplicates(&loadable_names) {
        panic!(
            "Assertion failed: Duplicate builtin struct registered across loadable sources: {}",
            name
        );
    }
    if let Some(src) = has_duplicates(&valid_sources) {
        panic!(
            "Assertion failed: Duplicate builtin files found: {}",
            src.display()
        );
    }
    loadable_names.sort();
    println!(
        "cargo:warning=Found {} bash loadables: {}",
        loadable_names.len(),
        loadable_names.join(" ")
    );
    (loadable_names, valid_sources)
}

fn generate_loadables_header(info: &(Vec<String>, Vec<PathBuf>)) {
    let mut content = String::new();
    content.push_str("/* Automatically generated by Cargo build.rs. Do not edit. */\n\n");
    content.push_str("#include <builtins.h>\n\n");
    for name in &info.0 {
        content.push_str(&format!("extern const struct builtin {}_struct;\n", name));
    }
    content.push_str("\nstatic const struct builtin *const bash_loadables_gen[] = {\n");
    for name in &info.0 {
        content.push_str(&format!("    &{}_struct,\n", name));
    }
    content.push_str("};\n");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let header_path = out_dir.join("bash_loadables_gen.h");
    fs::write(&header_path, content).expect("Failed to write bash_loadables_gen.h");
    println!("cargo:rerun-if-changed={}", header_path.display());
}

fn generate_bash_bindings(bash_source: &Path, bash_version: i32) {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let header = "src/bash_api_gen.h";

    // Bash API functions called from Rust that live in bash's own headers.
    const BASH_FUNCTIONS: &[&str] = &[
        "find_variable",
        "bind_variable",
        "find_function",
        "make_new_array_variable",
        "convert_var_to_array",
        "make_new_assoc_variable",
        "array_flush",
        "array_insert",
        "assoc_flush",
        "assoc_keys_to_word_list",
        "assoc_reference",
        "make_word",
        "make_word_list",
        "execute_shell_function",
        "dispose_words",
        "expand_string_to_string",
        "expand_string",
        "builtin_usage",
        "internal_getopt",
        "reset_internal_getopt",
    ];
    // Bash internal types referenced only behind pointers from Rust.
    // WORD_DESC and WORD_LIST are intentionally NOT opaque: Rust traverses the
    // word-list chain by direct field access (`(*list).next`, `(*word).word`).
    const OPAQUE_TYPES: &[&str] = &["SHELL_VAR", "ARRAY", "ARRAY_ELEMENT", "HASH_TABLE"];

    let mut builder = bindgen::Builder::default()
        .header(header)
        .clang_arg("-DHAVE_CONFIG_H")
        .clang_arg("-DHAVE_PPOLL=1")
        .clang_arg(format!("-DL_BASH_VERSION={}", bash_version))
        .clang_arg("-DSHELL")
        .clang_arg("-D_GNU_SOURCE=1")
        .clang_arg("-std=gnu99")
        .clang_args([
            format!("-I{}", bash_source.display()),
            format!("-I{}", bash_source.join("include").display()),
            format!("-I{}", bash_source.join("builtins").display()),
            format!("-I{}", bash_source.join("lib").display()),
            format!("-I{}", out_dir.display()),
        ])
        // Every l_* wrapper (bash_api_gen.h) plus the specific bash entry
        // points Rust calls directly. Note: the pattern is a regex, so `l_.*`
        // (not `l_*`, which means "l" + zero or more underscores).
        .allowlist_function("l_.*");
    for f in BASH_FUNCTIONS {
        builder = builder.allowlist_function(f);
    }
    // this_command_name (entrypoint.rs) and current_builtin (struct builtin *)
    // are read by Rust.
    builder = builder
        .allowlist_var("this_command_name")
        .allowlist_var("current_builtin")
        .allowlist_var("list_optarg")
        .allowlist_var("loptend");
    // Full definition of `struct builtin`; everything else stays opaque.
    builder = builder.allowlist_type("builtin");
    for ty in OPAQUE_TYPES {
        builder = builder.allowlist_type(ty).opaque_type(ty);
    }

    let bindings = builder
        .generate()
        .expect("bindgen failed for bash_api_gen.h");
    let bindings_out = out_dir.join("bash_api_gen.rs");
    bindings
        .write_to_file(&bindings_out)
        .expect("Failed to write bash_api_gen.rs");
    println!("cargo:rerun-if-changed={}", header);
}

fn compile_c_sources(
    bash_source: &Path,
    bash_version: i32,
    loadable_info: &(Vec<String>, Vec<PathBuf>),
) {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let bash_version_str = bash_version.to_string();
    let mut base_build = cc::Build::new();
    base_build
        .std("c99")
        .define("_GNU_SOURCE", None)
        .define("HAVE_CONFIG_H", None)
        .define("SHELL", None)
        .define("L_BASH_VERSION", Some(bash_version_str.as_str()))
        .define("HAVE_PPOLL", "1")
        .pic(true)
        .includes([
            bash_source,
            &bash_source.join("include"),
            &bash_source.join("builtins"),
            &bash_source.join("lib"),
            &out_dir,
        ]);
    base_build
        .clone()
        .flag("-w")
        .files(&loadable_info.1)
        .compile("bash_loadables");
    let glue_sources = ["poll.c", "sig.c", "bash_api.c", "cmd_ext.c", "L_builtin.c"];
    base_build
        .clone()
        .flag("-Wall")
        .flag("-Wextra")
        .files(glue_sources.into_iter().map(|x| Path::new("src").join(x)))
        .compile("c_sources");
    println!("cargo:rustc-link-lib=static=c_sources");
    // println!("cargo:rustc-link-lib=static=bash_glue");
    let link_args = [
        "-Wl,--no-gc-sections",
        "-Wl,--no-undefined-version",
        "-Wl,--undefined=L_builtin_struct",
    ];
    for arg in link_args {
        println!("cargo:rustc-link-arg={arg}");
    }
    for lib in ["dl", "m", "rt", "pthread"] {
        println!("cargo:rustc-link-lib={lib}");
    }
}

fn setup_version_script() {
    let version_script =
        fs::read_to_string("src/L_builtin.map").expect("Failed to read src/L_builtin.map");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let script_path = out_dir.join("version.map");
    fs::write(&script_path, version_script).expect("Failed to write version.map");
    println!(
        "cargo:rustc-link-arg=-Wl,--version-script={}",
        script_path.display()
    );
}
