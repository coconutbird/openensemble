//! Checks the repository rules in `AGENTS.md` that Clippy does not check.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_RUST_SOURCE_LINES: usize = 1_000;

#[test]
fn every_package_inherits_workspace_lints() {
    let root = workspace_root();
    let manifests = package_manifests(&root);
    assert!(
        !manifests.is_empty(),
        "no package manifests found under {root:?}"
    );

    let mut failures = Vec::new();
    for manifest in manifests {
        let contents = read_text(&manifest);
        if !section_has_workspace_true(&contents, "lints") {
            failures.push(relative_path(&root, &manifest));
        }
    }

    assert!(
        failures.is_empty(),
        "every package must contain `[lints] workspace = true`; missing in:\n{}",
        failures.join("\n")
    );
}

#[test]
fn rust_sources_respect_the_file_size_limit() {
    let root = workspace_root();
    let mut oversized = Vec::new();

    for source in rust_sources(&root) {
        let line_count = read_text(&source).lines().count();
        if line_count > MAX_RUST_SOURCE_LINES {
            oversized.push(format!(
                "{}: {line_count} lines",
                relative_path(&root, &source)
            ));
        }
    }

    assert!(
        oversized.is_empty(),
        "Rust source files may contain at most {MAX_RUST_SOURCE_LINES} lines:\n{}",
        oversized.join("\n")
    );
}

#[test]
fn packages_follow_cargo_project_layout() {
    let root = workspace_root();
    assert!(
        root.join("Cargo.lock").is_file(),
        "workspace Cargo.lock is missing"
    );

    let mut failures = Vec::new();
    for manifest in package_manifests(&root) {
        let package_root = manifest
            .parent()
            .expect("package manifest must have a parent directory");
        let source_root = package_root.join("src");
        if !source_root.is_dir() {
            failures.push(format!(
                "{}: package source must live under src",
                relative_path(&root, &manifest)
            ));
            continue;
        }
        if !has_default_or_additional_target(&source_root) {
            failures.push(format!(
                "{}: expected src/lib.rs, src/main.rs, or src/bin target",
                relative_path(&root, package_root)
            ));
        }
        validate_target_directory(&root, &source_root.join("bin"), &mut failures);
        validate_target_directory(&root, &package_root.join("examples"), &mut failures);
        validate_target_directory(&root, &package_root.join("benches"), &mut failures);
    }

    assert!(
        failures.is_empty(),
        "Cargo project layout violations:\n{}",
        failures.join("\n")
    );
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn package_manifests(root: &Path) -> Vec<PathBuf> {
    let mut manifests = repository_files(root)
        .into_iter()
        .filter(|path| path.file_name() == Some(OsStr::new("Cargo.toml")))
        .filter(|path| has_exact_section(&read_text(path), "package"))
        .collect::<Vec<_>>();
    manifests.sort();
    manifests
}

fn rust_sources(root: &Path) -> Vec<PathBuf> {
    repository_files(root)
        .into_iter()
        .filter(|path| path.extension() == Some(OsStr::new("rs")))
        .collect()
}

fn repository_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_repository_files(root, &mut files);
    files
}

fn collect_repository_files(directory: &Path, files: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| {
            panic!(
                "failed to read an entry under {}: {error}",
                directory.display()
            )
        });
        let file_type = entry.file_type().unwrap_or_else(|error| {
            panic!("failed to inspect {}: {error}", entry.path().display())
        });
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if file_type.is_dir() {
            if !is_ignored_directory(&path) {
                collect_repository_files(&path, files);
            }
        } else if file_type.is_file() {
            files.push(path);
        }
    }
}

fn is_ignored_directory(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(OsStr::to_str),
        Some(".git" | "target")
    )
}

fn read_text(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn has_exact_section(manifest: &str, wanted: &str) -> bool {
    manifest.lines().any(|line| {
        line.split('#')
            .next()
            .is_some_and(|code| code.trim() == format!("[{wanted}]"))
    })
}

fn section_has_workspace_true(manifest: &str, wanted: &str) -> bool {
    let mut current_section = None;
    for line in manifest.lines() {
        let code = line.split('#').next().unwrap_or_default().trim();
        if code.starts_with('[') && code.ends_with(']') {
            current_section = Some(&code[1..code.len() - 1]);
            continue;
        }
        if current_section == Some(wanted) {
            let Some((key, value)) = code.split_once('=') else {
                continue;
            };
            if key.trim() == "workspace" && value.trim() == "true" {
                return true;
            }
        }
    }
    false
}

fn has_default_or_additional_target(source_root: &Path) -> bool {
    source_root.join("lib.rs").is_file()
        || source_root.join("main.rs").is_file()
        || source_root.join("bin").is_dir()
}

fn validate_target_directory(root: &Path, directory: &Path, failures: &mut Vec<String>) {
    if !directory.is_dir() {
        return;
    }
    let entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()));
    for entry in entries {
        let path = entry
            .expect("target directory entry must be readable")
            .path();
        if path.is_dir() && !path.join("main.rs").is_file() {
            failures.push(format!(
                "{}: multi-file target directories must contain main.rs",
                relative_path(root, &path)
            ));
        }
    }
}
