//! The btop-style monitor under the effect list.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use adw::prelude::*;
use gtk::cairo;

use crate::i18n::Text;
use crate::monitor::{
    format_bytes, format_percent, format_temperature, temperature_fraction, zone, Reading, Zone,
};

const METER_HEIGHT: i32 = 9;
const CELL_WIDTH: i32 = 4;
const CELL_STEP: i32 = 6;
const HISTORY_ROWS: i32 = 6;
const DOT_SIZE: f64 = 2.0;
const DOT_STEP: i32 = 4;
/// Enough columns for the widest sidebar.
const HISTORY_LENGTH: usize = 60;
const DIM_ALPHA: f64 = 0.16;

/// A value at `position` along a meter, 0 to 1.
fn zone_color(position: f64, dark: bool) -> (f64, f64, f64) {
    match (zone(position), dark) {
        (Zone::Normal, true) => (0.20, 0.82, 0.48),
        (Zone::Warm, true) => (0.96, 0.83, 0.18),
        (Zone::Hot, true) => (1.00, 0.48, 0.39),
        (Zone::Normal, false) => (0.11, 0.52, 0.33),
        (Zone::Warm, false) => (0.61, 0.43, 0.01),
        (Zone::Hot, false) => (0.75, 0.11, 0.16),
    }
}

/// How many cells to light. Anything above zero lights one, so it never looks off.
fn lit_cells(fraction: f64, cells: i32) -> i32 {
    if fraction <= 0.0 {
        return 0;
    }
    ((fraction.min(1.0) * cells as f64).round() as i32).max(1)
}

fn is_dark() -> bool {
    adw::StyleManager::default().is_dark()
}

fn set_dim(area: &gtk::DrawingArea, cr: &cairo::Context) {
    let color = area.color();
    cr.set_source_rgba(
        color.red() as f64,
        color.green() as f64,
        color.blue() as f64,
        DIM_ALPHA,
    );
}

struct Meter {
    area: gtk::DrawingArea,
    value: gtk::Label,
    fraction: Rc<Cell<f64>>,
}

impl Meter {
    fn new(grid: &gtk::Grid, row: i32, title: &str, tooltip: &str) -> Self {
        let label = gtk::Label::new(Some(title));
        label.add_css_class("dim-label");
        label.set_xalign(0.0);

        let area = gtk::DrawingArea::new();
        area.set_content_height(METER_HEIGHT);
        area.set_hexpand(true);
        area.set_valign(gtk::Align::Center);
        let fraction = Rc::new(Cell::new(0.0));
        let shown = fraction.clone();
        area.set_draw_func(move |area, cr, width, height| {
            let cells = (width + CELL_STEP - CELL_WIDTH) / CELL_STEP;
            let lit = lit_cells(shown.get(), cells);
            let dark = is_dark();
            for cell in 0..cells {
                if cell < lit {
                    let (r, g, b) = zone_color(cell as f64 / cells as f64, dark);
                    cr.set_source_rgb(r, g, b);
                } else {
                    set_dim(area, cr);
                }
                let x = (cell * CELL_STEP) as f64;
                cr.rectangle(x, 0.0, CELL_WIDTH as f64, height as f64);
                let _ = cr.fill();
            }
        });

        let value = gtk::Label::new(Some("—"));
        value.set_xalign(1.0);
        value.set_width_chars(7);

        label.set_tooltip_text(Some(tooltip));
        area.set_tooltip_text(Some(tooltip));
        grid.attach(&label, 0, row, 1, 1);
        grid.attach(&area, 1, row, 1, 1);
        grid.attach(&value, 2, row, 1, 1);
        Self { area, value, fraction }
    }

    fn set(&self, fraction: Option<f64>, text: &str) {
        self.fraction.set(fraction.unwrap_or(0.0));
        self.value.set_text(text);
        self.area.queue_draw();
    }
}

/// A box with its title set into the top border. A `gtk::Frame` keeps the title
/// inside the border, so the title is overlaid on a plain bordered box instead.
fn boxed(title: &str, child: &impl IsA<gtk::Widget>) -> gtk::Overlay {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.add_css_class("monitor-box");
    content.append(child);
    let label = gtk::Label::new(Some(title));
    label.add_css_class("monitor-title");
    label.set_halign(gtk::Align::Start);
    label.set_valign(gtk::Align::Start);
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&content));
    overlay.add_overlay(&label);
    overlay
}

/// One of the clickable units on the border.
fn unit_button(label: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.add_css_class("monitor-unit");
    button.set_tooltip_text(Some(tooltip));
    button.set_cursor_from_name(Some("pointer"));
    button
}

fn meter_grid() -> gtk::Grid {
    gtk::Grid::builder().column_spacing(6).row_spacing(3).build()
}

fn history_graph(history: Rc<RefCell<VecDeque<f64>>>) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_content_height(HISTORY_ROWS * DOT_STEP);
    area.set_hexpand(true);
    area.set_draw_func(move |area, cr, width, height| {
        let history = history.borrow();
        let columns = (width / DOT_STEP) as usize;
        let dark = is_dark();
        for column in 0..columns {
            // Newest on the right, as in btop.
            let age = columns - 1 - column;
            let value = history.iter().rev().nth(age).copied().unwrap_or(0.0);
            let lit = lit_cells(value, HISTORY_ROWS);
            for row in 0..HISTORY_ROWS {
                if row < lit {
                    let (r, g, b) = zone_color(row as f64 / HISTORY_ROWS as f64, dark);
                    cr.set_source_rgb(r, g, b);
                } else if row == 0 {
                    set_dim(area, cr);
                } else {
                    continue;
                }
                let x = (column as i32 * DOT_STEP) as f64;
                let y = (height - (row + 1) * DOT_STEP) as f64;
                cr.rectangle(x, y, DOT_SIZE, DOT_SIZE);
                let _ = cr.fill();
            }
        }
    });
    area
}

pub struct MonitorPanel {
    pub widget: gtk::Box,
    usage: Meter,
    gpu_temperature: Meter,
    vram_temperature: Meter,
    vram_used: Meter,
    history: Rc<RefCell<VecDeque<f64>>>,
    graph: gtk::DrawingArea,
    last: Cell<Reading>,
    celsius_button: gtk::Button,
    fahrenheit_button: gtk::Button,
    fahrenheit: Cell<bool>,
}

impl MonitorPanel {
    /// `usage` is false where GPU usage cannot be measured, which hides it.
    pub fn new(text: &Text, usage: bool) -> Rc<Self> {
        let gpu_rows = meter_grid();
        let load = Meter::new(&gpu_rows, 0, text.load_short, text.usage_hint);
        let gpu_temperature = Meter::new(&gpu_rows, 1, text.temp_short, text.gpu_temperature);
        let history = Rc::new(RefCell::new(VecDeque::with_capacity(HISTORY_LENGTH)));
        let graph = history_graph(history.clone());
        graph.set_tooltip_text(Some(text.usage_hint));
        let gpu_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        gpu_box.append(&gpu_rows);
        gpu_box.append(&graph);
        if !usage {
            // Attached rows stay in the grid, so hide all three of its children.
            for column in 0..3 {
                if let Some(child) = gpu_rows.child_at(column, 0) {
                    child.set_visible(false);
                }
            }
            graph.set_visible(false);
        }

        let vram_rows = meter_grid();
        let vram_temperature = Meter::new(&vram_rows, 0, text.temp_short, text.vram_temperature);
        let vram_used = Meter::new(&vram_rows, 1, text.used_short, text.vram_used);

        let widget = gtk::Box::new(gtk::Orientation::Vertical, 4);
        widget.add_css_class("monitor");
        let gpu_frame = boxed("gpu", &gpu_box);
        let celsius_button = unit_button("°C", text.celsius);
        let fahrenheit_button = unit_button("°F", text.fahrenheit);
        let units = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        units.add_css_class("monitor-units");
        units.set_halign(gtk::Align::End);
        units.set_valign(gtk::Align::Start);
        units.append(&celsius_button);
        units.append(&fahrenheit_button);
        gpu_frame.add_overlay(&units);
        widget.append(&gpu_frame);
        widget.append(&boxed("vram", &vram_rows));

        // Meter colours are picked in code, so they must follow a theme change.
        let (areas, chart) = (
            [
                load.area.clone(),
                gpu_temperature.area.clone(),
                vram_temperature.area.clone(),
                vram_used.area.clone(),
            ],
            graph.clone(),
        );
        adw::StyleManager::default().connect_dark_notify(move |_| {
            areas.iter().for_each(|area| area.queue_draw());
            chart.queue_draw();
        });

        let panel = Rc::new(Self {
            widget,
            usage: load,
            gpu_temperature,
            vram_temperature,
            vram_used,
            history,
            graph,
            last: Cell::new(Reading::default()),
            celsius_button,
            fahrenheit_button,
            fahrenheit: Cell::new(false),
        });
        for (button, fahrenheit) in [
            (&panel.celsius_button, false),
            (&panel.fahrenheit_button, true),
        ] {
            let weak = Rc::downgrade(&panel);
            button.connect_clicked(move |_| {
                if let Some(panel) = weak.upgrade() {
                    panel.set_fahrenheit(fahrenheit);
                }
            });
        }
        panel.show_unit();
        panel
    }

    fn show_unit(&self) {
        let fahrenheit = self.fahrenheit.get();
        for (button, active) in [
            (&self.celsius_button, !fahrenheit),
            (&self.fahrenheit_button, fahrenheit),
        ] {
            if active {
                button.add_css_class("active");
            } else {
                button.remove_css_class("active");
            }
        }
    }

    fn render(&self) {
        let reading = self.last.get();
        let fahrenheit = self.fahrenheit.get();
        let temperature = |meter: &Meter, celsius: Option<f64>, limit| {
            let fraction = celsius.map(|celsius| temperature_fraction(celsius, limit));
            meter.set(fraction, &format_temperature(celsius, fahrenheit));
        };
        temperature(&self.gpu_temperature, reading.gpu_celsius, reading.gpu_limit);
        temperature(&self.vram_temperature, reading.vram_celsius, reading.vram_limit);
        self.usage.set(
            reading.gpu_percent.map(|percent| percent / 100.0),
            &format_percent(reading.gpu_percent),
        );
        self.vram_used
            .set(reading.vram_fraction(), &format_bytes(reading.vram_bytes));
    }

    pub fn show(&self, reading: Reading) {
        self.last.set(reading);
        if let Some(percent) = reading.gpu_percent {
            let mut history = self.history.borrow_mut();
            if history.len() == HISTORY_LENGTH {
                history.pop_front();
            }
            history.push_back(percent / 100.0);
        }
        self.graph.queue_draw();
        self.render();
    }

    fn set_fahrenheit(&self, fahrenheit: bool) {
        self.fahrenheit.set(fahrenheit);
        self.show_unit();
        self.render();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_lights_in_proportion() {
        assert_eq!(lit_cells(0.0, 20), 0);
        assert_eq!(lit_cells(0.5, 20), 10);
        assert_eq!(lit_cells(1.0, 20), 20);
    }

    #[test]
    fn any_value_lights_a_cell_and_none_overflow() {
        assert_eq!(lit_cells(0.001, 20), 1);
        assert_eq!(lit_cells(1.5, 20), 20);
    }
}
