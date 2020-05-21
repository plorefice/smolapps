/*! Collection of application-layer protocols built on top of [`smoltcp`].

This crate aims to follow the same guinding principles of `smoltcp`: simplicity and robustness.
It is a `#![no_std]`-first crate, designed for bare-metal, real-time systems.
Heap allocations (if at all present) are reduced to a minimum, and will always be feature gated.

The following protocols are implemented at this time:

* Simple Network Time Protocol (**SNTPv4**)
* Trivial File Transfer Protocol (**TFTP**)

All protocols are feature-gated. This reduces both compilation time and binary size, the latter
being a strong limiting factor in bare-metal applications.

For convenience, this crate re-exports `smoltcp` under the `net` name.

[`smoltcp`]: https://github.com/smoltcp-rs/smoltcp

# Examples

Examples on how to use the protocols and their implementations in this crate can be found in the
source repository. All examples are provided to run on a hosted Linux environment.

Before running the examples, you need to create a TAP interface with Internet access
usable by non-privileged users.

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

# Features

The following features can be enabled at the crate level:

## `sntp`

Compiles the SNTP protocol and client implementation. It has a dependency on `socket-udp`. Disabled by default.

## `tftp`

Compiles the TFTP protocol and server implementation. It has a dependency on `socket-udp`. Disabled by default.
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

#[cfg(feature = "tftp")]
pub mod tftp;
