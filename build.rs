use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let skills_root = manifest_dir.join("skills").join("system");
    println!("cargo:rerun-if-changed={}", skills_root.display());

    let mut packages = Vec::new();
    if skills_root.is_dir() {
        collect_skill_packages(&skills_root, &skills_root, &mut packages);
    }
    packages.sort_by(|left, right| left.bundle_path.cmp(&right.bundle_path));

    let mut output = String::from("const BUNDLED_SKILLS: &[BundledSkillPackage] = &[\n");
    for package in packages {
        output.push_str("    BundledSkillPackage {\n");
        output.push_str(&format!(
            "        bundle_path: {},\n",
            rust_string_literal(&package.bundle_path)
        ));
        output.push_str("        files: &[\n");
        for file in package.files {
            output.push_str("            BundledSkillFile {\n");
            output.push_str(&format!(
                "                relative_path: {},\n",
                rust_string_literal(&file.relative_path)
            ));
            output.push_str(&format!(
                "                contents: include_bytes!({}),\n",
                rust_string_literal(&file.absolute_path.display().to_string())
            ));
            output.push_str("            },\n");
        }
        output.push_str("        ],\n");
        output.push_str("    },\n");
    }
    output.push_str("];\n");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    fs::write(out_dir.join("bundled_skills.rs"), output).expect("write bundled_skills.rs");
}

struct Package {
    bundle_path: String,
    files: Vec<PackageFile>,
}

struct PackageFile {
    relative_path: String,
    absolute_path: PathBuf,
}

fn collect_skill_packages(root: &Path, current: &Path, packages: &mut Vec<Package>) {
    if current.join("SKILL.md").is_file() {
        packages.push(package_from_dir(root, current));
        return;
    }

    let Ok(entries) = fs::read_dir(current) else {
        return;
    };
    let mut dirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if hidden_path_component(&path) {
            continue;
        }
        if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            dirs.push(path);
        }
    }
    dirs.sort();
    for dir in dirs {
        collect_skill_packages(root, &dir, packages);
    }
}

fn package_from_dir(root: &Path, package_root: &Path) -> Package {
    let bundle_path = slash_path(package_root.strip_prefix(root).unwrap_or(package_root));
    let mut files = Vec::new();
    collect_package_files(package_root, package_root, &mut files);
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Package { bundle_path, files }
}

fn collect_package_files(package_root: &Path, current: &Path, files: &mut Vec<PackageFile>) {
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if hidden_path_component(&path) {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            collect_package_files(package_root, &path, files);
        } else if file_type.is_file() {
            files.push(PackageFile {
                relative_path: slash_path(path.strip_prefix(package_root).unwrap_or(&path)),
                absolute_path: path,
            });
        }
    }
}

fn hidden_path_component(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

fn slash_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn rust_string_literal(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => out.push_str(&format!("\\u{{{:x}}}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}
