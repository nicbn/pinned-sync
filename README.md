# Pinned synchronization primitives for Rust

[![crates.io](https://img.shields.io/crates/v/pinned_sync.svg)](https://crates.io/crates/pinned_sync) [![docs.rs](https://docs.rs/pinned_sync/badge.svg)](https://docs.rs/pinned_sync)

This crate implements [pinned synchronization primitives](https://github.com/rust-lang/rfcs/pull/3124).

## Limitations

As this is only a proof-of-concept and the goal is to have this in `std`,
where we can better integrate with `std` codebase, there are some limitations
to this crate, for example:

- There is redundancy all around.
- The guards for mutex and rwlock are not the same as the ones from `std`.
- Therefore, we can not integrate `Condvar` with `std` primitives, or
`std` `Condvar` with new primitives.

Tests and documentations are mostly copy-pasted from the `std` library.

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
