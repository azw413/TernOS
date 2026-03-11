use alloc::vec::Vec;
use esp_hal::{
    Blocking,
    analog::adc::{Adc, AdcCalLine, AdcChannel, AdcConfig, AdcPin, Attenuation},
    gpio::{AnalogPin, Input, InputConfig, InputPin},
    peripherals::ADC1,
};
use log::trace;
use tern_core::input::ButtonState;
use tern_core::platform::{ButtonId, PlatformInputEvent};

const ADC_THRESHOLDS_1: [i16; 4] = [2635, 2015, 1117, 3];
const ADC_THRESHOLDS_2: [i16; 2] = [1680, 3];
const ADC_TOLERANCE: i16 = 400;

type AdcCal<'a> = AdcCalLine<ADC1<'a>>;

pub struct GpioButtonState<'a, Pin1, Pin2, PinBatt>
where
    Pin1: AdcChannel + AnalogPin,
    Pin2: AdcChannel + AnalogPin,
    PinBatt: AdcChannel + AnalogPin,
{
    inner: ButtonState,
    input_events: Vec<PlatformInputEvent>,
    pin1: AdcPin<Pin1, ADC1<'a>, AdcCal<'a>>,
    pin2: AdcPin<Pin2, ADC1<'a>, AdcCal<'a>>,
    pin_batt: AdcPin<PinBatt, ADC1<'a>, AdcCal<'a>>,
    pin_power: Input<'a>,
    adc: Adc<'a, ADC1<'a>, Blocking>,
}

impl<'a, Pin1, Pin2, PinBatt> GpioButtonState<'a, Pin1, Pin2, PinBatt>
where
    Pin1: AdcChannel + AnalogPin,
    Pin2: AdcChannel + AnalogPin,
    PinBatt: AdcChannel + AnalogPin,
{
    pub fn new(
        pin1: Pin1,
        pin2: Pin2,
        pin_batt: PinBatt,
        pin_power: impl InputPin + 'a,
        adc: ADC1<'a>,
    ) -> Self {
        let mut adc_config = AdcConfig::new();

        let pin1 = adc_config.enable_pin_with_cal::<_, AdcCal>(pin1, Attenuation::_11dB);
        let pin2 = adc_config.enable_pin_with_cal::<_, AdcCal>(pin2, Attenuation::_11dB);
        let pin_batt = adc_config.enable_pin_with_cal::<_, AdcCal>(pin_batt, Attenuation::_11dB);
        let pin_power = Input::new(pin_power, InputConfig::default());
        let adc = Adc::new(adc, adc_config);
        GpioButtonState {
            inner: ButtonState::default(),
            input_events: Vec::with_capacity(16),
            pin1,
            pin2,
            pin_batt,
            pin_power,
            adc,
        }
    }

    fn get_button_from_adc(value: i16, thresholds: &[i16]) -> Option<u8> {
        if value > 3800 {
            return None;
        }
        for (i, &threshold) in thresholds.iter().enumerate() {
            if (value - threshold).abs() < ADC_TOLERANCE {
                return Some(i as u8);
            }
        }
        None
    }

    pub fn update(&mut self) {
        self.input_events.clear();
        let mut current: u8 = 0;
        let raw_button1 = nb::block!(self.adc.read_oneshot(&mut self.pin1)).unwrap();
        if let Some(button) = Self::get_button_from_adc(raw_button1 as _, &ADC_THRESHOLDS_1) {
            current |= 1 << button;
        }
        let raw_button2 = nb::block!(self.adc.read_oneshot(&mut self.pin2)).unwrap();
        if let Some(button) = Self::get_button_from_adc(raw_button2 as _, &ADC_THRESHOLDS_2) {
            current |= 1 << (button + 4);
        }
        if self.pin_power.is_low() {
            current |= 1 << 6;
        }
        trace!(
            "Button ADC Readings - Pin1: {}, Pin2: {}, Current State: {:07b}",
            raw_button1, raw_button2, current
        );
        self.inner.update(current);
        self.collect_button_events();
    }

    pub fn get_buttons(&self) -> ButtonState {
        self.inner
    }

    pub fn take_input_events(&mut self) -> Vec<PlatformInputEvent> {
        core::mem::take(&mut self.input_events)
    }

    pub fn read_battery_percent(&mut self) -> Option<u8> {
        const DIVIDER_MULTIPLIER: f32 = 2.0;
        let raw = nb::block!(self.adc.read_oneshot(&mut self.pin_batt)).ok()?;
        let mv = (raw as f32 * DIVIDER_MULTIPLIER) as u16;
        Some(Self::battery_percentage_from_mv(mv))
    }

    fn battery_percentage_from_mv(millivolts: u16) -> u8 {
        let volts = millivolts as f32 / 1000.0;
        let y = -144.9390 * volts * volts * volts
            + 1655.8629 * volts * volts
            - 6158.8520 * volts
            + 7501.3202;
        if y <= 0.0 {
            return 0;
        }
        if y >= 100.0 {
            return 100;
        }
        (y + 0.5) as u8
    }

    fn collect_button_events(&mut self) {
        const BUTTON_MAP: &[(tern_core::input::Buttons, ButtonId)] = &[
            (tern_core::input::Buttons::Left, ButtonId::Left),
            (tern_core::input::Buttons::Right, ButtonId::Right),
            (tern_core::input::Buttons::Up, ButtonId::Up),
            (tern_core::input::Buttons::Down, ButtonId::Down),
            (tern_core::input::Buttons::Confirm, ButtonId::Confirm),
            (tern_core::input::Buttons::Back, ButtonId::Back),
            (tern_core::input::Buttons::Power, ButtonId::Power),
        ];
        for (button, button_id) in BUTTON_MAP {
            if self.inner.is_pressed(*button) {
                self.input_events
                    .push(PlatformInputEvent::ButtonDown(*button_id));
            }
            if self.inner.is_released(*button) {
                self.input_events
                    .push(PlatformInputEvent::ButtonUp(*button_id));
            }
        }
    }
}
