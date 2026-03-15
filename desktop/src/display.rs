use log::info;
use tern_core::platform::{
    ButtonId, DisplayCaps, DisplayDensity, DisplayDevice, DisplayRotation, LogicalStyle,
    PlatformInputEvent,
};
use tern_core::{
    display::{DamageOverlayKind, DamageOverlayRect, GrayscaleMode, HEIGHT, RefreshMode, WIDTH},
    framebuffer::DisplayBuffers,
    input::{ButtonState, Buttons},
};

const BUFFER_SIZE: usize = WIDTH * HEIGHT / 8;
const DISPLAY_BUFFER_SIZE: usize = WIDTH * HEIGHT;

pub struct MinifbDisplay {
    is_grayscale: bool,
    // Simulated EInk buffers
    lsb_buffer: [u8; BUFFER_SIZE],
    msb_buffer: [u8; BUFFER_SIZE],
    // Actual display buffer
    display_buffer: [u32; DISPLAY_BUFFER_SIZE],
    window: minifb::Window,
    buttons: ButtonState,
    input_events: Vec<PlatformInputEvent>,
    mouse_down: bool,
    mouse_pos: Option<(i32, i32)>,
    cursor_restore: Vec<(usize, u32)>,
    damage_overlay: Vec<DamageOverlayRect>,
    damage_restore: Vec<(usize, u32)>,
}

#[derive(PartialEq, Eq, Debug)]
enum BlitMode {
    // Blit the active framebuffer as full black/white
    Full,
    Partial,
    // Blit the difference between LSB and MSB buffers
    Grayscale,
    // Render grayscale directly from LSB/MSB buffers
    GrayscaleOneshot,
    // Revert Greyscale to black/white
    GrayscaleRevert,
}

impl MinifbDisplay {
    pub fn new(window: minifb::Window) -> Self {
        let mut ret = Self {
            is_grayscale: false,
            lsb_buffer: [0; BUFFER_SIZE],
            msb_buffer: [0; BUFFER_SIZE],
            display_buffer: [0; DISPLAY_BUFFER_SIZE],
            window,
            buttons: ButtonState::default(),
            input_events: Vec::new(),
            mouse_down: false,
            mouse_pos: None,
            cursor_restore: Vec::new(),
            damage_overlay: Vec::new(),
            damage_restore: Vec::new(),
        };

        ret.display_buffer.fill(0xFFFFFFFF);

        ret
    }

    pub fn is_open(&self) -> bool {
        self.window.is_open() && !self.window.is_key_down(minifb::Key::Escape)
    }

    pub fn update_display(&mut self /*, window: &mut minifb::Window */) {
        self.restore_damage_overlay();
        self.draw_damage_overlay();
        self.restore_cursor_overlay();
        self.draw_cursor_overlay();
        self.window
            .update_with_buffer(&self.display_buffer, HEIGHT, WIDTH)
            .unwrap();
    }

    pub fn update(&mut self) {
        self.window.update();
        self.input_events.clear();
        let mut current: u8 = 0;
        let mut typed: [u16; 16] = [0; 16];
        let mut typed_len = 0usize;
        let pressed_keys = self.window.get_keys_pressed(minifb::KeyRepeat::No);
        let key_pressed = |key: minifb::Key| pressed_keys.contains(&key);
        if self.window.is_key_down(minifb::Key::Left) || key_pressed(minifb::Key::Left) {
            current |= 1 << (Buttons::Left as u8);
        }
        if self.window.is_key_down(minifb::Key::Right) || key_pressed(minifb::Key::Right) {
            current |= 1 << (Buttons::Right as u8);
        }
        if self.window.is_key_down(minifb::Key::Up) || key_pressed(minifb::Key::Up) {
            current |= 1 << (Buttons::Up as u8);
        }
        if self.window.is_key_down(minifb::Key::Down) || key_pressed(minifb::Key::Down) {
            current |= 1 << (Buttons::Down as u8);
        }
        if self.window.is_key_down(minifb::Key::Enter)
            || key_pressed(minifb::Key::Enter)
            || key_pressed(minifb::Key::Space)
        {
            current |= 1 << (Buttons::Confirm as u8);
        }
        if self.window.is_key_down(minifb::Key::Backspace) || key_pressed(minifb::Key::Backspace) {
            current |= 1 << (Buttons::Back as u8);
        }
        if self.window.is_key_down(minifb::Key::P) || key_pressed(minifb::Key::P) {
            current |= 1 << (Buttons::Power as u8);
        }
        for key in self.window.get_keys_pressed(minifb::KeyRepeat::Yes) {
            if typed_len >= typed.len() {
                break;
            }
            if let Some(ch) = map_key_to_char(
                key,
                self.window.is_key_down(minifb::Key::LeftShift)
                    || self.window.is_key_down(minifb::Key::RightShift),
            ) {
                typed[typed_len] = ch;
                typed_len += 1;
                self.input_events.push(PlatformInputEvent::KeyDown {
                    chr: ch,
                    key_code: ch,
                    modifiers: 0,
                });
            }
        }
        if let Some((mx, my)) = self.window.get_mouse_pos(minifb::MouseMode::Clamp) {
            let x = mx.round() as i32;
            let y = my.round() as i32;
            self.mouse_pos = Some((x, y));
            let left_down = self.window.get_mouse_down(minifb::MouseButton::Left);
            if left_down && !self.mouse_down {
                self.input_events.push(PlatformInputEvent::TouchDown { x, y });
            } else if left_down && self.mouse_down {
                self.input_events.push(PlatformInputEvent::TouchMove { x, y });
            } else if !left_down && self.mouse_down {
                self.input_events.push(PlatformInputEvent::TouchUp { x, y });
            }
            self.mouse_down = left_down;
        } else if self.mouse_down {
            self.mouse_down = false;
            self.mouse_pos = None;
        } else {
            self.mouse_pos = None;
        }
        self.buttons.update_with_typed(current, &typed[..typed_len]);
        self.collect_button_events();
    }

    pub fn get_buttons(&self) -> ButtonState {
        self.buttons
    }

    pub fn take_input_events(&mut self) -> Vec<PlatformInputEvent> {
        core::mem::take(&mut self.input_events)
    }

    fn collect_button_events(&mut self) {
        const BUTTON_MAP: &[(Buttons, ButtonId)] = &[
            (Buttons::Left, ButtonId::Left),
            (Buttons::Right, ButtonId::Right),
            (Buttons::Up, ButtonId::Up),
            (Buttons::Down, ButtonId::Down),
            (Buttons::Confirm, ButtonId::Confirm),
            (Buttons::Back, ButtonId::Back),
            (Buttons::Power, ButtonId::Power),
        ];
        for (button, button_id) in BUTTON_MAP {
            if self.buttons.is_pressed(*button) {
                self.input_events
                    .push(PlatformInputEvent::ButtonDown(*button_id));
            }
            if self.buttons.is_released(*button) {
                self.input_events
                    .push(PlatformInputEvent::ButtonUp(*button_id));
            }
        }
    }

    fn blit_internal(&mut self, mode: BlitMode) {
        info!("Blitting with mode: {:?}", mode);
        match mode {
            BlitMode::Full => {
                let fb = self.lsb_buffer;
                for (i, byte) in fb.iter().enumerate() {
                    for bit in 0..8 {
                        let pixel_index = i * 8 + bit;
                        let pixel_value = if (byte & (1 << (7 - bit))) != 0 {
                            0xFFFFFFFF
                        } else {
                            0xFF000000
                        };
                        self.set_portrait_pixel(pixel_index, pixel_value);
                    }
                }
            }
            BlitMode::Partial => {
                for i in 0..self.lsb_buffer.len() {
                    let curr_byte = self.lsb_buffer[i];
                    let prev_byte = self.msb_buffer[i];
                    for bit in 0..8 {
                        let current_bit = (curr_byte >> (7 - bit)) & 0x01;
                        let previous_bit = (prev_byte >> (7 - bit)) & 0x01;
                        if current_bit == previous_bit {
                            continue;
                        }
                        if current_bit == 1 {
                            let pixel_index = i * 8 + bit;
                            self.set_portrait_pixel(pixel_index, 0xFFFFFFFF);
                        } else {
                            let pixel_index = i * 8 + bit;
                            self.set_portrait_pixel(pixel_index, 0xFF000000);
                        }
                    }
                }
            }
            BlitMode::Grayscale => {
                for i in 0..self.lsb_buffer.len() {
                    let lsb_byte = self.lsb_buffer[i];
                    let msb_byte = self.msb_buffer[i];
                    for bit in 0..8 {
                        let pixel_index = i * 8 + bit;
                        let lsb_bit = (lsb_byte >> (7 - bit)) & 0x01;
                        let msb_bit = (msb_byte >> (7 - bit)) & 0x01;
                        let current_pixel = self.get_portrait_pixel(pixel_index);
                        let new_pixel = match (msb_bit, lsb_bit) {
                            (0, 0) => continue,
                            (0, 1) => current_pixel.saturating_sub(0x555555), // Black -> Dark Gray
                            (1, 0) => current_pixel.saturating_sub(0xAAAAAA), // Black -> Gray
                            (1, 1) => current_pixel.saturating_add(0x333333), // White -> Light Gray
                            _ => unreachable!(),
                        };
                        self.set_portrait_pixel(pixel_index, new_pixel);
                    }
                }
            }
            BlitMode::GrayscaleOneshot => {
                for i in 0..self.lsb_buffer.len() {
                    let lsb_byte = self.lsb_buffer[i];
                    let msb_byte = self.msb_buffer[i];
                    for bit in 0..8 {
                        let pixel_index = i * 8 + bit;
                        let lsb_bit = (lsb_byte >> (7 - bit)) & 0x01;
                        let msb_bit = (msb_byte >> (7 - bit)) & 0x01;
                        let new_pixel = match (msb_bit, lsb_bit) {
                            (0, 0) => 0xFFFFFFFF,
                            (0, 1) => 0xFFAAAAAA,
                            (1, 0) => 0xFF555555,
                            (1, 1) => 0xFF000000,
                            _ => unreachable!(),
                        };
                        self.set_portrait_pixel(pixel_index, new_pixel);
                    }
                }
            }
            BlitMode::GrayscaleRevert => {
                for i in 0..self.lsb_buffer.len() {
                    let lsb_byte = self.lsb_buffer[i];
                    let msb_byte = self.msb_buffer[i];
                    for bit in 0..8 {
                        let pixel_index = i * 8 + bit;
                        let lsb_bit = (lsb_byte >> (7 - bit)) & 0x01;
                        let msb_bit = (msb_byte >> (7 - bit)) & 0x01;
                        let current_pixel = self.get_portrait_pixel(pixel_index);
                        let new_pixel = match (msb_bit, lsb_bit) {
                            (0, 0) => continue,
                            (0, 1) => current_pixel.saturating_add(0x555555), // Dark Gray  -> Black
                            (1, 0) => current_pixel.saturating_add(0xAAAAAA), // Gray       -> Black
                            (1, 1) => current_pixel.saturating_sub(0x333333), // Light Gray -> White
                            _ => unreachable!(),
                        };
                        self.set_portrait_pixel(pixel_index, new_pixel);
                    }
                }
            }
        }
        self.update_display();
    }

    fn set_portrait_pixel(&mut self, landscape_index: usize, color: u32) {
        let x_land = (landscape_index % WIDTH) as i32;
        let y_land = (landscape_index / WIDTH) as i32;
        let x_portrait = (HEIGHT as i32 - 1) - y_land;
        let y_portrait = x_land;
        if x_portrait < 0 || y_portrait < 0 {
            return;
        }
        let x_portrait = x_portrait as usize;
        let y_portrait = y_portrait as usize;
        let idx = y_portrait * HEIGHT + x_portrait;
        if idx < self.display_buffer.len() {
            self.display_buffer[idx] = color;
        }
    }

    fn get_portrait_pixel(&self, landscape_index: usize) -> u32 {
        let x_land = (landscape_index % WIDTH) as i32;
        let y_land = (landscape_index / WIDTH) as i32;
        let x_portrait = (HEIGHT as i32 - 1) - y_land;
        let y_portrait = x_land;
        if x_portrait < 0 || y_portrait < 0 {
            return 0xFFFFFFFF;
        }
        let x_portrait = x_portrait as usize;
        let y_portrait = y_portrait as usize;
        let idx = y_portrait * HEIGHT + x_portrait;
        if idx < self.display_buffer.len() {
            self.display_buffer[idx]
        } else {
            0xFFFFFFFF
        }
    }

    fn restore_cursor_overlay(&mut self) {
        for (idx, color) in self.cursor_restore.drain(..) {
            if idx < self.display_buffer.len() {
                self.display_buffer[idx] = color;
            }
        }
    }

    fn restore_damage_overlay(&mut self) {
        for (idx, color) in self.damage_restore.drain(..) {
            if idx < self.display_buffer.len() {
                self.display_buffer[idx] = color;
            }
        }
    }

    fn draw_cursor_overlay(&mut self) {
        let Some((x, y)) = self.mouse_pos else {
            return;
        };
        for dy in -6..=6 {
            self.draw_cursor_pixel(x, y + dy, if dy == 0 { 0xFFFF0000 } else { 0xFF000000 });
        }
        for dx in -6..=6 {
            self.draw_cursor_pixel(x + dx, y, if dx == 0 { 0xFFFF0000 } else { 0xFF000000 });
        }
        self.draw_cursor_pixel(x, y, 0xFFFFFFFF);
    }

    fn draw_cursor_pixel(&mut self, x: i32, y: i32, color: u32) {
        if x < 0 || y < 0 || x >= HEIGHT as i32 || y >= WIDTH as i32 {
            return;
        }
        let idx = y as usize * HEIGHT + x as usize;
        if idx >= self.display_buffer.len() {
            return;
        }
        self.cursor_restore.push((idx, self.display_buffer[idx]));
        self.display_buffer[idx] = color;
    }

    fn draw_damage_overlay(&mut self) {
        let overlay = self.damage_overlay.clone();
        for item in overlay {
            self.draw_damage_rect(item);
        }
    }

    fn draw_damage_rect(&mut self, overlay: DamageOverlayRect) {
        let color = match overlay.kind {
            DamageOverlayKind::Old => 0xFFFF0000,
            DamageOverlayKind::New => 0xFF00AA00,
            DamageOverlayKind::Exposed => 0xFF0000FF,
            DamageOverlayKind::Presented => 0xFFFFA500,
        };
        let pattern = match overlay.kind {
            DamageOverlayKind::Old => 3,
            DamageOverlayKind::New => 1,
            DamageOverlayKind::Exposed => 2,
            DamageOverlayKind::Presented => 4,
        };
        let rect = overlay.rect;
        if rect.w <= 0 || rect.h <= 0 {
            return;
        }
        let x0 = rect.x;
        let y0 = rect.y;
        let x1 = rect.x + rect.w - 1;
        let y1 = rect.y + rect.h - 1;
        for x in x0..=x1 {
            if (x - x0) % pattern == 0 {
                self.draw_damage_pixel(x, y0, color);
                self.draw_damage_pixel(x, y1, color);
            }
        }
        for y in y0..=y1 {
            if (y - y0) % pattern == 0 {
                self.draw_damage_pixel(x0, y, color);
                self.draw_damage_pixel(x1, y, color);
            }
        }
        if matches!(overlay.kind, DamageOverlayKind::Presented) && rect.w > 2 && rect.h > 2 {
            for x in (x0 + 1)..=(x1 - 1) {
                if (x - x0) % pattern == 0 {
                    self.draw_damage_pixel(x, y0 + 1, color);
                    self.draw_damage_pixel(x, y1 - 1, color);
                }
            }
            for y in (y0 + 1)..=(y1 - 1) {
                if (y - y0) % pattern == 0 {
                    self.draw_damage_pixel(x0 + 1, y, color);
                    self.draw_damage_pixel(x1 - 1, y, color);
                }
            }
        }
    }

    fn draw_damage_pixel(&mut self, x: i32, y: i32, color: u32) {
        if x < 0 || y < 0 || x >= HEIGHT as i32 || y >= WIDTH as i32 {
            return;
        }
        let idx = y as usize * HEIGHT + x as usize;
        if idx >= self.display_buffer.len() {
            return;
        }
        self.damage_restore.push((idx, self.display_buffer[idx]));
        self.display_buffer[idx] = color;
    }
}

fn map_key_to_char(key: minifb::Key, shifted: bool) -> Option<u16> {
    use minifb::Key;
    let ch = match key {
        Key::Space => ' ',
        Key::A => if shifted { 'A' } else { 'a' },
        Key::B => if shifted { 'B' } else { 'b' },
        Key::C => if shifted { 'C' } else { 'c' },
        Key::D => if shifted { 'D' } else { 'd' },
        Key::E => if shifted { 'E' } else { 'e' },
        Key::F => if shifted { 'F' } else { 'f' },
        Key::G => if shifted { 'G' } else { 'g' },
        Key::H => if shifted { 'H' } else { 'h' },
        Key::I => if shifted { 'I' } else { 'i' },
        Key::J => if shifted { 'J' } else { 'j' },
        Key::K => if shifted { 'K' } else { 'k' },
        Key::L => if shifted { 'L' } else { 'l' },
        Key::M => if shifted { 'M' } else { 'm' },
        Key::N => if shifted { 'N' } else { 'n' },
        Key::O => if shifted { 'O' } else { 'o' },
        Key::P => if shifted { 'P' } else { 'p' },
        Key::Q => if shifted { 'Q' } else { 'q' },
        Key::R => if shifted { 'R' } else { 'r' },
        Key::S => if shifted { 'S' } else { 's' },
        Key::T => if shifted { 'T' } else { 't' },
        Key::U => if shifted { 'U' } else { 'u' },
        Key::V => if shifted { 'V' } else { 'v' },
        Key::W => if shifted { 'W' } else { 'w' },
        Key::X => if shifted { 'X' } else { 'x' },
        Key::Y => if shifted { 'Y' } else { 'y' },
        Key::Z => if shifted { 'Z' } else { 'z' },
        Key::Key0 => if shifted { ')' } else { '0' },
        Key::Key1 => if shifted { '!' } else { '1' },
        Key::Key2 => if shifted { '@' } else { '2' },
        Key::Key3 => if shifted { '#' } else { '3' },
        Key::Key4 => if shifted { '$' } else { '4' },
        Key::Key5 => if shifted { '%' } else { '5' },
        Key::Key6 => if shifted { '^' } else { '6' },
        Key::Key7 => if shifted { '&' } else { '7' },
        Key::Key8 => if shifted { '*' } else { '8' },
        Key::Key9 => if shifted { '(' } else { '9' },
        Key::Backspace => '\u{0008}',
        Key::Comma => if shifted { '<' } else { ',' },
        Key::Period => if shifted { '>' } else { '.' },
        Key::Slash => if shifted { '?' } else { '/' },
        Key::Semicolon => if shifted { ':' } else { ';' },
        Key::Apostrophe => if shifted { '"' } else { '\'' },
        Key::LeftBracket => if shifted { '{' } else { '[' },
        Key::RightBracket => if shifted { '}' } else { ']' },
        Key::Backslash => if shifted { '|' } else { '\\' },
        Key::Minus => if shifted { '_' } else { '-' },
        Key::Equal => if shifted { '+' } else { '=' },
        _ => return None,
    };
    Some(ch as u16)
}

impl tern_core::display::Display for MinifbDisplay {
    fn display(&mut self, buffers: &mut DisplayBuffers, mode: RefreshMode) {
        // revert grayscale first
        if self.is_grayscale {
            self.blit_internal(BlitMode::GrayscaleRevert);
            self.is_grayscale = false;
        }

        let current = buffers.get_active_buffer();
        let previous = buffers.get_inactive_buffer();
        self.lsb_buffer.copy_from_slice(&current[..]);
        self.msb_buffer.copy_from_slice(&previous[..]);
        if mode == RefreshMode::Fast {
            self.blit_internal(BlitMode::Partial);
        } else {
            self.blit_internal(BlitMode::Full);
        }
        buffers.swap_buffers();
    }
    fn set_damage_overlay(&mut self, overlay: &[DamageOverlayRect]) {
        self.damage_overlay.clear();
        self.damage_overlay.extend_from_slice(overlay);
    }
    fn copy_to_lsb(&mut self, buffers: &[u8; BUFFER_SIZE]) {
        self.lsb_buffer.copy_from_slice(buffers);
    }
    fn copy_to_msb(&mut self, buffers: &[u8; BUFFER_SIZE]) {
        self.msb_buffer.copy_from_slice(buffers);
    }
    fn copy_grayscale_buffers(&mut self, lsb: &[u8; BUFFER_SIZE], msb: &[u8; BUFFER_SIZE]) {
        self.lsb_buffer.copy_from_slice(lsb);
        self.msb_buffer.copy_from_slice(msb);
    }
    fn display_differential_grayscale(&mut self, _turn_off_screen: bool) {
        self.is_grayscale = true;
        self.blit_internal(BlitMode::Grayscale);
    }
    fn display_absolute_grayscale(&mut self, _: GrayscaleMode) {
        self.blit_internal(BlitMode::GrayscaleOneshot);
    }
}

impl DisplayDevice for MinifbDisplay {
    fn size_px(&self) -> (u32, u32) {
        (WIDTH as u32, HEIGHT as u32)
    }

    fn logical_density(&self) -> DisplayDensity {
        DisplayDensity::DeviceNative
    }

    fn caps(&self) -> DisplayCaps {
        DisplayCaps {
            partial_refresh: true,
            gray_levels: 16,
            bits_per_pixel: 32,
            rotation: DisplayRotation::Rotate90,
            logical_style: LogicalStyle::TernPortrait,
        }
    }

    fn present(&mut self, _mode: RefreshMode) {
        self.update_display();
    }
}
