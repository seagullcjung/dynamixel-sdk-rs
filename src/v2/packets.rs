use super::error::{CommError, DeviceError};
use std::cmp;
use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

const HEADER: [u8; 4] = [0xFF, 0xFF, 0xFD, 0x00];

const PING: u8 = 0x01;
const READ: u8 = 0x02;
const WRITE: u8 = 0x03;
const REG_WRITE: u8 = 0x04;
const ACTION: u8 = 0x05;
const FACTORY_RESET: u8 = 0x06;
const REBOOT: u8 = 0x08;
const CLEAR: u8 = 0x10;
const CONTROL_TABLE_BACKUP: u8 = 0x20;
const SYNC_READ: u8 = 0x82;
const SYNC_WRITE: u8 = 0x83;
const FAST_SYNC_READ: u8 = 0x8A;
const BULK_READ: u8 = 0x92;
const BULK_WRITE: u8 = 0x93;
const FAST_BULK_READ: u8 = 0x9A;

pub const BROADCAST_ID: u8 = 0xFE;

const CRC_TABLE: [u16; 256] = [
    0x0000, 0x8005, 0x800F, 0x000A, 0x801B, 0x001E, 0x0014, 0x8011, 0x8033, 0x0036, 0x003C, 0x8039,
    0x0028, 0x802D, 0x8027, 0x0022, 0x8063, 0x0066, 0x006C, 0x8069, 0x0078, 0x807D, 0x8077, 0x0072,
    0x0050, 0x8055, 0x805F, 0x005A, 0x804B, 0x004E, 0x0044, 0x8041, 0x80C3, 0x00C6, 0x00CC, 0x80C9,
    0x00D8, 0x80DD, 0x80D7, 0x00D2, 0x00F0, 0x80F5, 0x80FF, 0x00FA, 0x80EB, 0x00EE, 0x00E4, 0x80E1,
    0x00A0, 0x80A5, 0x80AF, 0x00AA, 0x80BB, 0x00BE, 0x00B4, 0x80B1, 0x8093, 0x0096, 0x009C, 0x8099,
    0x0088, 0x808D, 0x8087, 0x0082, 0x8183, 0x0186, 0x018C, 0x8189, 0x0198, 0x819D, 0x8197, 0x0192,
    0x01B0, 0x81B5, 0x81BF, 0x01BA, 0x81AB, 0x01AE, 0x01A4, 0x81A1, 0x01E0, 0x81E5, 0x81EF, 0x01EA,
    0x81FB, 0x01FE, 0x01F4, 0x81F1, 0x81D3, 0x01D6, 0x01DC, 0x81D9, 0x01C8, 0x81CD, 0x81C7, 0x01C2,
    0x0140, 0x8145, 0x814F, 0x014A, 0x815B, 0x015E, 0x0154, 0x8151, 0x8173, 0x0176, 0x017C, 0x8179,
    0x0168, 0x816D, 0x8167, 0x0162, 0x8123, 0x0126, 0x012C, 0x8129, 0x0138, 0x813D, 0x8137, 0x0132,
    0x0110, 0x8115, 0x811F, 0x011A, 0x810B, 0x010E, 0x0104, 0x8101, 0x8303, 0x0306, 0x030C, 0x8309,
    0x0318, 0x831D, 0x8317, 0x0312, 0x0330, 0x8335, 0x833F, 0x033A, 0x832B, 0x032E, 0x0324, 0x8321,
    0x0360, 0x8365, 0x836F, 0x036A, 0x837B, 0x037E, 0x0374, 0x8371, 0x8353, 0x0356, 0x035C, 0x8359,
    0x0348, 0x834D, 0x8347, 0x0342, 0x03C0, 0x83C5, 0x83CF, 0x03CA, 0x83DB, 0x03DE, 0x03D4, 0x83D1,
    0x83F3, 0x03F6, 0x03FC, 0x83F9, 0x03E8, 0x83ED, 0x83E7, 0x03E2, 0x83A3, 0x03A6, 0x03AC, 0x83A9,
    0x03B8, 0x83BD, 0x83B7, 0x03B2, 0x0390, 0x8395, 0x839F, 0x039A, 0x838B, 0x038E, 0x0384, 0x8381,
    0x0280, 0x8285, 0x828F, 0x028A, 0x829B, 0x029E, 0x0294, 0x8291, 0x82B3, 0x02B6, 0x02BC, 0x82B9,
    0x02A8, 0x82AD, 0x82A7, 0x02A2, 0x82E3, 0x02E6, 0x02EC, 0x82E9, 0x02F8, 0x82FD, 0x82F7, 0x02F2,
    0x02D0, 0x82D5, 0x82DF, 0x02DA, 0x82CB, 0x02CE, 0x02C4, 0x82C1, 0x8243, 0x0246, 0x024C, 0x8249,
    0x0258, 0x825D, 0x8257, 0x0252, 0x0270, 0x8275, 0x827F, 0x027A, 0x826B, 0x026E, 0x0264, 0x8261,
    0x0220, 0x8225, 0x822F, 0x022A, 0x823B, 0x023E, 0x0234, 0x8231, 0x8213, 0x0216, 0x021C, 0x8219,
    0x0208, 0x820D, 0x8207, 0x0202,
];

fn calc_crc(buf: &[u8]) -> u16 {
    let mut crc: u16 = 0;

    for byte in buf {
        let crc_h: u8 = (crc >> 8) as u8;
        let i: usize = (crc_h ^ byte) as usize;
        crc = (crc << 8) ^ CRC_TABLE[i];
    }

    crc
}

pub enum Reset {
    All,
    ExceptID,
    ExceptIDBaudrate,
}

pub enum Clear {
    Position,
    Error,
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

        let mut params = self.params.clone();
        if self.params.len() > 0 {
            let mut indices: Vec<usize> = vec![];
            for i in 0..params.len() - 2 {
                if params[i] == 0xFF && params[i + 1] == 0xFF && params[i + 2] == 0xFD {
                    indices.push(i + 3);
                }
            }

            indices.reverse();

            for i in indices {
                params.insert(i, 0xFD);
            }
        }

        let length = params.len() as u16 + 3;
        bytes.extend(length.to_le_bytes());

        bytes.push(self.instruction);

        bytes.extend(params);

        let crc = calc_crc(&bytes);
        bytes.extend(crc.to_le_bytes());

        bytes
    }

    pub fn write_to(&self, writer: &mut dyn Write, timeout: Duration) -> Result<(), io::Error> {
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

    pub fn read(id: u8, address: u16, length: u16) -> Self {
        let instruction = READ;
        let params = [address.to_le_bytes(), length.to_le_bytes()].concat();

        InstructionPacket {
            id,
            instruction,
            params,
        }
    }

    pub fn write(id: u8, address: u16, value: &[u8]) -> Self {
        let instruction = WRITE;
        let mut params: Vec<u8> = address.to_le_bytes().to_vec();
        params.extend(value);

        InstructionPacket {
            id,
            instruction,
            params,
        }
    }

    pub fn reg_write(id: u8, address: u16, value: &[u8]) -> Self {
        let instruction = REG_WRITE;
        let mut params: Vec<u8> = address.to_le_bytes().to_vec();
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

    pub fn factory_reset(id: u8, option: Reset) -> Self {
        let instruction = FACTORY_RESET;
        let param = match option {
            Reset::All => 0xFF,
            Reset::ExceptID => 0x01,
            Reset::ExceptIDBaudrate => 0x02,
        };

        let params = vec![param];

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

    pub fn clear(id: u8, option: Clear) -> Self {
        let instruction = CLEAR;
        let params = match option {
            Clear::Position => vec![0x01, 0x44, 0x58, 0x4C, 0x22],
            Clear::Error => vec![0x02, 0x45, 0x52, 0x43, 0x4C],
        };

        InstructionPacket {
            id,
            instruction,
            params,
        }
    }

    pub fn control_table_backup(id: u8) -> Self {
        let instruction = CONTROL_TABLE_BACKUP;
        let params = vec![0x01, 0x43, 0x54, 0x52, 0x4C];

        InstructionPacket {
            id,
            instruction,
            params,
        }
    }

    pub fn control_table_restore(id: u8) -> Self {
        let instruction = CONTROL_TABLE_BACKUP;
        let params = vec![0x02, 0x43, 0x54, 0x52, 0x4C];

        InstructionPacket {
            id,
            instruction,
            params,
        }
    }

    pub fn sync_read(ids: &[u8], address: u16, length: u16) -> Self {
        let id = BROADCAST_ID;
        let instruction = SYNC_READ;

        let params = [&address.to_le_bytes(), &length.to_le_bytes(), ids].concat();

        InstructionPacket {
            id,
            instruction,
            params,
        }
    }

    pub fn sync_write<const NUM: usize, const LEN: usize>(
        ids: &[u8; NUM],
        address: u16,
        values: &[[u8; LEN]; NUM],
    ) -> Self {
        let id = BROADCAST_ID;
        let instruction = SYNC_WRITE;

        let length = LEN as u16;

        let mut params = [address.to_le_bytes(), length.to_le_bytes()].concat();

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

    pub fn fast_sync_read(ids: &[u8], address: u16, length: u16) -> Self {
        let id = BROADCAST_ID;
        let instruction = FAST_SYNC_READ;

        let params = [&address.to_le_bytes(), &length.to_le_bytes(), ids].concat();

        InstructionPacket {
            id,
            instruction,
            params,
        }
    }

    pub fn bulk_read<const NUM: usize>(
        ids: &[u8; NUM],
        addresses: &[u16; NUM],
        lengths: &[u16; NUM],
    ) -> Self {
        let id = BROADCAST_ID;
        let instruction = BULK_READ;

        let mut params: Vec<u8> = vec![];
        for i in 0..ids.len() {
            params.push(ids[i]);
            params.extend(addresses[i].to_le_bytes());
            params.extend(lengths[i].to_le_bytes());
        }

        InstructionPacket {
            id,
            instruction,
            params,
        }
    }

    pub fn bulk_write<const NUM: usize>(
        ids: &[u8; NUM],
        addresses: &[u16; NUM],
        values: &[Vec<u8>; NUM],
    ) -> Self {
        let id = BROADCAST_ID;
        let instruction = BULK_WRITE;

        let mut params: Vec<u8> = vec![];
        for i in 0..ids.len() {
            params.push(ids[i]);
            params.extend(addresses[i].to_le_bytes());
            params.extend((values[i].len() as u16).to_le_bytes());
            params.extend(&values[i]);
        }

        InstructionPacket {
            id,
            instruction,
            params,
        }
    }

    pub fn fast_bulk_read<const LEN: usize>(
        ids: &[u8; LEN],
        addresses: &[u16; LEN],
        lengths: &[u16; LEN],
    ) -> Self {
        let id = BROADCAST_ID;
        let instruction = FAST_BULK_READ;

        let mut params: Vec<u8> = vec![];
        for i in 0..ids.len() {
            params.push(ids[i]);
            params.extend(addresses[i].to_le_bytes());
            params.extend(lengths[i].to_le_bytes());
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

    pub fn params(&self) -> Result<Vec<u8>, DeviceError> {
        let alert = self.error & 0x80 >> 7;

        if alert == 1 {
            return Err(DeviceError::Hardware(self.id));
        }

        let error_number = self.error & !0x80;

        if error_number > 0 {
            match error_number {
                1 => return Err(DeviceError::ResultFail(self.id)),
                2 => return Err(DeviceError::Instruction(self.id)),
                3 => return Err(DeviceError::CRC(self.id)),
                4 => return Err(DeviceError::DataRange(self.id)),
                5 => return Err(DeviceError::DataLength(self.id)),
                6 => return Err(DeviceError::DataLimit(self.id)),
                7 => return Err(DeviceError::Access(self.id)),
                _ => return Err(DeviceError::Unkown(self.id)),
            }
        } else {
            return Ok(self.params.clone());
        }
    }

    pub fn read_from(
        reader: &mut dyn Read,
        timeout: Duration,
        stuffed: bool,
    ) -> Result<Self, CommError> {
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

                if packet.len() >= 7 {
                    let length = u16::from_le_bytes(packet[5..7].try_into().unwrap()) as usize;

                    packet_length = cmp::max(packet_length, 7 + length);

                    if packet.len() == 7 + length {
                        let crc =
                            u16::from_le_bytes(packet[packet.len() - 2..].try_into().unwrap());

                        if calc_crc(&packet[..packet.len() - 2]) != crc {
                            return Err(CommError::Checksum);
                        }

                        let id = packet[4];

                        let instruction = packet[7];

                        let error = packet[8];

                        let mut params = packet[9..packet.len() - 2].to_vec();

                        let mut indices = Vec::new();
                        if stuffed && params.len() >= 4 {
                            let mut j: usize = 0;

                            for window in params.windows(4) {
                                if window == [0xFF, 0xFF, 0xFD, 0xFD] {
                                    indices.push(j + 3);
                                }

                                j += 1;
                            }
                        }

                        if indices.len() > 0 {
                            indices.reverse();

                            for j in indices {
                                if j < packet.len() - 2 - 9 {
                                    params.remove(j);
                                }
                            }
                        }

                        if instruction != 0x55 {
                            return Err(CommError::Instruction);
                        }

                        return Ok(StatusPacket { id, error, params });
                    }
                }
            }
        }

        Err(CommError::IO(io::Error::new(
            io::ErrorKind::TimedOut,
            "packet read timed out",
        )))
    }

    pub fn parse_subpackets(&self, lengths: &[u16]) -> Vec<Self> {
        let length = lengths[0] as usize;

        let mut packets: Vec<StatusPacket> = vec![];

        let sub_packet = StatusPacket {
            id: self.params[0],
            error: self.error,
            params: self.params[1..length + 1].to_vec(),
        };

        packets.push(sub_packet);

        let mut start = length + 1;
        for i in 1..lengths.len() {
            let length = lengths[i] as usize;
            let sub_packet = self.params[start..start + length + 4].to_vec();

            let sub_packet = StatusPacket {
                id: sub_packet[3],
                error: sub_packet[2],
                params: sub_packet[4..4 + length].to_vec(),
            };

            packets.push(sub_packet);

            start += length + 4;
        }

        packets
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

        let expected_bytes = [
            0xFF, 0xFF, 0xFD, 0x00, 0x01, 0x07, 0x00, 0x02, 0x84, 0x00, 0x04, 0x00, 0x1D, 0x15,
        ];

        assert_eq!(bytes, expected_bytes.to_vec());
    }

    #[rstest]
    #[timeout(Duration::from_millis(80))]
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

        let expected_bytes = [
            0xFF, 0xFF, 0xFD, 0x00, 0x01, 0x07, 0x00, 0x02, 0x84, 0x00, 0x04, 0x00, 0x1D, 0x15,
        ];

        assert_eq!(cursor.get_mut().to_vec(), expected_bytes);
    }

    #[rstest]
    #[timeout(Duration::from_millis(80))]
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
        let mut bytes = vec![0xFF, 0xFF, 0xFD, 0x00, 0x01, 0x08, 0x00, 0x55, 0x82];

        let params = vec![0x01, 0x02, 0x03, 0x04];
        bytes.extend(&params);

        let crc = calc_crc(&bytes);
        bytes.extend(crc.to_le_bytes());

        let mut cursor = Cursor::new([vec![0; i], bytes.clone(), vec![0; j]].concat());

        let packet = StatusPacket::read_from(&mut cursor, Duration::from_millis(80), false)
            .expect("read failed!");

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
    fn read_from_partial(#[values(0, 1, 2, 3, 4)] i: usize) {
        println!("{i}");

        let mut bytes = vec![0xFF, 0xFF, 0xFD, 0x00, 0x01, 0x08, 0x00, 0x55, 0x82];

        let params = vec![0x01, 0x02, 0x03, 0x04];
        bytes.extend(&params);

        let crc = calc_crc(&bytes);
        bytes.extend(crc.to_le_bytes());

        let mut cursor = Cursor::new([HEADER.to_vec(), vec![0xFF; i], bytes.clone()].concat());

        let packet = StatusPacket::read_from(&mut cursor, Duration::from_millis(80), false)
            .expect("read failed!");

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

        StatusPacket::read_from(&mut cursor, Duration::ZERO, false).expect("read timed out!");
    }

    #[rstest]
    #[timeout(Duration::from_millis(100))]
    #[should_panic(expected = "header not found!")]
    fn read_from_no_header(#[values(1, 2, 3)] n: usize) {
        let mut bytes = vec![0xFF, 0xFF, 0xFD, 0x00, 0x01, 0x08, 0x00, 0x55, 0x82];

        let params = vec![0x01, 0x02, 0x03, 0x04];
        bytes.extend(&params);

        let crc = calc_crc(&bytes);
        bytes.extend(crc.to_le_bytes());

        let mut cursor = Cursor::new(&bytes[n..]);

        StatusPacket::read_from(&mut cursor, Duration::from_millis(80), false)
            .expect("header not found!");
    }

    #[rstest]
    #[timeout(Duration::from_millis(100))]
    #[should_panic(expected = "read timed out!")]
    fn read_from_short(#[values(3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13)] n: usize) {
        let mut bytes = vec![0xFF, 0xFF, 0xFD, 0x00, 0x01, 0x08, 0x00, 0x55, 0x82];

        let params = vec![0x01, 0x02, 0x03, 0x04];
        bytes.extend(&params);

        let crc = calc_crc(&bytes);
        bytes.extend(crc.to_le_bytes());

        let mut cursor = Cursor::new(&bytes[..n]);

        StatusPacket::read_from(&mut cursor, Duration::from_millis(80), false)
            .expect("read timed out!");
    }

    #[rstest]
    fn remove_stuffing(#[values(0, 1, 2, 3, 4)] i: usize, #[values(0, 1, 2, 3, 4)] j: usize) {
        println!("{i}, {j}");

        let mut bytes = vec![0xFF, 0xFF, 0xFD, 0x00, 0x01];

        let params = [vec![0xFF; i], vec![0xFF, 0xFF, 0xFD, 0xFD], vec![0xFF; j]].concat();
        bytes.extend(((params.len() + 4) as u16).to_le_bytes());
        bytes.extend([0x55, 0x82]);

        bytes.extend(&params);

        let crc = calc_crc(&bytes);
        bytes.extend(crc.to_le_bytes());

        let mut cursor = Cursor::new(&bytes);

        let packet = StatusPacket::read_from(&mut cursor, Duration::from_millis(100), true)
            .expect("read failed!");

        let expected_params = [vec![0xFF; i], vec![0xFF, 0xFF, 0xFD], vec![0xFF; j]].concat();
        assert_eq!(
            packet,
            StatusPacket {
                id: 0x01,
                error: 0x82,
                params: expected_params
            }
        );
    }

    #[test]
    fn parse_subpackets() {
        let a: u32 = 0x28937423;
        let b: u16 = 0x2933;
        let c: u8 = 0x13;
        let values = (a, b, c);

        let mut params = vec![0x01];
        params.extend(a.to_le_bytes());

        params.extend([0x00, 0x00]);
        params.push(0x03);
        params.push(0x02);
        params.extend(b.to_le_bytes());

        params.extend([0x00, 0x00]);
        params.push(0x04);
        params.push(0x03);
        params.extend(c.to_le_bytes());

        let packet = StatusPacket {
            id: 0xFE,
            error: 0x02,
            params: params,
        };

        let lengths = [4, 2, 1];
        let packets = packet.parse_subpackets(&lengths);

        let expected_packets = vec![
            StatusPacket {
                id: 0x01,
                error: 0x02,
                params: values.0.to_le_bytes().to_vec(),
            },
            StatusPacket {
                id: 0x02,
                error: 0x03,
                params: values.1.to_le_bytes().to_vec(),
            },
            StatusPacket {
                id: 0x03,
                error: 0x04,
                params: values.2.to_le_bytes().to_vec(),
            },
        ];

        assert_eq!(packets, expected_packets);
    }
}
