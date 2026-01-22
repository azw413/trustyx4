use esp_hal::{
    Blocking,
    analog::adc::{Adc, AdcCalLine, AdcChannel, AdcConfig, AdcPin, Attenuation},
    gpio::{AnalogPin, Input, InputConfig, InputPin},
    peripherals::ADC1,
};
use log::trace;

pub enum Buttons {
    Back,
    Confirm,
    Left,
    Right,
    Up,
    Down,
    Power,
}

const ADC_THRESHOLDS_1: [i16; 4] = [2635, 2015, 1117, 3];
const ADC_THRESHOLDS_2: [i16; 2] = [1680, 3];
const ADC_TOLERANCE: i16 = 400;

type AdcCal<'a> = AdcCalLine<ADC1<'a>>;

pub struct ButtonState<'a, Pin1, Pin2>
where
    Pin1: AdcChannel + AnalogPin,
    Pin2: AdcChannel + AnalogPin,
{
    current: u8,
    previous: u8,
    pin1: AdcPin<Pin1, ADC1<'a>, AdcCal<'a>>,
    pin2: AdcPin<Pin2, ADC1<'a>, AdcCal<'a>>,
    pin_power: Input<'a>,
    adc: Adc<'a, ADC1<'a>, Blocking>,
}

impl<'a, Pin1: AdcChannel + AnalogPin, Pin2: AdcChannel + AnalogPin> ButtonState<'a, Pin1, Pin2> {
    pub fn new(pin1: Pin1, pin2: Pin2, pin_power: impl InputPin + 'a, adc: ADC1<'a>) -> Self {
        let mut adc_config = AdcConfig::new();

        let pin1 = adc_config.enable_pin_with_cal::<_, AdcCal>(pin1, Attenuation::_11dB);
        let pin2 = adc_config.enable_pin_with_cal::<_, AdcCal>(pin2, Attenuation::_11dB);
        let pin_power = Input::new(pin_power, InputConfig::default());
        let adc = Adc::new(adc, adc_config);
        ButtonState {
            current: 0,
            previous: 0,
            pin1,
            pin2,
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
        self.previous = self.current;
        self.current = 0;
        let raw_button1 = nb::block!(self.adc.read_oneshot(&mut self.pin1)).unwrap();
        if let Some(button) = Self::get_button_from_adc(raw_button1 as _, &ADC_THRESHOLDS_1) {
            self.current |= 1 << button;
        }
        let raw_button2 = nb::block!(self.adc.read_oneshot(&mut self.pin2)).unwrap();
        if let Some(button) = Self::get_button_from_adc(raw_button2 as _, &ADC_THRESHOLDS_2) {
            self.current |= 1 << (button + 4);
        }
        if self.pin_power.is_low() {
            self.current |= 1 << 6;
        }
        trace!(
            "Button ADC Readings - Pin1: {}, Pin2: {}, Current State: {:07b}",
            raw_button1, raw_button2, self.current
        );
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
}
