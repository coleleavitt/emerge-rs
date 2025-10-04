// endian.rs -- Endian utilities

pub fn decode_uint16_be(data: &[u8]) -> u16 {
    u16::from_be_bytes(data.try_into().unwrap())
}

pub fn decode_uint16_le(data: &[u8]) -> u16 {
    u16::from_le_bytes(data.try_into().unwrap())
}

pub fn decode_uint32_be(data: &[u8]) -> u32 {
    u32::from_be_bytes(data.try_into().unwrap())
}

pub fn decode_uint32_le(data: &[u8]) -> u32 {
    u32::from_le_bytes(data.try_into().unwrap())
}