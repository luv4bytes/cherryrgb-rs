use std::str::FromStr;

use crate::{
    calc_checksum,
    extensions::{OwnRGB8, ToVec},
};
use anyhow::{anyhow, Result};
use binrw::{binrw, BinWrite, BinWriterExt};

// Commands
#[binrw]
#[brw(repr = u8)]
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Command {
    TransactionStart = 0x01,
    TransactionEnd = 0x02,
    Unknown3 = 0x03,
    Unknown5 = 0x05,
    SetAnimation = 0x06,
    Unknown7 = 0x07,
    SetCustomLED = 0x0B,
    Unknown1B = 0x1B,
}

/// Purpose unknown, part of packet
#[binrw]
#[brw(repr = u8)]
#[derive(Eq, PartialEq, Debug)]
pub enum UnknownByte {
    Zero = 0,
    One = 1,
    Two = 2,
    Three = 3,
}

/// Modes support:
/// -> C: Color
/// -> S: Speed
#[binrw]
#[brw(repr = u8)]
#[derive(Eq, PartialEq, Debug)]
pub enum LightingMode {
    Wave = 0x00,      // CS
    Spectrum = 0x01,  // S
    Breathing = 0x02, // CS
    Static = 0x03,    // n/A
    Radar = 0x04,     // Unofficial
    Vortex = 0x05,    // Unofficial
    Fire = 0x06,      // Unofficial
    Stars = 0x07,     // Unofficial
    Rain = 0x0B,      // Unofficial (looks like Matrix :D)
    Custom = 0x08,
    Rolling = 0x0A,   // S
    Curve = 0x0C,     // CS
    WaveMid = 0x0E,   // Unoffical
    Scan = 0x0F,      // C
    Radiation = 0x12, // CS
    Ripples = 0x13,   // CS
    SingleKey = 0x15, // CS
}

impl FromStr for LightingMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mode = match s {
            "wave" => LightingMode::Wave,
            "spectrum" => LightingMode::Spectrum,
            "breathing" => LightingMode::Breathing,
            "static" => LightingMode::Static,
            "radar" => LightingMode::Radar,
            "vortex" => LightingMode::Vortex,
            "fire" => LightingMode::Fire,
            "stars" => LightingMode::Stars,
            "rain" => LightingMode::Rain,
            "custom" => LightingMode::Custom,
            "rolling" => LightingMode::Rolling,
            "curve" => LightingMode::Curve,
            "wave_mid" => LightingMode::WaveMid,
            "scan" => LightingMode::Scan,
            "radiation" => LightingMode::Radiation,
            "ripples" => LightingMode::Ripples,
            "single_key" => LightingMode::SingleKey,
            _ => return Err(anyhow!("Invalid mode supplied: {:?}", s)),
        };

        Ok(mode)
    }
}

/// Probably controlled at OS / driver level
/// Just defined here for completeness' sake
#[binrw]
#[brw(repr = u8)]
#[derive(Eq, PartialEq, Debug)]
pub enum UsbPollingRate {
    Low,    // 125Hz
    Medium, // 250 Hz
    High,   // 500 Hz
    Full,   // 1000 Hz
}

/// LED animation speed
#[binrw]
#[brw(repr = u8)]
#[derive(Eq, PartialEq, Debug)]
pub enum Speed {
    VeryFast = 0,
    Fast = 1,
    Medium = 2,
    Slow = 3,
    VerySlow = 4,
}

impl FromStr for Speed {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let speed = match s {
            "very_slow" => Speed::VerySlow,
            "slow" => Speed::Slow,
            "medium" => Speed::Medium,
            "fast" => Speed::Fast,
            "very_fast" => Speed::VeryFast,
            _ => return Err(anyhow!("Invalid mode supplied: {:?}", s)),
        };
        Ok(speed)
    }
}

/// LED brightness
#[binrw]
#[brw(repr = u8)]
#[derive(Eq, PartialEq, Debug)]
pub enum Brightness {
    Off = 0,
    Low = 1,
    Medium = 2,
    High = 3,
    Full = 4,
}

impl FromStr for Brightness {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let brightness = match s {
            "off" => Brightness::Off,
            "low" => Brightness::Low,
            "medium" => Brightness::Medium,
            "high" => Brightness::High,
            "full" => Brightness::Full,
            _ => return Err(anyhow!("Invalid mode supplied: {:?}", s)),
        };

        Ok(brightness)
    }
}

/// Common packet structure
#[binrw]
#[brw(magic = 4u8)]
#[derive(Debug)]
pub struct Packet {
    // magic, fixed to 0x04, see `br(magic = ...)`
    checksum: u8,
    unknown: UnknownByte,
    command: Command,
    data: [u8; 60],
}

impl Packet {
    pub fn new(unknown: UnknownByte, command: Command) -> Self {
        Self {
            checksum: 0,
            unknown,
            command,
            data: [0u8; 60],
        }
    }

    pub fn set_payload(&mut self, new_data: &[u8]) -> Result<()> {
        if new_data.len() > 60 {
            return Err(anyhow!("Payload exceeds 60 bytes"));
        }

        for (i, &byte) in new_data.iter().enumerate() {
            self.data[i] = byte;
        }
        Ok(())
    }

    pub fn update_checksum(&mut self) {
        self.checksum = calc_checksum(self.command.clone(), &self.data);
    }

    pub fn verify_checksum(&self) -> Result<()> {
        let calculated = calc_checksum(self.command.clone(), &self.data);
        if calculated == self.checksum {
            Ok(())
        } else {
            Err(anyhow!(
                "Invalid checksum, expected: {}, got: {}",
                calculated,
                self.checksum
            ))
        }
    }
}

/* LED Animation payload
///
///               brightness  rainbow
///                    |         |   COLOR
///                mode|speed    |  R  G  B
///                 |  |  |      |  |  |  |
///                 v  v  v      v  v  v  v
/// "09 00 00 55 00 12 03 03 00 00 7E 00 F4"
*/
#[binrw]
#[derive(Debug)]
pub struct LedAnimationPayload {
    unknown: [u8; 5],
    mode: LightingMode,
    brightness: Brightness,
    speed: Speed,
    pad: u8,
    rainbow: u8,
    color: OwnRGB8,
}

impl LedAnimationPayload {
    pub fn new(
        mode: LightingMode,
        brightness: Brightness,
        speed: Speed,
        color: OwnRGB8,
        rainbow: bool,
    ) -> Self {
        let rainbow = if rainbow { 1 } else { 0 };
        Self {
            unknown: [0x09, 0x00, 0x00, 0x55, 0x00],
            mode,
            brightness,
            speed,
            pad: 0,
            rainbow,
            color,
        }
    }
}

#[binrw]
#[derive(Debug)]
pub struct LedCustomPayload {
    #[br(temp)]
    #[bw(calc = key_leds_data.len() as u8)]
    data_len: u8,
    data_offset: u8,
    secondary_keys: u8,
    padding: u8,
    #[br(count = data_len)]
    key_leds_data: Vec<u8>,
}

#[derive(Default, Debug)]
pub struct CustomKeyLeds {
    key_leds: Vec<OwnRGB8>,
}

impl BinWrite for CustomKeyLeds {
    type Args = ();

    fn write_options<W: std::io::Write + std::io::Seek>(
        &self,
        writer: &mut W,
        _: &binrw::WriteOptions,
        _: Self::Args,
    ) -> binrw::BinResult<()> {
        for val in &self.key_leds {
            writer.write_ne(val)?;
        }
        Ok(())
    }
}

impl CustomKeyLeds {
    /// (64 byte packet - 4 byte packet header - 4 byte payload header)
    const CHUNK_SIZE: usize = 56;
    const TOTAL_KEYS: usize = 126;

    pub fn new() -> Self {
        Self {
            key_leds: (0..CustomKeyLeds::TOTAL_KEYS)
                .into_iter()
                .map(|_| OwnRGB8::default())
                .collect(),
        }
    }

    pub fn from_leds<C: Into<OwnRGB8>>(key_leds: Vec<C>) -> Result<Self> {
        if key_leds.len() > CustomKeyLeds::TOTAL_KEYS {
            return Err(anyhow!("Invalid number of key leds"));
        }

        Ok(Self {
            key_leds: key_leds.into_iter().map(|x| x.into()).collect(),
        })
    }

    pub fn set_led<C: Into<OwnRGB8>>(&mut self, key_index: usize, key: C) -> Result<()> {
        if key_index >= self.key_leds.len() {
            return Err(anyhow!("Key index out of bounds"));
        }

        self.key_leds[key_index] = key.into();
        Ok(())
    }

    pub fn get_payloads(self) -> Result<Vec<LedCustomPayload>> {
        let key_data = self.to_vec();

        let result = key_data
            .chunks(CustomKeyLeds::CHUNK_SIZE)
            .into_iter()
            .enumerate()
            .map(|(index, chunk)| {
                let mut are_secondary_keys = 0x00;
                let mut data_offset = index * CustomKeyLeds::CHUNK_SIZE;

                if data_offset > 0xFF {
                    data_offset %= 0x100;
                    are_secondary_keys = 0x01;
                }

                LedCustomPayload {
                    data_offset: data_offset as u8,
                    secondary_keys: are_secondary_keys,
                    padding: 0x00,
                    key_leds_data: chunk.to_vec(),
                }
            })
            .collect();

        Ok(result)
    }
}
