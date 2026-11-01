//! A colour wheel: hue around the rim, saturation towards the centre.

use std::cell::{Cell, RefCell};
use std::f64::consts::TAU;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{cairo, gdk};

use crate::color::{from_hue_sat, to_hue_sat, Rgb};

const SIZE: i32 = 196;
const MARKER_RADIUS: f64 = 7.0;

type Callback = Rc<RefCell<Option<Box<dyn Fn(Rgb)>>>>;

pub struct ColorWheel {
    pub widget: gtk::DrawingArea,
    color: Rc<Cell<Rgb>>,
    picked: Callback,
}

fn render(pixels: i32) -> Option<cairo::ImageSurface> {
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, pixels, pixels).ok()?;
    let stride = surface.stride() as usize;
    let radius = pixels as f64 / 2.0;
    {
        let mut data = surface.data().ok()?;
        for y in 0..pixels as usize {
            for x in 0..pixels as usize {
                let (dx, dy) = (x as f64 + 0.5 - radius, y as f64 + 0.5 - radius);
                let distance = dx.hypot(dy);
                let alpha = (radius - distance).clamp(0.0, 1.0);
                let (r, g, b) = from_hue_sat((-dy).atan2(dx) / TAU, (distance / radius).min(1.0));
                // Premultiplied native-endian ARGB.
                let scaled = |channel: u8| (channel as f64 * alpha) as u32;
                let argb =
                    ((alpha * 255.0) as u32) << 24 | scaled(r) << 16 | scaled(g) << 8 | scaled(b);
                data[y * stride + x * 4..][..4].copy_from_slice(&argb.to_ne_bytes());
            }
        }
    }
    Some(surface)
}

impl ColorWheel {
    pub fn new() -> Self {
        let widget = gtk::DrawingArea::new();
        widget.set_content_width(SIZE);
        widget.set_content_height(SIZE);
        widget.set_halign(gtk::Align::Center);
        widget.set_valign(gtk::Align::Center);
        widget.set_cursor(gdk::Cursor::from_name("crosshair", None).as_ref());

        let color: Rc<Cell<Rgb>> = Rc::new(Cell::new((255, 255, 255)));
        let picked: Callback = Rc::new(RefCell::new(None));
        let cache: RefCell<Option<(i32, cairo::ImageSurface)>> = RefCell::new(None);

        let shown = color.clone();
        widget.set_draw_func(move |area, cr, width, height| {
            let scale = area.scale_factor();
            let mut cached = cache.borrow_mut();
            if cached
                .as_ref()
                .is_none_or(|(rendered, _)| *rendered != scale)
            {
                *cached = render(SIZE * scale).map(|surface| (scale, surface));
            }
            let (left, top) = ((width - SIZE) as f64 / 2.0, (height - SIZE) as f64 / 2.0);
            if let Some((_, surface)) = cached.as_ref() {
                let _ = cr.save();
                cr.translate(left, top);
                cr.scale(1.0 / scale as f64, 1.0 / scale as f64);
                let _ = cr.set_source_surface(surface, 0.0, 0.0);
                let _ = cr.paint();
                let _ = cr.restore();
            }
            let (hue, saturation) = to_hue_sat(shown.get());
            let radius = SIZE as f64 / 2.0;
            let reach = saturation * radius;
            let (x, y) = (
                left + radius + reach * (hue * TAU).cos(),
                top + radius - reach * (hue * TAU).sin(),
            );
            cr.arc(x, y, MARKER_RADIUS, 0.0, TAU);
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.7);
            cr.set_line_width(3.5);
            let _ = cr.stroke_preserve();
            cr.set_source_rgb(1.0, 1.0, 1.0);
            cr.set_line_width(2.0);
            let _ = cr.stroke();
        });

        let pick = {
            let (area, color, picked) = (widget.clone(), color.clone(), picked.clone());
            move |x: f64, y: f64| {
                let radius = SIZE as f64 / 2.0;
                let (dx, dy) = (
                    x - area.width() as f64 / 2.0,
                    y - area.height() as f64 / 2.0,
                );
                let value = from_hue_sat((-dy).atan2(dx) / TAU, (dx.hypot(dy) / radius).min(1.0));
                color.set(value);
                area.queue_draw();
                if let Some(callback) = picked.borrow().as_ref() {
                    callback(value);
                }
            }
        };

        let drag = gtk::GestureDrag::new();
        let on_begin = pick.clone();
        drag.connect_drag_begin(move |_, x, y| on_begin(x, y));
        drag.connect_drag_update(move |gesture, dx, dy| {
            if let Some((x, y)) = gesture.start_point() {
                pick(x + dx, y + dy);
            }
        });
        widget.add_controller(drag);

        Self {
            widget,
            color,
            picked,
        }
    }

    pub fn set_color(&self, color: Rgb) {
        self.color.set(color);
        self.widget.queue_draw();
    }

    pub fn connect_picked(&self, callback: impl Fn(Rgb) + 'static) {
        *self.picked.borrow_mut() = Some(Box::new(callback));
    }
}
