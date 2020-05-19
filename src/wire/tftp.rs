//! Wire protocol definitions for the Trivial File Transfer Protocol (TFTP).
//!
//! See https://tools.ietf.org/html/rfc1350 for the TFTP specification.

// TODO: remove me once the TFTP client has been implemented!
#![allow(unused)]

use byteorder::{ByteOrder, NetworkEndian};
use core::str;
use smoltcp::{Error, Result};

enum_with_unknown! {
    /// One of the possible operations supported by TFTP.
    pub enum OpCode(u16) {
        Read = 1,
        Write = 2,
        Data = 3,
        Ack = 4,
        Error = 5,
    }
}

enum_with_unknown! {
    /// One of the possible error codes found in a TFTP error packet.
    pub enum ErrorCode(u16) {
        Undefined = 0,
        FileNotFound = 1,
        AccessViolation = 2,
        DiskFull = 3,
        IllegalOperation = 4,
        UnknownID = 5,
        FileExists = 6,
        NoSuchUser = 7,
    }
}

/// One of the possible operating modes supported by TFTP.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Mode {
    NetAscii,
    Octet,
    Mail,
    Unknown,
}

impl Mode {
    /// Returns the string representation of this `Mode`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Mode::NetAscii => "netascii",
            Mode::Octet => "octet",
            Mode::Mail => "mail",
            Mode::Unknown => "",
        }
    }
}

impl From<u8> for Mode {
    fn from(b: u8) -> Self {
        match b {
            b'N' | b'n' => Mode::NetAscii,
            b'O' | b'o' => Mode::Octet,
            b'M' | b'm' => Mode::Mail,
            _ => Mode::Unknown,
        }
    }
}
/// A read/write wrapper around a Simple Network Time Protocol v4 packet buffer.
#[derive(Debug, Eq, PartialEq)]
pub struct Packet<T: AsRef<[u8]>> {
    buffer: T,
}

pub(crate) mod field {
    #![allow(non_snake_case)]
    #![allow(unused)]

    use core::ops;

    type Field = ops::Range<usize>;
    type Rest = ops::RangeFrom<usize>;

    // Shared fields
    pub const OPCODE: Field = 0..2;

    // DATA/ACK fields
    pub const BLOCK: Field = 2..4;
    pub const DATA: Rest = 4..;

    // ERR fields
    pub const ERROR_CODE: Field = 2..4;
    pub const ERROR_STRING: Rest = 4..;
}

impl<T: AsRef<[u8]>> Packet<T> {
    /// Imbues a raw octet buffer with TFTP packet structure.
    pub fn new_unchecked(buffer: T) -> Packet<T> {
        Packet { buffer }
    }

    /// Shorthand for a combination of [new_unchecked] and [check_len].
    ///
    /// [new_unchecked]: #method.new_unchecked
    /// [check_len]: #method.check_len
    pub fn new_checked(buffer: T) -> Result<Packet<T>> {
        let packet = Self::new_unchecked(buffer);
        packet.check_len()?;
        Ok(packet)
    }

    /// Ensures that no accessor method will panic if called.
    /// Returns `Err(Error::Truncated)` if the buffer is too short.
    ///
    /// [set_header_len]: #method.set_header_len
    pub fn check_len(&self) -> Result<()> {
        let len = self.buffer.as_ref().len();
        if len < field::OPCODE.end {
            Err(Error::Truncated)
        } else {
            let end = match self.opcode() {
                OpCode::Read | OpCode::Write | OpCode::Error => self.find_last_null_byte()?,
                OpCode::Data | OpCode::Ack => field::BLOCK.end,
                OpCode::Unknown(_) => return Err(Error::Malformed),
            };
            if len < end {
                Err(Error::Truncated)
            } else {
                Ok(())
            }
        }
    }

    /// Returns the OpCode of this packet.
    pub fn opcode(&self) -> OpCode {
        NetworkEndian::read_u16(&self.buffer.as_ref()[field::OPCODE]).into()
    }

    /// Returns the filename contained in this packet.
    pub fn filename(&self) -> &str {
        let start = field::OPCODE.end;
        let len = self
            .buffer
            .as_ref()
            .iter()
            .skip(start)
            .position(|b| *b == 0)
            .unwrap();

        let data = self.buffer.as_ref();
        str::from_utf8(&data[start..start + len]).unwrap()
    }

    /// Returns the operating mode of this packet.
    pub fn mode(&self) -> Mode {
        let start = field::OPCODE.end + self.filename().len() + 1;
        self.buffer.as_ref()[start].into()
    }

    /// Returns the block number of this packet.
    pub fn block_number(&self) -> u16 {
        NetworkEndian::read_u16(&self.buffer.as_ref()[field::BLOCK]).into()
    }

    /// Returns the data contained in this packet.
    pub fn data(&self) -> &[u8] {
        &self.buffer.as_ref()[field::DATA]
    }

    /// Returns the error code of this packet.
    pub fn error_code(&self) -> ErrorCode {
        NetworkEndian::read_u16(&self.buffer.as_ref()[field::ERROR_CODE]).into()
    }

    /// Returns the error message of this packet.
    pub fn error_msg(&self) -> &str {
        let data = self.buffer.as_ref();
        str::from_utf8(&data[field::ERROR_STRING.start..data.len() - 1]).unwrap()
    }

    /// Returns the index immediately following the last NULL byte of this packet.
    fn find_last_null_byte(&self) -> Result<usize> {
        self.buffer
            .as_ref()
            .iter()
            .rposition(|b| *b == 0)
            .map(|p| p + 1) // account for 0-based indexing
            .ok_or(Error::Truncated)
    }
}

impl<T: AsRef<[u8]> + AsMut<[u8]>> Packet<T> {
    /// Sets the OpCode of this packet.
    pub fn set_opcode(&mut self, op: OpCode) {
        let data = &mut self.buffer.as_mut()[field::OPCODE];
        NetworkEndian::write_u16(data, op.into());
    }

    /// Sets the filename and the operating mode of this packet.
    pub fn set_filename_and_mode(&mut self, fname: &str, mode: Mode) {
        let data = self.buffer.as_mut();
        let mode = mode.as_str();

        let fn_start = field::OPCODE.end;
        let mode_start = fn_start + fname.len() + 1;
        let mode_end = mode_start + mode.len();

        data[fn_start..mode_start - 1].copy_from_slice(fname.as_bytes());
        data[mode_start..mode_end].copy_from_slice(mode.as_bytes());
        data[mode_start - 1] = 0;
        data[data.len() - 1] = 0;
    }

    /// Sets the block number of this packet.
    pub fn set_block_number(&mut self, blk: u16) {
        let data = &mut self.buffer.as_mut()[field::BLOCK];
        NetworkEndian::write_u16(data, blk);
    }

    /// Sets the data contained in this packet.
    pub fn set_data(&mut self, data: &[u8]) {
        self.buffer.as_mut()[field::DATA].copy_from_slice(data);
    }

    /// Sets the error code of this packet.
    pub fn set_error_code(&mut self, code: ErrorCode) {
        let data = &mut self.buffer.as_mut()[field::ERROR_CODE];
        NetworkEndian::write_u16(data, code.into());
    }

    /// Sets the error message of this packet.
    pub fn set_error_msg(&mut self, msg: &str) {
        let data = &mut self.buffer.as_mut()[field::ERROR_STRING];
        let len = data.len();

        data[0..len - 1].copy_from_slice(msg.as_bytes());
        data[len - 1] = 0;
    }
}

/// A high-level representation of a Trivial File Transfer Protocol packet.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Repr<'a> {
    /// Read request (RRQ) packet.
    ReadRequest { filename: &'a str, mode: Mode },
    /// Write request (WRQ) packet.
    WriteRequest { filename: &'a str, mode: Mode },
    /// Data (DATA) packet.
    Data { block_num: u16, data: &'a [u8] },
    /// Acknowledgment (ACK) packet.
    Ack { block_num: u16 },
    /// Error (ERR) packet.
    Error { code: ErrorCode, msg: &'a str },
}

impl<'a> Repr<'a> {
    /// Return the length of a packet that will be emitted from this high-level representation.
    pub fn buffer_len(&self) -> usize {
        match self {
            Repr::ReadRequest { filename, mode } | Repr::WriteRequest { filename, mode } => {
                2 + filename.len() + 1 + mode.as_str().len() + 1
            }
            Repr::Data { data, .. } => 2 + 2 + data.len(),
            Repr::Error { msg, .. } => 2 + 2 + msg.len() + 1,
            Repr::Ack { .. } => 4,
        }
    }

    /// Parse a TFTP packet and return its high-level representation.
    pub fn parse<T>(packet: &'a Packet<&T>) -> Result<Self>
    where
        T: AsRef<[u8]> + ?Sized,
    {
        Ok(match packet.opcode() {
            OpCode::Read => Repr::ReadRequest {
                filename: packet.filename(),
                mode: packet.mode(),
            },
            OpCode::Write => Repr::WriteRequest {
                filename: packet.filename(),
                mode: packet.mode(),
            },
            OpCode::Data => Repr::Data {
                block_num: packet.block_number(),
                data: packet.data(),
            },
            OpCode::Ack => Repr::Ack {
                block_num: packet.block_number(),
            },
            OpCode::Error => Repr::Error {
                code: packet.error_code(),
                msg: packet.error_msg(),
            },
            OpCode::Unknown(_) => return Err(Error::Malformed),
        })
    }

    /// Emit a high-level representation into a TFTP packet.
    pub fn emit<T>(&self, packet: &mut Packet<&mut T>) -> Result<()>
    where
        T: AsRef<[u8]> + AsMut<[u8]> + ?Sized,
    {
        Ok(match self {
            &Self::ReadRequest { filename, mode } => {
                packet.set_opcode(OpCode::Read);
                packet.set_filename_and_mode(filename, mode);
            }
            &Self::WriteRequest { filename, mode } => {
                packet.set_opcode(OpCode::Write);
                packet.set_filename_and_mode(filename, mode);
            }
            &Self::Data { block_num, data } => {
                packet.set_opcode(OpCode::Data);
                packet.set_block_number(block_num);
                packet.set_data(data);
            }
            &Self::Ack { block_num } => {
                packet.set_opcode(OpCode::Ack);
                packet.set_block_number(block_num);
            }
            &Self::Error { code, msg } => {
                packet.set_opcode(OpCode::Error);
                packet.set_error_code(code);
                packet.set_error_msg(msg);
            }
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    static RRQ_BYTES: [u8; 20] = [
        0x00, 0x01, 0x72, 0x66, 0x63, 0x31, 0x33, 0x35, 0x30, 0x2e, 0x74, 0x78, 0x74, 0x00, 0x6f,
        0x63, 0x74, 0x65, 0x74, 0x00,
    ];

    static WRQ_BYTES: [u8; 20] = [
        0x00, 0x02, 0x72, 0x66, 0x63, 0x31, 0x33, 0x35, 0x30, 0x2e, 0x74, 0x78, 0x74, 0x00, 0x6f,
        0x63, 0x74, 0x65, 0x74, 0x00,
    ];

    static DATA_BYTES: [u8; 516] = [
        0x00, 0x03, 0x00, 0x01, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x0a, 0x4e, 0x65, 0x74, 0x77, 0x6f,
        0x72, 0x6b, 0x20, 0x57, 0x6f, 0x72, 0x6b, 0x69, 0x6e, 0x67, 0x20, 0x47, 0x72, 0x6f, 0x75,
        0x70, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x4b, 0x2e, 0x20,
        0x53, 0x6f, 0x6c, 0x6c, 0x69, 0x6e, 0x73, 0x0a, 0x52, 0x65, 0x71, 0x75, 0x65, 0x73, 0x74,
        0x20, 0x46, 0x6f, 0x72, 0x20, 0x43, 0x6f, 0x6d, 0x6d, 0x65, 0x6e, 0x74, 0x73, 0x3a, 0x20,
        0x31, 0x33, 0x35, 0x30, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20, 0x20, 0x4d, 0x49, 0x54, 0x0a, 0x53, 0x54, 0x44, 0x3a, 0x20, 0x33, 0x33, 0x20, 0x20,
        0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x4a, 0x75, 0x6c, 0x79, 0x20, 0x31,
        0x39, 0x39, 0x32, 0x0a, 0x4f, 0x62, 0x73, 0x6f, 0x6c, 0x65, 0x74, 0x65, 0x73, 0x3a, 0x20,
        0x52, 0x46, 0x43, 0x20, 0x37, 0x38, 0x33, 0x0a, 0x0a, 0x0a, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20, 0x54, 0x48, 0x45, 0x20, 0x54, 0x46, 0x54, 0x50, 0x20, 0x50, 0x52, 0x4f, 0x54, 0x4f,
        0x43, 0x4f, 0x4c, 0x20, 0x28, 0x52, 0x45, 0x56, 0x49, 0x53, 0x49, 0x4f, 0x4e, 0x20, 0x32,
        0x29, 0x0a, 0x0a, 0x53, 0x74, 0x61, 0x74, 0x75, 0x73, 0x20, 0x6f, 0x66, 0x20, 0x74, 0x68,
        0x69, 0x73, 0x20, 0x4d, 0x65, 0x6d, 0x6f, 0x0a, 0x0a, 0x20, 0x20, 0x20, 0x54, 0x68, 0x69,
        0x73, 0x20, 0x52, 0x46, 0x43, 0x20, 0x73, 0x70, 0x65, 0x63, 0x69, 0x66, 0x69, 0x65, 0x73,
        0x20, 0x61, 0x6e, 0x20, 0x49, 0x41, 0x42, 0x20, 0x73, 0x74, 0x61, 0x6e, 0x64, 0x61, 0x72,
        0x64, 0x73, 0x20, 0x74, 0x72, 0x61, 0x63, 0x6b, 0x20, 0x70, 0x72, 0x6f, 0x74, 0x6f, 0x63,
        0x6f, 0x6c, 0x20, 0x66, 0x6f, 0x72, 0x20, 0x74, 0x68, 0x65, 0x20, 0x49, 0x6e, 0x74, 0x65,
        0x72, 0x6e, 0x65, 0x74, 0x0a, 0x20, 0x20, 0x20, 0x63, 0x6f, 0x6d, 0x6d, 0x75, 0x6e, 0x69,
        0x74, 0x79, 0x2c, 0x20, 0x61, 0x6e, 0x64, 0x20, 0x72, 0x65, 0x71, 0x75, 0x65, 0x73, 0x74,
        0x73, 0x20, 0x64, 0x69, 0x73, 0x63, 0x75, 0x73, 0x73, 0x69, 0x6f, 0x6e, 0x20, 0x61, 0x6e,
        0x64, 0x20, 0x73, 0x75, 0x67, 0x67, 0x65, 0x73, 0x74, 0x69, 0x6f, 0x6e, 0x73, 0x20, 0x66,
        0x6f, 0x72, 0x20, 0x69, 0x6d, 0x70, 0x72, 0x6f, 0x76, 0x65, 0x6d, 0x65, 0x6e, 0x74, 0x73,
        0x2e, 0x0a, 0x20, 0x20, 0x20, 0x50, 0x6c, 0x65, 0x61, 0x73, 0x65, 0x20, 0x72, 0x65, 0x66,
        0x65, 0x72, 0x20, 0x74, 0x6f, 0x20, 0x74, 0x68, 0x65, 0x20, 0x63, 0x75, 0x72, 0x72, 0x65,
        0x6e, 0x74, 0x20, 0x65, 0x64, 0x69, 0x74, 0x69, 0x6f, 0x6e, 0x20, 0x6f, 0x66, 0x20, 0x74,
        0x68, 0x65, 0x20, 0x22, 0x49, 0x41,
    ];

    static ACK_BYTES: [u8; 4] = [0x00, 0x04, 0x00, 0x09];

    static ERR_BYTES: [u8; 10] = [0x00, 0x05, 0x00, 0x06, 0x45, 0x72, 0x72, 0x6f, 0x72, 0x00];

    #[test]
    fn test_deconstruct() {
        let packet = Packet::new_unchecked(&RRQ_BYTES[..]);
        assert_eq!(packet.opcode(), OpCode::Read);
        assert_eq!(packet.filename(), "rfc1350.txt");
        assert_eq!(packet.mode(), Mode::Octet);

        let packet = Packet::new_unchecked(&WRQ_BYTES[..]);
        assert_eq!(packet.opcode(), OpCode::Write);
        assert_eq!(packet.filename(), "rfc1350.txt");
        assert_eq!(packet.mode(), Mode::Octet);

        let packet = Packet::new_unchecked(&DATA_BYTES[..]);
        assert_eq!(packet.opcode(), OpCode::Data);
        assert_eq!(packet.block_number(), 1);
        assert_eq!(packet.data(), &DATA_BYTES[4..]);

        let packet = Packet::new_unchecked(&ACK_BYTES[..]);
        assert_eq!(packet.opcode(), OpCode::Ack);
        assert_eq!(packet.block_number(), 9);

        let packet = Packet::new_unchecked(&ERR_BYTES[..]);
        assert_eq!(packet.opcode(), OpCode::Error);
        assert_eq!(packet.error_code(), ErrorCode::FileExists);
        assert_eq!(packet.error_msg(), "Error");
    }

    #[test]
    fn test_construct() {
        let mut packet = Packet::new_unchecked(vec![0xa5; 20]);
        packet.set_opcode(OpCode::Read);
        packet.set_filename_and_mode("rfc1350.txt", Mode::Octet);
        assert_eq!(&packet.buffer[..], &RRQ_BYTES[..]);

        let mut packet = Packet::new_unchecked(vec![0xa5; 20]);
        packet.set_opcode(OpCode::Write);
        packet.set_filename_and_mode("rfc1350.txt", Mode::Octet);
        assert_eq!(&packet.buffer[..], &WRQ_BYTES[..]);

        let mut packet = Packet::new_unchecked(vec![0xa5; 516]);
        packet.set_opcode(OpCode::Data);
        packet.set_block_number(1);
        packet.set_data(&DATA_BYTES[4..]);
        assert_eq!(&packet.buffer[..], &DATA_BYTES[..]);

        let mut packet = Packet::new_unchecked(vec![0xa5; 4]);
        packet.set_opcode(OpCode::Ack);
        packet.set_block_number(9);
        assert_eq!(&packet.buffer[..], &ACK_BYTES[..]);

        let mut packet = Packet::new_unchecked(vec![0xa5; 10]);
        packet.set_opcode(OpCode::Error);
        packet.set_error_code(ErrorCode::FileExists);
        packet.set_error_msg("Error");
        assert_eq!(&packet.buffer[..], &ERR_BYTES[..]);
    }

    #[test]
    fn test_parse() {
        for (repr, bytes) in vec![
            (
                Repr::ReadRequest {
                    filename: "rfc1350.txt",
                    mode: Mode::Octet,
                },
                &RRQ_BYTES[..],
            ),
            (
                Repr::WriteRequest {
                    filename: "rfc1350.txt",
                    mode: Mode::Octet,
                },
                &WRQ_BYTES[..],
            ),
            (
                Repr::Data {
                    block_num: 1,
                    data: &DATA_BYTES[4..],
                },
                &DATA_BYTES[..],
            ),
            (Repr::Ack { block_num: 9 }, &ACK_BYTES[..]),
            (
                Repr::Error {
                    code: ErrorCode::FileExists,
                    msg: "Error",
                },
                &ERR_BYTES[..],
            ),
        ]
        .into_iter()
        {
            let packet = Packet::new_unchecked(bytes);
            let res = Repr::parse(&packet).unwrap();
            assert_eq!(res, repr);
        }
    }

    #[test]
    fn test_emit() {
        for (repr, bytes) in vec![
            (
                Repr::ReadRequest {
                    filename: "rfc1350.txt",
                    mode: Mode::Octet,
                },
                &RRQ_BYTES[..],
            ),
            (
                Repr::WriteRequest {
                    filename: "rfc1350.txt",
                    mode: Mode::Octet,
                },
                &WRQ_BYTES[..],
            ),
            (
                Repr::Data {
                    block_num: 1,
                    data: &DATA_BYTES[4..],
                },
                &DATA_BYTES[..],
            ),
            (Repr::Ack { block_num: 9 }, &ACK_BYTES[..]),
            (
                Repr::Error {
                    code: ErrorCode::FileExists,
                    msg: "Error",
                },
                &ERR_BYTES[..],
            ),
        ]
        .into_iter()
        {
            let mut buff = vec![0xa5; bytes.len()];
            let mut packet = Packet::new_unchecked(&mut buff);
            repr.emit(&mut packet).unwrap();
            assert_eq!(&packet.buffer[..], bytes);
        }
    }
}
