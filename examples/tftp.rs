/*! Example of a TFTP server serving files from the root of the filesystem.

This example only works on Linux. See the documentation for a step-by-step guide
on how to setup your machine to run this example.

Finally, run the example with:

```no_rust
cargo run --example tftp --features "tftp tap"
```

You should now be able to connect to `192.168.69.1:69` using a TFTP client
and read/write files from/to your filesystem.
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
    tftp::{Context, Handle, Server},
};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    os::unix::io::AsRawFd,
};

struct RootFilesystem;

impl Context for RootFilesystem {
    type Handle = File;

    fn open(&mut self, filename: &str, write_mode: bool) -> Result<Self::Handle, ()> {
        fs::OpenOptions::new()
            .read(true)
            .write(write_mode)
            .open(filename)
            .map(File)
            .map_err(|_| ())
    }

    fn close(&mut self, mut handle: Self::Handle) {
        handle.0.flush().ok();
    }
}

struct File(fs::File);

impl Handle for File {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()> {
        self.0.read(buf).map_err(|_| ())
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, ()> {
        self.0.write(buf).map_err(|_| ())
    }
}

fn main() {
    env_logger::from_env(Env::default().default_filter_or("trace")).init();

    let device = TapInterface::new("tap0").unwrap();
    let fd = device.as_raw_fd();

    let mut sockets = SocketSet::new(vec![]);

    let neighbor_cache = NeighborCache::new(BTreeMap::new());
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

    let mut tftp = Server::new(
        &mut sockets,
        UdpSocketBuffer::new([UdpPacketMetadata::EMPTY; 2], vec![0; 1032]),
        UdpSocketBuffer::new([UdpPacketMetadata::EMPTY; 2], vec![0; 1032]),
        Instant::now(),
    );

    let mut transfers = vec![].into();

    loop {
        let timestamp = Instant::now();

        iface.poll(&mut sockets, timestamp).ok();

        if let Err(e) = tftp.serve(&mut sockets, &mut RootFilesystem, &mut transfers, timestamp) {
            error!("TFTP error: {}", e);
        };

        let mut timeout = tftp.next_poll(timestamp);

        iface
            .poll_delay(&sockets, timestamp)
            .map(|sockets_timeout| timeout = sockets_timeout);

        phy_wait(fd, Some(timeout)).unwrap_or_else(|e| error!("Wait error: {}", e));
    }
}
