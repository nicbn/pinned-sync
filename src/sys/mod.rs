cfg_if::cfg_if! {
    if #[cfg(unix)] {
        mod unix;
        pub use unix::*;
    } else {
        mod fallback;
        pub use fallback::*;
    }
}
