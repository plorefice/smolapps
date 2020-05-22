# smolapps

[![crates.io badge](https://img.shields.io/crates/v/smolapps.svg)](https://crates.io/crates/smolapps)
[![docs.rs badge](https://docs.rs/smolapps/badge.svg)](https://docs.rs/smolapps)
[![Build Status](https://travis-ci.com/plorefice/smolapps.svg?branch=master)](https://travis-ci.com/plorefice/smolapps)

> Collection of application-layer protocols built on top of [`smoltcp`].

This crate aims to follow the same guinding principles of `smoltcp`: simplicity and robustness.
It is a `#![no_std]`-first crate, designed for bare-metal, real-time systems.
Heap allocations (if at all present) will be reduced to a minimum, and will always be feature gated.

**This crate is still a major WIP, and as such the API may change significantly even in patch releases.**

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

The following features can be enabled at the crate level and are _enabled_ by default:

* `sntp` enables compilation of the SNTP client
* `tftp` enables compilation of the TFTP server

## License

Copyright © 2020 Pietro Lorefice

Dual licensed under your choice of either of:

* Apache License, Version 2.0, (LICENSE-APACHE or <http://www.apache.org/licenses/LICENSE-2.0)>
* MIT license (LICENSE-MIT or <http://opensource.org/licenses/MIT)>
