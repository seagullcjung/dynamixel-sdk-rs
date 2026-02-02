use super::error::DynamixelError;
use super::packets::{BROADCAST_ID, Clear, InstructionPacket, Reset, StatusPacket};
use std::collections::HashMap;
use std::io;
use std::time::Duration;

pub struct MotorInfo {
    model_number: u16,
    firmware_version: u8,
}

#[derive(Debug)]
pub struct Motor {
    id: u8,
    baud_rate: u32,
    model_number: u16,
    firmware_version: u8,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum ReturnLevel {
    PING,
    READ,
    ALL,
}

pub struct BusBuilder {
    path: String,
    timeout: Duration,
    baud_rate: u32,
    return_level: ReturnLevel,
}

impl BusBuilder {
    #[must_use]
    pub fn baud_rate(mut self, baud_rate: u32) -> Self {
        self.baud_rate = baud_rate;
        self
    }

    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    #[must_use]
    pub fn return_level(mut self, return_level: ReturnLevel) -> Self {
        self.return_level = return_level;
        self
    }

    pub fn connect(self) -> Result<Bus, serialport::Error> {
        Bus::connect(&self)
    }
}

pub fn new(path: &str, baud_rate: u32) -> BusBuilder {
    BusBuilder {
        path: path.into(),
        baud_rate,
        timeout: Duration::MAX,
        return_level: ReturnLevel::ALL,
    }
}

pub struct Bus {
    port: Box<dyn serialport::SerialPort>,
    return_level: ReturnLevel,
}

impl Bus {
    pub fn connect(builder: &BusBuilder) -> Result<Self, serialport::Error> {
        let port = serialport::new(&builder.path, builder.baud_rate)
            .timeout(builder.timeout)
            .open()?;

        let return_level = ReturnLevel::ALL;

        Ok(Bus { port, return_level })
    }

    pub fn return_level(&self) -> ReturnLevel {
        self.return_level
    }

    pub fn set_return_level(&mut self, return_level: ReturnLevel) {
        self.return_level = return_level;
    }

    pub fn timeout(&self) -> Duration {
        self.port.timeout()
    }

    pub fn set_timeout(&mut self, timeout: Duration) -> Result<(), serialport::Error> {
        self.port.set_timeout(timeout)?;

        Ok(())
    }

    pub fn baud_rate(&self) -> Result<u32, io::Error> {
        let baud_rate = self.port.baud_rate()?;

        Ok(baud_rate)
    }

    pub fn set_baud_rate(&mut self, baud_rate: u32) -> Result<(), serialport::Error> {
        self.port.set_baud_rate(baud_rate)?;

        Ok(())
    }

    pub fn ping(&mut self, id: u8) -> Result<HashMap<u8, MotorInfo>, DynamixelError> {
        if id > BROADCAST_ID {
            return Err(DynamixelError::InvalidID);
        }

        let packet = InstructionPacket::ping(id);

        let timeout = self.port.timeout();

        packet.write_to(&mut self.port, timeout)?;

        fn parse_ping(params: Vec<u8>) -> MotorInfo {
            let model_number = u16::from_le_bytes(params[..2].try_into().unwrap());
            let firmware_version = params[2];

            MotorInfo {
                model_number,
                firmware_version,
            }
        }

        let mut map = HashMap::new();

        if id < BROADCAST_ID {
            let packet = StatusPacket::read_from(&mut self.port, timeout, true)?;

            let params = packet.params()?;
            let info = parse_ping(params);

            map.insert(packet.id(), info);

            return Ok(map);
        }

        loop {
            let packet = StatusPacket::read_from(&mut self.port, timeout, true)?;

            let params = packet.params()?;

            let info = parse_ping(params);

            map.insert(packet.id(), info);
        }
    }

    pub fn scan(&mut self, baud_rates: &[u32]) -> Result<Vec<Motor>, DynamixelError> {
        let mut motors = Vec::new();

        let original_baud_rate = self.port.baud_rate()?;

        for &baud_rate in baud_rates {
            self.set_baud_rate(baud_rate)?;

            let map = match self.ping(BROADCAST_ID) {
                Ok(map) => map,
                Err(e) => {
                    self.port.set_baud_rate(original_baud_rate)?;
                    return Err(e);
                }
            };

            if !map.is_empty() {
                for (&id, info) in &map {
                    let model_number = info.model_number;
                    let firmware_version = info.firmware_version;
                    motors.push(Motor {
                        id,
                        baud_rate,
                        model_number,
                        firmware_version,
                    });
                }
            }
        }

        self.port.set_baud_rate(original_baud_rate)?;

        Ok(motors)
    }

    pub fn read(&mut self, id: u8, address: u16, length: u16) -> Result<Vec<u8>, DynamixelError> {
        if id >= BROADCAST_ID {
            return Err(DynamixelError::InvalidID);
        }

        let packet = InstructionPacket::read(id, address, length);

        let timeout = self.port.timeout();
        packet.write_to(&mut self.port, timeout)?;

        let packet = StatusPacket::read_from(&mut self.port, timeout, true)?;

        let params = packet.params()?;
        if params.len() != length as usize {
            return Err(DynamixelError::ParamLength);
        }
        Ok(params)
    }

    pub fn write(&mut self, id: u8, address: u16, value: &[u8]) -> Result<(), DynamixelError> {
        if id >= BROADCAST_ID {
            return Err(DynamixelError::InvalidID);
        }

        let packet = InstructionPacket::write(id, address, value);

        let timeout = self.port.timeout();
        packet.write_to(&mut self.port, timeout)?;

        let only_ping = self.return_level == ReturnLevel::PING;
        let upto_read = self.return_level == ReturnLevel::READ;
        if only_ping | upto_read {
            return Ok(());
        }

        let packet = StatusPacket::read_from(&mut self.port, timeout, false)?;

        if packet.params()?.len() > 0 {
            return Err(DynamixelError::NotEmpty);
        }

        Ok(())
    }

    pub fn reg_write(&mut self, id: u8, address: u16, value: &[u8]) -> Result<(), DynamixelError> {
        if id >= BROADCAST_ID {
            return Err(DynamixelError::InvalidID);
        }

        let packet = InstructionPacket::reg_write(id, address, value);

        let timeout = self.port.timeout();
        packet.write_to(&mut self.port, timeout)?;

        let only_ping = self.return_level == ReturnLevel::PING;
        let upto_read = self.return_level == ReturnLevel::READ;
        if only_ping | upto_read {
            return Ok(());
        }

        let packet = StatusPacket::read_from(&mut self.port, timeout, false)?;

        if packet.params()?.len() > 0 {
            return Err(DynamixelError::NotEmpty);
        }

        Ok(())
    }

    pub fn action(&mut self, id: u8) -> Result<(), DynamixelError> {
        if id > BROADCAST_ID {
            return Err(DynamixelError::InvalidID);
        }

        let packet = InstructionPacket::action(id);

        let timeout = self.port.timeout();
        packet.write_to(&mut self.port, timeout)?;

        let only_ping = self.return_level == ReturnLevel::PING;
        let upto_read = self.return_level == ReturnLevel::READ;
        let is_broadcast = id == BROADCAST_ID;
        if only_ping | upto_read | is_broadcast {
            return Ok(());
        }

        let packet = StatusPacket::read_from(&mut self.port, timeout, false)?;

        if packet.params()?.len() > 0 {
            return Err(DynamixelError::NotEmpty);
        }

        Ok(())
    }

    pub fn factory_reset(&mut self, id: u8, option: Reset) -> Result<(), DynamixelError> {
        if id >= BROADCAST_ID {
            return Err(DynamixelError::InvalidID);
        }

        let packet = InstructionPacket::factory_reset(id, option);

        let timeout = self.port.timeout();
        packet.write_to(&mut self.port, timeout)?;

        let only_ping = self.return_level == ReturnLevel::PING;
        let upto_read = self.return_level == ReturnLevel::READ;
        if only_ping | upto_read {
            return Ok(());
        }

        let packet = StatusPacket::read_from(&mut self.port, timeout, false)?;

        if packet.params()?.len() > 0 {
            return Err(DynamixelError::NotEmpty);
        }

        Ok(())
    }

    pub fn reboot(&mut self, id: u8) -> Result<(), DynamixelError> {
        if id >= BROADCAST_ID {
            return Err(DynamixelError::InvalidID);
        }

        let packet = InstructionPacket::reboot(id);

        let timeout = self.port.timeout();
        packet.write_to(&mut self.port, timeout)?;

        let only_ping = self.return_level == ReturnLevel::PING;
        let upto_read = self.return_level == ReturnLevel::READ;
        if only_ping | upto_read {
            return Ok(());
        }

        let packet = StatusPacket::read_from(&mut self.port, timeout, false)?;

        if packet.params()?.len() > 0 {
            return Err(DynamixelError::NotEmpty);
        }

        Ok(())
    }

    pub fn clear(&mut self, id: u8, option: Clear) -> Result<(), DynamixelError> {
        if id >= BROADCAST_ID {
            return Err(DynamixelError::InvalidID);
        }

        let packet = InstructionPacket::clear(id, option);

        let timeout = self.port.timeout();
        packet.write_to(&mut self.port, timeout)?;

        let only_ping = self.return_level == ReturnLevel::PING;
        let upto_read = self.return_level == ReturnLevel::READ;
        if only_ping | upto_read {
            return Ok(());
        }

        let packet = StatusPacket::read_from(&mut self.port, timeout, false)?;

        if packet.params()?.len() > 0 {
            return Err(DynamixelError::NotEmpty);
        }

        Ok(())
    }

    pub fn control_table_backup(&mut self, id: u8) -> Result<(), DynamixelError> {
        if id >= BROADCAST_ID {
            return Err(DynamixelError::InvalidID);
        }

        let packet = InstructionPacket::control_table_backup(id);

        let timeout = self.port.timeout();
        packet.write_to(&mut self.port, timeout)?;

        let only_ping = self.return_level == ReturnLevel::PING;
        let upto_read = self.return_level == ReturnLevel::READ;
        if only_ping | upto_read {
            return Ok(());
        }

        let packet = StatusPacket::read_from(&mut self.port, timeout, false)?;

        if packet.params()?.len() > 0 {
            return Err(DynamixelError::NotEmpty);
        }

        Ok(())
    }

    pub fn control_table_restore(&mut self, id: u8) -> Result<(), DynamixelError> {
        if id >= BROADCAST_ID {
            return Err(DynamixelError::InvalidID);
        }

        let packet = InstructionPacket::control_table_restore(id);

        let timeout = self.port.timeout();
        packet.write_to(&mut self.port, timeout)?;

        let only_ping = self.return_level == ReturnLevel::PING;
        let upto_read = self.return_level == ReturnLevel::READ;
        if only_ping | upto_read {
            return Ok(());
        }

        let packet = StatusPacket::read_from(&mut self.port, timeout, false)?;

        if packet.params()?.len() > 0 {
            return Err(DynamixelError::NotEmpty);
        }

        Ok(())
    }

    pub fn sync_read(
        &mut self,
        ids: &[u8],
        address: u16,
        length: u16,
    ) -> Result<HashMap<u8, Vec<u8>>, DynamixelError> {
        for &id in ids {
            if id >= BROADCAST_ID {
                return Err(DynamixelError::InvalidID);
            }
        }

        let packet = InstructionPacket::sync_read(ids, address, length);

        let timeout = self.port.timeout();
        packet.write_to(&mut self.port, timeout)?;

        let mut map = HashMap::new();
        for _ in 0..length {
            let packet = StatusPacket::read_from(&mut self.port, timeout, true)?;

            let params = packet.params()?;
            if params.len() != length as usize {
                return Err(DynamixelError::ParamLength);
            }

            map.insert(packet.id(), params);
        }

        Ok(map)
    }

    pub fn sync_write<const NUM: usize, const LEN: usize>(
        &mut self,
        ids: &[u8; NUM],
        address: u16,
        values: &[[u8; LEN]; NUM],
    ) -> Result<(), DynamixelError> {
        for &id in ids {
            if id >= BROADCAST_ID {
                return Err(DynamixelError::InvalidID);
            }
        }

        let packet = InstructionPacket::sync_write(ids, address, values);

        let timeout = self.port.timeout();
        packet.write_to(&mut self.port, timeout)?;

        let only_ping = self.return_level == ReturnLevel::PING;
        let upto_read = self.return_level == ReturnLevel::READ;
        if only_ping | upto_read {
            return Ok(());
        }

        let packet = StatusPacket::read_from(&mut self.port, timeout, false)?;

        if packet.params()?.len() > 0 {
            return Err(DynamixelError::NotEmpty);
        }

        Ok(())
    }

    pub fn fast_sync_read(
        &mut self,
        ids: &[u8],
        address: u16,
        length: u16,
    ) -> Result<HashMap<u8, Vec<u8>>, DynamixelError> {
        for &id in ids {
            if id >= BROADCAST_ID {
                return Err(DynamixelError::InvalidID);
            }
        }

        let packet = InstructionPacket::sync_read(ids, address, length);

        let timeout = self.port.timeout();
        packet.write_to(&mut self.port, timeout)?;

        let packet = StatusPacket::read_from(&mut self.port, timeout, false)?;

        let lengths = vec![length; ids.len()];

        let mut map = HashMap::new();
        for packet in packet.parse_subpackets(&lengths) {
            let params = packet.params()?;
            if params.len() != length as usize {
                return Err(DynamixelError::ParamLength);
            }

            map.insert(packet.id(), packet.params()?);
        }

        Ok(map)
    }

    pub fn bulk_read<const NUM: usize>(
        &mut self,
        ids: &[u8; NUM],
        addresses: &[u16; NUM],
        lengths: &[u16; NUM],
    ) -> Result<HashMap<u8, Vec<u8>>, DynamixelError> {
        for &id in ids {
            if id >= BROADCAST_ID {
                return Err(DynamixelError::InvalidID);
            }
        }

        let packet = InstructionPacket::bulk_read(ids, addresses, lengths);

        let timeout = self.port.timeout();
        packet.write_to(&mut self.port, timeout)?;

        let mut id_to_length = HashMap::new();
        for i in 0..NUM {
            id_to_length.insert(ids[i], lengths[i]);
        }

        let mut map = HashMap::new();
        for _ in 0..NUM {
            let packet = StatusPacket::read_from(&mut self.port, timeout, true)?;

            let length = id_to_length[&packet.id()];
            let params = packet.params()?;

            if params.len() != length as usize {
                return Err(DynamixelError::ParamLength);
            }

            map.insert(packet.id(), packet.params()?);
        }

        Ok(map)
    }

    pub fn bulk_write<const NUM: usize, const LEN: usize>(
        &mut self,
        ids: &[u8; NUM],
        addresses: &[u16; NUM],
        values: &[Vec<u8>; NUM],
    ) -> Result<(), DynamixelError> {
        for &id in ids {
            if id >= BROADCAST_ID {
                return Err(DynamixelError::InvalidID);
            }
        }

        let packet = InstructionPacket::bulk_write(ids, addresses, values);

        let timeout = self.port.timeout();
        packet.write_to(&mut self.port, timeout)?;

        let only_ping = self.return_level == ReturnLevel::PING;
        let upto_read = self.return_level == ReturnLevel::READ;
        if only_ping | upto_read {
            return Ok(());
        }

        let packet = StatusPacket::read_from(&mut self.port, timeout, false)?;

        if packet.params()?.len() > 0 {
            return Err(DynamixelError::NotEmpty);
        }

        Ok(())
    }

    pub fn fast_bulk_read<const NUM: usize>(
        &mut self,
        ids: &[u8; NUM],
        addresses: &[u16; NUM],
        lengths: &[u16; NUM],
    ) -> Result<HashMap<u8, Vec<u8>>, DynamixelError> {
        for &id in ids {
            if id >= BROADCAST_ID {
                return Err(DynamixelError::InvalidID);
            }
        }

        let packet = InstructionPacket::bulk_read(ids, addresses, lengths);

        let timeout = self.port.timeout();
        packet.write_to(&mut self.port, timeout)?;

        let packet = StatusPacket::read_from(&mut self.port, timeout, false)?;

        let mut id_to_length = HashMap::new();
        for i in 0..NUM {
            id_to_length.insert(ids[i], lengths[i]);
        }

        let mut map = HashMap::new();
        let packets = packet.parse_subpackets(lengths);
        for i in 0..packets.len() {
            let packet = &packets[i];

            let length = id_to_length[&packet.id()];
            let params = packet.params()?;

            if params.len() != length as usize {
                return Err(DynamixelError::ParamLength);
            }

            map.insert(packet.id(), packet.params()?);
        }

        Ok(map)
    }
}
