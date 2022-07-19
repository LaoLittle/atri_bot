fn main() {
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-arg=-install_name");
        println!("cargo:rustc-link-arg=@rpath/libasynclib.dylib");
    }
}