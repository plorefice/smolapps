//! Trivial File Transfer Protocol server implementation.

use crate::net::{
    self,
    socket::{SocketHandle, SocketSet, UdpSocket, UdpSocketBuffer},
    time::{Duration, Instant},
    wire::{IpAddress, IpEndpoint},
    Error,
};
use crate::wire::tftp::*;

/// Maximum number of retransmissions attempted by the server before giving up.
const MAX_RETRIES: u8 = 10;

/// Interval between consecutive retries in case of no answer.
const RETRY_TIMEOUT: Duration = Duration { millis: 200 };

/// IANA port for TFTP servers.
const TFTP_PORT: u16 = 69;

/// An abstraction over a filesystem representation containing files that implement [`Handle`].
pub trait Filesystem {
    /// The `Handle` type used by this filesystem.
    type Handle: Handle;

    /// Attempts to open a file in read-only mode if `write` is `false`,
    /// otherwise in read-write mode.
    fn open(&mut self, filename: &str, write_mode: bool) -> Result<Self::Handle, ()>;

    /// Closes the file handle, flushing all pending changes to disk if necessary.
    fn close(&mut self, handle: Self::Handle);
}

/// An open file handle returned by a `Filesystem::open()` operation.
pub trait Handle {
    /// Pulls some bytes from this handle into the specified buffer, returning how many bytes were read.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, ()>;

    /// Writes a buffer into this handle's buffer, returning how many bytes were written.
    fn write(&mut self, buf: &[u8]) -> Result<usize, ()>;
}

/// TFTP server.
pub struct Server<FS: Filesystem> {
    udp_handle: SocketHandle,
    filesystem: FS,
    transfer: Option<Transfer<FS::Handle>>,
    next_poll: Instant,
}

struct Transfer<H> {
    handle: H,
    ep: IpEndpoint,

    is_write: bool,
    block_num: u16,
    // FIXME: I'd reeeally love to avoid a potential stack allocation this big :\
    last_data: [u8; 512],
    last_len: usize,

    retries: u8,
    timeout: Instant,
}

impl<FS> Server<FS>
where
    FS: Filesystem,
{
    /// Creates a TFTP server, serving files from the specified `filesystem`.
    pub fn new<'a, 'b, 'c>(
        sockets: &mut SocketSet<'a, 'b, 'c>,
        rx_buffer: UdpSocketBuffer<'b, 'c>,
        tx_buffer: UdpSocketBuffer<'b, 'c>,
        filesystem: FS,
        now: Instant,
    ) -> Self {
        let socket = UdpSocket::new(rx_buffer, tx_buffer);
        let udp_handle = sockets.add(socket);

        net_trace!("TFTP initialised");

        Server {
            udp_handle,
            filesystem,
            transfer: None,
            next_poll: now,
        }
    }

    /// Returns the duration until the next poll activity.
    ///
    /// Useful for suspending execution after polling.
    pub fn next_poll(&self, now: Instant) -> Duration {
        self.next_poll - now
    }

    /// Processes incoming packets.
    pub fn poll(&mut self, sockets: &mut SocketSet, now: Instant) -> net::Result<()> {
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
                        self.send_error(
                            &mut *socket,
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
                        return self.send_error(
                            &mut *socket,
                            ErrorCode::AccessViolation,
                            "Malformed packet",
                        );
                    }
                };

                let is_write = tftp_packet.opcode() == OpCode::Write;

                match tftp_repr {
                    Repr::ReadRequest { filename, .. } | Repr::WriteRequest { filename, .. } => {
                        // Multiple connections not supported
                        if self.transfer.is_some() {
                            return self.send_error(
                                &mut *socket,
                                ErrorCode::AccessViolation,
                                "Only one connection at a time is supported",
                            );
                        }

                        // Open file handle
                        let handle = match self.filesystem.open(filename, is_write) {
                            Ok(handle) => handle,
                            Err(_) => {
                                return self.send_error(
                                    &mut *socket,
                                    ErrorCode::FileNotFound,
                                    "Unable to open requested file",
                                );
                            }
                        };

                        // Allocate new transfer
                        self.transfer = Some(Transfer {
                            handle,
                            ep,
                            is_write,
                            block_num: 1,
                            last_data: [0; 512],
                            last_len: 0,
                            retries: 0,
                            timeout: now + Duration::from_millis(50),
                        });

                        net_debug!(
                            "tftp: {} request from {}",
                            if is_write { "write" } else { "read" },
                            ep
                        );

                        if is_write {
                            self.send_ack(&mut *socket, 0)?;
                        } else {
                            self.send_data(&mut *socket)?;
                        }
                    }
                    Repr::Data { .. } | Repr::Ack { .. } if self.transfer.is_none() => {
                        // Data request on unconnected socket
                        if self.transfer.is_none() {
                            return self.send_error(
                                &mut *socket,
                                ErrorCode::AccessViolation,
                                "No connection",
                            );
                        }
                    }
                    Repr::Data { block_num, data } => {
                        if let Some(xfer) = &mut self.transfer {
                            // Reset retransmission counter
                            xfer.timeout = now + RETRY_TIMEOUT;
                            xfer.retries = 0;

                            // Make sure this is a write connection
                            if !xfer.is_write {
                                return self.send_error(
                                    &mut *socket,
                                    ErrorCode::AccessViolation,
                                    "Not a write connection",
                                );
                            }

                            let last_block = data.len() < 512;

                            // Write data to the destination file
                            match xfer.handle.write(data) {
                                Ok(_) => {
                                    self.send_ack(&mut *socket, block_num)?;
                                    if last_block {
                                        self.close_transfer();
                                    }
                                }
                                Err(_) => {
                                    self.send_error(
                                        &mut *socket,
                                        ErrorCode::AccessViolation,
                                        "Error writing file",
                                    )?;
                                    self.close_transfer();
                                }
                            }
                        }
                    }
                    Repr::Ack { block_num } => {
                        if let Some(xfer) = &mut self.transfer {
                            // Reset retransmission counter
                            xfer.timeout = now + RETRY_TIMEOUT;
                            xfer.retries = 0;

                            // Make sure this is a read connection
                            if xfer.is_write {
                                return self.send_error(
                                    &mut *socket,
                                    ErrorCode::AccessViolation,
                                    "Not a read connection",
                                );
                            }

                            if block_num != xfer.block_num {
                                return self.send_error(
                                    &mut *socket,
                                    ErrorCode::UnknownID,
                                    "Wrong block number",
                                );
                            }

                            if xfer.last_len != 512 {
                                xfer.block_num += 1;
                                self.send_data(&mut *socket)?;
                            } else {
                                self.close_transfer();
                            }
                        }
                    }
                    Repr::Error { .. } => {
                        return self.send_error(
                            &mut *socket,
                            ErrorCode::IllegalOperation,
                            "Unknown operation",
                        );
                    }
                }

                Ok(())
            }
            Err(Error::Exhausted) if now >= self.next_poll => {
                if socket.can_send() {
                    self.process_timeout(&mut socket, now)?;
                }
                Ok(())
            }
            Err(e) => return Err(e),
        }
    }

    fn process_timeout(&mut self, socket: &mut UdpSocket, now: Instant) -> net::Result<()> {
        if let Some(xfer) = &mut self.transfer {
            if now >= xfer.timeout && xfer.retries < MAX_RETRIES {
                xfer.retries += 1;
                self.resend_data(socket)?;
            } else {
                self.close_transfer();
            }
        }
        Ok(())
    }

    fn send_data(&mut self, socket: &mut UdpSocket) -> net::Result<()> {
        if let Some(xfer) = &mut self.transfer {
            // Read next chunk
            xfer.last_len = match xfer.handle.read(&mut xfer.last_data[..]) {
                Ok(n) => n,
                Err(_) => {
                    self.send_error(
                        socket,
                        ErrorCode::AccessViolation,
                        "Error occurred while reading the file",
                    )?;
                    self.close_transfer();
                    return Ok(());
                }
            };

            self.resend_data(socket)?;
        }
        Ok(())
    }

    fn resend_data(&mut self, socket: &mut UdpSocket) -> net::Result<()> {
        if let Some(xfer) = &self.transfer {
            net_trace!("tftp: sending data block #{}", xfer.block_num);

            let data = Repr::Data {
                block_num: xfer.block_num,
                data: &xfer.last_data[..xfer.last_len],
            };
            let payload = socket.send(data.buffer_len(), xfer.ep)?;
            let mut pkt = Packet::new_unchecked(payload);
            data.emit(&mut pkt)?;
        }
        Ok(())
    }

    fn send_ack(&mut self, socket: &mut UdpSocket, block: u16) -> net::Result<()> {
        if let Some(Transfer { ep, .. }) = &self.transfer {
            net_trace!("tftp: sending ack #{}", block);

            let ack = Repr::Ack { block_num: block };
            let payload = socket.send(ack.buffer_len(), *ep)?;
            let mut pkt = Packet::new_unchecked(payload);
            ack.emit(&mut pkt)?;
        }
        Ok(())
    }

    fn send_error(
        &mut self,
        socket: &mut UdpSocket,
        code: ErrorCode,
        msg: &str,
    ) -> net::Result<()> {
        if let Some(Transfer { ep, .. }) = &self.transfer {
            net_debug!("tftp: {:?}, message: {}", code, msg);

            let err = Repr::Error { code, msg };
            let payload = socket.send(err.buffer_len(), *ep)?;
            let mut pkt = Packet::new_unchecked(payload);
            err.emit(&mut pkt)?;
        }
        Ok(())
    }

    fn close_transfer(&mut self) {
        if let Some(Transfer { handle, .. }) = self.transfer.take() {
            net_debug!("tftp: closing");

            self.filesystem.close(handle);
        }
    }
}
