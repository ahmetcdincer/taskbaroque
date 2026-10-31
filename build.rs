use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/taskbaroque.ico");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("app_res.o");
    let status = Command::new("windres")
        .args(["--input", "assets/app.rc", "--output"])
        .arg(&out)
        .args(["--output-format=coff", "--include-dir", "assets"])
        .status();
    match status {
        Ok(s) if s.success() => println!("cargo:rustc-link-arg-bins={}", out.display()),
        _ => println!("cargo:warning=windres bulunamadı; uygulama simgesi gömülmedi"),
    }
}
