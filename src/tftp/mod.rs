//! Trivial File Transfer Protocol server implementation.

use crate::net::{
    self,
    socket::{SocketHandle, SocketSet, UdpSocket, UdpSocketBuffer},
    time::{Duration, Instant},
    wire::{IpAddress, IpEndpoint},
    Error,
};
use crate::wire::tftp::*;
use managed::ManagedSlice;

/// Maximum number of retransmissions attempted by the server before giving up.
const MAX_RETRIES: u8 = 10;

/// Interval between consecutive retries in case of no answer.
const RETRY_TIMEOUT: Duration = Duration { millis: 200 };

/// IANA port for TFTP servers.
const TFTP_PORT: u16 = 69;

/// The context over which the [`Server`] will operate.
///
/// The context allows the [`Server`] to open and close [`Handle`]s to files.
/// It does not impose any restriction on the context hierarchy: it could be a flat
/// structure or implement a directory tree. It is up to the implementors to define,
/// if required, the concepts of path separators and nesting levels.
///
/// [`Server`]: struct.Server.html
/// [`Handle`]: trait.Handle.html
pub trait Context {
    /// The `Handle` type used by this `Context`.
    type Handle: Handle;

    /// Attempts to open a file in read-only mode if `write_mode` is `false`,
    /// otherwise in read-write mode.
    ///
    /// The `filename` contained in the request packet is provided as-is: no modifications
    /// are applied besides stripping the NULL terminator.
    fn open(&mut self, filename: &str, write_mode: bool) -> Result<Self::Handle, ()>;

    /// Closes the file handle, flushing all pending changes to disk if necessary.
    fn close(&mut self, handle: Self::Handle);
}

/// An open file handle returned by a [`Context::open()`] operation.
///
/// [`Context::open()`]: trait.Context.html#tymethod.open
pub trait Handle {
    /// Pulls some bytes from this handle into the specified buffer, returning how many bytes were read.
    ///
    /// `buf` is guaranteed to be exactly 512 bytes long, the maximum packet size allowed by the protocol.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()>;

    /// Writes a buffer into this handle's buffer, returning how many bytes were written.
    ///
    /// `buf` can be anywhere from 0 to 512 bytes long.
    fn write(&mut self, buf: &[u8]) -> Result<usize, ()>;
}

/// TFTP server.
pub struct Server {
    udp_handle: SocketHandle,
    next_poll: Instant,
}

impl Server {
    /// Creates a TFTP server.
    ///
    /// A new socket will be allocated and added to the provided `SocketSet`.
    ///
    /// # Usage
    ///
    /// ```rust
    /// use smolapps::tftp::Server;
    /// use smolapps::net::socket::{SocketSet, UdpSocketBuffer, UdpPacketMetadata};
    /// use smolapps::net::time::Instant;
    /// use smolapps::net::wire::IpAddress;
    ///
    /// let mut sockets_entries: [_; 1] = Default::default();
    /// let mut sockets = SocketSet::new(&mut sockets_entries[..]);
    ///
    /// let mut tftp_rx_storage: [u8; 1048] = [0; 1048];
    /// let mut tftp_rx_metadata: [_; 2] = [UdpPacketMetadata::EMPTY; 2];
    ///
    /// let mut tftp_tx_storage: [u8; 1048] = [0; 1048];
    /// let mut tftp_tx_metadata: [_; 2] = [UdpPacketMetadata::EMPTY; 2];
    ///
    /// let tftp_rx_buffer = UdpSocketBuffer::new(
    ///     &mut tftp_rx_metadata[..],
    ///     &mut tftp_rx_storage[..]
    /// );
    /// let tftp_tx_buffer = UdpSocketBuffer::new(
    ///     &mut tftp_tx_metadata[..],
    ///     &mut tftp_tx_storage[..],
    /// );
    ///
    /// let mut tftp = Server::new(
    ///     &mut sockets,
    ///     tftp_rx_buffer,
    ///     tftp_tx_buffer,
    ///     Instant::from_secs(0),
    /// );
    /// ```
    pub fn new<'a, 'b, 'c>(
        sockets: &mut SocketSet<'a, 'b, 'c>,
        rx_buffer: UdpSocketBuffer<'b, 'c>,
        tx_buffer: UdpSocketBuffer<'b, 'c>,
        now: Instant,
    ) -> Self {
        let socket = UdpSocket::new(rx_buffer, tx_buffer);
        let udp_handle = sockets.add(socket);

        net_trace!("TFTP initialised");

        Server {
            udp_handle,
            next_poll: now,
        }
    }

    /// Returns the duration until the next poll activity.
    ///
    /// Useful for suspending execution after polling.
    pub fn next_poll(&self, now: Instant) -> Duration {
        self.next_poll - now
    }

    /// Serves files from the provided context and manages any active transfers.
    ///
    /// This function must be called after `Interface::poll()` to handle packed transmission
    /// and reception. File errors are handled internally by relaying an error packet to the client
    /// and terminating the transfer, if necessary.
    ///
    /// The `context` and the active `transfers` need to be persisted across calls to this function.
    pub fn serve<'a, C>(
        &mut self,
        sockets: &mut SocketSet,
        context: &mut C,
        transfers: &mut ManagedSlice<'a, Option<Transfer<C::Handle>>>,
        now: Instant,
    ) -> net::Result<()>
    where
        C: Context,
    {
        let mut socket = sockets.get::<UdpSocket>(self.udp_handle);

        // Bind the socket if necessary
        if !socket.is_open() {
            socket.bind(IpEndpoint {
                addr: IpAddress::Unspecified,
                port: TFTP_PORT,
            })?;
        }

        // Schedule next activation
        self.next_poll = now + Duration::from_millis(50);

        // Process incoming packets
        match socket.recv() {
            Ok((data, ep)) => {
                // Validate packet length
                let tftp_packet = match Packet::new_checked(data) {
                    Ok(tftp_packet) => tftp_packet,
                    Err(_) => {
                        send_error(
                            &mut *socket,
                            ep,
                            ErrorCode::AccessViolation,
                            "Packet truncated",
                        )?;
                        return Ok(());
                    }
                };

                // Validate packet contents
                let tftp_repr = match Repr::parse(&tftp_packet) {
                    Ok(tftp_repr) => tftp_repr,
                    Err(_) => {
                        return send_error(
                            &mut *socket,
                            ep,
                            ErrorCode::AccessViolation,
                            "Malformed packet",
                        );
                    }
                };

                // Retrieve the index of the transfer associated to the remote endpoint
                let xfer_idx = transfers.iter_mut().position(|xfer| {
                    if let Some(xfer) = xfer {
                        if xfer.ep == ep {
                            return true;
                        }
                    }
                    false
                });

                let is_write = tftp_packet.opcode() == OpCode::Write;

                match (tftp_repr, xfer_idx) {
                    (Repr::ReadRequest { .. }, Some(_)) | (Repr::WriteRequest { .. }, Some(_)) => {
                        // Multiple connections from the same host are not supported
                        net_debug!("tftp: multiple connection attempts from {}", ep);

                        return send_error(
                            &mut *socket,
                            ep,
                            ErrorCode::AccessViolation,
                            "Multiple connections not supported",
                        );
                    }
                    (Repr::ReadRequest { filename, mode, .. }, None)
                    | (Repr::WriteRequest { filename, mode, .. }, None) => {
                        if mode != Mode::Octet {
                            return send_error(
                                &mut *socket,
                                ep,
                                ErrorCode::IllegalOperation,
                                "Only octet mode is supported",
                            );
                        }

                        // Find the first free transfer available, or allocate one if possible
                        let opt_idx =
                            transfers.iter().position(|t| t.is_none()).or_else(
                                || match transfers {
                                    ManagedSlice::Borrowed(_) => None,
                                    #[cfg(feature = "std")]
                                    ManagedSlice::Owned(v) => {
                                        let idx = v.len();
                                        v.push(None);
                                        Some(idx)
                                    }
                                },
                            );

                        if let Some(idx) = opt_idx {
                            // Open file handle
                            let handle = match context.open(filename, is_write) {
                                Ok(handle) => handle,
                                Err(_) => {
                                    net_debug!("tftp: unable to open requested file");
                                    return send_error(
                                        &mut *socket,
                                        ep,
                                        ErrorCode::FileNotFound,
                                        "Unable to open requested file",
                                    );
                                }
                            };

                            // Allocate new transfer
                            let mut xfer = Transfer {
                                handle,
                                ep,
                                is_write,
                                block_num: 1,
                                last_data: None,
                                last_len: 0,
                                retries: 0,
                                timeout: now + Duration::from_millis(50),
                            };

                            net_debug!(
                                "tftp: {} request from {}",
                                if is_write { "write" } else { "read" },
                                ep
                            );

                            if is_write {
                                xfer.send_ack(&mut *socket, 0)?;
                            } else {
                                xfer.send_data(&mut *socket)?;
                            }

                            // Enque transfer
                            transfers[idx] = Some(xfer);
                        } else {
                            // Exhausted transfers buffer
                            net_debug!("tftp: connections exhausted");

                            return send_error(
                                &mut *socket,
                                ep,
                                ErrorCode::AccessViolation,
                                "No more available connections",
                            );
                        }
                    }
                    (Repr::Data { .. }, None) | (Repr::Ack { .. }, None) => {
                        // Data request on unconnected socket
                        return send_error(
                            &mut *socket,
                            ep,
                            ErrorCode::AccessViolation,
                            "Data packet without active transfer",
                        );
                    }
                    (Repr::Data { block_num, data }, Some(idx)) => {
                        let xfer = transfers[idx].as_mut().unwrap();

                        // Reset retransmission counter
                        xfer.timeout = now + RETRY_TIMEOUT;
                        xfer.retries = 0;

                        // Make sure this is a write connection
                        if !xfer.is_write {
                            return send_error(
                                &mut *socket,
                                ep,
                                ErrorCode::AccessViolation,
                                "Not a write connection",
                            );
                        }

                        // Unexpected packet, resend ACK for (block_num - 1)
                        if block_num != xfer.block_num {
                            return xfer.send_ack(&mut *socket, xfer.block_num - 1);
                        }

                        // Update block number
                        xfer.block_num += 1;

                        // Write data to the destination file
                        match xfer.handle.write(data) {
                            Ok(_) => {
                                let last_block = data.len() < 512;

                                // Send ACK and optionally close the transfer
                                xfer.send_ack(&mut *socket, block_num)?;
                                if last_block {
                                    self.close_transfer(context, &mut transfers[idx]);
                                }
                            }
                            Err(_) => {
                                send_error(
                                    &mut *socket,
                                    ep,
                                    ErrorCode::AccessViolation,
                                    "Error writing file",
                                )?;
                                self.close_transfer(context, &mut transfers[idx]);
                            }
                        }
                    }
                    (Repr::Ack { block_num }, Some(idx)) => {
                        let xfer = transfers[idx].as_mut().unwrap();

                        // Reset retransmission counter
                        xfer.timeout = now + RETRY_TIMEOUT;
                        xfer.retries = 0;

                        // Make sure this is a read connection
                        if xfer.is_write {
                            return send_error(
                                &mut *socket,
                                ep,
                                ErrorCode::AccessViolation,
                                "Not a read connection",
                            );
                        }

                        // Unexpected ACK, resend previous block
                        if block_num != xfer.block_num {
                            return xfer.resend_data(&mut *socket);
                        }

                        // Update block number
                        xfer.block_num += 1;

                        if xfer.last_len == 512 {
                            xfer.send_data(&mut *socket)?;
                        } else {
                            self.close_transfer(context, &mut transfers[idx]);
                        }
                    }
                    (Repr::Error { .. }, _) => {
                        return send_error(
                            &mut *socket,
                            ep,
                            ErrorCode::IllegalOperation,
                            "Unknown operation",
                        );
                    }
                }

                Ok(())
            }
            Err(Error::Exhausted) => {
                // Nothing to receive, process outgoing packets
                if socket.can_send() && now >= self.next_poll {
                    for xfer in transfers.iter_mut() {
                        let do_drop = if let Some(xfer) = xfer {
                            xfer.process_timeout(&mut socket, now)?
                        } else {
                            false
                        };

                        if do_drop {
                            self.close_transfer(context, xfer);
                        }
                    }
                }
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Terminates a transfer, releasing the handle and freeing up the transfer slot.
    fn close_transfer<C>(&mut self, context: &mut C, xfer: &mut Option<Transfer<C::Handle>>)
    where
        C: Context,
    {
        if let Some(xfer) = xfer.take() {
            net_debug!("tftp: closing {}", xfer.ep);
            context.close(xfer.handle);
        }
    }
}

/// An active TFTP transfer.
pub struct Transfer<H> {
    handle: H,
    ep: IpEndpoint,

    is_write: bool,
    block_num: u16,
    // FIXME: I'd reeeally love to avoid a potential stack allocation this big :\
    last_data: Option<[u8; 512]>,
    last_len: usize,

    retries: u8,
    timeout: Instant,
}

impl<H> Transfer<H>
where
    H: Handle,
{
    fn process_timeout(&mut self, socket: &mut UdpSocket, now: Instant) -> net::Result<bool> {
        if now >= self.timeout && self.retries < MAX_RETRIES {
            self.retries += 1;
            self.resend_data(socket).map(|_| false)
        } else {
            net_debug!("tftp: connection timeout");
            Ok(true)
        }
    }

    fn send_data(&mut self, socket: &mut UdpSocket) -> net::Result<bool> {
        // Allocate data
        if self.last_data.is_none() {
            self.last_data = Some([0; 512]);
        }

        // Read next chunk
        self.last_len = match self.handle.read(&mut self.last_data.as_mut().unwrap()[..]) {
            Ok(n) => n,
            Err(_) => {
                send_error(
                    socket,
                    self.ep,
                    ErrorCode::AccessViolation,
                    "Error occurred while reading the file",
                )?;
                return Ok(false);
            }
        };

        self.resend_data(socket).map(|_| false)
    }

    fn resend_data(&mut self, socket: &mut UdpSocket) -> net::Result<()> {
        if let Some(last_data) = &self.last_data {
            net_trace!("tftp: sending data block #{}", self.block_num);

            let data = Repr::Data {
                block_num: self.block_num,
                data: &last_data[..self.last_len],
            };
            let payload = socket.send(data.buffer_len(), self.ep)?;
            let mut pkt = Packet::new_unchecked(payload);
            data.emit(&mut pkt)?;
        }
        Ok(())
    }

    fn send_ack(&mut self, socket: &mut UdpSocket, block: u16) -> net::Result<()> {
        net_trace!("tftp: sending ack #{}", block);

        let ack = Repr::Ack { block_num: block };
        let payload = socket.send(ack.buffer_len(), self.ep)?;
        let mut pkt = Packet::new_unchecked(payload);
        ack.emit(&mut pkt)
    }
}

fn send_error(
    socket: &mut UdpSocket,
    ep: IpEndpoint,
    code: ErrorCode,
    msg: &str,
) -> net::Result<()> {
    net_debug!("tftp: {:?}, message: {}", code, msg);

    let err = Repr::Error { code, msg };
    let payload = socket.send(err.buffer_len(), ep)?;
    let mut pkt = Packet::new_unchecked(payload);
    err.emit(&mut pkt)
}
