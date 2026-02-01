use std::io;

#[derive(thiserror::Error, Debug)]
pub enum PacketError {
    #[error("status packet is corrupted")]
    Checksum,
    #[error("status packet insruction is not 0x55")]
    Instruction,
    #[error("serial communication error")]
    Serial(#[from] serialport::Error),
    #[error("io error")]
    Io(#[from] io::Error),
}

#[derive(thiserror::Error, Debug)]
pub enum DeviceError {
    #[error("failed to process the packet on id={0}")]
    ResultFail(u8),
    #[error("undefined instruction or action was used without reg write on id={0}")]
    Instruction(u8),
    #[error("corrupted packet received on id={0}")]
    CRC(u8),
    #[error("invalid data range on id={0}")]
    DataRange(u8),
    #[error("invalid data length on id={0}")]
    DataLength(u8),
    #[error("data limit exceeded on id={0}")]
    DataLimit(u8),
    #[error("invalid access on id={0}")]
    Access(u8),
    #[error("there is an hardware issue on id={0}, check the hardware error status")]
    Hardware(u8),
    #[error("unkown error detected on id={0}")]
    Unkown(u8),
}

#[derive(thiserror::Error, Debug)]
pub enum DynamixelError {
    #[error("device side error")]
    Device(#[from] DeviceError),
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
