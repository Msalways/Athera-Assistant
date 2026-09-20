fn main() {
    println!("cargo:rerun-if-env-changed=AETHRA_LOCAL_CHAT_LIB_DIR");
    if std::env::var_os("CARGO_FEATURE_NATIVE_FFI").is_some() {
        let directory = std::env::var("AETHRA_LOCAL_CHAT_LIB_DIR")
            .expect("AETHRA_LOCAL_CHAT_LIB_DIR must contain the built native shim");
        println!("cargo:rustc-link-search=native={directory}");
        println!("cargo:rustc-link-lib=dylib=athera_local_chat");
    }
}
