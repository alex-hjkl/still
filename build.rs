fn main() {
    println!("cargo:rerun-if-changed=assets/windows/still.rc");
    println!("cargo:rerun-if-changed=assets/app-icon/still.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // GPUI loads the application's icon from Windows resource ID 1.
        embed_resource::compile("assets/windows/still.rc", embed_resource::NONE)
            .manifest_required()
            .expect("Could not embed the Still app icon");
    }
}
