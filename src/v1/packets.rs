use super::error::{DeviceError, PacketError};
use std::cmp;
use std::collections::HashSet;
use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

const HEADER: [u8; 2] = [0xFF, 0xFF];

const PING: u8 = 0x01;
const READ: u8 = 0x02;
const WRITE: u8 = 0x03;
const REG_WRITE: u8 = 0x04;
const ACTION: u8 = 0x05;
const FACTORY_RESET: u8 = 0x06;
const REBOOT: u8 = 0x08;
const SYNC_WRITE: u8 = 0x83;
const BULK_READ: u8 = 0x92;

pub const BROADCAST_ID: u8 = 0xFE;

fn calc_checksum(buf: &[u8]) -> u8 {
    let mut sum: u16 = 0;
    for &byte in buf {
        sum += byte as u16;
    }

    !(sum as u8)
}

#[derive(Debug, PartialEq, Eq)]
pub struct InstructionPacket {
    pub id: u8,
    pub instruction: u8,
    pub params: Vec<u8>,
}

impl InstructionPacket {
    fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::from(HEADER);

        bytes.push(self.id);

        let length = self.params.len() as u8 + 2;
        bytes.push(length);

        bytes.push(self.instruction);

        bytes.extend(self.params.clone());

        let checksum = calc_checksum(&bytes[2..]);
        bytes.push(checksum);

        bytes
    }

    pub fn write_to<T: Write>(
        &self,
        writer: &mut T,
        timeout: Duration,
    ) -> Result<(), std::io::Error> {
        let mut buf = self.as_bytes();

        let t0 = Instant::now();
        while t0.elapsed() <= timeout {
            let n = writer.write(&buf)?;

            if n == 0 {
                return Ok(());
            }

            buf.drain(..n);
        }

        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "packet write timed out",
        ))
    }

    pub fn ping(id: u8) -> Self {
        let instruction = PING;
        let params = vec![];

        InstructionPacket {
            id,
            instruction,
            params,
        }
    }

    pub fn read(id: u8, address: u8, length: u8) -> Self {
        let instruction = READ;
        let params = Vec::from([address, length]);

        InstructionPacket {
            id,
            instruction,
            params,
        }
    }

    pub fn write(id: u8, address: u8, value: &[u8]) -> Self {
        let instruction = WRITE;
        let mut params = Vec::from([address]);
        params.extend(value);

        InstructionPacket {
            id,
            instruction,
            params,
        }
    }

    pub fn reg_write(id: u8, address: u8, value: &[u8]) -> Self {
        let instruction = REG_WRITE;
        let mut params = Vec::from([address]);
        params.extend(value);

        InstructionPacket {
            id,
            instruction,
            params,
        }
    }

    pub fn action(id: u8) -> Self {
        let instruction = ACTION;
        let params: Vec<u8> = vec![];

        InstructionPacket {
            id,
            instruction,
            params,
        }
    }

    pub fn factory_reset(id: u8) -> Self {
        assert_ne!(id, BROADCAST_ID, "broadcast ID (0xFE) cannot be used");

        let instruction = FACTORY_RESET;
        let params = vec![];

        InstructionPacket {
            id,
            instruction,
            params,
        }
    }

    pub fn reboot(id: u8) -> Self {
        let instruction = REBOOT;
        let params: Vec<u8> = vec![];

        InstructionPacket {
            id,
            instruction,
            params,
        }
    }

    pub fn sync_write<const NUM: usize, const LEN: usize>(
        ids: &[u8; NUM],
        address: u8,
        values: &[[u8; LEN]; NUM],
    ) -> Self {
        let id = BROADCAST_ID;
        let instruction = SYNC_WRITE;

        let length = LEN as u8;

        let mut params = Vec::from([address, length]);

        for i in 0..ids.len() {
            params.push(ids[i]);
            params.extend(values[i]);
        }

        InstructionPacket {
            id,
            instruction,
            params,
        }
    }

    pub fn bulk_read<const NUM: usize>(
        ids: &[u8; NUM],
        addresses: &[u8; NUM],
        lengths: &[u8; NUM],
    ) -> Self {
        let id = BROADCAST_ID;
        let instruction = BULK_READ;

        let mut params: Vec<u8> = vec![0];
        for i in 0..ids.len() {
            params.push(lengths[i]);
            params.push(ids[i]);
            params.push(addresses[i]);
        }

        InstructionPacket {
            id,
            instruction,
            params,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct StatusPacket {
    id: u8,
    error: u8,
    params: Vec<u8>,
}

impl StatusPacket {
    pub fn id(&self) -> u8 {
        self.id
    }

    pub fn params(&self) -> Result<Vec<u8>, HashSet<DeviceError>> {
        let mut error = self.error & 0x7F;

        if error == 0 {
            return Ok(self.params.clone());
        }
        let mut mask = 0x01;

        let mut errors = HashSet::new();

        for _ in 0..6 {
            error = error & mask;

            let e = match error & mask {
                1 => DeviceError::InputVoltage(self.id),
                2 => DeviceError::AngleLimit(self.id),
                4 => DeviceError::Overheating(self.id),
                8 => DeviceError::Range(self.id),
                16 => DeviceError::Checksum(self.id),
                32 => DeviceError::Overload(self.id),
                64 => DeviceError::Instruction(self.id),
                _ => DeviceError::Unkown(self.id),
            };

            mask <<= 1;

            errors.insert(e);
        }

        Err(errors)
    }

    pub fn read_from<T: Read>(reader: &mut T, timeout: Duration) -> Result<Self, PacketError> {
        let mut packet = Vec::new();

        let mut packet_length = 11;

        let t0 = Instant::now();
        while t0.elapsed() <= timeout {
            let mut tmp = vec![0; packet_length - packet.len()];

            let n = reader.read(&mut tmp)?;
            packet.extend(&tmp[..n]);

            let mut i = 0;
            let mut starts = Vec::new();
            for window in packet.windows(HEADER.len()) {
                if window == HEADER {
                    starts.push(i);
                }

                i += 1;
            }

            if starts.len() == 0 {
                if packet.len() >= HEADER.len() * 2 {
                    packet.drain(..HEADER.len());
                }
            } else {
                let last_start = starts[starts.len() - 1];

                if last_start > 0 {
                    packet.drain(..last_start);
                }

                if packet.len() >= 4 {
                    let length = packet[3] as usize;

                    packet_length = cmp::max(packet_length, 4 + length);

                    if packet.len() == 4 + length {
                        let checksum = packet[packet.len() - 1];

                        if calc_checksum(&packet[2..packet.len() - 1]) != checksum {
                            return Err(PacketError::Checksum);
                        }

                        let id = packet[2];
                        let error = packet[4];

                        let params = (&packet[5..packet.len() - 1]).to_vec();

                        return Ok(StatusPacket { id, error, params });
                    }
                }
            }
        }

        Err(PacketError::from(io::Error::new(
            io::ErrorKind::TimedOut,
            "packet read timed out",
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::io::Cursor;

    #[test]
    fn as_bytes() {
        let packet = InstructionPacket {
            id: 0x01,
            instruction: 0x02,
            params: vec![0x84, 0x00, 0x04, 0x00],
        };
        let bytes = packet.as_bytes();

        let expected_bytes = [0xFF, 0xFF, 0x01, 0x06, 0x02, 0x84, 0x00, 0x04, 0x00, 0x6E];

        assert_eq!(bytes, expected_bytes.to_vec());
    }

    #[rstest]
    #[timeout(Duration::from_millis(100))]
    fn write_to() {
        let packet = InstructionPacket {
            id: 0x01,
            instruction: 0x02,
            params: vec![0x84, 0x00, 0x04, 0x00],
        };

        let mut cursor = Cursor::new(Vec::new());

        packet
            .write_to(&mut cursor, Duration::from_millis(100))
            .expect("write failed!");

        let expected_bytes = [0xFF, 0xFF, 0x01, 0x06, 0x02, 0x84, 0x00, 0x04, 0x00, 0x6E];

        assert_eq!(cursor.get_mut().to_vec(), expected_bytes);
    }

    #[rstest]
    #[timeout(Duration::from_millis(100))]
    #[should_panic(expected = "write timed out!")]
    fn write_to_timeout() {
        let packet = InstructionPacket {
            id: 0x01,
            instruction: 0x02,
            params: vec![0x84, 0x00, 0x04, 0x00],
        };

        let mut cursor = Cursor::new(Vec::new());

        packet
            .write_to(&mut cursor, Duration::ZERO)
            .expect("write timed out!");
    }

    #[rstest]
    #[timeout(Duration::from_millis(100))]
    fn read_from(
        #[values(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14)] i: usize,
        #[values(0, 1)] j: usize,
    ) {
        println!("{i}, {j}");
        let mut bytes = vec![0xFF, 0xFF, 0x01, 0x07, 0x82];

        let params = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        bytes.extend(&params);

        let checksum = calc_checksum(&bytes[2..]);
        bytes.push(checksum);

        let mut cursor = Cursor::new([vec![0; i], bytes.clone(), vec![0; j]].concat());

        let packet =
            StatusPacket::read_from(&mut cursor, Duration::from_millis(100)).expect("read failed!");

        assert_eq!(
            packet,
            StatusPacket {
                id: 0x01,
                error: 0x82,
                params
            }
        );
    }

    #[rstest]
    #[timeout(Duration::from_millis(100))]
    fn read_from_partial(#[values(0, 1, 2, 3, 4)] n: usize) {
        println!("{n}");

        let mut bytes = vec![0xFF, 0xFF, 0x01, 0x07, 0x82];

        let params = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        bytes.extend(&params);

        let checksum = calc_checksum(&bytes[2..]);
        bytes.push(checksum);

        let mut padding = Vec::new();
        for i in 0..n {
            if i % 2 == 0 {
                padding.push(0x11);
            } else {
                padding.push(0xFF);
            }
        }

        padding.push(0x11);

        let mut cursor = Cursor::new([HEADER.to_vec(), padding, bytes.clone()].concat());

        let packet =
            StatusPacket::read_from(&mut cursor, Duration::from_millis(100)).expect("read failed!");

        assert_eq!(
            packet,
            StatusPacket {
                id: 0x01,
                error: 0x82,
                params
            }
        );
    }

    #[rstest]
    #[timeout(Duration::from_millis(100))]
    #[should_panic(expected = "read timed out!")]
    fn read_from_timeout() {
        let mut cursor = Cursor::new(Vec::new());

        StatusPacket::read_from(&mut cursor, Duration::ZERO).expect("read timed out!");
    }

    #[rstest]
    #[timeout(Duration::from_millis(100))]
    #[should_panic(expected = "header not found!")]
    fn read_from_no_header(#[values(1, 2, 3)] n: usize) {
        let mut bytes = vec![0xFF, 0xFF, 0x01, 0x07, 0x82];

        let params = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        bytes.extend(&params);

        let checksum = calc_checksum(&bytes[2..]);
        bytes.push(checksum);

        let mut cursor = Cursor::new(&bytes[n..]);

        StatusPacket::read_from(&mut cursor, Duration::from_millis(80)).expect("header not found!");
    }

    #[rstest]
    #[timeout(Duration::from_millis(100))]
    #[should_panic(expected = "read timed out!")]
    fn read_from_short(#[values(3, 4, 5, 6, 7, 8, 9, 10)] n: usize) {
        let mut bytes = vec![0xFF, 0xFF, 0x01, 0x07, 0x82];

        let params = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        bytes.extend(&params);

        let checksum = calc_checksum(&bytes[2..]);
        bytes.push(checksum);

        let mut cursor = Cursor::new(&bytes[..n]);

        StatusPacket::read_from(&mut cursor, Duration::from_millis(80)).expect("read timed out!");
    }
}
