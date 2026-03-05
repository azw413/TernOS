extern crate alloc;

#[derive(Clone, Debug, Default)]
pub struct MemoryMap {
    pub base: u32,
    pub data: alloc::vec::Vec<u8>,
    overlays: alloc::vec::Vec<MemorySegment>,
}

#[derive(Clone, Debug)]
struct MemorySegment {
    base: u32,
    data: alloc::vec::Vec<u8>,
}

impl MemoryMap {
    pub fn new(base: u32) -> Self {
        Self {
            base,
            data: alloc::vec::Vec::new(),
            overlays: alloc::vec::Vec::new(),
        }
    }

    pub fn with_data(base: u32, data: alloc::vec::Vec<u8>) -> Self {
        Self {
            base,
            data,
            overlays: alloc::vec::Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn end(&self) -> u32 {
        self.base.saturating_add(self.data.len() as u32)
    }

    pub fn contains_addr(&self, addr: u32) -> bool {
        if addr >= self.base {
            let off = (addr - self.base) as usize;
            if off < self.data.len() {
                return true;
            }
        }
        self.overlays.iter().any(|seg| {
            if addr < seg.base {
                return false;
            }
            let off = (addr - seg.base) as usize;
            off < seg.data.len()
        })
    }

    pub fn read_u16_be(&self, addr: u32) -> Option<u16> {
        let b0 = self.read_u8(addr)?;
        let b1 = self.read_u8(addr.saturating_add(1))?;
        Some(u16::from_be_bytes([b0, b1]))
    }

    pub fn read_u8(&self, addr: u32) -> Option<u8> {
        if addr >= self.base {
            let off = (addr - self.base) as usize;
            if let Some(v) = self.data.get(off).copied() {
                return Some(v);
            }
        }
        for seg in self.overlays.iter().rev() {
            if addr < seg.base {
                continue;
            }
            let off = (addr - seg.base) as usize;
            if let Some(v) = seg.data.get(off).copied() {
                return Some(v);
            }
        }
        None
    }

    pub fn read_u32_be(&self, addr: u32) -> Option<u32> {
        let b0 = self.read_u8(addr)?;
        let b1 = self.read_u8(addr.saturating_add(1))?;
        let b2 = self.read_u8(addr.saturating_add(2))?;
        let b3 = self.read_u8(addr.saturating_add(3))?;
        Some(u32::from_be_bytes([b0, b1, b2, b3]))
    }

    pub fn write_u8(&mut self, addr: u32, value: u8) -> bool {
        for seg in self.overlays.iter_mut().rev() {
            if addr < seg.base {
                continue;
            }
            let off = (addr - seg.base) as usize;
            if let Some(slot) = seg.data.get_mut(off) {
                *slot = value;
                return true;
            }
        }
        if addr >= self.base {
            let off = (addr - self.base) as usize;
            if let Some(slot) = self.data.get_mut(off) {
                *slot = value;
                return true;
            }
        }
        false
    }

    pub fn write_u16_be(&mut self, addr: u32, value: u16) -> bool {
        let [b0, b1] = value.to_be_bytes();
        self.write_u8(addr, b0) && self.write_u8(addr.saturating_add(1), b1)
    }

    pub fn write_u32_be(&mut self, addr: u32, value: u32) -> bool {
        let [b0, b1, b2, b3] = value.to_be_bytes();
        self.write_u8(addr, b0)
            && self.write_u8(addr.saturating_add(1), b1)
            && self.write_u8(addr.saturating_add(2), b2)
            && self.write_u8(addr.saturating_add(3), b3)
    }

    pub fn upsert_overlay(&mut self, base: u32, data: alloc::vec::Vec<u8>) {
        if let Some(seg) = self.overlays.iter_mut().find(|seg| seg.base == base) {
            seg.data = data;
            return;
        }
        self.overlays.push(MemorySegment { base, data });
    }

    pub fn remove_overlay(&mut self, base: u32) {
        if let Some(pos) = self.overlays.iter().position(|seg| seg.base == base) {
            self.overlays.swap_remove(pos);
        }
    }
}
