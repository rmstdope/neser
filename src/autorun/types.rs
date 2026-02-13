use serde::{Deserialize, Serialize};

pub const AUTORUN_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutorunFrame {
    pub player1: u8,
    pub player2: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutorunFile {
    pub version: u32,
    pub frames: Vec<AutorunFrame>,
    pub checksum: u32,
}
