use cfg_if::cfg_if;

cfg_if! {
    if #[cfg(any(
        target_os = "macos",
        target_os = "ios",
    ))] {
        mod macos;
        pub use macos::*;
    } else if #[cfg(any(
        target_os = "linux",
        target_os = "android"
    ))] {
        mod linux;
        pub use linux::*;
    } else if #[cfg(windows)] {
        mod windows;
        pub use windows::*;
    } else {
        mod unknown;
        pub use unknown::*;
    }
}
