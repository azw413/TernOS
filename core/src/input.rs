#[repr(C)]
#[derive(Clone, Copy)]
pub enum Buttons {
    Back,
    Confirm,
    Left,
    Right,
    Up,
    Down,
    Power,
}

#[derive(Clone, Copy, Default)]
pub struct ButtonState {
    current: u8,
    previous: u8,
    typed: [u16; 16],
    typed_len: u8,
}

impl ButtonState {
    pub fn update(&mut self, current: u8) {
        self.previous = self.current;
        self.current = current;
        self.typed_len = 0;
    }

    pub fn update_with_typed(&mut self, current: u8, typed: &[u16]) {
        self.previous = self.current;
        self.current = current;
        let max = typed.len().min(self.typed.len());
        self.typed_len = max as u8;
        if max > 0 {
            self.typed[..max].copy_from_slice(&typed[..max]);
        }
    }

    fn held(&self) -> u8 {
        self.current & self.previous
    }

    fn pressed(&self) -> u8 {
        self.current & !self.previous
    }

    fn released(&self) -> u8 {
        !self.current & self.previous
    }

    pub fn is_held(&self, button: Buttons) -> bool {
        let mask = 1 << (button as u8);
        (self.held() & mask) != 0
    }

    pub fn is_pressed(&self, button: Buttons) -> bool {
        let mask = 1 << (button as u8);
        (self.pressed() & mask) != 0
    }

    pub fn is_released(&self, button: Buttons) -> bool {
        let mask = 1 << (button as u8);
        (self.released() & mask) != 0
    }

    pub fn typed_chars(&self) -> &[u16] {
        &self.typed[..self.typed_len as usize]
    }
}
