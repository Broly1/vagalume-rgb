//! The LED controller's register protocol. See docs/PROTOCOL.md.
//!
//! Pure data and byte building: nothing here touches hardware. A frame is a run
//! of (register, value) pairs sent in one I2C transaction.

use crate::color::{parse_color, Rgb};
use crate::config::ModeSettings;
use crate::error::{Error, Result};

pub type Frame = Vec<u8>;

pub const REG_BYPASS: u8 = 0x0F;
pub const REG_MODE: u8 = 0x10;
pub const REG_DIRECTION: u8 = 0x14;
pub const REG_STRIP_LENGTH: u8 = 0x27; // LED count; written once at connect.
pub const REG_BRIGHTNESS: u8 = 0x3E;

pub const STRIP_LENGTH: u8 = 0x0E;
pub const DEFAULT_BRIGHTNESS: u8 = 0x88;

pub const DIRECTION_LEFT: u8 = 0;
pub const DIRECTION_RIGHT: u8 = 1;

pub const CUSTOM_COLOR_REGS: [u8; 3] = [0x1A, 0x1B, 0x1C];
pub const BREATHING_COLOR_REGS: [u8; 3] = [0xC9, 0xCA, 0xCB];

#[derive(Debug)]
pub struct Param {
    pub key: &'static str,
    pub label: &'static str,
    pub reg: u8,
    pub min: i32,
    pub max: i32,
    pub default: i32,
    /// When set, the wire value is `invert_from - value`.
    pub invert_from: Option<i32>,
    pub toggle: bool,
}

impl Param {
    const fn new(
        key: &'static str,
        label: &'static str,
        reg: u8,
        (min, max, default): (i32, i32, i32),
    ) -> Self {
        Self {
            key,
            label,
            reg,
            min,
            max,
            default,
            invert_from: None,
            toggle: false,
        }
    }

    const fn inverted(mut self, from: i32) -> Self {
        self.invert_from = Some(from);
        self
    }

    const fn toggle(mut self) -> Self {
        self.toggle = true;
        self
    }

    pub fn wire(&self, value: i32) -> u8 {
        let value = value.clamp(self.min, self.max);
        match self.invert_from {
            Some(from) => (from - value) as u8,
            None => value as u8,
        }
    }
}

#[derive(Debug)]
pub struct Mode {
    pub key: &'static str,
    pub name: &'static str,
    pub id: u8,
    pub params: &'static [Param],
    pub has_direction: bool,
    /// Direction goes before the mode select instead of after it.
    pub direction_first: bool,
    pub color_regs: Option<[u8; 3]>,
}

pub static MODES: [Mode; 7] = [
    Mode {
        key: "rainbow",
        name: "Rainbow",
        id: 0x00,
        params: &[
            Param::new("response", "Response", 0x18, (2, 13, 2)),
            Param::new("speed", "Speed", 0x19, (0, 16, 5)),
        ],
        has_direction: true,
        direction_first: true,
        color_regs: None,
    },
    Mode {
        key: "chase",
        name: "Chase",
        id: 0x04,
        params: &[
            Param::new("interval", "Interval", 0x12, (0, 9, 1)),
            Param::new("response", "Response", 0x11, (2, 13, 10)).inverted(15),
            Param::new("bidirectional", "Both directions", 0x29, (0, 1, 1)).toggle(),
        ],
        has_direction: true,
        direction_first: false,
        color_regs: None,
    },
    Mode {
        key: "gradient",
        name: "Gradient",
        id: 0x05,
        params: &[Param::new("response", "Response", 0x13, (2, 13, 10)).inverted(15)],
        has_direction: false,
        direction_first: false,
        color_regs: None,
    },
    Mode {
        key: "sequential",
        name: "Sequential",
        id: 0x03,
        params: &[
            Param::new("response", "Response", 0x16, (0, 5, 2)).inverted(5),
            Param::new("speed", "Speed", 0x17, (0, 16, 16)).inverted(16),
        ],
        has_direction: true,
        direction_first: false,
        color_regs: None,
    },
    Mode {
        key: "stacking",
        name: "Stacking",
        id: 0x06,
        params: &[Param::new("speed", "Speed", 0x20, (2, 13, 10)).inverted(15)],
        has_direction: true,
        direction_first: false,
        color_regs: None,
    },
    Mode {
        key: "breathing",
        name: "Breathing",
        id: 0x02,
        params: &[Param::new("tempo", "Tempo", 0xC8, (2, 13, 6)).inverted(15)],
        has_direction: false,
        direction_first: false,
        color_regs: Some(BREATHING_COLOR_REGS),
    },
    Mode {
        key: "custom",
        name: "Custom",
        id: 0x01,
        params: &[],
        has_direction: false,
        direction_first: false,
        color_regs: Some(CUSTOM_COLOR_REGS),
    },
];

pub fn find_mode(key: &str) -> Option<&'static Mode> {
    MODES.iter().find(|mode| mode.key == key)
}

/// The only registers the transport will accept. This chip is the card's
/// management controller, not just an LED driver, so anything else is refused
/// before it reaches the bus.
fn is_writable(reg: u8) -> bool {
    matches!(reg, REG_BYPASS | REG_MODE | REG_DIRECTION | REG_BRIGHTNESS)
        || CUSTOM_COLOR_REGS.contains(&reg)
        || BREATHING_COLOR_REGS.contains(&reg)
        || MODES
            .iter()
            .flat_map(|mode| mode.params)
            .any(|param| param.reg == reg)
}

pub fn validate_frame(frame: &[u8]) -> Result<()> {
    if frame.is_empty() || !frame.len().is_multiple_of(2) {
        return Err(Error::Invalid(format!(
            "frame must be non-empty register/value pairs: {}",
            hex(frame)
        )));
    }
    for &[reg, value] in frame.as_chunks::<2>().0 {
        if !(is_writable(reg) || (reg == REG_STRIP_LENGTH && value == STRIP_LENGTH)) {
            return Err(Error::Invalid(format!(
                "register {reg:#04x} is not allowlisted"
            )));
        }
    }
    Ok(())
}

pub fn hex(frame: &[u8]) -> String {
    frame
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The single frame sent once on connecting.
pub fn init_frame(led_on: bool, bypass: bool) -> Frame {
    vec![
        REG_BRIGHTNESS,
        if led_on { DEFAULT_BRIGHTNESS } else { 0 },
        REG_BYPASS,
        bypass as u8,
        REG_STRIP_LENGTH,
        STRIP_LENGTH,
    ]
}

pub fn brightness_frame(value: u8) -> Frame {
    vec![REG_BRIGHTNESS, value]
}

pub fn bypass_frame(enabled: bool) -> Frame {
    vec![REG_BYPASS, enabled as u8]
}

pub fn direction_frame(direction: u8) -> Frame {
    vec![REG_DIRECTION, (direction != 0) as u8]
}

pub fn param_frame(param: &Param, value: i32) -> Frame {
    vec![param.reg, param.wire(value)]
}

pub fn color_frame(mode: &Mode, color: Rgb) -> Result<Frame> {
    let regs = mode
        .color_regs
        .ok_or_else(|| Error::Invalid(format!("{} has no colour", mode.name)))?;
    Ok(vec![regs[0], color.0, regs[1], color.1, regs[2], color.2])
}

/// Frames that select a mode and apply all its settings.
pub fn mode_frames(mode: &Mode, settings: &ModeSettings, led_on: bool) -> Result<Vec<Frame>> {
    let select = vec![REG_MODE, mode.id];
    let direction = direction_frame(settings.direction.unwrap_or(DIRECTION_LEFT));
    let mut frames = match (mode.has_direction, mode.direction_first) {
        (true, true) => vec![direction, select],
        (true, false) => vec![select, direction],
        (false, _) => vec![select],
    };
    frames.push(brightness_frame(if led_on {
        settings.brightness
    } else {
        0
    }));
    for param in mode.params {
        let value = settings
            .params
            .get(param.key)
            .copied()
            .unwrap_or(param.default);
        frames.push(param_frame(param, value));
    }
    if mode.color_regs.is_some() {
        let text = settings.color.as_deref().unwrap_or("#FFFFFF");
        frames.push(color_frame(mode, parse_color(text)?)?);
    }
    Ok(frames)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::default_mode_settings;

    fn compact(frame: &[u8]) -> String {
        hex(frame).replace(' ', "")
    }

    fn frames(key: &str, edit: impl FnOnce(&mut ModeSettings)) -> Vec<String> {
        let mut settings = default_mode_settings(key);
        edit(&mut settings);
        let mode = find_mode(key).unwrap();
        mode_frames(mode, &settings, true)
            .unwrap()
            .iter()
            .map(|f| compact(f))
            .collect()
    }

    // Expected bytes are written out by hand so they cannot follow a table change.
    #[test]
    fn rainbow_sends_direction_first() {
        assert_eq!(
            frames("rainbow", |_| ()),
            ["1400", "1000", "3e88", "1802", "1905"]
        );
    }

    #[test]
    fn chase_inverts_response() {
        assert_eq!(
            frames("chase", |_| ()),
            ["1004", "1400", "3e88", "1201", "1105", "2901"]
        );
    }

    #[test]
    fn gradient() {
        assert_eq!(frames("gradient", |_| ()), ["1005", "3e88", "1305"]);
    }

    #[test]
    fn sequential_inverts_from_five_and_sixteen() {
        assert_eq!(
            frames("sequential", |_| ()),
            ["1003", "1400", "3e88", "1603", "1700"]
        );
    }

    #[test]
    fn stacking() {
        assert_eq!(frames("stacking", |_| ()), ["1006", "1400", "3e88", "2005"]);
    }

    #[test]
    fn breathing_uses_its_own_colour_registers() {
        let out = frames("breathing", |s| s.color = Some("#102030".into()));
        assert_eq!(out, ["1002", "3e88", "c809", "c910ca20cb30"]);
    }

    #[test]
    fn custom_uses_its_own_colour_registers() {
        let out = frames("custom", |s| s.color = Some("#102030".into()));
        assert_eq!(out, ["1001", "3e88", "1a101b201c30"]);
    }

    #[test]
    fn right_direction() {
        assert!(frames("stacking", |s| s.direction = Some(1)).contains(&"1401".to_string()));
    }

    #[test]
    fn led_off_forces_zero_brightness() {
        let mode = find_mode("custom").unwrap();
        let out = mode_frames(mode, &default_mode_settings("custom"), false).unwrap();
        assert_eq!(compact(&out[1]), "3e00");
    }

    #[test]
    fn values_are_clamped_to_range() {
        let param = &find_mode("stacking").unwrap().params[0];
        assert_eq!(param.wire(99), 2);
        assert_eq!(param.wire(-5), 13);
    }

    #[test]
    fn init_frame_layout() {
        assert_eq!(compact(&init_frame(true, false)), "3e880f00270e");
        assert_eq!(compact(&init_frame(false, true)), "3e000f01270e");
    }

    #[test]
    fn every_generated_frame_is_allowlisted() {
        for mode in &MODES {
            for frame in mode_frames(mode, &default_mode_settings(mode.key), true).unwrap() {
                validate_frame(&frame).unwrap();
            }
        }
        validate_frame(&init_frame(true, true)).unwrap();
    }

    #[test]
    fn rejects_unknown_register() {
        assert!(validate_frame(&[0x00, 0x01]).is_err());
    }

    #[test]
    fn init_register_only_accepts_its_one_value() {
        assert!(validate_frame(&[REG_STRIP_LENGTH, 0x01]).is_err());
    }

    #[test]
    fn rejects_odd_and_empty_frames() {
        for bad in [&[][..], &[0x10], &[0x10, 0x00, 0x3e]] {
            assert!(validate_frame(bad).is_err());
        }
    }

    #[test]
    fn colourless_mode_refuses_colour() {
        assert!(color_frame(find_mode("chase").unwrap(), (1, 2, 3)).is_err());
    }
}
