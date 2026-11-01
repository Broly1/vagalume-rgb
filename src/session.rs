//! What the window does, without the window: applies each change to the settings
//! and collects the frames to send. Keeping this free of the toolkit lets the
//! behaviour be tested without a display.

use crate::color::{parse_color, to_hex, Rgb};
use crate::config::Config;
use crate::controller::led_frame;
use crate::error::Result;
use crate::protocol::{self, find_mode, Frame, Mode};

#[derive(Default)]
pub struct Session {
    pub config: Config,
    /// Frames waiting for the debounce timer. A newer value for a key replaces
    /// an unsent one, so dragging a slider sends only where it stopped.
    pending: Vec<(&'static str, Vec<Frame>)>,
}

impl Session {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            pending: Vec::new(),
        }
    }

    pub fn mode(&self) -> &'static Mode {
        find_mode(&self.config.mode).expect("mode is always validated")
    }

    fn queue(&mut self, key: &'static str, frames: Vec<Frame>) {
        match self
            .pending
            .iter_mut()
            .find(|(existing, _)| *existing == key)
        {
            Some(entry) => entry.1 = frames,
            None => self.pending.push((key, frames)),
        }
    }

    pub fn take_frames(&mut self) -> Vec<Frame> {
        self.pending
            .drain(..)
            .flat_map(|(_, frames)| frames)
            .collect()
    }

    pub fn select_mode(&mut self, key: &str) -> Result<()> {
        let mode = find_mode(key).expect("the sidebar only offers known modes");
        self.config.mode = mode.key.to_string();
        let frames = protocol::mode_frames(mode, self.config.settings(), self.config.led)?;
        self.queue("mode", frames);
        Ok(())
    }

    pub fn set_brightness(&mut self, value: u8) {
        self.config.settings_mut().brightness = value;
        if self.config.led {
            self.queue("brightness", vec![protocol::brightness_frame(value)]);
        }
    }

    pub fn set_direction(&mut self, direction: u8) {
        self.config.settings_mut().direction = Some(direction);
        self.queue("direction", vec![protocol::direction_frame(direction)]);
    }

    pub fn set_param(&mut self, key: &'static str, value: i32) {
        let param = self.mode().params.iter().find(|param| param.key == key);
        let param = param.expect("rows are built from the active mode");
        self.config
            .settings_mut()
            .params
            .insert(key.to_string(), value);
        self.queue(key, vec![protocol::param_frame(param, value)]);
    }

    pub fn color(&self) -> Rgb {
        let text = self.config.settings().color.as_deref().unwrap_or("#FFFFFF");
        parse_color(text).unwrap_or((255, 255, 255))
    }

    pub fn set_color(&mut self, color: Rgb) -> Result<()> {
        let frame = protocol::color_frame(self.mode(), color)?;
        self.config.settings_mut().color = Some(to_hex(color));
        self.queue("color", vec![frame]);
        Ok(())
    }

    pub fn set_led(&mut self, on: bool) {
        self.config.led = on;
        self.queue("brightness", vec![led_frame(&self.config)]);
    }

    pub fn set_bypass(&mut self, on: bool) {
        self.config.bypass = on;
        self.queue("bypass", vec![protocol::bypass_frame(on)]);
    }

    pub fn apply_profile(&mut self, name: &str) -> Result<()> {
        self.config.apply_profile(name)?;
        let mut frames = vec![protocol::bypass_frame(false)];
        frames.extend(protocol::mode_frames(
            self.mode(),
            self.config.settings(),
            true,
        )?);
        self.pending.clear();
        self.queue("mode", frames);
        Ok(())
    }

    pub fn reset(&mut self) -> Result<()> {
        self.config.reset_lighting();
        let mut frames = vec![protocol::bypass_frame(false)];
        frames.extend(protocol::mode_frames(
            self.mode(),
            self.config.settings(),
            true,
        )?);
        self.pending.clear();
        self.queue("mode", frames);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{hex, MODES};

    fn sent(session: &mut Session) -> Vec<String> {
        session
            .take_frames()
            .iter()
            .map(|frame| hex(frame).replace(' ', ""))
            .collect()
    }

    #[test]
    fn selecting_a_mode_sends_its_full_sequence() {
        let mut session = Session::default();
        session.select_mode("breathing").unwrap();
        assert_eq!(sent(&mut session), ["1002", "3e88", "c809", "c9ffcaffcbff"]);
    }

    #[test]
    fn slider_burst_sends_only_the_last_value() {
        let mut session = Session::default();
        for value in [10, 20, 30] {
            session.set_brightness(value);
        }
        assert_eq!(sent(&mut session), ["3e1e"]);
    }

    #[test]
    fn led_off_then_slider_stays_dark() {
        let mut session = Session::default();
        session.set_led(false);
        session.set_brightness(200);
        assert_eq!(sent(&mut session), ["3e00"]);
        assert_eq!(session.config.settings().brightness, 200);
    }

    #[test]
    fn led_back_on_uses_the_saved_brightness() {
        let mut session = Session::default();
        session.set_brightness(200);
        session.set_led(false);
        session.set_led(true);
        assert_eq!(sent(&mut session), ["3ec8"]);
    }

    #[test]
    fn colour_uses_the_active_mode_registers() {
        let mut session = Session::default();
        session.select_mode("custom").unwrap();
        session.take_frames();
        session.set_color((0x10, 0x20, 0x30)).unwrap();
        assert_eq!(sent(&mut session), ["1a101b201c30"]);
        assert_eq!(session.config.settings().color.as_deref(), Some("#102030"));
    }

    #[test]
    fn param_inverts_on_the_wire_but_saves_the_slider_value() {
        let mut session = Session::default();
        session.select_mode("stacking").unwrap();
        session.take_frames();
        session.set_param("speed", 3);
        assert_eq!(sent(&mut session), ["200c"]);
        assert_eq!(session.config.settings().params["speed"], 3);
    }

    #[test]
    fn each_mode_keeps_its_own_settings() {
        let mut session = Session::default();
        session.select_mode("custom").unwrap();
        session.set_brightness(10);
        session.select_mode("breathing").unwrap();
        assert_eq!(session.config.settings().brightness, 0x88);
        session.select_mode("custom").unwrap();
        assert_eq!(session.config.settings().brightness, 10);
    }

    #[test]
    fn colour_is_refused_on_colourless_modes() {
        let mut session = Session::default();
        assert!(session.set_color((1, 2, 3)).is_err());
        assert!(session.take_frames().is_empty());
    }

    #[test]
    fn reset_returns_to_the_default_mode_and_clears_bypass() {
        let mut session = Session::default();
        session.set_bypass(true);
        session.select_mode("custom").unwrap();
        session.take_frames();
        session.reset().unwrap();
        assert_eq!(session.config.mode, "rainbow");
        assert!(!session.config.bypass);
        assert_eq!(sent(&mut session)[..3], ["0f00", "1400", "1000"]);
    }

    #[test]
    fn applying_a_profile_replaces_pending_changes_and_takes_control() {
        let mut session = Session::default();
        session.set_brightness(10);
        session.set_led(false);
        session.apply_profile("Night").unwrap();
        assert_eq!(sent(&mut session), ["0f00", "1001", "3e18", "1aff1b6a1c20"]);
        assert!(session.config.led);
    }

    #[test]
    fn unknown_profile_sends_nothing() {
        let mut session = Session::default();
        assert!(session.apply_profile("Nope").is_err());
        assert!(session.take_frames().is_empty());
    }

    #[test]
    fn every_mode_can_be_selected() {
        let mut session = Session::default();
        for mode in &MODES {
            session.select_mode(mode.key).unwrap();
            for frame in session.take_frames() {
                protocol::validate_frame(&frame).unwrap();
            }
        }
    }
}
