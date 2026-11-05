use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=packaging/windows");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let version = env::var("CARGO_PKG_VERSION").expect("package version");
    let parts: Vec<u16> = version
        .split('.')
        .map(|part| {
            part.parse()
                .expect("Windows resources require a numeric version")
        })
        .collect();
    assert_eq!(
        parts.len(),
        3,
        "Windows resources require major.minor.patch"
    );
    let numbers = format!("{},{},{},0", parts[0], parts[1], parts[2]);
    let manifest = fs::read_to_string("packaging/windows/application.manifest.in")
        .expect("read application manifest")
        .replace("@VERSION@", &format!("{version}.0"));
    fs::write(out.join("application.manifest"), manifest).expect("write manifest");
    fs::copy("packaging/windows/klicky.ico", out.join("klicky.ico")).expect("copy icon");
    let template =
        fs::read_to_string("packaging/windows/application.rc.in").expect("read resource template");
    for bin in ["klicky", "klicky-desktop"] {
        let resource = template
            .replace("@VERSION_NUMBERS@", &numbers)
            .replace("@VERSION@", &version)
            .replace("@BINARY@", bin);
        let path = out.join(format!("{bin}.rc"));
        fs::write(&path, resource).expect("write resource script");
        embed_resource::compile_for(&path, [bin], embed_resource::NONE)
            .manifest_required()
            .expect("Windows SDK resource compilation is required");
    }
}
