/*! Low-level packet access and construction.

The `wire` module deals with the packet *representation*.
Refer to the [module-level documentation] in `smoltcp` for additional details.
*/

#[cfg(feature = "sntp")]
pub(crate) mod sntp;
