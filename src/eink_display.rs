//! SSD1677 E-Ink Display Driver
//! 
//! This module provides a driver for the SSD1677 e-ink display controller
//! optimized for the GDEQ0426T82 4.26" 800x480 e-paper display.

use esp_hal::delay::Delay;
use embedded_hal::digital::{InputPin, OutputPin};
use embedded_graphics::{
    pixelcolor::BinaryColor,
    prelude::*,
    Pixel,
};
use log::{info, error};

// SSD1677 Command Definitions
#[allow(dead_code)]
mod commands {
    // Initialization and reset
    pub const SOFT_RESET: u8 = 0x12;
    pub const BOOSTER_SOFT_START: u8 = 0x0C;
    pub const DRIVER_OUTPUT_CONTROL: u8 = 0x01;
    pub const BORDER_WAVEFORM: u8 = 0x3C;
    pub const TEMP_SENSOR_CONTROL: u8 = 0x18;

    // RAM and buffer management
    pub const DATA_ENTRY_MODE: u8 = 0x11;
    pub const SET_RAM_X_RANGE: u8 = 0x44;
    pub const SET_RAM_Y_RANGE: u8 = 0x45;
    pub const SET_RAM_X_COUNTER: u8 = 0x4E;
    pub const SET_RAM_Y_COUNTER: u8 = 0x4F;
    pub const WRITE_RAM_BW: u8 = 0x24;
    pub const WRITE_RAM_RED: u8 = 0x26;
    pub const AUTO_WRITE_BW_RAM: u8 = 0x46;
    pub const AUTO_WRITE_RED_RAM: u8 = 0x47;

    // Display update and refresh
    pub const DISPLAY_UPDATE_CTRL1: u8 = 0x21;
    pub const DISPLAY_UPDATE_CTRL2: u8 = 0x22;
    pub const MASTER_ACTIVATION: u8 = 0x20;

    // LUT and voltage settings
    pub const WRITE_LUT: u8 = 0x32;
    pub const GATE_VOLTAGE: u8 = 0x03;
    pub const SOURCE_VOLTAGE: u8 = 0x04;
    pub const WRITE_VCOM: u8 = 0x2C;
    pub const WRITE_TEMP: u8 = 0x1A;

    // Power management
    pub const DEEP_SLEEP: u8 = 0x10;
}

// Display update control modes
const CTRL1_NORMAL: u8 = 0x00;
const CTRL1_BYPASS_RED: u8 = 0x40;

// Data entry mode
const DATA_ENTRY_X_INC_Y_DEC: u8 = 0x01;

// Temperature sensor control
const TEMP_SENSOR_INTERNAL: u8 = 0x80;

/// Custom LUT for grayscale fast refresh
#[allow(dead_code)]
const LUT_GRAYSCALE: &[u8] = &[
    // 00 black/white
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // 01 light gray
    0x54, 0x54, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // 10 gray
    0xAA, 0xA0, 0xA8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // 11 dark gray
    0xA2, 0x22, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // L4 (VCOM)
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // TP/RP groups (global timing)
    0x01, 0x01, 0x01, 0x01, 0x00,  // G0
    0x01, 0x01, 0x01, 0x01, 0x00,  // G1
    0x01, 0x01, 0x01, 0x01, 0x00,  // G2
    0x00, 0x00, 0x00, 0x00, 0x00,  // G3
    0x00, 0x00, 0x00, 0x00, 0x00,  // G4
    0x00, 0x00, 0x00, 0x00, 0x00,  // G5
    0x00, 0x00, 0x00, 0x00, 0x00,  // G6
    0x00, 0x00, 0x00, 0x00, 0x00,  // G7
    0x00, 0x00, 0x00, 0x00, 0x00,  // G8
    0x00, 0x00, 0x00, 0x00, 0x00,  // G9
    // Frame rate
    0x8F, 0x8F, 0x8F, 0x8F, 0x8F,
    // Voltages (VGH, VSH1, VSH2, VSL, VCOM)
    0x17, 0x41, 0xA8, 0x32, 0x30,
    // Reserved
    0x00, 0x00,
];

/// Refresh modes for the display
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum RefreshMode {
    /// Full refresh with complete waveform
    Full,
    /// Half refresh (1720ms) - balanced quality and speed
    Half,
    /// Fast refresh using custom LUT
    Fast,
}

/// Display rotation/orientation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rotation {
    /// No rotation (landscape, 800x480)
    Rotate0,
    /// 90° clockwise (portrait, 480x800)
    Rotate90,
    /// 180° rotation (landscape upside-down, 800x480)
    Rotate180,
    /// 270° clockwise / 90° counter-clockwise (portrait, 480x800)
    Rotate270,
}

/// E-Ink Display driver for SSD1677
pub struct EInkDisplay<'d, SPI, CS, DC, RST, BUSY>
where
    SPI: embedded_hal::spi::SpiBus,
    CS: OutputPin,
    DC: OutputPin,
    RST: OutputPin,
    BUSY: InputPin,
{
    spi: SPI,
    cs: CS,
    dc: DC,
    rst: RST,
    busy: BUSY,
    delay: Delay,
    frame_buffer_0: &'d mut [u8],
    frame_buffer_1: &'d mut [u8],
    active_buffer: bool, // false = buffer_0, true = buffer_1
    is_screen_on: bool,
    custom_lut_active: bool,
    in_grayscale_mode: bool,
    rotation: Rotation,
}

impl<'d, SPI, CS, DC, RST, BUSY> EInkDisplay<'d, SPI, CS, DC, RST, BUSY>
where
    SPI: embedded_hal::spi::SpiBus,
    CS: OutputPin,
    DC: OutputPin,
    RST: OutputPin,
    BUSY: InputPin,
{
    /// Display dimensions
    pub const WIDTH: usize = 800;
    pub const HEIGHT: usize = 480;
    pub const WIDTH_BYTES: usize = Self::WIDTH / 8;
    pub const BUFFER_SIZE: usize = Self::WIDTH_BYTES * Self::HEIGHT;

    /// Create a new EInkDisplay instance
    pub fn new(
        spi: SPI,
        cs: CS,
        dc: DC,
        rst: RST,
        busy: BUSY,
        delay: Delay,
        frame_buffer_0: &'d mut [u8],
        frame_buffer_1: &'d mut [u8],
    ) -> Result<Self, &'static str> {
        if frame_buffer_0.len() < Self::BUFFER_SIZE || frame_buffer_1.len() < Self::BUFFER_SIZE {
            return Err("Frame buffers too small");
        }

        // Initialize buffers to white
        frame_buffer_0.fill(0xFF);
        frame_buffer_1.fill(0xFF);

        Ok(Self {
            spi,
            cs,
            dc,
            rst,
            busy,
            delay,
            frame_buffer_0,
            frame_buffer_1,
            active_buffer: false,
            is_screen_on: false,
            custom_lut_active: false,
            in_grayscale_mode: false,
            rotation: Rotation::Rotate0,
        })
    }

    /// Initialize the display
    pub fn begin(&mut self) -> Result<(), &'static str> {
        info!("Initializing E-Ink Display");

        // Reset display
        self.reset_display();

        // Initialize display controller
        self.init_display_controller()?;

        info!("E-Ink Display initialized");
        Ok(())
    }

    /// Get reference to the current frame buffer
    pub fn frame_buffer(&mut self) -> &mut [u8] {
        if self.active_buffer {
            &mut self.frame_buffer_1[..Self::BUFFER_SIZE]
        } else {
            &mut self.frame_buffer_0[..Self::BUFFER_SIZE]
        }
    }

    /// Clear the current frame buffer
    pub fn clear_screen(&mut self, color: u8) {
        self.frame_buffer().fill(color);
    }

    /// Swap the active frame buffer
    pub fn swap_buffers(&mut self) {
        self.active_buffer = !self.active_buffer;
    }

    /// Set the display rotation
    pub fn set_rotation(&mut self, rotation: Rotation) {
        self.rotation = rotation;
    }

    /// Get the current rotation
    pub fn rotation(&self) -> Rotation {
        self.rotation
    }

    /// Display the current frame buffer
    pub fn display_buffer(&mut self, mode: RefreshMode) -> Result<(), &'static str> {
        let mut actual_mode = mode;
        
        if !self.is_screen_on {
            // Force half refresh if screen is off
            actual_mode = RefreshMode::Half;
        }

        // If currently in grayscale mode, revert first to black/white
        if self.in_grayscale_mode {
            self.in_grayscale_mode = false;
            // Note: grayscale revert not fully implemented in this basic version
        }

        // Set up full screen RAM area
        self.set_ram_area(0, 0, Self::WIDTH as u16, Self::HEIGHT as u16)?;

        // Get raw pointers to avoid borrow checker issues
        let current_ptr = if self.active_buffer {
            self.frame_buffer_1.as_ptr()
        } else {
            self.frame_buffer_0.as_ptr()
        };
        
        let previous_ptr = if self.active_buffer {
            self.frame_buffer_0.as_ptr()
        } else {
            self.frame_buffer_1.as_ptr()
        };

        // SAFETY: We know the pointers are valid and we're only reading from them
        // We're not modifying the buffers during this operation
        unsafe {
            let current_slice = core::slice::from_raw_parts(current_ptr, Self::BUFFER_SIZE);
            let previous_slice = core::slice::from_raw_parts(previous_ptr, Self::BUFFER_SIZE);
            
            match actual_mode {
                RefreshMode::Full | RefreshMode::Half => {
                    // For full refresh, write current buffer to both RAM buffers
                    self.write_ram_buffer(commands::WRITE_RAM_BW, current_slice)?;
                    self.write_ram_buffer(commands::WRITE_RAM_RED, current_slice)?;
                }
                RefreshMode::Fast => {
                    // For fast refresh, write current to BW and previous to RED
                    self.write_ram_buffer(commands::WRITE_RAM_BW, current_slice)?;
                    self.write_ram_buffer(commands::WRITE_RAM_RED, previous_slice)?;
                }
            }
        }

        // Swap active buffer for next time
        self.swap_buffers();

        // Refresh the display
        self.refresh_display(actual_mode, false)?;

        Ok(())
    }

    /// Enter deep sleep mode
    pub fn deep_sleep(&mut self) -> Result<(), &'static str> {
        info!("Entering deep sleep mode");
        self.send_command(commands::DEEP_SLEEP)?;
        self.send_data(&[0x01])?;
        Ok(())
    }

    // ========================================================================
    // Low-level display control methods
    // ========================================================================

    fn reset_display(&mut self) {
        info!("Resetting display");
        let _ = self.rst.set_high();
        self.delay.delay_millis(20);
        let _ = self.rst.set_low();
        self.delay.delay_millis(2);
        let _ = self.rst.set_high();
        self.delay.delay_millis(20);
        info!("Display reset complete");
    }

    fn send_command(&mut self, command: u8) -> Result<(), &'static str> {
        let _ = self.dc.set_low(); // Command mode
        let _ = self.cs.set_low();
        self.spi.write(&[command]).map_err(|_| "SPI write failed")?;
        self.spi.flush().map_err(|_| "SPI flush failed")?;
        let _ = self.cs.set_high();
        Ok(())
    }

    fn send_data(&mut self, data: &[u8]) -> Result<(), &'static str> {
        let _ = self.dc.set_high(); // Data mode
        let _ = self.cs.set_low();
        self.spi.write(data).map_err(|_| "SPI write failed")?;
        self.spi.flush().map_err(|_| "SPI flush failed")?;
        let _ = self.cs.set_high();
        Ok(())
    }

    fn wait_while_busy(&mut self, comment: &str) {
        let mut iterations = 0u32;
        while self.busy.is_high().unwrap_or(false) {
            self.delay.delay_millis(1);
            iterations += 1;
            if iterations > 10000 {
                error!("Timeout waiting for busy: {}", comment);
                break;
            }
        }
        info!("Wait complete: {} ({} ms)", comment, iterations);
    }

    fn init_display_controller(&mut self) -> Result<(), &'static str> {
        info!("Initializing SSD1677 controller");

        // Soft reset
        self.send_command(commands::SOFT_RESET)?;
        self.wait_while_busy("SOFT_RESET");

        // Temperature sensor control (internal)
        self.send_command(commands::TEMP_SENSOR_CONTROL)?;
        self.send_data(&[TEMP_SENSOR_INTERNAL])?;

        // Booster soft-start control (GDEQ0426T82 specific values)
        self.send_command(commands::BOOSTER_SOFT_START)?;
        self.send_data(&[0xAE, 0xC7, 0xC3, 0xC0, 0x40])?;

        // Driver output control: set display height (480) and scan direction
        let height: u16 = 480;
        self.send_command(commands::DRIVER_OUTPUT_CONTROL)?;
        self.send_data(&[
            ((height - 1) % 256) as u8,  // gates A0..A7 (low byte)
            ((height - 1) / 256) as u8,  // gates A8..A9 (high byte)
            0x02,                         // SM=1 (interlaced), TB=0
        ])?;

        // Border waveform control
        self.send_command(commands::BORDER_WAVEFORM)?;
        self.send_data(&[0x01])?;

        // Set up full screen RAM area
        self.set_ram_area(0, 0, Self::WIDTH as u16, Self::HEIGHT as u16)?;

        // Clear RAM buffers
        info!("Clearing RAM buffers");
        self.send_command(commands::AUTO_WRITE_BW_RAM)?;
        self.send_data(&[0xF7])?;
        self.wait_while_busy("AUTO_WRITE_BW_RAM");

        self.send_command(commands::AUTO_WRITE_RED_RAM)?;
        self.send_data(&[0xF7])?;
        self.wait_while_busy("AUTO_WRITE_RED_RAM");

        info!("SSD1677 controller initialized");
        Ok(())
    }

    fn set_ram_area(&mut self, x: u16, y: u16, w: u16, h: u16) -> Result<(), &'static str> {
        // Reverse Y coordinate (gates are reversed on this display)
        let y = Self::HEIGHT as u16 - y - h;

        // Set data entry mode (X increment, Y decrement for reversed gates)
        self.send_command(commands::DATA_ENTRY_MODE)?;
        self.send_data(&[DATA_ENTRY_X_INC_Y_DEC])?;

        // Set RAM X address range (start, end) - X is in PIXELS
        self.send_command(commands::SET_RAM_X_RANGE)?;
        self.send_data(&[
            (x % 256) as u8,            // start low byte
            (x / 256) as u8,            // start high byte
            ((x + w - 1) % 256) as u8,  // end low byte
            ((x + w - 1) / 256) as u8,  // end high byte
        ])?;

        // Set RAM Y address range (start, end) - Y is in PIXELS
        self.send_command(commands::SET_RAM_Y_RANGE)?;
        self.send_data(&[
            ((y + h - 1) % 256) as u8,  // start low byte
            ((y + h - 1) / 256) as u8,  // start high byte
            (y % 256) as u8,            // end low byte
            (y / 256) as u8,            // end high byte
        ])?;

        // Set RAM X address counter - X is in PIXELS
        self.send_command(commands::SET_RAM_X_COUNTER)?;
        self.send_data(&[
            (x % 256) as u8,  // low byte
            (x / 256) as u8,  // high byte
        ])?;

        // Set RAM Y address counter - Y is in PIXELS
        self.send_command(commands::SET_RAM_Y_COUNTER)?;
        self.send_data(&[
            ((y + h - 1) % 256) as u8,  // low byte
            ((y + h - 1) / 256) as u8,  // high byte
        ])?;

        Ok(())
    }

    fn write_ram_buffer(&mut self, ram_buffer: u8, data: &[u8]) -> Result<(), &'static str> {
        let buffer_name = if ram_buffer == commands::WRITE_RAM_BW { "BW" } else { "RED" };
        info!("Writing frame buffer to {} RAM ({} bytes)", buffer_name, data.len());

        self.send_command(ram_buffer)?;
        
        // Write data in chunks to avoid issues with large transfers
        const CHUNK_SIZE: usize = 4096;
        for chunk in data.chunks(CHUNK_SIZE) {
            self.send_data(chunk)?;
        }

        info!("{} RAM write complete", buffer_name);
        Ok(())
    }

    fn refresh_display(&mut self, mode: RefreshMode, turn_off_screen: bool) -> Result<(), &'static str> {
        // Configure Display Update Control 1
        self.send_command(commands::DISPLAY_UPDATE_CTRL1)?;
        let ctrl1 = match mode {
            RefreshMode::Fast => CTRL1_NORMAL,
            RefreshMode::Full | RefreshMode::Half => CTRL1_BYPASS_RED,
        };
        self.send_data(&[ctrl1])?;

        // Select appropriate display mode based on refresh type
        let mut display_mode = 0x00u8;

        // Enable counter and analog if not already on
        if !self.is_screen_on {
            self.is_screen_on = true;
            display_mode |= 0xC0;  // Set CLOCK_ON and ANALOG_ON bits
        }

        // Turn off screen if requested
        if turn_off_screen {
            self.is_screen_on = false;
            display_mode |= 0x03;  // Set ANALOG_OFF_PHASE and CLOCK_OFF bits
        }

        match mode {
            RefreshMode::Full => {
                display_mode |= 0x34;
            }
            RefreshMode::Half => {
                // Write high temp to the register for a faster refresh
                self.send_command(commands::WRITE_TEMP)?;
                self.send_data(&[0x5A])?;
                display_mode |= 0xD4;
            }
            RefreshMode::Fast => {
                display_mode |= if self.custom_lut_active { 0x0C } else { 0x1C };
            }
        }

        // Power on and refresh display
        let refresh_type = match mode {
            RefreshMode::Full => "full",
            RefreshMode::Half => "half",
            RefreshMode::Fast => "fast",
        };
        info!("Powering on display 0x{:02X} ({} refresh)", display_mode, refresh_type);
        
        self.send_command(commands::DISPLAY_UPDATE_CTRL2)?;
        self.send_data(&[display_mode])?;

        self.send_command(commands::MASTER_ACTIVATION)?;

        // Wait for display to finish updating
        info!("Waiting for display refresh");
        self.wait_while_busy(refresh_type);

        Ok(())
    }
}

// Implement DrawTarget for embedded_graphics integration
impl<SPI, CS, DC, RST, BUSY> DrawTarget for EInkDisplay<'_, SPI, CS, DC, RST, BUSY>
where
    SPI: embedded_hal::spi::SpiBus,
    CS: OutputPin,
    DC: OutputPin,
    RST: OutputPin,
    BUSY: InputPin,
{
    type Color = BinaryColor;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let rotation = self.rotation;
        let buffer = self.frame_buffer();
        
        for Pixel(coord, color) in pixels.into_iter() {
            // Transform coordinates based on rotation
            let (x, y) = match rotation {
                Rotation::Rotate0 => {
                    // No rotation (landscape, 800x480)
                    if coord.x < 0 || coord.x >= Self::WIDTH as i32 || coord.y < 0 || coord.y >= Self::HEIGHT as i32 {
                        continue;
                    }
                    (coord.x as usize, coord.y as usize)
                }
                Rotation::Rotate90 => {
                    // 90° clockwise: (x,y) -> (WIDTH-1-y, x)
                    // Screen becomes 480x800 (portrait)
                    if coord.x < 0 || coord.x >= Self::HEIGHT as i32 || coord.y < 0 || coord.y >= Self::WIDTH as i32 {
                        continue;
                    }
                    (Self::WIDTH - 1 - coord.y as usize, coord.x as usize)
                }
                Rotation::Rotate180 => {
                    // 180° rotation: (x,y) -> (WIDTH-1-x, HEIGHT-1-y)
                    if coord.x < 0 || coord.x >= Self::WIDTH as i32 || coord.y < 0 || coord.y >= Self::HEIGHT as i32 {
                        continue;
                    }
                    (Self::WIDTH - 1 - coord.x as usize, Self::HEIGHT - 1 - coord.y as usize)
                }
                Rotation::Rotate270 => {
                    // 270° clockwise: (x,y) -> (y, HEIGHT-1-x)
                    // Screen becomes 480x800 (portrait)
                    if coord.x < 0 || coord.x >= Self::HEIGHT as i32 || coord.y < 0 || coord.y >= Self::WIDTH as i32 {
                        continue;
                    }
                    (coord.y as usize, Self::HEIGHT - 1 - coord.x as usize)
                }
            };

            let byte_index = y * Self::WIDTH_BYTES + (x / 8);
            let bit_index = 7 - (x % 8);

            match color {
                BinaryColor::On => {
                    // Black pixel - clear bit (0 = black in e-ink)
                    buffer[byte_index] &= !(1 << bit_index);
                }
                BinaryColor::Off => {
                    // White pixel - set bit (1 = white in e-ink)
                    buffer[byte_index] |= 1 << bit_index;
                }
            }
        }

        Ok(())
    }
}

impl<SPI, CS, DC, RST, BUSY> OriginDimensions for EInkDisplay<'_, SPI, CS, DC, RST, BUSY>
where
    SPI: embedded_hal::spi::SpiBus,
    CS: OutputPin,
    DC: OutputPin,
    RST: OutputPin,
    BUSY: InputPin,
{
    fn size(&self) -> Size {
        match self.rotation {
            Rotation::Rotate0 | Rotation::Rotate180 => {
                Size::new(Self::WIDTH as u32, Self::HEIGHT as u32)
            }
            Rotation::Rotate90 | Rotation::Rotate270 => {
                Size::new(Self::HEIGHT as u32, Self::WIDTH as u32)
            }
        }
    }
}
