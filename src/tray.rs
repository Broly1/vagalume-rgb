//! System tray icon, through the StatusNotifierItem interface. KDE has this
//! built in; GNOME needs an AppIndicator extension. Without a tray host the
//! window simply stays the only way in.

use std::f64::consts::TAU;
use std::path::Path;

use gtk::cairo;
use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::{CheckmarkItem, StandardItem};
use ksni::{Icon, MenuItem};

use crate::i18n::Text;
use crate::{APP_ID, APP_NAME};

#[derive(Clone, Debug)]
pub enum TrayEvent {
    Show,
    ToggleLighting,
    Profile(String),
    Quit,
}

/// What the tray menu shows; the window sends a new one whenever it changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrayState {
    pub lighting: bool,
    pub profiles: Vec<String>,
    pub active: Option<String>,
}

struct Tray {
    text: &'static Text,
    events: async_channel::Sender<TrayEvent>,
    icons: Vec<Icon>,
    state: TrayState,
}

impl Tray {
    fn notify(&self, event: TrayEvent) {
        let _ = self.events.send_blocking(event);
    }
}

/// Square sizes offered to the tray; hosts pick the closest to their panel.
const ICON_SIZES: [i32; 7] = [16, 22, 24, 32, 48, 64, 128];

/// The app icon's disc fills the tray slot: (centre, radius) on its 512-unit canvas.
const DISC: (f64, f64) = (256.0, 240.0);
/// How far the V is scaled in, so its tips and glow sit inside the disc.
const INSET: f64 = 0.88;

/// The app icon: a V drawn by a firefly's light trail, ending at its glow. The
/// dark disc keeps it readable on any panel. Small sizes get a heavier, brighter
/// stroke because the trail's dark start would otherwise melt into the disc.
fn draw_firefly(cr: &cairo::Context, size: i32) -> Result<(), cairo::Error> {
    let small = size <= 32;
    let (center, radius) = DISC;
    let scale = size as f64 / (radius * 2.0);
    cr.scale(scale, scale);
    cr.translate(radius - center, radius - center);

    cr.arc(center, center, radius, 0.0, TAU);
    cr.set_source_rgb(0.05, 0.067, 0.125);
    cr.fill_preserve()?;
    cr.clip();
    cr.translate(center, center);
    cr.scale(INSET, INSET);
    cr.translate(-center, -center);

    let trail = cairo::LinearGradient::new(110.0, 120.0, 388.0, 400.0);
    match small {
        true => trail.add_color_stop_rgb(0.0, 0.33, 0.42, 0.70),
        false => trail.add_color_stop_rgb(0.0, 0.165, 0.208, 0.376),
    }
    trail.add_color_stop_rgb(1.0, 0.843, 0.961, 0.227);
    cr.move_to(110.0, 120.0);
    cr.line_to(256.0, 400.0);
    cr.line_to(388.0, 150.0);
    cr.set_source(&trail)?;
    cr.set_line_width(if small { 62.0 } else { 46.0 });
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);
    cr.stroke()?;

    let (x, y) = (396.0, 130.0);
    let glow = cairo::RadialGradient::new(x, y, 22.0, x, y, 112.0);
    glow.add_color_stop_rgba(0.0, 0.949, 1.0, 0.439, 1.0);
    glow.add_color_stop_rgba(1.0, 0.847, 1.0, 0.188, 0.0);
    cr.set_source(&glow)?;
    cr.arc(x, y, 112.0, 0.0, TAU);
    cr.fill()?;
    cr.set_source_rgb(0.992, 1.0, 0.816);
    cr.arc(x, y, if small { 44.0 } else { 36.0 }, 0.0, TAU);
    cr.fill()
}

/// One size of the firefly, as the straight big-endian ARGB the tray spec uses.
pub fn firefly_icon(size: i32) -> Result<Icon, cairo::Error> {
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, size, size)?;
    draw_firefly(&cairo::Context::new(&surface)?, size)?;
    surface.flush();
    let stride = surface.stride() as usize;
    let side = size as usize;
    let source = surface.data().map_err(|_| cairo::Error::SurfaceFinished)?;
    let mut data = Vec::with_capacity(side * side * 4);
    for row in source.chunks_exact(stride) {
        for pixel in row[..side * 4].as_chunks::<4>().0 {
            let [alpha, red, green, blue] = u32::from_ne_bytes(*pixel).to_be_bytes();
            let straight = |channel: u8| match alpha {
                0 => 0,
                _ => (channel as u32 * 255 / alpha as u32).min(255) as u8,
            };
            data.extend([alpha, straight(red), straight(green), straight(blue)]);
        }
    }
    Ok(Icon {
        width: size,
        height: size,
        data,
    })
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        APP_ID.to_string()
    }

    fn title(&self) -> String {
        APP_NAME.to_string()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        self.icons.clone()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.notify(TrayEvent::Show);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items: Vec<MenuItem<Self>> = vec![
            StandardItem {
                label: self.text.show.to_string(),
                activate: Box::new(|tray: &mut Self| tray.notify(TrayEvent::Show)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            CheckmarkItem {
                label: self.text.lighting.to_string(),
                checked: self.state.lighting,
                activate: Box::new(|tray: &mut Self| tray.notify(TrayEvent::ToggleLighting)),
                ..Default::default()
            }
            .into(),
        ];
        if !self.state.profiles.is_empty() {
            items.push(MenuItem::Separator);
        }
        for name in &self.state.profiles {
            let chosen = name.clone();
            items.push(
                CheckmarkItem {
                    label: name.clone(),
                    checked: self.state.active.as_ref() == Some(name),
                    activate: Box::new(move |tray: &mut Self| {
                        tray.notify(TrayEvent::Profile(chosen.clone()));
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }
        items.push(MenuItem::Separator);
        items.push(
            StandardItem {
                label: self.text.quit.to_string(),
                activate: Box::new(|tray: &mut Self| tray.notify(TrayEvent::Quit)),
                ..Default::default()
            }
            .into(),
        );
        items
    }
}

pub struct TrayHandle(Handle<Tray>);

impl TrayHandle {
    pub fn update(&self, state: TrayState) {
        self.0.update(move |tray| tray.state = state);
    }

    pub fn shutdown(&self) {
        self.0.shutdown().wait();
    }
}

/// Returns `None` when the desktop has no tray host.
pub fn spawn(
    text: &'static Text,
    events: async_channel::Sender<TrayEvent>,
    state: TrayState,
) -> Option<TrayHandle> {
    let icons = ICON_SIZES
        .iter()
        .filter_map(|&size| firefly_icon(size).ok())
        .collect();
    // A Flatpak sandbox may not claim the usual per-process bus name.
    let sandboxed = Path::new("/.flatpak-info").exists();
    Tray {
        text,
        events,
        icons,
        state,
    }
    .disable_dbus_name(sandboxed)
    .spawn()
    .ok()
    .map(TrayHandle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(icon: &Icon, x: i32, y: i32) -> [u8; 4] {
        let start = ((y * icon.width + x) * 4) as usize;
        icon.data[start..start + 4].try_into().unwrap()
    }

    fn tray(state: TrayState) -> (Tray, async_channel::Receiver<TrayEvent>) {
        let (events, inbox) = async_channel::unbounded();
        let text = crate::i18n::text("en");
        (
            Tray {
                text,
                events,
                icons: Vec::new(),
                state,
            },
            inbox,
        )
    }

    fn checkmarks(tray: &Tray) -> Vec<(String, bool)> {
        ksni::Tray::menu(tray)
            .into_iter()
            .filter_map(|item| match item {
                MenuItem::Checkmark(item) => Some((item.label, item.checked)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn menu_shows_lighting_and_marks_the_active_profile() {
        let (tray, _inbox) = tray(TrayState {
            lighting: true,
            profiles: vec!["Gaming".into(), "Night".into()],
            active: Some("Night".into()),
        });
        let expected = [("Lighting", true), ("Gaming", false), ("Night", true)]
            .map(|(label, checked)| (label.to_string(), checked));
        assert_eq!(checkmarks(&tray), expected);
    }

    #[test]
    fn clicking_a_profile_reports_its_name() {
        let (mut tray, inbox) = tray(TrayState {
            profiles: vec!["Night".into()],
            ..TrayState::default()
        });
        let item = ksni::Tray::menu(&tray)
            .into_iter()
            .filter_map(|item| match item {
                MenuItem::Checkmark(item) => Some(item),
                _ => None,
            })
            .next_back()
            .unwrap();
        (item.activate)(&mut tray);
        assert!(matches!(inbox.try_recv(), Ok(TrayEvent::Profile(name)) if name == "Night"));
    }

    #[test]
    fn every_size_has_the_exact_byte_length() {
        for size in ICON_SIZES {
            let icon = firefly_icon(size).unwrap();
            assert_eq!((icon.width, icon.height), (size, size));
            assert_eq!(icon.data.len(), (size * size * 4) as usize);
        }
    }

    #[test]
    fn outside_the_disc_is_clear_and_the_glow_is_bright() {
        let icon = firefly_icon(64).unwrap();
        for (x, y) in [(0, 0), (63, 0), (0, 63), (63, 63)] {
            assert_eq!(pixel(&icon, x, y)[0], 0);
        }
        let [alpha, red, green, _] = pixel(&icon, 48, 17);
        assert_eq!(alpha, 255);
        assert!(red > 230 && green > 230, "{red} {green}");
    }

    #[test]
    fn disc_edges_keep_their_colour_after_unpremultiplying() {
        let icon = firefly_icon(128).unwrap();
        let edge: Vec<_> = (0..40)
            .flat_map(|y| (0..40).map(move |x| (x, y)))
            .map(|(x, y)| pixel(&icon, x, y))
            .filter(|p| (40..200).contains(&p[0]))
            .collect();
        assert!(!edge.is_empty());
        assert!(edge.iter().all(|p| (28..=36).contains(&p[3])), "{edge:?}");
    }

    #[test]
    fn the_point_of_the_v_stays_visible_at_every_size() {
        for size in ICON_SIZES {
            let icon = firefly_icon(size).unwrap();
            let tip = (size as f64 * 0.74) as i32;
            let [alpha, _, green, _] = pixel(&icon, size / 2, tip);
            assert!(alpha == 255 && green > 150, "{size}px: {alpha} {green}");
        }
    }
}
