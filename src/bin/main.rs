#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use esp_hal::peripherals::{ADC1, ADC2};
use microreader::eink_display::{EInkDisplay, RefreshMode, Rotation};
use microreader::buttons::*;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::main;
use esp_hal::spi::master::{Config, Spi};
use esp_hal::time::Rate;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig};
use esp_hal::spi::Mode;
use log::info;
use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{Circle, PrimitiveStyle, Rectangle},
    text::Text,
};

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

// Statically allocate frame buffers (96KB total)
const EINK_BUFFER_SIZE: usize = 800 / 8 * 480; // 48000 bytes
static mut FRAME_BUFFER_0: [u8; EINK_BUFFER_SIZE] = [0x0; EINK_BUFFER_SIZE];
static mut FRAME_BUFFER_1: [u8; EINK_BUFFER_SIZE] = [0x0; EINK_BUFFER_SIZE];

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[main]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 66000);
    esp_alloc::heap_allocator!(size: 210000);

    info!("Setting up GPIO pins");
    let cs = Output::new(peripherals.GPIO21, Level::High, OutputConfig::default());
    let dc = Output::new(peripherals.GPIO4, Level::High, OutputConfig::default());
    let busy = Input::new(peripherals.GPIO6, InputConfig::default());
    let rst = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());

    info!("Initializing SPI for E-Ink Display");
    let spi_cfg = Config::default()
        .with_frequency(Rate::from_mhz(40))
        .with_mode(Mode::_0);
    let spi = Spi::new(peripherals.SPI2, spi_cfg)
        .expect("Failed to create SPI")
        .with_sck(peripherals.GPIO8)
        .with_mosi(peripherals.GPIO10);

    let delay = Delay::new();

    info!("SPI initialized");

    // Get mutable references to static frame buffers
    let (frame_buffer_0, frame_buffer_1) = unsafe { 
        (&mut FRAME_BUFFER_0[..], &mut FRAME_BUFFER_1[..])
    };

    // Create E-Ink Display instance
    info!("Creating E-Ink Display driver");
    let mut display = EInkDisplay::new(
        spi,
        cs,
        dc,
        rst,
        busy,
        delay,
        frame_buffer_0,
        frame_buffer_1,
    )
    .expect("Failed to create E-Ink Display");

    // Initialize the display
    display.begin().expect("Failed to initialize display");

    // Clear screen to white
    info!("Clearing screen");
    display.clear(BinaryColor::Off).ok();

    info!("Drawing with embedded_graphics");

    let mut button_state = ButtonState::new(peripherals.GPIO1, peripherals.GPIO2, peripherals.GPIO3, peripherals.ADC1);
    let mut dirty = true;
    
    info!("Display complete! Starting rotation demo...");

    // Cycle through rotations every second
    let rotations = [Rotation::Rotate0, Rotation::Rotate90, Rotation::Rotate180, Rotation::Rotate270];
    let mut rotation_index = 3;

    loop {
        delay.delay_millis(10);

        button_state.update();
        if button_state.is_pressed(Buttons::Left) {
            rotation_index = (rotation_index + rotations.len() - 1) % rotations.len();
            info!("Button Left Pressed");
        } else if button_state.is_pressed(Buttons::Right) {
            rotation_index = (rotation_index + 1) % rotations.len();
            info!("Button Right Pressed");
        } else if !dirty {
            continue;
        }
        
        let new_rotation = rotations[rotation_index];
        
        info!("Setting rotation to {:?}", new_rotation);
        display.set_rotation(new_rotation);
        
        // Clear and redraw with new rotation
        display.clear(BinaryColor::Off).ok();
        
        // Get the current display size (changes with rotation)
        let size = display.size() - Size::new(20, 20);
        
        // Draw a border rectangle that fits the rotated display
        Rectangle::new(Point::new(10, 10), size)
            .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 2))
            .draw(&mut display)
            .ok();

        // Draw some circles
        Circle::new(Point::new(100, 100), 80)
            .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 3))
            .draw(&mut display)
            .ok();

        Circle::new(Point::new(200, 100), 60)
            .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
            .draw(&mut display)
            .ok();

        // Draw text
        let text_style = MonoTextStyle::new(&FONT_10X20, BinaryColor::On);
        Text::new("Hello from rust", Point::new(20, 30), text_style)
            .draw(&mut display)
            .ok();

        display.display_buffer(RefreshMode::Fast).ok();
        dirty = false;
    }
}
