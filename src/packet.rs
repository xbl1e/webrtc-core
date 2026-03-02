pub struct AudioPacket {
    pub len: usize,
    pub data: [u8; 1500],
    pub timestamp: u64,
    pub seq: u16,
    pub ssrc: u32,
    pub layer: u8,
}

impl AudioPacket {
    pub fn from_slice(s: &[u8]) -> Self {
        let mut data = [0u8; 1500];
        let len = s.len().min(1500);
        data[..len].copy_from_slice(&s[..len]);
        let seq = if len >= 4 {
            u16::from_be_bytes([data[2], data[3]])
        } else {
            0
        };
        let timestamp = if len >= 8 {
            u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as u64
        } else {
            0u64
        };
        let ssrc = if len >= 12 {
            u32::from_be_bytes([data[8], data[9], data[10], data[11]])
        } else {
            0
        };
        Self {
            len,
            data,
            timestamp,
            seq,
            ssrc,
            layer: 0,
        }
    }

    pub fn payload_mut(&mut self) -> &mut [u8] {
        &mut self.data[..self.len]
    }

    pub fn reserve_tail(&mut self, extra: usize) -> bool {
        if self.len + extra <= self.data.len() {
            true
        } else {
            false
        }
    }
}
