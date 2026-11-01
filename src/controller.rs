//! Turns settings into frames.

use crate::config::Config;
use crate::device::Transport;
use crate::error::Result;
use crate::protocol::{self, find_mode, Frame};

/// The whole saved state: the selected mode with all its settings.
pub fn apply_frames(config: &Config) -> Result<Vec<Frame>> {
    let mode = find_mode(&config.mode).expect("mode is always validated");
    protocol::mode_frames(mode, config.settings(), config.led)
}

/// Connect, then apply. The card may be left in bypass, so an effect selected
/// without the connect frame is set but never shown.
pub fn restore_frames(config: &Config) -> Result<Vec<Frame>> {
    let mut frames = vec![protocol::init_frame(config.led, config.bypass)];
    frames.extend(apply_frames(config)?);
    Ok(frames)
}

pub fn led_frame(config: &Config) -> Frame {
    protocol::brightness_frame(if config.led { config.settings().brightness } else { 0 })
}

pub fn send_all(transport: &mut dyn Transport, frames: &[Frame]) -> Result<()> {
    frames.iter().try_for_each(|frame| transport.send(frame).map(drop))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::DryRunTransport;

    #[test]
    fn restore_sends_init_then_mode() {
        let mut transport = DryRunTransport::new(false);
        send_all(&mut transport, &restore_frames(&Config::default()).unwrap()).unwrap();
        assert_eq!(transport.frames[0], [0x3e, 0x88, 0x0f, 0x00, 0x27, 0x0e]);
        assert_eq!(transport.frames[1], [0x14, 0x00]);
    }
}
