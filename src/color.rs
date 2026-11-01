//! Colour text and conversions.

use crate::error::{Error, Result};

pub type Rgb = (u8, u8, u8);

pub fn parse_color(text: &str) -> Result<Rgb> {
    let trimmed = text.trim();
    let digits = trimmed.strip_prefix('#').unwrap_or(trimmed);
    let bad = || Error::Invalid(format!("expected #RRGGBB, got {text:?}"));
    if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad());
    }
    let value = u32::from_str_radix(digits, 16).map_err(|_| bad())?;
    Ok(((value >> 16) as u8, (value >> 8) as u8, value as u8))
}

pub fn to_hex((r, g, b): Rgb) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

/// Hue in 0..1, saturation in 0..1, full value.
pub fn from_hue_sat(hue: f64, saturation: f64) -> Rgb {
    let sector = hue.rem_euclid(1.0) * 6.0;
    let index = sector.floor() as u32 % 6;
    let fraction = sector - sector.floor();
    let channel = |v: f64| ((1.0 - saturation * v) * 255.0).round() as u8;
    let (low, falling, rising) = (channel(1.0), channel(fraction), channel(1.0 - fraction));
    match index {
        0 => (255, rising, low),
        1 => (falling, 255, low),
        2 => (low, 255, rising),
        3 => (low, falling, 255),
        4 => (rising, low, 255),
        _ => (255, low, falling),
    }
}

/// Hue and saturation of a colour, for placing it on the wheel.
pub fn to_hue_sat((r, g, b): Rgb) -> (f64, f64) {
    let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let max = r.max(g).max(b);
    let delta = max - r.min(g).min(b);
    if delta == 0.0 {
        return (0.0, 0.0);
    }
    let hue = if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    (hue / 6.0, delta / max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_with_and_without_hash() {
        assert_eq!(parse_color("#0000ff").unwrap(), (0, 0, 255));
        assert_eq!(parse_color(" 102030 ").unwrap(), (0x10, 0x20, 0x30));
    }

    #[test]
    fn rejects_malformed_text() {
        for bad in ["", "#12345", "#1234567", "#GGGGGG", "+12345", "#ééé"] {
            assert!(parse_color(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn formats_uppercase() {
        assert_eq!(to_hex((0, 0xAB, 255)), "#00ABFF");
    }

    #[test]
    fn primaries_land_on_expected_hues() {
        assert_eq!(from_hue_sat(0.0, 1.0), (255, 0, 0));
        assert_eq!(from_hue_sat(1.0 / 3.0, 1.0), (0, 255, 0));
        assert_eq!(from_hue_sat(2.0 / 3.0, 1.0), (0, 0, 255));
        assert_eq!(from_hue_sat(0.5, 0.0), (255, 255, 255));
    }

    #[test]
    fn hue_and_saturation_round_trip() {
        for color in [(255, 0, 0), (0, 0, 255), (255, 128, 0), (128, 255, 200), (255, 0, 128)] {
            let (hue, saturation) = to_hue_sat(color);
            let back = from_hue_sat(hue, saturation);
            let near = |a: u8, b: u8| a.abs_diff(b) <= 1;
            assert!(near(back.0, color.0) && near(back.1, color.1) && near(back.2, color.2));
        }
    }
}
