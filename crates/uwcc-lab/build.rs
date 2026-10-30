fn main() {
    slint_build::compile("ui/lab.slint").expect("Slint-UI kompilieren");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=app.rc");
        println!("cargo:rerun-if-changed=../../assets/icon.ico");
        embed_resource::compile("app.rc", embed_resource::NONE).manifest_optional().expect("Programmsymbol einbinden");
    }
}
