use super::error::{DynamixelError, PacketError};
use super::packets::{BROADCAST_ID, InstructionPacket, StatusPacket};
use std::collections::HashMap;
use std::io;
use std::time::Duration;

#[derive(Debug)]
pub struct Motor {
    id: u8,
    baud_rate: u32,
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

    pub fn baud_rate(&self) -> Result<u32, serialport::Error> {
        let baud_rate = self.port.baud_rate()?;

        Ok(baud_rate)
    }

    pub fn set_baud_rate(&mut self, baud_rate: u32) -> Result<(), serialport::Error> {
        self.port.set_baud_rate(baud_rate)?;

        Ok(())
    }

    pub fn ping(&mut self, id: u8) -> Result<Vec<u8>, DynamixelError> {
        let packet = InstructionPacket::ping(id);

        let timeout = self.port.timeout();

        packet.write_to(&mut self.port, timeout)?;

        let mut ids = Vec::new();

        if id != BROADCAST_ID {
            let packet = StatusPacket::read_from(&mut self.port, timeout)?;

            ids.push(packet.id());

            return Ok(ids);
        }

        loop {
            let packet = StatusPacket::read_from(&mut self.port, timeout)?;
            ids.push(packet.id());
        }
    }

    pub fn scan(&mut self, baud_rates: &[u32]) -> Result<Vec<Motor>, DynamixelError> {
        let mut motors = Vec::new();

        let original_baud_rate = self.port.baud_rate()?;

        for &baud_rate in baud_rates {
            self.port.set_baud_rate(baud_rate)?;

            let ids = match self.ping(BROADCAST_ID) {
                Ok(ids) => ids,
                Err(e) => {
                    self.port.set_baud_rate(original_baud_rate)?;
                    return Err(e);
                }
            };

            for id in ids {
                motors.push(Motor { id, baud_rate });
            }
        }

        self.port.set_baud_rate(original_baud_rate)?;

        Ok(motors)
    }
}
