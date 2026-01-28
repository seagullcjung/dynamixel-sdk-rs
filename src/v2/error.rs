use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
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

#[derive(Error, Debug)]
pub enum CommError {
    #[error("connection error")]
    IO(#[from] io::Error),
    #[error("status packet is corrupted")]
    Checksum,
    #[error("instruction for status packet is not 0x55")]
    Instruction,
}
