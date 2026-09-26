//! Backend-selected public API for Opus bindings.
//!
//! By default this crate uses `libopus_sys`, while `backend-mousiki` provides
//! an alternative pure-Rust implementation behind the same public API.
#![warn(missing_docs)]

#[cfg(all(feature = "backend-libopus", feature = "backend-mousiki"))]
compile_error!("features `backend-libopus` and `backend-mousiki` are mutually exclusive");

#[cfg(not(any(feature = "backend-libopus", feature = "backend-mousiki")))]
compile_error!("one of `backend-libopus` or `backend-mousiki` must be enabled");

#[cfg(all(
	feature = "backend-mousiki",
	any(feature = "static", feature = "dynamic", feature = "bundled")
))]
compile_error!(
	"features `static`, `dynamic`, and `bundled` are only supported with `backend-libopus`"
);

#[cfg(all(feature = "backend-libopus", not(feature = "backend-mousiki")))]
mod backend_libopus;
#[cfg(all(feature = "backend-mousiki", not(feature = "backend-libopus")))]
mod backend_mousiki;

#[cfg(all(feature = "backend-libopus", not(feature = "backend-mousiki")))]
pub use backend_libopus::*;
#[cfg(all(feature = "backend-mousiki", not(feature = "backend-libopus")))]
pub use backend_mousiki::*;
