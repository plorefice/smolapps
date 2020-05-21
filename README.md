# smolapps

> Collection of application-layer protocols built on top of [`smoltcp`].

This crate aims to follow the same guinding principles of `smoltcp`: simplicity and robustness.
It is a `#![no_std]`-first crate, designed for bare-metal, real-time systems.
Heap allocations (if at all present) will be reduced to a minimum, and will always be feature gated.

The following protocols are implemented at this time:

* Simple Network Time Protocol (**SNTPv4**)
* Trivial File Transfer Protocol (**TFTP**)

[`smoltcp`]: https://github.com/smoltcp-rs/smoltcp

## Requirements

* Rust 1.43+

## Examples

See the [examples] directory for examples on how to use this crate in a hosted Linux environment.
Before running the examples, you need to create a TAP interface with Internet access usable by non-privileged users.

To spawn a TAP interface named `tap0`, run the following commands:

```no_rust
sudo ip tuntap add name tap0 mode tap user $USER
sudo ip link set tap0 up
sudo ip addr add 192.168.69.100/24 dev tap0
```

To forward IPv4 traffic to/from the interface, run:

```no_rust
sudo iptables -t nat -A POSTROUTING -s 192.168.69.0/24 -j MASQUERADE
sudo sysctl net.ipv4.ip_forward=1
```

Adjust the interface IP appropriately if you happen to already be on a 192.168.69.0/24 network.
If you do, remember to adjust the example accordingly.

For bare-metal examples, refer to the documentation and the [loopback example] of smoltcp.

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
