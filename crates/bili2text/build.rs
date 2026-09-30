use std::env;

fn main() {
    // macOS：sherpa-rs（transcribe feature）以 shared dylib 链接，构建脚本会把
    // libonnxruntime / libsherpa-onnx-c-api 拷贝到可执行文件旁。cargo install 的
    // 产物需要 @executable_path rpath 才能在运行时找到它们（否则 dyld 直接崩）。
    // 轻量构建（无 transcribe）不链接这些 dylib，无需 rpath。
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "macos" && env::var("CARGO_FEATURE_TRANSCRIBE").is_ok() {
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
