fn main() {
    println!("cargo:rerun-if-env-changed=NEEDLE_LIB_DIR");
    if std::env::var_os("CARGO_FEATURE_NATIVE_STATIC").is_some() {
        let directory = std::env::var("NEEDLE_LIB_DIR")
            .expect("native-static requires the verified Needle 2 library directory");
        use sha2::{Digest, Sha256};
        let archive = std::path::Path::new(&directory).join("libneedle.a");
        println!("cargo:rerun-if-changed={}", archive.display());
        let hash = format!(
            "{:x}",
            Sha256::digest(std::fs::read(archive).expect("Needle archive missing"))
        );
        assert_eq!(
            hash, "93738ae3a9488cbc3104eb65bf49093683c26eb01b0d257e980e499d1d06a9f4",
            "Needle archive checksum mismatch"
        );
        println!("cargo:rustc-link-search=native={directory}");
        println!("cargo:rustc-link-lib=static=needle");
        if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
            println!("cargo:rustc-link-lib=c++_shared");
            println!("cargo:rustc-link-lib=log");
        }
    }
}
