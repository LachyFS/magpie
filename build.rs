fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/windows.rc");
        println!("cargo:rerun-if-changed=assets/magpie.ico");
        embed_resource::compile("assets/windows.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("Could not embed the Magpie icon");
    }
}
