use std::collections::HashSet;
use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PacketError {
    #[error("serial communication error")]
    Serial(#[from] serialport::Error),
    #[error("io error")]
    IO(#[from] io::Error),
    #[error("status packet is corrupted")]
    Checksum,
}

#[derive(Error, Debug, PartialEq, Eq, Hash)]
pub enum DeviceError {
    #[error("applied voltage out of range on id={0}")]
    InputVoltage(u8),
    #[error("goal position is out of range from CW Angle Limit to CCW Angle Limit on id={0}")]
    AngleLimit(u8),
    #[error(
        "internal temporature is outside of operatingt temperature set in the control cable on id={0}"
    )]
    Overheating(u8),
    #[error("invalid data range on id={0}")]
    Range(u8),
    #[error("corrupted packet received on id={0}")]
    Checksum(u8),
    #[error("current load cannot by controlled on id={0}")]
    Overload(u8),
    #[error("undefined instruction or action instruction was used without reg write on id={0}")]
    Instruction(u8),
    #[error("unkown error detected on id={0}")]
    Unkown(u8),
}

#[derive(thiserror::Error, Debug)]
pub enum DynamixelError {
    #[error("device side error")]
    Device(HashSet<DeviceError>),
    #[error("params should be empty")]
    NotEmpty,
    #[error("param length is incorrect")]
    ParamLength,
    #[error("packet error")]
    Packet(#[from] PacketError),
    #[error("serial communication error")]
    Serial(#[from] serialport::Error),
    #[error("io error")]
    Io(#[from] io::Error),
}

impl From<HashSet<DeviceError>> for DynamixelError {
    fn from(errors: HashSet<DeviceError>) -> DynamixelError {
        DynamixelError::Device(errors)
    }
}
