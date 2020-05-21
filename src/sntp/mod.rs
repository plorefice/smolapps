//! Simple Network Time Protocol client implementation.

use crate::net::{
    socket::{SocketHandle, SocketSet, UdpSocket, UdpSocketBuffer},
    time::{Duration, Instant},
    wire::{IpAddress, IpEndpoint},
    {Error, Result},
};
use crate::wire::sntp::{LeapIndicator, Packet, ProtocolMode, Repr, Stratum, Timestamp};

/// Minimum interval between requests (defaults to one minute)
const MIN_REQUEST_INTERVAL: Duration = Duration { millis: 60 * 1_000 };

/// Maximum interval between requests (defaults to one day)
const MAX_REQUEST_INTERVAL: Duration = Duration {
    millis: 24 * 60 * 60 * 1_000,
};

/// Number of seconds between 1970 and Feb 7, 2036 06:28:16 UTC (epoch 1).
/// Used for NTP to Unix timestamp conversion.
const DIFF_SEC_1970_2036: u32 = 2085978496;

/// IANA port for SNTP servers.
const SNTP_PORT: u16 = 123;

/// SNTPv4 client.
///
/// You must call `Client::poll()` after `Interface::poll()` to send
/// and receive SNTP packets.
pub struct Client {
    udp_handle: SocketHandle,
    ntp_server: IpAddress,
    /// When to send next request.
    next_request: Instant,
    /// Current timeout interval.
    curr_interval: Duration,
}

impl Client {
    /// Create a new SNTPv4 client performing requests to the specified server.
    ///
    /// # Usage
    ///
    /// ```rust
    /// use smolapps::sntp::Client;
    /// use smolapps::net::socket::{SocketSet, UdpSocketBuffer, UdpPacketMetadata};
    /// use smolapps::net::time::Instant;
    /// use smolapps::net::wire::IpAddress;
    ///
    /// let mut sockets_entries: [_; 1] = Default::default();
    /// let mut sockets = SocketSet::new(&mut sockets_entries[..]);
    ///
    /// let mut sntp_rx_storage: [u8; 128] = [0; 128];
    /// let mut sntp_rx_metadata: [_; 1] = [UdpPacketMetadata::EMPTY; 1];
    ///
    /// let mut sntp_tx_storage: [u8; 128] = [0; 128];
    /// let mut sntp_tx_metadata: [_; 1] = [UdpPacketMetadata::EMPTY; 1];
    ///
    /// let sntp_rx_buffer = UdpSocketBuffer::new(
    ///     &mut sntp_rx_metadata[..],
    ///     &mut sntp_rx_storage[..]
    /// );
    /// let sntp_tx_buffer = UdpSocketBuffer::new(
    ///     &mut sntp_tx_metadata[..],
    ///     &mut sntp_tx_storage[..],
    /// );
    ///
    /// let mut sntp = Client::new(
    ///     &mut sockets,
    ///     sntp_rx_buffer, sntp_tx_buffer,
    ///     IpAddress::v4(62, 112, 134, 4),
    ///     Instant::from_secs(0),
    /// );
    /// ```
    pub fn new<'a, 'b, 'c>(
        sockets: &mut SocketSet<'a, 'b, 'c>,
        rx_buffer: UdpSocketBuffer<'b, 'c>,
        tx_buffer: UdpSocketBuffer<'b, 'c>,
        ntp_server: IpAddress,
        now: Instant,
    ) -> Self
    where
        'b: 'c,
    {
        let socket = UdpSocket::new(rx_buffer, tx_buffer);
        let udp_handle = sockets.add(socket);

        net_trace!("SNTP initialised");

        Client {
            udp_handle,
            ntp_server,
            next_request: now,
            curr_interval: MIN_REQUEST_INTERVAL,
        }
    }

    /// Returns the duration until the next packet request.
    ///
    /// Useful for suspending execution after polling.
    pub fn next_poll(&self, now: Instant) -> Duration {
        self.next_request - now
    }

    /// Processes incoming packets, and sends SNTP requests when timeouts expire.
    ///
    /// If a valid response is received, the Unix timestamp (ie. seconds since
    /// epoch) corresponding to the received NTP timestamp is returned.
    pub fn poll(&mut self, sockets: &mut SocketSet, now: Instant) -> Result<Option<u32>> {
        let mut socket = sockets.get::<UdpSocket>(self.udp_handle);

        // Bind the socket if necessary
        if !socket.is_open() {
            socket.bind(IpEndpoint {
                addr: IpAddress::Unspecified,
                port: SNTP_PORT,
            })?;
        }

        // Process incoming packets
        let timestamp = match socket.recv() {
            Ok((payload, _)) => self.receive(payload),
            Err(Error::Exhausted) => None,
            Err(e) => return Err(e),
        };

        match timestamp {
            Some(ts) => {
                // A valid timestamp was received.
                // Increase the request interval to its maximum and return the timestamp.
                self.next_request = now + MAX_REQUEST_INTERVAL;
                Ok(Some(ts))
            }
            None if socket.can_send() && now >= self.next_request => {
                // The timeout has expired.
                // Send a request, set the timeout and increment interval using exponential backoff.
                self.request(&mut *socket)?;
                self.next_request = now + self.curr_interval;
                self.curr_interval = MAX_REQUEST_INTERVAL.min(self.curr_interval * 2);
                Ok(None)
            }
            None => Ok(None),
        }
    }

    /// Processes a response from the SNTP server.
    fn receive(&mut self, data: &[u8]) -> Option<u32> {
        let sntp_packet = match Packet::new_checked(data) {
            Ok(sntp_packet) => sntp_packet,
            Err(e) => {
                net_debug!("SNTP invalid pkt: {:?}", e);
                return None;
            }
        };
        let sntp_repr = match Repr::parse(&sntp_packet) {
            Ok(sntp_repr) => sntp_repr,
            Err(e) => {
                net_debug!("SNTP error parsing pkt: {:?}", e);
                return None;
            }
        };

        if sntp_repr.protocol_mode != ProtocolMode::Server {
            net_debug!(
                "Invalid mode in SNTP response: {:?}",
                sntp_repr.protocol_mode
            );
            return None;
        }
        if sntp_repr.stratum == Stratum::KissOfDeath {
            net_debug!("SNTP kiss o' death received, doing nothing");
            return None;
        }

        // Perform conversion from NTP timestamp to Unix timestamp
        let timestamp = sntp_repr
            .xmit_timestamp
            .sec
            .wrapping_add(DIFF_SEC_1970_2036);

        Some(timestamp)
    }

    /// Sends a request to the configured SNTP ntp_server.
    fn request(&mut self, socket: &mut UdpSocket) -> Result<()> {
        let sntp_repr = Repr {
            leap_indicator: LeapIndicator::NoWarning,
            version: 4,
            protocol_mode: ProtocolMode::Client,
            stratum: Stratum::KissOfDeath,
            poll_interval: 0,
            precision: 0,
            root_delay: 0,
            root_dispersion: 0,
            ref_identifier: [0, 0, 0, 0],
            ref_timestamp: Timestamp { sec: 0, frac: 0 },
            orig_timestamp: Timestamp { sec: 0, frac: 0 },
            recv_timestamp: Timestamp { sec: 0, frac: 0 },
            xmit_timestamp: Timestamp { sec: 0, frac: 0 },
        };

        let endpoint = IpEndpoint {
            addr: self.ntp_server,
            port: SNTP_PORT,
        };

        net_trace!("SNTP send request to {}: {:?}", endpoint, sntp_repr);

        let mut packet = socket.send(sntp_repr.buffer_len(), endpoint)?;
        let mut sntp_packet = Packet::new_unchecked(&mut packet);
        sntp_repr.emit(&mut sntp_packet)?;

        Ok(())
    }
}
