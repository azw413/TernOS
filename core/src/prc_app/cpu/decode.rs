#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodedOp {
    Unknown(u16),
    ATrap(u16),
    Trap(u8),
}

pub fn decode_word(word: u16) -> DecodedOp {
    if (word & 0xF000) == 0xA000 {
        return DecodedOp::ATrap(word);
    }
    if (word & 0xFFF0) == 0x4E40 {
        return DecodedOp::Trap((word & 0x000F) as u8);
    }
    DecodedOp::Unknown(word)
}
