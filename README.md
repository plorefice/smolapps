# smolapps

[![crates.io badge](https://img.shields.io/crates/v/smolapps.svg)](https://crates.io/crates/smolapps)
[![docs.rs badge](https://docs.rs/smolapps/badge.svg)](https://docs.rs/smolapps)

> Collection of application-layer protocols built on top of [`smoltcp`].

This crate aims to follow the same guinding principles of `smoltcp`: simplicity and robustness.
It is a `#![no_std]`-first crate, designed for bare-metal, real-time systems.
Heap allocations (if at all present) will be reduced to a minimum, and will always be feature gated.

The following protocols are implemented at this time:

* Simple Network Time Protocol (**SNTPv4**, client only)
* Trivial File Transfer Protocol (**TFTP**, server only)

[`smoltcp`]: https://github.com/smoltcp-rs/smoltcp

## Requirements

* Rust 1.43+

## Examples

See the [examples] directory for examples and instructions on how to use this crate
in a hosted Linux environment. For bare-metal examples, refer to the documentation
and the [loopback example] of smoltcp.

[examples]: examples/
[loopback example]: https://github.com/smoltcp-rs/smoltcp/blob/master/examples/loopback.rs

## Features

The following features can be enabled at the crate level and are _disabled_ by default:

* `sntp` to enable compilation of the SNTP client
* `tftp` to enable compilation of the TFTP server

## License

Copyright © 2020 Pietro Lorefice

Dual licensed under your choice of either of:

* Apache License, Version 2.0, (LICENSE-APACHE or <http://www.apache.org/licenses/LICENSE-2.0)>
* MIT license (LICENSE-MIT or <http://opensource.org/licenses/MIT)>
