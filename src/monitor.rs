//! Read-only GPU readings from sysfs and procfs. Nothing here touches the I2C bus.
//!
//! Temperatures come from the driver's hwmon directory. Usage comes from each
//! process's DRM fdinfo, because the xe idle residency counter was seen frozen
//! while the GPU was in use. Fdinfo only covers processes this user can read, so
//! the desktop compositor may be missing, and in a Flatpak only the sandbox is
//! visible, which would look like an idle GPU. GPU usage is left out there.
//!
//! VRAM comes from the driver's memory query on the render node, which sees the
//! whole card. Fdinfo is the fallback for a kernel that reports no used figure.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use crate::device::find_device;

const BYTES_PER_MIB: u64 = 1024 * 1024;
const BYTES_PER_GIB: u64 = 1024 * BYTES_PER_MIB;

/// `DRM_IOCTL_XE_DEVICE_QUERY`.
const XE_DEVICE_QUERY: libc::c_ulong = 0xC028_6440;
const XE_QUERY_MEM_REGIONS: u32 = 1;
const XE_REGION_CLASS_VRAM: u16 = 1;
/// Size of `struct drm_xe_mem_region`, and of the header before the first one.
const XE_REGION_SIZE: usize = 88;
const XE_REGIONS_HEADER: usize = 8;

/// Where the temperature rings start, and the limit if the driver gives none.
const COOL_CELSIUS: f64 = 30.0;
const DEFAULT_LIMIT_CELSIUS: f64 = 100.0;
/// How far along a meter the colour changes.
const WARM_FRACTION: f64 = 0.7;
const HOT_FRACTION: f64 = 0.85;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Reading {
    pub gpu_celsius: Option<f64>,
    pub vram_celsius: Option<f64>,
    /// Critical limits, from the driver.
    pub gpu_limit: Option<f64>,
    pub vram_limit: Option<f64>,
    /// The busiest engine class. Needs two samples.
    pub gpu_percent: Option<f64>,
    pub vram_bytes: Option<u64>,
    pub vram_total: Option<u64>,
}

impl Reading {
    pub fn vram_fraction(&self) -> Option<f64> {
        let total = self.vram_total.filter(|total| *total > 0)?;
        Some(self.vram_bytes? as f64 / total as f64)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    Normal,
    Warm,
    Hot,
}

pub fn zone(fraction: f64) -> Zone {
    if fraction >= HOT_FRACTION {
        Zone::Hot
    } else if fraction >= WARM_FRACTION {
        Zone::Warm
    } else {
        Zone::Normal
    }
}

pub fn temperature_fraction(celsius: f64, limit: Option<f64>) -> f64 {
    let limit = limit.unwrap_or(DEFAULT_LIMIT_CELSIUS);
    ((celsius - COOL_CELSIUS) / (limit - COOL_CELSIUS)).clamp(0.0, 1.0)
}

pub fn usage_available(root: &Path) -> bool {
    !root.join(".flatpak-info").exists()
}

/// Reads `tempN_<field>` of the sensor with this label, in degrees.
fn temperature(hwmon: &Path, label: &str, field: &str) -> Option<f64> {
    for entry in fs::read_dir(hwmon).ok()?.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(base) = name.strip_suffix("_label").filter(|base| base.starts_with("temp")) else {
            continue;
        };
        if fs::read_to_string(&path).is_ok_and(|text| text.trim() == label) {
            let input = fs::read_to_string(hwmon.join(format!("{base}_{field}"))).ok()?;
            return Some(input.trim().parse::<f64>().ok()? / 1000.0);
        }
    }
    None
}

/// Matches the kernel's `struct drm_xe_device_query`.
#[repr(C)]
struct XeDeviceQuery {
    extensions: u64,
    query: u32,
    size: u32,
    data: u64,
    reserved: [u64; 2],
}

fn xe_query(node: &File, size: u32, data: u64) -> io::Result<u32> {
    let mut query = XeDeviceQuery {
        extensions: 0,
        query: XE_QUERY_MEM_REGIONS,
        size,
        data,
        reserved: [0; 2],
    };
    // SAFETY: `query` outlives the call and matches the layout this ioctl expects.
    // `data` is null or the start of a buffer of at least `size` bytes.
    let result = unsafe {
        libc::ioctl(node.as_raw_fd(), XE_DEVICE_QUERY as _, &mut query as *mut XeDeviceQuery)
    };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(query.size)
}

/// Total and used bytes of VRAM from `struct drm_xe_query_mem_regions`.
fn parse_regions(bytes: &[u8]) -> Option<(u64, u64)> {
    let u64_at = |at: usize| Some(u64::from_ne_bytes(bytes.get(at..at + 8)?.try_into().ok()?));
    let count = u32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?) as usize;
    let mut vram = None;
    for index in 0..count {
        let at = XE_REGIONS_HEADER + index * XE_REGION_SIZE;
        let class = u16::from_ne_bytes(bytes.get(at..at + 2)?.try_into().ok()?);
        if class == XE_REGION_CLASS_VRAM {
            let (total, used) = vram.unwrap_or((0, 0));
            vram = Some((total + u64_at(at + 8)?, used + u64_at(at + 16)?));
        }
    }
    vram
}

fn query_vram(node: &File) -> Option<(u64, u64)> {
    let size = xe_query(node, 0, 0).ok()?;
    // In u64s, because the regions are aligned to 8 bytes.
    let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
    xe_query(node, size, buffer.as_mut_ptr() as u64).ok()?;
    // SAFETY: any bytes are valid, and the slice stays within `buffer`.
    let bytes = unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), size as usize) };
    parse_regions(bytes)
}

fn open_render_node(sysfs: &Path, pci_address: &str) -> Option<File> {
    let directory = sysfs.join("bus/pci/devices").join(pci_address).join("drm");
    let name = fs::read_dir(directory).ok()?.flatten().find_map(|entry| {
        let name = entry.file_name().to_string_lossy().into_owned();
        name.starts_with("renderD").then_some(name)
    })?;
    let path = Path::new("/dev/dri").join(name);
    OpenOptions::new().read(true).write(true).open(path).ok()
}

fn find_hwmon(sysfs: &Path, pci_address: &str) -> Option<PathBuf> {
    let directory = sysfs.join("bus/pci/devices").join(pci_address).join("hwmon");
    let mut entries: Vec<_> = fs::read_dir(directory).ok()?.flatten().map(|e| e.path()).collect();
    entries.sort();
    entries.into_iter().next()
}

#[derive(Debug, Default, PartialEq)]
struct Client {
    id: u64,
    /// Engine class to (busy cycles, total cycles).
    cycles: HashMap<String, (u64, u64)>,
    /// Engine class to its engine count, where that is more than one.
    capacity: HashMap<String, u64>,
    vram_total: u64,
    vram_shared: u64,
}

fn parse_bytes(value: &str) -> Option<u64> {
    let mut parts = value.split_whitespace();
    let number: u64 = parts.next()?.parse().ok()?;
    Some(match parts.next() {
        None => number,
        Some("KiB") => number * 1024,
        Some("MiB") => number * BYTES_PER_MIB,
        Some("GiB") => number * BYTES_PER_GIB,
        Some(_) => return None,
    })
}

/// Reads one DRM fd's fdinfo, or `None` if it is another card's.
fn parse_client(text: &str, pci_address: &str) -> Option<Client> {
    let mut client = Client::default();
    let (mut ours, mut has_id) = (false, false);
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else { continue };
        let value = value.trim();
        match key {
            "drm-pdev" => ours = value == pci_address,
            "drm-client-id" => {
                client.id = value.parse().ok()?;
                has_id = true;
            }
            "drm-total-vram0" => client.vram_total = parse_bytes(value).unwrap_or(0),
            "drm-shared-vram0" => client.vram_shared = parse_bytes(value).unwrap_or(0),
            _ => {
                let number = || value.parse::<u64>().unwrap_or(0);
                if let Some(class) = key.strip_prefix("drm-total-cycles-") {
                    client.cycles.entry(class.to_string()).or_default().1 = number();
                } else if let Some(class) = key.strip_prefix("drm-cycles-") {
                    client.cycles.entry(class.to_string()).or_default().0 = number();
                } else if let Some(class) = key.strip_prefix("drm-engine-capacity-") {
                    client.capacity.insert(class.to_string(), number().max(1));
                }
            }
        }
    }
    (ours && has_id).then_some(client)
}

fn read_clients(proc_root: &Path, pci_address: &str) -> Vec<Client> {
    let mut clients = Vec::new();
    let mut seen = HashSet::new();
    let Ok(processes) = fs::read_dir(proc_root) else { return clients };
    for process in processes.flatten() {
        if !process.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        // Other users' processes cannot be read, which is expected.
        let Ok(descriptors) = fs::read_dir(process.path().join("fd")) else { continue };
        for descriptor in descriptors.flatten() {
            let drm = fs::read_link(descriptor.path())
                .is_ok_and(|target| target.starts_with("/dev/dri"));
            if !drm {
                continue;
            }
            let info = process.path().join("fdinfo").join(descriptor.file_name());
            let Ok(text) = fs::read_to_string(info) else { continue };
            // A client shared between descriptors or processes counts once.
            if let Some(client) = parse_client(&text, pci_address) {
                if seen.insert(client.id) {
                    clients.push(client);
                }
            }
        }
    }
    clients
}

/// Clients can share buffers, so shared memory is counted once rather than per client.
fn vram_used(clients: &[Client]) -> u64 {
    let private: u64 = clients
        .iter()
        .map(|client| client.vram_total.saturating_sub(client.vram_shared))
        .sum();
    private + clients.iter().map(|client| client.vram_shared).max().unwrap_or(0)
}

type Counters = HashMap<u64, HashMap<String, (u64, u64)>>;

fn busiest_engine(previous: &Counters, clients: &[Client]) -> Option<f64> {
    if previous.is_empty() {
        return None;
    }
    let mut classes: HashMap<&str, f64> = HashMap::new();
    for client in clients {
        let Some(before) = previous.get(&client.id) else { continue };
        for (class, (busy, total)) in &client.cycles {
            let Some((old_busy, old_total)) = before.get(class) else { continue };
            let elapsed = total.saturating_sub(*old_total);
            if elapsed == 0 {
                continue;
            }
            let capacity = client.capacity.get(class).copied().unwrap_or(1);
            let fraction = busy.saturating_sub(*old_busy) as f64 / (elapsed * capacity) as f64;
            *classes.entry(class).or_default() += fraction;
        }
    }
    let busiest = classes.values().copied().fold(0.0, f64::max);
    Some((busiest * 100.0).min(100.0))
}

pub struct Monitor {
    sysfs: PathBuf,
    proc_root: PathBuf,
    usage: bool,
    previous: Counters,
    /// Kept open, because every open is a new client for the driver to track.
    render_node: Option<File>,
}

impl Monitor {
    pub fn new(sysfs: &Path, proc_root: &Path, usage: bool) -> Self {
        Self {
            sysfs: sysfs.to_path_buf(),
            proc_root: proc_root.to_path_buf(),
            usage,
            previous: Counters::new(),
            render_node: None,
        }
    }

    pub fn sample(&mut self) -> Reading {
        let mut reading = Reading::default();
        let Ok(device) = find_device(&self.sysfs) else { return reading };
        if let Some(hwmon) = find_hwmon(&self.sysfs, &device.pci_address) {
            reading.gpu_celsius = temperature(&hwmon, "pkg", "input");
            reading.vram_celsius = temperature(&hwmon, "vram", "input");
            reading.gpu_limit = temperature(&hwmon, "pkg", "crit");
            reading.vram_limit = temperature(&hwmon, "vram", "crit");
        }
        if self.render_node.is_none() {
            self.render_node = open_render_node(&self.sysfs, &device.pci_address);
        }
        if let Some((total, used)) = self.render_node.as_ref().and_then(query_vram) {
            reading.vram_total = Some(total);
            reading.vram_bytes = (used > 0).then_some(used);
        }
        if self.usage {
            let clients = read_clients(&self.proc_root, &device.pci_address);
            reading.gpu_percent = busiest_engine(&self.previous, &clients);
            reading.vram_bytes = reading.vram_bytes.or(Some(vram_used(&clients)));
            self.previous = clients.into_iter().map(|c| (c.id, c.cycles)).collect();
        }
        reading
    }
}

pub fn format_temperature(celsius: Option<f64>, fahrenheit: bool) -> String {
    match celsius {
        None => "—".to_string(),
        Some(celsius) if fahrenheit => format!("{:.0}°F", celsius * 9.0 / 5.0 + 32.0),
        Some(celsius) => format!("{celsius:.0}°C"),
    }
}

pub fn format_percent(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_string(), |value| format!("{value:.0}%"))
}

pub fn format_bytes(value: Option<u64>) -> String {
    match value {
        None => "—".to_string(),
        Some(value) if value >= BYTES_PER_GIB => {
            format!("{:.1} GiB", value as f64 / BYTES_PER_GIB as f64)
        }
        Some(value) => format!("{} MiB", value / BYTES_PER_MIB),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PCI: &str = "0000:08:00.0";

    fn fdinfo(id: u64, rcs: (u64, u64), vcs: (u64, u64)) -> String {
        format!(
            "drm-driver:\txe\ndrm-client-id:\t{id}\ndrm-pdev:\t{PCI}\n\
             drm-total-vram0:\t2048 KiB\ndrm-shared-vram0:\t1024 KiB\n\
             drm-cycles-rcs:\t{}\ndrm-total-cycles-rcs:\t{}\n\
             drm-cycles-vcs:\t{}\ndrm-total-cycles-vcs:\t{}\n\
             drm-engine-capacity-vcs:\t2\n",
            rcs.0, rcs.1, vcs.0, vcs.1
        )
    }

    fn counters(clients: Vec<Client>) -> Counters {
        clients.into_iter().map(|c| (c.id, c.cycles)).collect()
    }

    #[test]
    fn parses_a_client() {
        let client = parse_client(&fdinfo(7, (10, 20), (30, 40)), PCI).unwrap();
        assert_eq!(client.id, 7);
        assert_eq!(client.cycles["rcs"], (10, 20));
        assert_eq!(client.capacity["vcs"], 2);
        assert_eq!((client.vram_total, client.vram_shared), (2 << 20, 1 << 20));
    }

    #[test]
    fn ignores_other_cards() {
        assert_eq!(parse_client(&fdinfo(7, (0, 0), (0, 0)), "0000:01:00.0"), None);
    }

    #[test]
    fn sizes_may_have_no_unit_or_a_larger_one() {
        assert_eq!(parse_bytes("0"), Some(0));
        assert_eq!(parse_bytes("6 MiB"), Some(6 << 20));
        assert_eq!(parse_bytes("2 GiB"), Some(2 << 30));
        assert_eq!(parse_bytes("2 TiB"), None);
    }

    #[test]
    fn usage_needs_two_samples() {
        let client = parse_client(&fdinfo(1, (0, 0), (0, 0)), PCI).unwrap();
        assert_eq!(busiest_engine(&Counters::new(), &[client]), None);
    }

    #[test]
    fn usage_is_the_busiest_class_added_across_clients() {
        let parse = |id, rcs, vcs| parse_client(&fdinfo(id, rcs, vcs), PCI).unwrap();
        let before = counters(vec![parse(1, (0, 0), (0, 0)), parse(2, (0, 0), (0, 0))]);
        let after = [parse(1, (25, 100), (0, 100)), parse(2, (25, 100), (10, 100))];
        assert_eq!(busiest_engine(&before, &after), Some(50.0));
    }

    #[test]
    fn usage_divides_by_the_engine_count() {
        let parse = |rcs, vcs| parse_client(&fdinfo(1, rcs, vcs), PCI).unwrap();
        let before = counters(vec![parse((0, 0), (0, 0))]);
        assert_eq!(busiest_engine(&before, &[parse((0, 100), (100, 100))]), Some(50.0));
    }

    #[test]
    fn usage_never_passes_100_percent() {
        let parse = |id, rcs| parse_client(&fdinfo(id, rcs, (0, 0)), PCI).unwrap();
        let before = counters(vec![parse(1, (0, 0)), parse(2, (0, 0))]);
        let after = [parse(1, (90, 100)), parse(2, (90, 100))];
        assert_eq!(busiest_engine(&before, &after), Some(100.0));
    }

    #[test]
    fn a_new_client_is_skipped_until_it_has_a_baseline() {
        let parse = |id, rcs| parse_client(&fdinfo(id, rcs, (0, 0)), PCI).unwrap();
        let before = counters(vec![parse(1, (0, 0))]);
        let after = [parse(1, (10, 100)), parse(2, (1000, 1000))];
        assert_eq!(busiest_engine(&before, &after), Some(10.0));
    }

    #[test]
    fn shared_vram_counts_once() {
        let clients = [
            parse_client(&fdinfo(1, (0, 0), (0, 0)), PCI).unwrap(),
            parse_client(&fdinfo(2, (0, 0), (0, 0)), PCI).unwrap(),
        ];
        assert_eq!(vram_used(&clients), 3 << 20);
    }

    #[test]
    fn finds_temperatures_by_label_and_reads_only_the_named_sensor() {
        let dir = tempfile::tempdir().unwrap();
        for (name, value) in [
            ("temp2_label", "pkg\n"),
            ("temp2_input", "52000\n"),
            ("temp3_label", "vram\n"),
            ("temp3_input", "56500\n"),
            ("fan1_input", "220\n"),
        ] {
            fs::write(dir.path().join(name), value).unwrap();
        }
        assert_eq!(temperature(dir.path(), "pkg", "input"), Some(52.0));
        assert_eq!(temperature(dir.path(), "vram", "input"), Some(56.5));
        assert_eq!(temperature(dir.path(), "pcie", "input"), None);
    }

    fn region(class: u16, total: u64, used: u64) -> Vec<u8> {
        let mut bytes = vec![0; XE_REGION_SIZE];
        bytes[..2].copy_from_slice(&class.to_ne_bytes());
        bytes[8..16].copy_from_slice(&total.to_ne_bytes());
        bytes[16..24].copy_from_slice(&used.to_ne_bytes());
        bytes
    }

    fn regions(items: &[Vec<u8>], claimed: u32) -> Vec<u8> {
        let mut bytes = claimed.to_ne_bytes().to_vec();
        bytes.extend([0; 4]);
        items.iter().for_each(|item| bytes.extend(item));
        bytes
    }

    #[test]
    fn query_struct_matches_the_kernel_abi() {
        assert_eq!(std::mem::size_of::<XeDeviceQuery>(), 40);
        assert_eq!(std::mem::offset_of!(XeDeviceQuery, data), 16);
    }

    #[test]
    fn vram_skips_system_memory() {
        let bytes = regions(&[region(0, 100, 50), region(1, 30, 20)], 2);
        assert_eq!(parse_regions(&bytes), Some((30, 20)));
    }

    #[test]
    fn vram_adds_every_tile() {
        let bytes = regions(&[region(1, 30, 20), region(1, 40, 5)], 2);
        assert_eq!(parse_regions(&bytes), Some((70, 25)));
    }

    #[test]
    fn a_short_region_list_gives_nothing() {
        assert_eq!(parse_regions(&regions(&[region(1, 30, 20)], 2)), None);
        assert_eq!(parse_regions(&[]), None);
    }

    #[test]
    fn vram_fraction_needs_a_total() {
        let reading = Reading { vram_bytes: Some(3), vram_total: Some(12), ..Reading::default() };
        assert_eq!(reading.vram_fraction(), Some(0.25));
        let reading = Reading { vram_bytes: Some(3), ..Reading::default() };
        assert_eq!(reading.vram_fraction(), None);
    }

    #[test]
    fn zones_change_along_the_meter() {
        assert_eq!(zone(0.69), Zone::Normal);
        assert_eq!(zone(0.7), Zone::Warm);
        assert_eq!(zone(0.85), Zone::Hot);
    }

    #[test]
    fn temperature_rings_stay_in_range() {
        assert_eq!(temperature_fraction(20.0, Some(100.0)), 0.0);
        assert_eq!(temperature_fraction(65.0, Some(100.0)), 0.5);
        assert_eq!(temperature_fraction(120.0, Some(100.0)), 1.0);
    }

    #[test]
    fn no_card_gives_an_empty_reading() {
        let dir = tempfile::tempdir().unwrap();
        let mut monitor = Monitor::new(dir.path(), dir.path(), true);
        assert_eq!(monitor.sample(), Reading::default());
    }

    #[test]
    fn a_flatpak_has_no_usage() {
        let dir = tempfile::tempdir().unwrap();
        assert!(usage_available(dir.path()));
        fs::write(dir.path().join(".flatpak-info"), "").unwrap();
        assert!(!usage_available(dir.path()));
    }

    #[test]
    fn formats_values() {
        assert_eq!(format_temperature(Some(52.4), false), "52°C");
        assert_eq!(format_temperature(Some(100.0), true), "212°F");
        assert_eq!(format_temperature(None, true), "—");
        assert_eq!(format_percent(None), "—");
        assert_eq!(format_percent(Some(7.6)), "8%");
        assert_eq!(format_bytes(Some(512 << 20)), "512 MiB");
        assert_eq!(format_bytes(Some(3 << 29)), "1.5 GiB");
    }
}
