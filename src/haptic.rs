//! HID++ 4.5 wire format for MX Master 4 `HapticFeedback` (`0x19B0`).

use anyhow::{Result, bail};

use crate::config::Waveform;

pub const HAPTIC_FEATURE_ID: u16 = 0x19B0;
pub const REPORT_ID_LONG: u8 = 0x11;
pub const REPORT_LENGTH: usize = 20;
const GET_CAPABILITIES_FUNCTION: u8 = 0x00;
const PLAY_WAVEFORM_FUNCTION: u8 = 0x04;

/// A discovered `HapticFeedback` endpoint on one direct HID++ device.
#[derive(Clone, Copy)]
pub struct HapticEndpoint {
    device_index: u8,
    feature_index: u8,
    supported_waveforms: u32,
}

impl HapticEndpoint {
    pub fn discover(
        device_index: u8,
        feature_index: u8,
        request: impl FnOnce([u8; REPORT_LENGTH]) -> Result<[u8; REPORT_LENGTH]>,
    ) -> Result<Self> {
        let response = request(make_request(
            device_index,
            feature_index,
            GET_CAPABILITIES_FUNCTION,
            1,
            [0; 3],
        ))?;
        validate_response(
            &response,
            device_index,
            feature_index,
            GET_CAPABILITIES_FUNCTION,
            1,
        )?;

        // Solaar reads bytes 4..8 of the Haptic feature payload as a big-endian
        // waveform bitmap. HID++ long-report payload begins at byte 4 here.
        let supported_waveforms =
            u32::from_be_bytes([response[8], response[9], response[10], response[11]]);
        Ok(Self {
            device_index,
            feature_index,
            supported_waveforms,
        })
    }

    pub const fn feature_index(&self) -> u8 {
        self.feature_index
    }

    pub const fn supported_waveforms(&self) -> u32 {
        self.supported_waveforms
    }

    pub fn play(
        &self,
        waveform: Waveform,
        send: impl FnOnce([u8; REPORT_LENGTH]) -> Result<()>,
    ) -> Result<()> {
        if !waveform.is_advertised_by(self.supported_waveforms) {
            bail!(
                "{} is not advertised by the connected MX Master 4",
                waveform
            );
        }
        send(make_request(
            self.device_index,
            self.feature_index,
            PLAY_WAVEFORM_FUNCTION,
            1,
            [waveform.id(), 0, 0],
        ))
    }
}

pub fn make_request(
    device_index: u8,
    feature_index: u8,
    function: u8,
    software_id: u8,
    payload: [u8; 3],
) -> [u8; REPORT_LENGTH] {
    let mut report = [0; REPORT_LENGTH];
    report[0] = REPORT_ID_LONG;
    report[1] = device_index;
    report[2] = feature_index;
    report[3] = (function << 4) | (software_id & 0x0f);
    report[4..7].copy_from_slice(&payload);
    report
}

fn validate_response(
    response: &[u8; REPORT_LENGTH],
    device_index: u8,
    feature_index: u8,
    function: u8,
    software_id: u8,
) -> Result<()> {
    if response[0] != REPORT_ID_LONG {
        bail!("unexpected HID++ report ID 0x{:02X}", response[0]);
    }
    if response[1] == device_index && response[2] == 0xff && response[3] == feature_index {
        bail!(
            "the mouse rejected the HID++ request with error 0x{:02X}",
            response[5]
        );
    }
    let function_software = (function << 4) | (software_id & 0x0f);
    if response[1] != device_index
        || response[2] != feature_index
        || response[3] != function_software
    {
        bail!(
            "received a non-matching HID++ response: {:02X} {:02X} {:02X}",
            response[1],
            response[2],
            response[3]
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{REPORT_ID_LONG, make_request};
    use crate::config::Waveform;

    #[test]
    fn haptic_play_is_a_long_ble_report_with_function_four() {
        let report = make_request(0xff, 0x0b, 0x04, 0x01, [Waveform::Knock.id(), 0, 0]);
        assert_eq!(
            &report[..7],
            &[REPORT_ID_LONG, 0xff, 0x0b, 0x41, 0x0c, 0x00, 0x00]
        );
        assert!(report[7..].iter().all(|byte| *byte == 0));
    }
}
