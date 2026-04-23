use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=tsconfig.json");
    println!("cargo:rerun-if-changed=package.json");
    println!("cargo:rerun-if-changed=package-lock.json");

    let ui_dir = manifest_dir().join("benches").join("all_websites").join("ui");
    emit_rerun_if_changed(&ui_dir);

    let tsc = tsc_path();
    let status = Command::new(&tsc)
        .arg("-p")
        .arg("tsconfig.json")
        .current_dir(manifest_dir())
        .status()
        .unwrap_or_else(|err| panic!(
            "Failed to run `{}`: {}. Did you run `npm install` at the repo root?",
            tsc.display(),
            err,
        ));

    if !status.success() {
        panic!("TypeScript compilation failed with status {status}");
    }
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR was not set"))
}

fn tsc_path() -> PathBuf {
    let mut path = manifest_dir();
    path.push("node_modules");
    path.push(".bin");
    path.push(tsc_binary_name());
    path
}

fn tsc_binary_name() -> OsString {
    if cfg!(windows) {
        OsString::from("tsc.cmd")
    } else {
        OsString::from("tsc")
    }
}

fn emit_rerun_if_changed(path: &Path) {
    if path.is_file() {
        println!("cargo:rerun-if-changed={}", path.display());
        return;
    }

    let entries = fs::read_dir(path).unwrap_or_else(|err| panic!(
        "Failed to read {}: {}",
        path.display(),
        err,
    ));
    for entry in entries {
        let entry = entry.unwrap_or_else(|err| panic!(
            "Failed to read an entry under {}: {}",
            path.display(),
            err,
        ));
        emit_rerun_if_changed(&entry.path());
    }
}
