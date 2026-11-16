use std::{
    ffi::CString,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use hidapi::{HidApi, HidDevice};

use crate::{
    config::Waveform,
    haptic::{HAPTIC_FEATURE_ID, HapticEndpoint, REPORT_ID_LONG, REPORT_LENGTH, make_request},
};

const LOGITECH_VENDOR_ID: u16 = 0x046D;
const MX_MASTER_4_FOR_MAC_PRODUCT_ID: u16 = 0xB042;
const BLE_HIDPP_USAGE_PAGE: u16 = 0xFF43;
const BLE_HIDPP_USAGE: u16 = 0x0202;
const DIRECT_DEVICE_INDEX: u8 = 0xFF;
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);
const INITIAL_RECONNECT_DELAY: Duration = Duration::from_millis(500);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(10);
/// How often the HID reader thread wakes up to check whether it should exit.
const READER_POLL_MS: i32 = 1_000;

#[derive(Debug, Clone)]
pub struct ListedDevice {
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub usage_page: u16,
    pub usage_id: u16,
}

impl ListedDevice {
    pub fn is_ble_hidpp(&self) -> bool {
        (self.usage_page, self.usage_id) == (BLE_HIDPP_USAGE_PAGE, BLE_HIDPP_USAGE)
    }

    pub fn is_mx4_target(&self) -> bool {
        self.vendor_id == LOGITECH_VENDOR_ID
            && self.product_id == MX_MASTER_4_FOR_MAC_PRODUCT_ID
            && self.is_ble_hidpp()
    }

    pub fn transport_name(&self) -> &'static str {
        if self.is_ble_hidpp() {
            "Bluetooth LE HID++"
        } else {
            "other HID++ transport"
        }
    }
}

/// Whether an OS device path (e.g. a Windows raw input device name) belongs
/// to the MX Master 4 for Mac. USB-style paths spell the IDs as
/// `VID_046D&PID_B042`; Bluetooth LE HID paths as `VID&02046d_PID&b042`
/// (the `02` prefix is the Bluetooth SIG vendor-ID source).
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn is_mx4_device_path(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    let vid = format!("{LOGITECH_VENDOR_ID:04x}");
    let pid = format!("{MX_MASTER_4_FOR_MAC_PRODUCT_ID:04x}");
    let vid_ok = [format!("vid_{vid}"), format!("vid&01{vid}"), format!("vid&02{vid}")]
        .iter()
        .any(|needle| path.contains(needle.as_str()));
    let pid_ok = [format!("pid_{pid}"), format!("pid&{pid}")]
        .iter()
        .any(|needle| path.contains(needle.as_str()));
    vid_ok && pid_ok
}

pub fn list_devices() -> Result<Vec<ListedDevice>> {
    let api = hid_api()?;
    Ok(api
        .device_list()
        .filter(|device| {
            device.vendor_id() == LOGITECH_VENDOR_ID
                && is_hidpp_long_collection(device.usage_page(), device.usage())
        })
        .map(|device| ListedDevice {
            name: device
                .product_string()
                .unwrap_or("unknown Logitech device")
                .to_owned(),
            vendor_id: device.vendor_id(),
            product_id: device.product_id(),
            usage_page: device.usage_page(),
            usage_id: device.usage(),
        })
        .collect())
}

pub struct DeviceManager {
    connected: Option<ConnectedDevice>,
    expected_name: Option<String>,
    next_attempt: Instant,
    reconnect_delay: Duration,
    debug: bool,
}

impl DeviceManager {
    pub fn new(debug: bool) -> Self {
        Self {
            connected: None,
            expected_name: None,
            next_attempt: Instant::now(),
            reconnect_delay: INITIAL_RECONNECT_DELAY,
            debug,
        }
    }

    pub fn with_expected_name(debug: bool, expected_name: String) -> Self {
        let mut manager = Self::new(debug);
        let expected_name = expected_name.trim();
        if !expected_name.is_empty() {
            manager.expected_name = Some(expected_name.to_owned());
        }
        manager
    }

    pub fn play(&mut self, waveform: Waveform) -> Result<()> {
        self.ensure_connected()?;
        let Some(device) = self.connected.as_ref() else {
            bail!(
                "MX Master 4 for Mac is not connected or its BLE HID++ collection is not accessible yet"
            )
        };
        if let Err(error) = device.play(waveform) {
            self.debug(format_args!(
                "haptic request failed; reconnecting: {error:#}"
            ));
            self.mark_disconnected();
            return Err(error);
        }
        Ok(())
    }

    /// Returns the waveform bitmap advertised by the mouse, connecting if needed.
    pub fn supported_waveforms(&mut self) -> Result<u32> {
        self.ensure_connected()?;
        match self.connected.as_ref() {
            Some(device) => Ok(device.haptic.supported_waveforms()),
            None => bail!(
                "MX Master 4 for Mac is not connected or its BLE HID++ collection is not accessible yet"
            ),
        }
    }

    /// Closes the HID connection; the next request reconnects.
    pub fn disconnect(&mut self) {
        self.connected = None;
    }

    /// Skips the reconnect backoff so a user-initiated action tries right away.
    pub fn connect_now(&mut self) {
        if self.connected.is_none() {
            self.next_attempt = Instant::now();
        }
    }

    pub fn reconnect_if_due(&mut self) {
        if self.connected.is_none() && Instant::now() >= self.next_attempt {
            if let Err(error) = self.ensure_connected() {
                self.debug(format_args!("reconnect attempt failed: {error:#}"));
            }
        }
    }

    fn ensure_connected(&mut self) -> Result<()> {
        if self.connected.is_some() || Instant::now() < self.next_attempt {
            return Ok(());
        }

        match connect_target(self.expected_name.as_deref()) {
            Ok(device) => {
                self.debug(format_args!(
                    "connected: {} ({:04X}:{:04X}), BLE HID++, HAPTIC feature index 0x{:02X}",
                    device.info.name,
                    device.info.vendor_id,
                    device.info.product_id,
                    device.haptic.feature_index()
                ));
                self.debug(format_args!(
                    "HapticFeedback waveform capabilities: 0x{:08X}",
                    device.haptic.supported_waveforms()
                ));
                self.connected = Some(device);
                self.reconnect_delay = INITIAL_RECONNECT_DELAY;
                self.next_attempt = Instant::now();
                Ok(())
            }
            Err(error) => {
                self.schedule_retry();
                Err(error)
            }
        }
    }

    fn mark_disconnected(&mut self) {
        self.connected = None;
        self.schedule_retry();
    }

    fn schedule_retry(&mut self) {
        self.next_attempt = Instant::now() + self.reconnect_delay;
        self.reconnect_delay = self
            .reconnect_delay
            .saturating_mul(2)
            .min(MAX_RECONNECT_DELAY);
    }

    fn debug(&self, message: std::fmt::Arguments<'_>) {
        if self.debug {
            eprintln!("debug: {message}");
        }
    }
}

struct ConnectedDevice {
    // Keeps hidapi's context alive for both open, non-exclusive handles.
    _api: HidApi,
    info: ListedDevice,
    transport: HidapiTransport,
    haptic: HapticEndpoint,
}

impl ConnectedDevice {
    fn play(&self, waveform: Waveform) -> Result<()> {
        self.haptic
            .play(waveform, |report| self.transport.send(report))
    }
}

fn connect_target(expected_name: Option<&str>) -> Result<ConnectedDevice> {
    let api = hid_api()?;
    let Some(device_info) = api.device_list().find(|device| {
        device.vendor_id() == LOGITECH_VENDOR_ID
            && device.product_id() == MX_MASTER_4_FOR_MAC_PRODUCT_ID
            && (device.usage_page(), device.usage()) == (BLE_HIDPP_USAGE_PAGE, BLE_HIDPP_USAGE)
            && name_matches(device.product_string().unwrap_or_default(), expected_name)
    }) else {
        bail!(
            "no MX Master 4 for Mac (046D:B042) Bluetooth LE HID++ collection matching the configured device name is visible"
        )
    };

    let info = ListedDevice {
        name: device_info
            .product_string()
            .unwrap_or("MX Master 4 for Mac")
            .to_owned(),
        vendor_id: device_info.vendor_id(),
        product_id: device_info.product_id(),
        usage_page: device_info.usage_page(),
        usage_id: device_info.usage(),
    };
    let path = CString::new(device_info.path().to_bytes())
        .context("the HID device path contained an interior NUL")?;
    let reader = api
        .open_path(&path)
        .context("macOS denied shared read access to the MX Master 4 HID collection")?;
    let writer = api
        .open_path(&path)
        .context("macOS denied shared write access to the MX Master 4 HID collection")?;
    let transport = HidapiTransport::new(reader, writer)?;

    let feature_response = transport.request(make_request(
        DIRECT_DEVICE_INDEX,
        0,
        0,
        1,
        [(HAPTIC_FEATURE_ID >> 8) as u8, HAPTIC_FEATURE_ID as u8, 0],
    ))?;
    let feature_index = parse_root_feature_response(&feature_response)?;
    let haptic = HapticEndpoint::discover(DIRECT_DEVICE_INDEX, feature_index, |report| {
        transport.request(report)
    })?;
    Ok(ConnectedDevice {
        _api: api,
        info,
        transport,
        haptic,
    })
}

fn hid_api() -> Result<HidApi> {
    let api = HidApi::new().context("could not initialize hidapi")?;
    #[cfg(target_os = "macos")]
    if api.get_open_exclusive() {
        bail!(
            "hidapi did not enter macOS shared-device mode; rebuild with feature macos-shared-device"
        )
    }
    Ok(api)
}

fn parse_root_feature_response(report: &[u8; REPORT_LENGTH]) -> Result<u8> {
    if report[0] != REPORT_ID_LONG
        || report[1] != DIRECT_DEVICE_INDEX
        || report[2] != 0
        || report[3] != 1
    {
        bail!("unexpected Root/GetFeature response while looking up HapticFeedback")
    }
    if report[4] == 0 {
        bail!("the MX Master 4 does not advertise HapticFeedback (0x19B0)")
    }
    Ok(report[4])
}

fn name_matches(name: &str, expected_name: Option<&str>) -> bool {
    expected_name.is_none_or(|expected| {
        let normalized_name = name.to_ascii_lowercase().replace('_', " ");
        let normalized_expected = expected.to_ascii_lowercase().replace('_', " ");
        normalized_name.contains(&normalized_expected)
    })
}

fn is_hidpp_long_collection(usage_page: u16, usage_id: u16) -> bool {
    matches!(
        (usage_page, usage_id),
        (0xFF00, 0x0002) | (BLE_HIDPP_USAGE_PAGE, BLE_HIDPP_USAGE) | (0xFF43, 0x0602)
    )
}

/// Uses two non-exclusive hidapi handles. The reader blocks in one dedicated
/// thread; writes are therefore not held behind an idle read timeout.
struct HidapiTransport {
    writer: Mutex<HidDevice>,
    responses: mpsc::Receiver<[u8; REPORT_LENGTH]>,
    stop: Arc<AtomicBool>,
}

impl HidapiTransport {
    fn new(reader: HidDevice, writer: HidDevice) -> Result<Self> {
        let (sender, responses) = mpsc::sync_channel(32);
        let stop = Arc::new(AtomicBool::new(false));
        let reader_stop = Arc::clone(&stop);
        thread::Builder::new()
            .name("mx4-haptic-hid-reader".to_owned())
            .spawn(move || reader_loop(reader, sender, &reader_stop))?;
        Ok(Self {
            writer: Mutex::new(writer),
            responses,
            stop,
        })
    }

    fn send(&self, report: [u8; REPORT_LENGTH]) -> Result<()> {
        let writer = self
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("HID writer lock is poisoned"))?;
        writer
            .write(&report)
            .context("could not write the HID++ long report")?;
        Ok(())
    }

    fn request(&self, report: [u8; REPORT_LENGTH]) -> Result<[u8; REPORT_LENGTH]> {
        while self.responses.try_recv().is_ok() {}
        self.send(report)?;

        let wanted = (report[1], report[2], report[3]);
        let deadline = Instant::now() + RESPONSE_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!("HID++ request timed out")
            }
            match self.responses.recv_timeout(remaining) {
                Ok(response) if (response[1], response[2], response[3]) == wanted => {
                    return Ok(response);
                }
                Ok(response)
                    if response[1] == wanted.0
                        && response[2] == 0xff
                        && response[3] == wanted.1 =>
                {
                    return Ok(response);
                }
                Ok(_) => continue,
                Err(mpsc::RecvTimeoutError::Timeout) => bail!("HID++ request timed out"),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    bail!("the MX Master 4 HID reader disconnected")
                }
            }
        }
    }
}

impl Drop for HidapiTransport {
    fn drop(&mut self) {
        // Without this the reader thread (and its open HID handle) outlived
        // every dropped connection, because it only noticed the closed channel
        // when the mouse happened to send another long report. The thread
        // exits within READER_POLL_MS; it is not joined so drop never blocks.
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn reader_loop(
    reader: HidDevice,
    sender: mpsc::SyncSender<[u8; REPORT_LENGTH]>,
    stop: &AtomicBool,
) {
    let mut buffer = [0; 64];
    while !stop.load(Ordering::Relaxed) {
        match reader.read_timeout(&mut buffer, READER_POLL_MS) {
            Ok(length) if length >= REPORT_LENGTH && buffer[0] == REPORT_ID_LONG => {
                let mut report = [0; REPORT_LENGTH];
                report.copy_from_slice(&buffer[..REPORT_LENGTH]);
                // try_send: unsolicited reports nobody waits for must not block
                // the reader once the response queue is full.
                if let Err(mpsc::TrySendError::Disconnected(_)) = sender.try_send(report) {
                    return;
                }
            }
            Ok(_) => {}
            Err(_) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BLE_HIDPP_USAGE, BLE_HIDPP_USAGE_PAGE, ListedDevice, is_mx4_device_path,
        parse_root_feature_response,
    };

    #[test]
    fn recognises_mx4_device_paths() {
        assert!(is_mx4_device_path(
            r"\\?\HID#{00001812-0000-1000-8000-00805f9b34fb}_Dev_VID&02046d_PID&b042_REV&0012_d1a2b3c4d5e6&Col01#9&1a2b3c4d&0&0000#{378de44c-56ef-11d1-bc8c-00a0c91405dd}"
        ));
        assert!(is_mx4_device_path(r"\\?\HID#VID_046D&PID_B042&MI_00#7&abc#{378de44c}"));
        // Another Logitech mouse over the same Bolt receiver / other vendor.
        assert!(!is_mx4_device_path(r"\\?\HID#VID_046D&PID_C548&MI_01&Col01#8&abc#{378de44c}"));
        assert!(!is_mx4_device_path(r"\\?\HID#VID_1532&PID_B042&MI_00#7&abc#{378de44c}"));
        assert!(!is_mx4_device_path(r"\\?\HID#SYNA7DB5&Col02#5&abc#{378de44c}"));
    }

    #[test]
    fn mx4_matching_requires_vid_pid_and_ble_vendor_collection() {
        let device = ListedDevice {
            name: "MX Master 4 for Mac".to_owned(),
            vendor_id: 0x046D,
            product_id: 0xB042,
            usage_page: BLE_HIDPP_USAGE_PAGE,
            usage_id: BLE_HIDPP_USAGE,
        };
        assert!(device.is_mx4_target());
        assert!(
            !ListedDevice {
                usage_page: 0xFF00,
                usage_id: 0x0002,
                ..device
            }
            .is_mx4_target()
        );
    }

    #[test]
    fn root_response_returns_dynamic_feature_index() {
        let mut report = [0; 20];
        report[..5].copy_from_slice(&[0x11, 0xff, 0x00, 0x01, 0x0b]);
        assert_eq!(
            parse_root_feature_response(&report).expect("valid root response"),
            0x0b
        );
    }
}
