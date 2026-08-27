use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_RUST_SOURCE_LINES: usize = 1_000;
const COMPATIBILITY_TARGET_NAMES: &[&str] = &["terrain_viewer"];

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
fn rust_sources_respect_repository_limits_without_lint_bypasses() {
    let root = workspace_root();
    let mut oversized = Vec::new();
    let mut bypasses = Vec::new();

    for source in rust_sources(&root) {
        let contents = read_text(&source);
        let relative = relative_path(&root, &source);
        let line_count = contents.lines().count();
        if line_count > MAX_RUST_SOURCE_LINES {
            oversized.push(format!("{relative}: {line_count} lines"));
        }
        for (index, line) in contents.lines().enumerate() {
            if is_lint_bypass(line) {
                bypasses.push(format!("{relative}:{}: {}", index + 1, line.trim()));
            }
        }
    }

    assert!(
        oversized.is_empty(),
        "Rust source files may contain at most {MAX_RUST_SOURCE_LINES} lines:\n{}",
        oversized.join("\n")
    );
    assert!(
        bypasses.is_empty(),
        "lint bypass attributes are not permitted; fix the reported lint instead:\n{}",
        bypasses.join("\n")
    );
    assert_no_custom_size_thresholds(&root);
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
        validate_integration_tests(&root, &package_root.join("tests"), &mut failures);
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

fn is_lint_bypass(line: &str) -> bool {
    let compact = line
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    (compact.starts_with("#[") || compact.starts_with("#!["))
        && (compact.contains("allow(") || compact.contains("expect("))
}

fn assert_no_custom_size_thresholds(root: &Path) {
    let mut failures = Vec::new();
    for path in repository_files(root).into_iter().filter(|path| {
        matches!(
            path.file_name().and_then(OsStr::to_str),
            Some("clippy.toml" | ".clippy.toml")
        )
    }) {
        for (index, line) in read_text(&path).lines().enumerate() {
            if line.contains("too-many-lines-threshold") {
                failures.push(format!(
                    "{}:{}: {}",
                    relative_path(root, &path),
                    index + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "custom `too_many_lines` thresholds are not permitted:\n{}",
        failures.join("\n")
    );
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
        if path.is_file() && path.extension() == Some(OsStr::new("rs")) {
            validate_target_name(root, &path, failures);
        } else if path.is_dir() {
            if !path.join("main.rs").is_file() {
                failures.push(format!(
                    "{}: multi-file target directories must contain main.rs",
                    relative_path(root, &path)
                ));
            }
            validate_target_name(root, &path, failures);
        }
    }
}

fn validate_integration_tests(root: &Path, directory: &Path, failures: &mut Vec<String>) {
    if !directory.is_dir() {
        return;
    }
    let entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()));
    for entry in entries {
        let path = entry.expect("test directory entry must be readable").path();
        if path.is_file() && path.extension() == Some(OsStr::new("rs")) {
            validate_target_name(root, &path, failures);
        }
    }
}

fn validate_target_name(root: &Path, path: &Path, failures: &mut Vec<String>) {
    let name = if path.is_dir() {
        path.file_name().and_then(OsStr::to_str)
    } else {
        path.file_stem().and_then(OsStr::to_str)
    };
    let Some(name) = name else {
        failures.push(format!(
            "{}: target name is not UTF-8",
            relative_path(root, path)
        ));
        return;
    };
    if !is_kebab_case(name) && !COMPATIBILITY_TARGET_NAMES.contains(&name) {
        failures.push(format!(
            "{}: new Cargo target names must use kebab-case",
            relative_path(root, path)
        ));
    }
}

fn is_kebab_case(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}
