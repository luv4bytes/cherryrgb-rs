mod extensions;
/// CHERRY G80-3000N RGB TKL experiments
/// No warranty or liability for possible damages
/// Use at your own risk!
mod models;

use anyhow::{Context, Result};
use rusb::UsbContext;
use std::time::Duration;

// Re-exports
pub use extensions::{OwnRGB8, ToVec};
pub use hex;
pub use models::{
    Brightness, Command, CustomKeyLeds, LedAnimationPayload, LightingMode, Packet, Speed,
    UnknownByte,
};
pub use rgb;
pub use rusb;

// Constants
const CHERRY_USB_VID: u16 = 0x046a;
const G30_3000N_RGB_TKL_USB_PID: u16 = 0x00dd;
const INTERFACE_NUM: u8 = 1;
const INTERRUPT_EP: u8 = 0x82;
static TIMEOUT: Duration = Duration::from_millis(1000);

/// Calculate packet checksum (index 1 in payload)
fn calc_checksum(command: Command, data: &[u8]) -> u8 {
    let sum = data.iter().map(|&i| i as u32).sum::<u32>() + (command as u32);

    (sum & 0xFF) as u8
}

// Prepend magic, checksum, unknown and command to payload
fn prepare_packet(unknown: UnknownByte, command: Command, payload: &[u8]) -> Result<Vec<u8>> {
    let mut packet = Packet::new(unknown, command);
    // Append payload
    packet.set_payload(payload)?;
    // Set checksum
    packet.update_checksum();

    Ok(packet.to_vec())
}

/// Writes a control packet first, then reads interrupt packet
fn send_payload(
    device: &rusb::DeviceHandle<rusb::Context>,
    unknown: UnknownByte,
    command: Command,
    payload: &[u8],
) -> Result<Vec<u8>> {
    // Prepend magic + checksum
    let packet = prepare_packet(unknown, command, payload)?;

    let mut response = [0u8; 64];
    device
        .write_control(
            rusb::request_type(
                rusb::Direction::Out,
                rusb::RequestType::Class,
                rusb::Recipient::Interface,
            ),
            0x09,    // Request - SET_REPORT
            0x0204,  // Value - ReportId: 4, ReportType: Output
            0x0001,  // Index
            &packet, // Data
            TIMEOUT,
        )
        .context("Control Write failure")?;
    log::debug!("# >> CONTROL TRANSFER\n{:?}\n", hex::encode(&packet));

    device
        .read_interrupt(
            INTERRUPT_EP,  // Endpoint
            &mut response, // read buffer
            TIMEOUT,
        )
        .context("Interrupt read failure")?;
    log::debug!("# << INTERRUPT TRANSFER\n{:?}\n", hex::encode(&response));

    Ok(response.to_vec())
}

/// Start RGB setting transaction
fn start_transaction(device: &rusb::DeviceHandle<rusb::Context>) -> Result<()> {
    send_payload(device, UnknownByte::Zero, Command::TransactionStart, &[])?;

    Ok(())
}

/// End RGB setting transaction
fn end_transaction(device: &rusb::DeviceHandle<rusb::Context>) -> Result<()> {
    send_payload(device, UnknownByte::Zero, Command::TransactionEnd, &[])?;

    Ok(())
}

/// Just taken 1:1 from usb capture
pub fn fetch_device_state(device: &rusb::DeviceHandle<rusb::Context>) -> Result<()> {
    start_transaction(device)?;
    send_payload(device, UnknownByte::Zero, Command::Unknown3, &[0x22])?;
    send_payload(device, UnknownByte::Zero, Command::Unknown7, &[0x38, 0x00])?;
    send_payload(device, UnknownByte::Zero, Command::Unknown7, &[0x38, 0x38])?;
    send_payload(device, UnknownByte::Zero, Command::Unknown7, &[0x38, 0x70])?;
    send_payload(device, UnknownByte::Zero, Command::Unknown7, &[0x38, 0xA8])?;
    send_payload(device, UnknownByte::One, Command::Unknown7, &[0x38, 0xE0])?;
    send_payload(
        device,
        UnknownByte::Zero,
        Command::Unknown7,
        &[0x38, 0x18, 0x01],
    )?;
    send_payload(
        device,
        UnknownByte::Zero,
        Command::Unknown7,
        &[0x2A, 0x50, 0x01],
    )?;
    send_payload(device, UnknownByte::Zero, Command::Unknown1B, &[0x38, 0x00])?;
    send_payload(device, UnknownByte::Zero, Command::Unknown1B, &[0x38, 0x38])?;
    send_payload(device, UnknownByte::Zero, Command::Unknown1B, &[0x0E, 0x70])?;
    end_transaction(device)?;

    Ok(())
}

/// Set LED animation from different modes
pub fn set_led_animation<C: Into<OwnRGB8>>(
    device: &rusb::DeviceHandle<rusb::Context>,
    mode: LightingMode,
    brightness: Brightness,
    speed: Speed,
    color: C,
    rainbow: bool,
) -> Result<()> {
    let payload: Vec<u8> =
        LedAnimationPayload::new(mode, brightness, speed, color.into(), rainbow).to_vec();

    start_transaction(device)?;
    // Send main payload
    send_payload(device, UnknownByte::One, Command::SetAnimation, &payload)?;
    // Send unknown / ?static? bytes
    send_payload(
        device,
        UnknownByte::Zero,
        Command::SetAnimation,
        &[0x01, 0x18, 0x00, 0x55, 0x01],
    )?;

    end_transaction(device)?;
    Ok(())
}

/// Set custom color for each individual key
pub fn set_custom_colors(
    device: &rusb::DeviceHandle<rusb::Context>,
    key_leds: CustomKeyLeds,
) -> Result<()> {
    // Set custom led mode
    set_led_animation(
        device,
        LightingMode::Custom,
        Brightness::Full,
        Speed::Slow,
        OwnRGB8::default(),
        false,
    )?;

    for payload in key_leds.get_payloads()? {
        send_payload(
            device,
            UnknownByte::Zero,
            Command::SetCustomLED,
            &payload.to_vec(),
        )?;
    }

    Ok(())
}

/// Reset custom key colors to default
pub fn reset_custom_colors(device: &rusb::DeviceHandle<rusb::Context>) -> Result<()> {
    // Create array of blank / off LEDs
    set_custom_colors(device, CustomKeyLeds::new())?;

    // Payloads, type: 0x5
    send_payload(device, UnknownByte::Zero, Command::Unknown5, &[0x01])?;
    send_payload(device, UnknownByte::Zero, Command::Unknown5, &[0x19])?;
    Ok(())
}

/// Find supported Cherry USB keyboard
pub fn find_device() -> Result<rusb::DeviceHandle<rusb::Context>> {
    // Search / init usb keyboard
    let ctx = rusb::Context::new().context("Failed to create libusb context")?;

    let device_handle = ctx
        .open_device_with_vid_pid(CHERRY_USB_VID, G30_3000N_RGB_TKL_USB_PID)
        .context("Keyboard not found")?;

    Ok(device_handle)
}

/// Init USB device by verifying number of configurations and claiming appropriate interface
pub fn init_device(device_handle: &mut rusb::DeviceHandle<rusb::Context>) -> Result<()> {
    let device = device_handle.device();
    let device_desc = device
        .device_descriptor()
        .context("Failed to read device descriptor")?;
    let config_desc = device
        .active_config_descriptor()
        .context("Failed to get config descriptor")?;

    log::debug!(
        "* Connected to: Bus {:03} Device {:03} ID {:04x}:{:04x}",
        device.bus_number(),
        device.address(),
        device_desc.vendor_id(),
        device_desc.product_id()
    );

    assert_eq!(device_desc.num_configurations(), 1);
    assert_eq!(config_desc.num_interfaces(), 2);

    let kernel_driver_active = device_handle
        .kernel_driver_active(INTERFACE_NUM)
        .context("kernel_driver_active")?;

    if kernel_driver_active {
        device_handle
            .detach_kernel_driver(INTERFACE_NUM)
            .context("Failed to detach active kernel driver")?;
    }

    device_handle
        .claim_interface(INTERFACE_NUM)
        .context("Failed to claim interface")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use binrw::BinReaderExt;
    use rgb::{ComponentSlice, RGB8};
    use std::io::Cursor;

    /// Some captures packets
    fn packets() -> Vec<&'static str> {
        //                              brightness
        //     checksum                mode|speed      color
        //      |                       |  |  |         |
        //      v                       v  v  v         v
        vec![
            "04 69 01 06 09 00 00 55 00 00 03 02 00 01 FF", //       00 - wave - regular
            "04 6B 01 06 09 00 00 55 00 00 03 04 00 01 FF", //       01 - wave - slow
            "04 67 01 06 09 00 00 55 00 00 03 00 00 01 FF", //       02 - wave - fast
            "04 68 01 06 09 00 00 55 00 00 03 01 00 01 FF", //       03 - wave - another speed
            "04 69 01 06 09 00 00 55 00 01 03 01 00 01 FF", //       04 - spectrum - fast
            "04 68 01 06 09 00 00 55 00 01 03 00 00 01 FF", //       05 - spectrum - fastest
            "04 6C 01 06 09 00 00 55 00 01 03 04 00 01 FF", //       06 - spectrum - slow
            "04 6B 01 06 09 00 00 55 00 01 03 03 00 01 FF", //       07 - breathing
            "04 6C 01 06 09 00 00 55 00 02 03 03 00 01 FF", //       08 - breathing - slow
            "04 74 01 06 09 00 00 55 00 0A 03 03 00 01 FF", //       09 - Rolling
            "04 76 01 06 09 00 00 55 00 0C 03 03 00 01 FF", //       10 - Curve
            "04 79 01 06 09 00 00 55 00 0F 03 03 00 01 FF", //       11 - Scan
            "04 7C 01 06 09 00 00 55 00 12 03 03 00 01 FF", //       12 - Radiation
            "04 EE 01 06 09 00 00 55 00 12 03 03 00 00 7E 00 F4", // 13 - Radiation
            "04 EF 01 06 09 00 00 55 00 13 03 03 00 00 7E 00 F4", // 14 - Ripples - slow
            "04 EC 01 06 09 00 00 55 00 13 03 00 00 00 7E 00 F4", // 15 - Rippples - fast
            "04 EE 01 06 09 00 00 55 00 15 03 00 00 00 7E 00 F4", // 16 - Single Key
            "04 DC 01 06 09 00 00 55 00 03 03 00 00 00 7E 00 F4", // 17 - Static - Purple
            "04 4D 01 06 09 00 00 55 00 03 03 00 00 00 E0 03 00", // 18 - Static - Red
            "04 52 01 06 09 00 00 55 00 08 03 00 00 00 E0 03 00", // 19 - Custom
            // start / end transaction packets
            "04 01 00 01",
            "04 02 00 02",
            // fetch device info packets
            "04 25 00 03 22 00 00",
            "04 3f 00 07 38 00 00",
            "04 77 00 07 38 38 00",
            "04 af 00 07 38 70 00",
            "04 e7 00 07 38 a8 00",
            "04 1f 01 07 38 e0 00",
            "04 58 00 07 38 18 01",
            "04 82 00 07 2a 50 01",
            "04 53 00 1b 38 00 00",
            "04 8b 00 1b 38 38 00",
            "04 99 00 1b 0e 70 00",
            // Unknown
            "04 43 00 0b 38 00 00",
            "04 7b 00 0b 38 38 00",
            "04 b3 00 0b 38 70 00",
            "04 eb 00 0b 38 a8 00",
            "04 23 01 0b 38 e0 00",
            "04 5c 00 0b 38 18 01",
            "04 86 00 0b 2a 50 01",
        ]
    }

    #[test]
    fn test_checksum() {
        for (index, &pkt_str) in packets().iter().enumerate() {
            let pkt =
                hex::decode(pkt_str.replace(" ", "")).expect("Failed to convert pkt hexstream");

            let expected_checksum = pkt[1];
            let mut cursor = Cursor::new(&pkt[3..]);
            let command: Command = cursor.read_ne().expect("Failed to read command");
            let calcd_checksum = calc_checksum(command, &pkt[4..]);

            assert_eq!(
                expected_checksum, calcd_checksum,
                "Failed checksum for pkt {} data={:?}",
                index, pkt_str
            );
        }
    }

    #[test]
    fn serialize_rgb8() {
        #[rustfmt::skip]
        assert_eq!(RGB8 {r: 232,g: 211,b: 75}.as_slice(),&[232, 211, 75]);
        #[rustfmt::skip]
        assert_eq!(RGB8 {r: 232, g: 0, b: 75}.as_slice(), &[232, 0, 75]);
        #[rustfmt::skip]
        assert_eq!(RGB8 { r: 0, g: 0, b: 75 }.as_slice(), &[0, 0, 75]);
    }

    #[test]
    fn serialize_led_animation_payload() {
        let buf: Vec<u8> = LedAnimationPayload::new(
            LightingMode::Vortex,
            Brightness::Full,
            Speed::VerySlow,
            OwnRGB8::new(244, 255, 100),
            false,
        )
        .to_vec();
        assert_eq!(
            vec![0x09, 0x00, 0x00, 0x55, 0x00, 0x05, 0x04, 0x04, 0x00, 0x00, 0xF4, 0xFF, 0x64],
            buf
        );
        let buf: Vec<u8> = LedAnimationPayload::new(
            LightingMode::Vortex,
            Brightness::Full,
            Speed::VerySlow,
            OwnRGB8::new(244, 255, 100),
            true,
        )
        .to_vec();
        assert_eq!(
            vec![0x09, 0x00, 0x00, 0x55, 0x00, 0x05, 0x04, 0x04, 0x00, 0x01, 0xF4, 0xFF, 0x64],
            buf
        );
        let buf: Vec<u8> = LedAnimationPayload::new(
            LightingMode::Rolling,
            Brightness::Full,
            Speed::VerySlow,
            OwnRGB8::new(244, 255, 100),
            false,
        )
        .to_vec();
        assert_eq!(
            vec![0x09, 0x00, 0x00, 0x55, 0x00, 0x0A, 0x04, 0x04, 0x00, 0x00, 0xF4, 0xFF, 0x64],
            buf
        );
        let buf: Vec<u8> = LedAnimationPayload::new(
            LightingMode::Vortex,
            Brightness::Full,
            Speed::Medium,
            OwnRGB8::new(244, 255, 100),
            false,
        )
        .to_vec();
        assert_eq!(
            vec![0x09, 0x00, 0x00, 0x55, 0x00, 0x05, 0x04, 0x02, 0x00, 0x00, 0xF4, 0xFF, 0x64],
            buf
        );
        let buf: Vec<u8> = LedAnimationPayload::new(
            LightingMode::Vortex,
            Brightness::Low,
            Speed::Medium,
            OwnRGB8::new(244, 255, 100),
            false,
        )
        .to_vec();
        assert_eq!(
            vec![0x09, 0x00, 0x00, 0x55, 0x00, 0x05, 0x01, 0x02, 0x00, 0x00, 0xF4, 0xFF, 0x64],
            buf
        );
    }

    #[test]
    fn prep_packet() {
        assert_eq!(
            prepare_packet(UnknownByte::Three, Command::TransactionStart, &[0x42, 0x94]).unwrap()
                [..6],
            vec![0x04, 0xD7, 0x03, 0x01, 0x42, 0x94]
        );
        assert_eq!(
            prepare_packet(UnknownByte::One, Command::TransactionStart, &[0x47]).unwrap()[..5],
            vec![0x04, 0x48, 0x01, 0x01, 0x47]
        );
        assert_eq!(
            prepare_packet(UnknownByte::Three, Command::SetAnimation, &[]).unwrap()[..4],
            vec![0x04, 0x06, 0x03, 0x06]
        );
    }
}
