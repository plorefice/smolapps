/*! Example of using the SNTP client to obtain the current timestamp from an actual time server.

This example only works on Linux. See the documentation for a step-by-step guide
on how to setup your machine to run this example.

Finally, run the example with:

```no_rust
cargo run --example sntp --features "sntp tap"
```

You should see something like `SNTP timestamp received: 1589793181` printed to stdout.
*/

#[macro_use]
extern crate log;

use env_logger::Env;
use smolapps::{
    net::iface::{EthernetInterfaceBuilder, NeighborCache, Routes},
    net::phy::{wait as phy_wait, TapInterface},
    net::socket::{SocketSet, UdpPacketMetadata, UdpSocketBuffer},
    net::time::Instant,
    net::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address},
    sntp::Client,
};
use std::collections::BTreeMap;
use std::os::unix::io::AsRawFd;
use std::str::FromStr;

fn main() {
    env_logger::from_env(Env::default().default_filter_or("info")).init();

    let device = TapInterface::new("tap0").unwrap();
    let fd = device.as_raw_fd();

    let server = IpAddress::from_str("62.112.134.4").expect("invalid address format");

    let neighbor_cache = NeighborCache::new(BTreeMap::new());

    let sntp_rx_buffer = UdpSocketBuffer::new([UdpPacketMetadata::EMPTY; 1], vec![0; 900]);
    let sntp_tx_buffer = UdpSocketBuffer::new([UdpPacketMetadata::EMPTY; 1], vec![0; 600]);
    let mut sockets = SocketSet::new(vec![]);

    let ethernet_addr = EthernetAddress([0x02, 0x00, 0x00, 0x00, 0x00, 0x02]);
    let ip_addrs = [IpCidr::new(IpAddress::v4(192, 168, 69, 1), 24)];
    let default_v4_gw = Ipv4Address::new(192, 168, 69, 100);

    let mut routes_storage = [None; 1];
    let mut routes = Routes::new(&mut routes_storage[..]);
    routes.add_default_ipv4_route(default_v4_gw).unwrap();

    let mut iface = EthernetInterfaceBuilder::new(device)
        .ethernet_addr(ethernet_addr)
        .neighbor_cache(neighbor_cache)
        .ip_addrs(ip_addrs)
        .routes(routes)
        .finalize();

    let mut sntp = Client::new(
        &mut sockets,
        sntp_rx_buffer,
        sntp_tx_buffer,
        server,
        Instant::now(),
    );

    loop {
        let timestamp = Instant::now();

        iface.poll(&mut sockets, timestamp).map(|_| ()).ok();

        let network_time = sntp.poll(&mut sockets, timestamp).unwrap_or_else(|e| {
            error!("SNTP error: {}", e);
            None
        });

        if let Some(t) = network_time {
            info!("SNTP timestamp received: {:?}", t);
        }

        let mut timeout = sntp.next_poll(timestamp);

        iface
            .poll_delay(&sockets, timestamp)
            .map(|sockets_timeout| timeout = sockets_timeout);

        phy_wait(fd, Some(timeout)).unwrap_or_else(|e| error!("Wait error: {}", e));
    }
}
