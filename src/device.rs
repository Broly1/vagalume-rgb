//! Finding the controller in sysfs and talking to it through /dev/i2c-N.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::time::Duration;

use crate::error::{Error, Result};
use crate::protocol::{hex, validate_frame, Frame};

pub const I2C_ADDRESS: u16 = 0x28;
const I2C_RDWR: libc::c_ulong = 0x0707;
const I2C_M_RD: u16 = 0x0001;

/// (PCI vendor, subsystem vendor, subsystem device) of boards confirmed to carry
/// this controller. The kernel registers "amc" on other cards too, and their
/// protocol may differ.
const SUPPORTED_BOARDS: [(u16, u16, u16); 1] = [(0x8086, 0x207E, 0xA002)];

const RETRY_DELAYS: [Duration; 2] = [Duration::from_millis(50), Duration::from_millis(100)];

/// Left after every frame. A mode change sent with other registers turns the
/// LEDs off, and this much separation is known to work.
const SETTLE: Duration = Duration::from_millis(50);

pub trait Transport: Send {
    /// Sends one frame and returns the byte the controller answered with.
    fn send(&mut self, frame: &[u8]) -> Result<u8>;

    fn bus(&self) -> Option<u32> {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub bus: u32,
    pub pci_address: String,
    pub board: Option<(u16, u16, u16)>,
}

impl DeviceInfo {
    pub fn path(&self) -> String {
        format!("/dev/i2c-{}", self.bus)
    }

    pub fn supported(&self) -> bool {
        self.board.is_some_and(|board| SUPPORTED_BOARDS.contains(&board))
    }

    pub fn board_text(&self) -> String {
        match self.board {
            Some((a, b, c)) => format!("{a:04x}:{b:04x}:{c:04x}"),
            None => "unknown".to_string(),
        }
    }
}

fn read_hex(path: &Path) -> Option<u16> {
    let text = fs::read_to_string(path).ok()?;
    u16::from_str_radix(text.trim().trim_start_matches("0x"), 16).ok()
}

fn read_board(pci: &Path) -> Option<(u16, u16, u16)> {
    Some((
        read_hex(&pci.join("vendor"))?,
        read_hex(&pci.join("subsystem_vendor"))?,
        read_hex(&pci.join("subsystem_device"))?,
    ))
}

pub fn find_device(sysfs: &Path) -> Result<DeviceInfo> {
    let suffix = format!("-{I2C_ADDRESS:04x}");
    let mut nodes: Vec<_> = fs::read_dir(sysfs.join("bus/i2c/devices"))
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    nodes.retain(|node| node.to_string_lossy().ends_with(&suffix));
    nodes.sort();

    for node in nodes {
        let named_amc = fs::read_to_string(node.join("name"))
            .is_ok_and(|name| name.trim() == "amc");
        let file_name = node.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let bus = file_name.split('-').next().and_then(|bus| bus.parse().ok());
        let (true, Some(bus)) = (named_amc, bus) else { continue };

        let resolved = fs::canonicalize(&node).unwrap_or(node);
        let pci = resolved.ancestors().find(|dir| dir.join("subsystem_vendor").exists());
        return Ok(match pci {
            Some(pci) => DeviceInfo {
                bus,
                pci_address: pci.file_name().unwrap_or_default().to_string_lossy().into_owned(),
                board: read_board(pci),
            },
            None => DeviceInfo { bus, pci_address: "unknown".to_string(), board: None },
        });
    }
    Err(Error::Device(
        "No controller found: expected an 'amc' I2C device at 0x28. \
         Is the xe driver loaded, and the i2c-dev module?"
            .to_string(),
    ))
}

/// Matches the kernel's `struct i2c_msg`.
#[repr(C)]
pub struct I2cMsg {
    pub addr: u16,
    pub flags: u16,
    pub len: u16,
    pub buf: *mut u8,
}

/// Matches the kernel's `struct i2c_rdwr_ioctl_data`.
#[repr(C)]
struct I2cRdwrData {
    msgs: *mut I2cMsg,
    nmsgs: u32,
}

pub trait I2cBus: Send {
    fn transfer(&mut self, msgs: &mut [I2cMsg]) -> io::Result<()>;
}

struct DeviceBus(File);

impl I2cBus for DeviceBus {
    fn transfer(&mut self, msgs: &mut [I2cMsg]) -> io::Result<()> {
        let mut data = I2cRdwrData { msgs: msgs.as_mut_ptr(), nmsgs: msgs.len() as u32 };
        // SAFETY: `data` and the messages and buffers it points to outlive the
        // call, and their layouts match what the I2C_RDWR ioctl expects.
        let result = unsafe {
            libc::ioctl(self.0.as_raw_fd(), I2C_RDWR as _, &mut data as *mut I2cRdwrData)
        };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

/// Sends frames in the one transaction shape the controller is known to accept.
///
/// Always a combined write + 1-byte read. A plain multi-byte read wedged this
/// bus during development, so no other shape is exposed.
pub struct I2cTransport<B> {
    bus: B,
    number: u32,
    sleep: fn(Duration),
}

impl<B: I2cBus> I2cTransport<B> {
    fn exchange(&mut self, frame: &[u8]) -> io::Result<u8> {
        let mut out = frame.to_vec();
        let mut back = [0u8];
        let mut msgs = [
            I2cMsg { addr: I2C_ADDRESS, flags: 0, len: out.len() as u16, buf: out.as_mut_ptr() },
            I2cMsg { addr: I2C_ADDRESS, flags: I2C_M_RD, len: 1, buf: back.as_mut_ptr() },
        ];
        self.bus.transfer(&mut msgs)?;
        Ok(back[0])
    }
}

fn retryable(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(libc::EAGAIN | libc::EIO | libc::ENXIO | libc::EREMOTEIO)
    )
}

impl<B: I2cBus> Transport for I2cTransport<B> {
    fn send(&mut self, frame: &[u8]) -> Result<u8> {
        validate_frame(frame)?;
        let mut delays = RETRY_DELAYS.iter();
        loop {
            let error = match self.exchange(frame) {
                Ok(byte) => {
                    (self.sleep)(SETTLE);
                    return Ok(byte);
                }
                Err(error) => error,
            };
            if error.raw_os_error() == Some(libc::ETIMEDOUT) {
                return Err(Error::Device(
                    "The I2C bus timed out and may be stuck. Reboot; if that does not clear it, \
                     shut down, switch off the PSU for 30 seconds, then start again."
                        .to_string(),
                ));
            }
            match delays.next() {
                Some(delay) if retryable(&error) => (self.sleep)(*delay),
                _ => return Err(Error::Device(format!("I2C write failed: {error}"))),
            }
        }
    }

    fn bus(&self) -> Option<u32> {
        Some(self.number)
    }
}

/// Validates and records frames without touching hardware.
pub struct DryRunTransport {
    pub frames: Vec<Frame>,
    echo: bool,
}

impl DryRunTransport {
    pub fn new(echo: bool) -> Self {
        Self { frames: Vec::new(), echo }
    }
}

impl Transport for DryRunTransport {
    fn send(&mut self, frame: &[u8]) -> Result<u8> {
        validate_frame(frame)?;
        if self.echo {
            println!("would send: {}", hex(frame));
        }
        self.frames.push(frame.to_vec());
        Ok(0)
    }
}

fn open_error(path: &str, error: &io::Error) -> Error {
    Error::Device(match error.kind() {
        io::ErrorKind::PermissionDenied => format!(
            "No permission to open {path}. Install the udev rule (see the README), then reboot."
        ),
        _ => format!("Cannot open {path}: {error}"),
    })
}

pub fn open_transport(allow_unknown_board: bool, sysfs: &Path) -> Result<Box<dyn Transport>> {
    let info = find_device(sysfs)?;
    if !info.supported() && !allow_unknown_board {
        return Err(Error::Device(format!(
            "Board {} on i2c-{} is not a confirmed board, so nothing was sent. \
             Its 0x28 controller may not speak this protocol.",
            info.board_text(),
            info.bus
        )));
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(info.path())
        .map_err(|e| open_error(&info.path(), &e))?;
    Ok(Box::new(I2cTransport {
        bus: DeviceBus(file),
        number: info.bus,
        sleep: std::thread::sleep,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    fn fake_sysfs(root: &Path, name: &str, subsystem: (&str, &str)) {
        let pci = root.join("devices/pci0000:00/0000:08:00.0");
        let client = pci.join("i2c_designware.2048/i2c-12/12-0028");
        fs::create_dir_all(&client).unwrap();
        fs::write(client.join("name"), format!("{name}\n")).unwrap();
        fs::write(pci.join("vendor"), "0x8086\n").unwrap();
        fs::write(pci.join("subsystem_vendor"), format!("{}\n", subsystem.0)).unwrap();
        fs::write(pci.join("subsystem_device"), format!("{}\n", subsystem.1)).unwrap();
        let bus = root.join("bus/i2c/devices");
        fs::create_dir_all(&bus).unwrap();
        symlink(&client, bus.join("12-0028")).unwrap();
    }

    #[test]
    fn finds_supported_board() {
        let dir = tempfile::tempdir().unwrap();
        fake_sysfs(dir.path(), "amc", ("0x207e", "0xa002"));
        let info = find_device(dir.path()).unwrap();
        assert_eq!((info.bus, info.path().as_str()), (12, "/dev/i2c-12"));
        assert_eq!(info.pci_address, "0000:08:00.0");
        assert!(info.supported());
    }

    #[test]
    fn ignores_other_device_names() {
        let dir = tempfile::tempdir().unwrap();
        fake_sysfs(dir.path(), "eeprom", ("0x207e", "0xa002"));
        assert!(find_device(dir.path()).is_err());
    }

    #[test]
    fn refuses_other_boards_before_opening_the_bus() {
        let dir = tempfile::tempdir().unwrap();
        fake_sysfs(dir.path(), "amc", ("0x1234", "0x0001"));
        let Err(error) = open_transport(false, dir.path()) else { panic!("should refuse") };
        assert!(error.to_string().contains("nothing was sent"), "{error}");
    }

    type Transfer = (u16, u16, u16, Vec<u8>);

    #[test]
    fn permission_errors_point_at_the_udev_rule() {
        let denied = io::Error::from_raw_os_error(libc::EACCES);
        assert!(open_error("/dev/i2c-1", &denied).to_string().contains("udev rule"));
    }

    #[derive(Clone, Default)]
    struct FakeBus {
        calls: Arc<AtomicUsize>,
        seen: Arc<Mutex<Vec<Transfer>>>,
        fail_with: Option<i32>,
    }

    impl I2cBus for FakeBus {
        fn transfer(&mut self, msgs: &mut [I2cMsg]) -> io::Result<()> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(code) = self.fail_with {
                return Err(io::Error::from_raw_os_error(code));
            }
            for msg in msgs.iter() {
                // SAFETY: the transport points each message at a buffer of `len` bytes.
                let bytes = unsafe { std::slice::from_raw_parts(msg.buf, msg.len as usize) };
                self.seen.lock().unwrap().push((msg.addr, msg.flags, msg.len, bytes.to_vec()));
            }
            // SAFETY: the second message's buffer is one writable byte.
            unsafe { *msgs[1].buf = 0xFE };
            Ok(())
        }
    }

    fn transport(bus: FakeBus) -> I2cTransport<FakeBus> {
        I2cTransport { bus, number: 12, sleep: |_| () }
    }

    #[test]
    fn struct_layouts_match_the_kernel_abi() {
        assert_eq!(std::mem::size_of::<I2cMsg>(), 16);
        assert_eq!(std::mem::offset_of!(I2cMsg, len), 4);
        assert_eq!(std::mem::offset_of!(I2cMsg, buf), 8);
        assert_eq!(std::mem::size_of::<I2cRdwrData>(), 16);
        assert_eq!(std::mem::offset_of!(I2cRdwrData, nmsgs), 8);
    }

    #[test]
    fn pauses_after_every_frame() {
        thread_local!(static SLEPT: std::cell::Cell<Duration> = const { std::cell::Cell::new(Duration::ZERO) });
        let mut transport = I2cTransport {
            bus: FakeBus::default(),
            number: 12,
            sleep: |delay| SLEPT.set(SLEPT.get() + delay),
        };
        transport.send(&[0x10, 0x02]).unwrap();
        transport.send(&[0x3E, 0x88]).unwrap();
        assert_eq!(SLEPT.get(), SETTLE * 2);
    }

    #[test]
    fn sends_write_then_one_byte_read_only() {
        let bus = FakeBus::default();
        let seen = bus.seen.clone();
        assert_eq!(transport(bus).send(&[0x10, 0x02]).unwrap(), 0xFE);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0], (0x28, 0, 2, vec![0x10, 0x02]));
        assert_eq!((seen[1].0, seen[1].1, seen[1].2), (0x28, I2C_M_RD, 1));
    }

    #[test]
    fn refuses_bad_frame_before_any_transfer() {
        let bus = FakeBus::default();
        let calls = bus.calls.clone();
        assert!(transport(bus).send(&[0xFF, 0x00]).is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn timeout_is_not_retried() {
        let bus = FakeBus { fail_with: Some(libc::ETIMEDOUT), ..Default::default() };
        let calls = bus.calls.clone();
        assert!(transport(bus).send(&[0x10, 0x00]).is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn busy_is_retried_twice() {
        let bus = FakeBus { fail_with: Some(libc::EIO), ..Default::default() };
        let calls = bus.calls.clone();
        assert!(transport(bus).send(&[0x10, 0x00]).is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn other_errors_are_not_retried() {
        let bus = FakeBus { fail_with: Some(libc::EACCES), ..Default::default() };
        let calls = bus.calls.clone();
        assert!(transport(bus).send(&[0x10, 0x00]).is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
