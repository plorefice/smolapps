/*! Collection of application-layer protocols built on top of [`smoltcp`].

This crate aims to follow the same guinding principles of `smoltcp`: simplicity and robustness.
It is a `#![no_std]`-first crate, designed for bare-metal, real-time systems.
Heap allocations (if at all present) are reduced to a minimum, and will always be feature gated.

The following protocols are implemented at this time:

* Simple Network Time Protocol (**SNTPv4**)

All protocols are feature-gated. This reduces both compilation time and binary size, the latter
being a strong limiting factor in bare-metal applications.

For convenience, this crate re-exports `smoltcp` under the `net` name.

[`smoltcp`]: https://github.com/smoltcp-rs/smoltcp

# Examples

Examples on how to use the protocols and their implementations in this crate can be found in the
source repository.

# Features

The following features can be enabled at the crate level:

* `sntp`

Compiles the SNTP protocol and client implementation. It has a dependency on `socket-udp`. Disabled by default.
*/

#![deny(warnings)]
#![deny(missing_docs)]
#![deny(unsafe_code)]
#![no_std]

#[cfg(any(test, feature = "std"))]
#[macro_use]
extern crate std;

#[cfg(feature = "log")]
#[macro_use(trace, debug)]
extern crate log;

// Re-export smoltcp
pub use smoltcp as net;

#[macro_use]
mod macros;
mod wire;

#[cfg(feature = "sntp")]
pub mod sntp;
