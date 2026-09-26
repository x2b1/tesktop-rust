# `opus2`

Safe Rust bindings for [libopus](https://opus-codec.org/) with an optional
pure-Rust `mousiki` backend. The rustdoc
includes brief descriptions for methods, and detailed API information can be
found at the [libopus documentation][upstream docs].

[crates.io] - [docs.rs] - [upstream docs]

[crates.io]: https://crates.io/crates/opus2
[docs.rs]: https://docs.rs/opus2/
[upstream docs]: https://opus-codec.org/docs/opus_api-1.5/

## External dependencies

With the default `backend-libopus`, you need either:

* pkg-config and opus headers/libraries
* cmake, make, and a C compiler

These requirements come from [libopus_sys](https://crates.io/crates/libopus_sys), where details about overriding these defaults can be found.

## Features

- `backend-libopus` (default): Use `libopus_sys`
- `backend-mousiki`: Use the pure-Rust `mousiki` backend
- `static` (default): Statically link to libopus
- `dynamic`: Dynamically link to libopus
- `bundled`: Build and bundle libopus from source

`static`, `dynamic`, and `bundled` only apply to `backend-libopus`.

## License

Licensed under either of

 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
