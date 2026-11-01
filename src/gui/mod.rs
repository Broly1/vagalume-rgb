//! The window. Every control writes through `Session`; this file only wires
//! widgets to it.

mod wheel;
mod worker;

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::{gdk, gio, glib};

use crate::color::{parse_color, to_hex, Rgb};
use crate::config::{config_path, load_config, save_config};
use crate::error::Result;
use crate::i18n::{self, Text};
use crate::protocol::{find_mode, Param, DIRECTION_LEFT, DIRECTION_RIGHT, MODES};
use crate::session::Session;
use crate::tray::{self, TrayEvent, TrayHandle, TrayState};
use crate::{APP_ID, APP_NAME};
use wheel::ColorWheel;
use worker::{Event, Worker};

const DEBOUNCE: Duration = Duration::from_millis(120);
const PRESETS: [Rgb; 7] = [
    (255, 0, 0),
    (255, 255, 0),
    (0, 255, 0),
    (0, 0, 255),
    (128, 128, 128),
    (128, 0, 128),
    (255, 255, 255),
];

struct Ui {
    text: &'static Text,
    session: RefCell<Session>,
    /// Where settings are saved; `None` on a dry run.
    save_path: Option<PathBuf>,
    worker: RefCell<Worker>,
    /// True while widgets are being set from saved state rather than by the user.
    loading: Cell<bool>,
    flush_timer: RefCell<Option<glib::SourceId>>,
    tray: RefCell<Option<TrayHandle>>,
    hold: RefCell<Option<gio::ApplicationHoldGuard>>,

    app: adw::Application,
    window: adw::ApplicationWindow,
    split: adw::NavigationSplitView,
    title: adw::WindowTitle,
    content_page: adw::NavigationPage,
    banner: adw::Banner,
    effects: gtk::ListBox,
    color_group: adw::PreferencesGroup,
    wheel: ColorWheel,
    channels: [adw::SpinRow; 3],
    hex: adw::EntryRow,
    settings_group: adw::PreferencesGroup,
    direction_row: adw::ActionRow,
    direction: adw::ToggleGroup,
    brightness: gtk::Scale,
    param_rows: RefCell<Vec<gtk::Widget>>,
    led_row: adw::SwitchRow,
    bypass_row: adw::SwitchRow,
    keep_row: adw::SwitchRow,
    profile_group: adw::PreferencesGroup,
    /// Each profile's name, row and "this one is showing" mark.
    profile_rows: RefCell<Vec<(String, adw::ActionRow, gtk::Image)>>,
    tray_state: RefCell<TrayState>,
}

/// A slider row with its current value at the end.
fn slider_row(title: &str, min: f64, max: f64) -> (adw::ActionRow, gtk::Scale) {
    let row = adw::ActionRow::builder().title(title).build();
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, min, max, 1.0);
    scale.set_size_request(220, -1);
    scale.set_valign(gtk::Align::Center);
    scale.set_round_digits(0);
    let value = gtk::Label::new(None);
    value.add_css_class("numeric");
    value.add_css_class("dim-label");
    value.add_css_class("value-label");
    value.set_xalign(1.0);
    let label = value.clone();
    scale.connect_value_changed(move |scale| label.set_text(&(scale.value() as i64).to_string()));
    row.add_suffix(&scale);
    row.add_suffix(&value);
    (row, scale)
}

fn effect_row(index: usize, text: &Text) -> gtk::ListBoxRow {
    let mode = &MODES[index];
    let preview = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    preview.add_css_class("effect-preview");
    preview.add_css_class(&format!("effect-{}", mode.key));
    preview.set_valign(gtk::Align::Center);
    let label = gtk::Label::new(Some(text.mode_name(mode)));
    label.set_xalign(0.0);
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    content.append(&preview);
    content.append(&label);
    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&content));
    row
}

fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("style.css"));
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn apply_theme(theme: &str) {
    adw::StyleManager::default().set_color_scheme(match theme {
        "light" => adw::ColorScheme::ForceLight,
        "dark" => adw::ColorScheme::ForceDark,
        _ => adw::ColorScheme::Default,
    });
}

impl Ui {
    fn build(
        app: &adw::Application,
        session: Session,
        save_path: Option<PathBuf>,
        worker: Worker,
    ) -> Rc<Self> {
        let text = i18n::text(&session.config.language);

        // Sidebar: the effect list.
        let effects = gtk::ListBox::new();
        effects.add_css_class("navigation-sidebar");
        for index in 0..MODES.len() {
            effects.append(&effect_row(index, text));
        }
        let sidebar_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&effects)
            .vexpand(true)
            .build();
        let sidebar_view = adw::ToolbarView::new();
        sidebar_view.add_top_bar(&adw::HeaderBar::new());
        sidebar_view.set_content(Some(&sidebar_scroll));
        let sidebar_page = adw::NavigationPage::builder()
            .title(APP_NAME)
            .child(&sidebar_view)
            .build();

        // Colour group.
        let wheel = ColorWheel::new();
        let channels = [text.red, text.green, text.blue].map(|title| {
            let row = adw::SpinRow::with_range(0.0, 255.0, 1.0);
            row.set_title(title);
            row
        });
        let hex = adw::EntryRow::builder()
            .title(text.hex)
            .show_apply_button(true)
            .build();
        hex.set_max_length(7);
        let fields = gtk::ListBox::new();
        fields.add_css_class("boxed-list");
        fields.set_selection_mode(gtk::SelectionMode::None);
        fields.set_valign(gtk::Align::Center);
        fields.set_hexpand(true);
        channels.iter().for_each(|row| fields.append(row));
        fields.append(&hex);

        let swatches = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        swatches.set_halign(gtk::Align::Center);
        let swatch_buttons: Vec<gtk::Button> = (0..PRESETS.len())
            .map(|index| {
                let button = gtk::Button::new();
                button.add_css_class("swatch");
                button.add_css_class(&format!("swatch-{index}"));
                button.set_tooltip_text(Some(&to_hex(PRESETS[index])));
                swatches.append(&button);
                button
            })
            .collect();
        let wheel_column = gtk::Box::new(gtk::Orientation::Vertical, 14);
        wheel_column.append(&wheel.widget);
        wheel_column.append(&swatches);
        wheel_column.set_valign(gtk::Align::Center);

        let color_card = gtk::Box::new(gtk::Orientation::Horizontal, 18);
        color_card.add_css_class("card");
        color_card.add_css_class("color-card");
        color_card.append(&wheel_column);
        color_card.append(&fields);
        let color_group = adw::PreferencesGroup::builder().title(text.color).build();
        color_group.add(&color_card);

        // Settings group.
        let settings_group = adw::PreferencesGroup::builder()
            .title(text.settings)
            .build();
        let direction = adw::ToggleGroup::new();
        direction.set_valign(gtk::Align::Center);
        for (name, label) in [("left", text.left), ("right", text.right)] {
            direction.add(adw::Toggle::builder().name(name).label(label).build());
        }
        let direction_row = adw::ActionRow::builder().title(text.direction).build();
        direction_row.add_suffix(&direction);
        let (brightness_row, brightness) = slider_row(text.brightness, 0.0, 255.0);
        settings_group.add(&direction_row);
        settings_group.add(&brightness_row);

        // Output group.
        let output_group = adw::PreferencesGroup::builder().title(text.output).build();
        let led_row = adw::SwitchRow::builder().title(text.lighting).build();
        let bypass_row = adw::SwitchRow::builder()
            .title(text.bypass)
            .subtitle(text.bypass_hint)
            .build();
        let keep_row = adw::SwitchRow::builder()
            .title(text.keep)
            .subtitle(text.keep_hint)
            .build();
        output_group.add(&led_row);
        output_group.add(&bypass_row);
        output_group.add(&keep_row);

        let profile_group = adw::PreferencesGroup::builder()
            .title(text.profiles)
            .build();
        let add_profile = gtk::Button::builder()
            .icon_name("list-add-symbolic")
            .tooltip_text(text.save_profile)
            .valign(gtk::Align::Center)
            .build();
        add_profile.add_css_class("flat");
        profile_group.set_header_suffix(Some(&add_profile));

        let page = adw::PreferencesPage::new();
        page.add(&profile_group);
        page.add(&color_group);
        page.add(&settings_group);
        page.add(&output_group);

        // Header.
        let menu = gio::Menu::new();
        let appearance = gio::Menu::new();
        appearance.append(Some(text.system), Some("app.theme::system"));
        appearance.append(Some(text.light), Some("app.theme::light"));
        appearance.append(Some(text.dark), Some("app.theme::dark"));
        menu.append_section(Some(text.appearance), &appearance);
        let actions = gio::Menu::new();
        actions.append(Some(text.reset), Some("app.reset"));
        actions.append(Some(text.about), Some("app.about"));
        actions.append(Some(text.quit), Some("app.quit"));
        menu.append_section(None, &actions);
        let menu_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(&menu)
            .primary(true)
            .tooltip_text(text.settings)
            .build();

        let title = adw::WindowTitle::new("", text.connecting);
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&title));
        header.pack_end(&menu_button);

        let banner = adw::Banner::new("");
        banner.set_use_markup(false);
        banner.set_button_label(Some(text.retry));

        let content_view = adw::ToolbarView::new();
        content_view.add_top_bar(&header);
        content_view.add_top_bar(&banner);
        content_view.set_content(Some(&page));
        let content_page = adw::NavigationPage::builder()
            .title(APP_NAME)
            .child(&content_view)
            .build();

        let split = adw::NavigationSplitView::builder()
            .sidebar(&sidebar_page)
            .content(&content_page)
            .min_sidebar_width(210.0)
            .max_sidebar_width(250.0)
            .build();

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title(APP_NAME)
            .icon_name(APP_ID)
            .default_width(860)
            .default_height(790)
            .width_request(400)
            .height_request(420)
            .content(&split)
            .build();

        let narrow = adw::BreakpointCondition::parse("max-width: 680sp").expect("valid condition");
        let breakpoint = adw::Breakpoint::new(narrow);
        breakpoint.add_setter(&split, "collapsed", Some(&true.to_value()));
        breakpoint.add_setter(
            &color_card,
            "orientation",
            Some(&gtk::Orientation::Vertical.to_value()),
        );
        window.add_breakpoint(breakpoint);

        let ui = Rc::new(Self {
            text,
            session: RefCell::new(session),
            save_path,
            worker: RefCell::new(worker),
            loading: Cell::new(false),
            flush_timer: RefCell::new(None),
            tray: RefCell::new(None),
            hold: RefCell::new(None),
            app: app.clone(),
            window,
            split,
            title,
            content_page,
            banner,
            effects,
            color_group,
            wheel,
            channels,
            hex,
            settings_group,
            direction_row,
            direction,
            brightness,
            param_rows: RefCell::new(Vec::new()),
            led_row,
            bypass_row,
            keep_row,
            profile_group,
            profile_rows: RefCell::new(Vec::new()),
            tray_state: RefCell::new(TrayState::default()),
        });
        ui.connect_controls(&swatch_buttons);
        ui.install_actions();
        ui.load_state();
        ui.rebuild_profiles();
        let weak = Rc::downgrade(&ui);
        add_profile.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.confirm_save_profile();
            }
        });
        ui
    }

    fn connect_controls(self: &Rc<Self>, swatches: &[gtk::Button]) {
        let weak = Rc::downgrade(self);
        // Runs a handler unless the change came from loading saved state.
        let handle = move |action: &dyn Fn(&Rc<Self>)| {
            if let Some(ui) = weak.upgrade() {
                if !ui.loading.get() {
                    action(&ui);
                }
            }
        };

        let on = handle.clone();
        self.effects.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                let index = row.index() as usize;
                on(&|ui| ui.select_effect(index));
            }
        });
        let on = handle.clone();
        self.effects.connect_row_activated(move |_, _| {
            // On a narrow window this also moves on from the list to the settings.
            on(&|ui| ui.split.set_show_content(true));
        });

        let on = handle.clone();
        self.direction.connect_active_name_notify(move |group| {
            let right = group.active_name().as_deref() == Some("right");
            on(&|ui| {
                let value = if right {
                    DIRECTION_RIGHT
                } else {
                    DIRECTION_LEFT
                };
                ui.session.borrow_mut().set_direction(value);
                ui.schedule_flush();
            });
        });

        let on = handle.clone();
        self.brightness.connect_value_changed(move |scale| {
            let value = scale.value() as u8;
            on(&|ui| {
                ui.session.borrow_mut().set_brightness(value);
                ui.schedule_flush();
            });
        });

        let on = handle.clone();
        self.wheel
            .connect_picked(move |color| on(&|ui| ui.pick_color(color)));

        for row in &self.channels {
            let on = handle.clone();
            row.connect_value_notify(move |_| {
                on(&|ui| {
                    let [r, g, b] = ui.channels.each_ref().map(|row| row.value() as u8);
                    ui.pick_color((r, g, b));
                });
            });
        }

        let on = handle.clone();
        self.hex.connect_apply(move |entry| {
            let typed = parse_color(&entry.text());
            on(&|ui| match &typed {
                Ok(color) => {
                    ui.hex.remove_css_class("error");
                    ui.pick_color(*color);
                }
                Err(_) => ui.hex.add_css_class("error"),
            });
        });

        for (button, color) in swatches.iter().zip(PRESETS) {
            let on = handle.clone();
            button.connect_clicked(move |_| on(&|ui| ui.pick_color(color)));
        }

        let on = handle.clone();
        self.led_row.connect_active_notify(move |row| {
            let active = row.is_active();
            on(&|ui| {
                ui.session.borrow_mut().set_led(active);
                ui.schedule_flush();
            });
        });
        let on = handle.clone();
        self.bypass_row.connect_active_notify(move |row| {
            let active = row.is_active();
            on(&|ui| {
                ui.session.borrow_mut().set_bypass(active);
                ui.schedule_flush();
            });
        });
        let on = handle.clone();
        self.keep_row.connect_active_notify(move |row| {
            let active = row.is_active();
            on(&|ui| {
                ui.session.borrow_mut().config.keep_running = active;
                ui.schedule_flush();
            });
        });

        let on = handle.clone();
        self.banner.connect_button_clicked(move |banner| {
            banner.set_revealed(false);
            on(&|ui| {
                ui.title.set_subtitle(ui.text.connecting);
                let config = ui.session.borrow().config.clone();
                ui.worker.borrow().connect(config);
            });
        });

        let on = handle;
        self.window.connect_close_request(move |window| {
            let hide = Rc::new(Cell::new(false));
            let decided = hide.clone();
            on(&|ui| {
                decided.set(ui.session.borrow().config.keep_running && ui.tray.borrow().is_some());
                if decided.get() {
                    ui.hold.replace(Some(ui.app.hold()));
                    ui.flush();
                } else {
                    ui.shutdown();
                }
            });
            if hide.get() {
                window.set_visible(false);
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
    }

    fn install_actions(self: &Rc<Self>) {
        let theme = self.session.borrow().config.theme.clone();
        apply_theme(&theme);
        let action = gio::SimpleAction::new_stateful(
            "theme",
            Some(glib::VariantTy::STRING),
            &theme.to_variant(),
        );
        let weak = Rc::downgrade(self);
        action.connect_activate(move |action, value| {
            let (Some(ui), Some(value)) = (weak.upgrade(), value) else {
                return;
            };
            let Some(theme) = value.str() else { return };
            action.set_state(value);
            apply_theme(theme);
            ui.session.borrow_mut().config.theme = theme.to_string();
            ui.schedule_flush();
        });
        self.app.add_action(&action);

        let reset = gio::SimpleAction::new("reset", None);
        let weak = Rc::downgrade(self);
        reset.connect_activate(move |_, _| {
            if let Some(ui) = weak.upgrade() {
                ui.confirm_reset();
            }
        });
        self.app.add_action(&reset);

        let about = gio::SimpleAction::new("about", None);
        let weak = Rc::downgrade(self);
        about.connect_activate(move |_, _| {
            if let Some(ui) = weak.upgrade() {
                let dialog = adw::AboutDialog::builder()
                    .application_name(APP_NAME)
                    .version(env!("CARGO_PKG_VERSION"))
                    .comments(ui.text.about_text)
                    .build();
                if gtk::IconTheme::for_display(&WidgetExt::display(&ui.window)).has_icon(APP_ID) {
                    dialog.set_application_icon(APP_ID);
                }
                dialog.present(Some(&ui.window));
            }
        });
        self.app.add_action(&about);

        let quit = gio::SimpleAction::new("quit", None);
        let weak = Rc::downgrade(self);
        quit.connect_activate(move |_, _| {
            if let Some(ui) = weak.upgrade() {
                ui.quit();
            }
        });
        self.app.add_action(&quit);
        self.app.set_accels_for_action("app.quit", &["<Control>q"]);
    }

    /// Asks first, because this discards saved colours and cannot be undone.
    fn confirm_reset(self: &Rc<Self>) {
        let dialog = adw::AlertDialog::new(Some(self.text.reset_title), Some(self.text.reset_body));
        dialog.add_responses(&[
            ("cancel", self.text.cancel),
            ("reset", self.text.reset_confirm),
        ]);
        dialog.set_response_appearance("reset", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let weak = Rc::downgrade(self);
        dialog.connect_response(Some("reset"), move |_, _| {
            if let Some(ui) = weak.upgrade() {
                let done = ui.session.borrow_mut().reset();
                ui.report(done);
                ui.load_state();
                ui.schedule_flush();
            }
        });
        dialog.present(Some(&self.window));
    }

    fn rebuild_profiles(self: &Rc<Self>) {
        for (_, row, _) in self.profile_rows.borrow_mut().drain(..) {
            self.profile_group.remove(&row);
        }
        let profiles = self.session.borrow().config.profiles.clone();
        for (name, profile) in profiles {
            let mode = find_mode(&profile.mode).expect("profiles are validated on load");
            let row = adw::ActionRow::builder()
                .title(name.as_str())
                .subtitle(self.text.mode_name(mode))
                .use_markup(false)
                .activatable(true)
                .build();
            let mark = gtk::Image::from_icon_name("object-select-symbolic");
            row.add_prefix(&mark);
            let delete = gtk::Button::builder()
                .icon_name("user-trash-symbolic")
                .tooltip_text(self.text.delete)
                .valign(gtk::Align::Center)
                .build();
            delete.add_css_class("flat");
            row.add_suffix(&delete);

            let (weak, chosen) = (Rc::downgrade(self), name.clone());
            row.connect_activated(move |_| {
                if let Some(ui) = weak.upgrade() {
                    ui.apply_profile(&chosen);
                }
            });
            let (weak, chosen) = (Rc::downgrade(self), name.clone());
            delete.connect_clicked(move |_| {
                if let Some(ui) = weak.upgrade() {
                    ui.confirm_delete_profile(chosen.clone());
                }
            });
            self.profile_group.add(&row);
            self.profile_rows.borrow_mut().push((name, row, mark));
        }
        self.show_active_profile();
    }

    /// Marks the showing profile here and in the tray.
    fn show_active_profile(&self) {
        let state = {
            let session = self.session.borrow();
            let config = &session.config;
            TrayState {
                lighting: config.led,
                profiles: config.profiles.keys().cloned().collect(),
                active: config.active_profile().map(str::to_string),
            }
        };
        for (name, _, mark) in self.profile_rows.borrow().iter() {
            // Hidden by opacity so every name stays aligned.
            let active = state.active.as_ref() == Some(name);
            mark.set_opacity(if active { 1.0 } else { 0.0 });
        }
        if *self.tray_state.borrow() != state {
            self.tray_state.replace(state.clone());
            if let Some(tray) = self.tray.borrow().as_ref() {
                tray.update(state);
            }
        }
    }

    fn apply_profile(self: &Rc<Self>, name: &str) {
        let done = self.session.borrow_mut().apply_profile(name);
        self.report(done);
        self.load_state();
        self.schedule_flush();
    }

    fn confirm_save_profile(self: &Rc<Self>) {
        let dialog = adw::AlertDialog::new(
            Some(self.text.save_profile),
            Some(self.text.save_profile_hint),
        );
        let entry = adw::EntryRow::builder()
            .title(self.text.profile_name)
            .activates_default(true)
            .build();
        entry.set_max_length(32);
        let list = gtk::ListBox::new();
        list.add_css_class("boxed-list");
        list.set_selection_mode(gtk::SelectionMode::None);
        list.append(&entry);
        dialog.set_extra_child(Some(&list));
        dialog.add_responses(&[("cancel", self.text.cancel), ("save", self.text.save)]);
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_response_enabled("save", false);
        dialog.set_default_response(Some("save"));
        dialog.set_close_response("cancel");

        let answer = dialog.clone();
        entry.connect_changed(move |entry| {
            answer.set_response_enabled("save", !entry.text().trim().is_empty());
        });
        let weak = Rc::downgrade(self);
        dialog.connect_response(Some("save"), move |_, _| {
            if let Some(ui) = weak.upgrade() {
                let done = ui.session.borrow_mut().config.save_profile(&entry.text());
                ui.report(done);
                ui.rebuild_profiles();
                ui.schedule_flush();
            }
        });
        dialog.present(Some(&self.window));
    }

    fn confirm_delete_profile(self: &Rc<Self>, name: String) {
        let dialog = adw::AlertDialog::new(Some(self.text.delete_profile), Some(&name));
        dialog.add_responses(&[("cancel", self.text.cancel), ("delete", self.text.delete)]);
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let weak = Rc::downgrade(self);
        dialog.connect_response(Some("delete"), move |_, _| {
            if let Some(ui) = weak.upgrade() {
                let done = ui.session.borrow_mut().config.delete_profile(&name);
                ui.report(done);
                ui.rebuild_profiles();
                ui.schedule_flush();
            }
        });
        dialog.present(Some(&self.window));
    }

    /// Shows the saved state without sending anything.
    fn load_state(self: &Rc<Self>) {
        self.loading.set(true);
        let (mode, config) = {
            let session = self.session.borrow();
            (session.mode(), session.config.clone())
        };
        let settings = config.settings();
        let index = MODES
            .iter()
            .position(|item| item.key == mode.key)
            .unwrap_or(0);
        self.effects
            .select_row(self.effects.row_at_index(index as i32).as_ref());
        self.content_page.set_title(self.text.mode_name(mode));
        self.title.set_title(self.text.mode_name(mode));

        self.brightness.set_value(settings.brightness as f64);
        self.direction_row.set_visible(mode.has_direction);
        let right = settings.direction == Some(DIRECTION_RIGHT);
        self.direction
            .set_active_name(Some(if right { "right" } else { "left" }));
        self.color_group.set_visible(mode.color_regs.is_some());
        if mode.color_regs.is_some() {
            self.show_color(self.session.borrow().color());
        }

        for row in self.param_rows.borrow_mut().drain(..) {
            self.settings_group.remove(&row);
        }
        for param in mode.params {
            let value = settings
                .params
                .get(param.key)
                .copied()
                .unwrap_or(param.default);
            let row = self.param_row(param, value);
            self.settings_group.add(&row);
            self.param_rows.borrow_mut().push(row);
        }

        self.led_row.set_active(config.led);
        self.bypass_row.set_active(config.bypass);
        self.keep_row.set_active(config.keep_running);
        self.loading.set(false);
    }

    fn param_row(self: &Rc<Self>, param: &'static Param, value: i32) -> gtk::Widget {
        let label = self.text.param_label(param);
        let weak = Rc::downgrade(self);
        let change = move |value: i32| {
            if let Some(ui) = weak.upgrade() {
                if !ui.loading.get() {
                    ui.session.borrow_mut().set_param(param.key, value);
                    ui.schedule_flush();
                }
            }
        };
        if param.toggle {
            let row = adw::SwitchRow::builder()
                .title(label)
                .active(value != 0)
                .build();
            row.connect_active_notify(move |row| change(row.is_active() as i32));
            row.upcast()
        } else {
            let (row, scale) = slider_row(label, param.min as f64, param.max as f64);
            scale.connect_value_changed(move |scale| change(scale.value() as i32));
            scale.set_value(value as f64);
            // A value equal to the range start emits no change, so the label
            // would otherwise stay empty.
            scale.emit_by_name::<()>("value-changed", &[]);
            row.upcast()
        }
    }

    fn show_color(&self, color: Rgb) {
        let was_loading = self.loading.replace(true);
        self.wheel.set_color(color);
        let (r, g, b) = color;
        for (row, value) in self.channels.iter().zip([r, g, b]) {
            row.set_value(value as f64);
        }
        self.hex.set_text(&to_hex(color));
        self.hex.remove_css_class("error");
        self.loading.set(was_loading);
    }

    fn select_effect(self: &Rc<Self>, index: usize) {
        // GTK repeats this signal when the window maps, and a mode select would
        // restart the running effect.
        if self.session.borrow().config.mode == MODES[index].key {
            return;
        }
        let done = self.session.borrow_mut().select_mode(MODES[index].key);
        self.report(done);
        self.load_state();
        self.schedule_flush();
    }

    fn pick_color(self: &Rc<Self>, color: Rgb) {
        self.show_color(color);
        let done = self.session.borrow_mut().set_color(color);
        self.report(done);
        self.schedule_flush();
    }

    fn report(&self, result: Result<()>) {
        if let Err(error) = result {
            self.show_failure(&error.to_string());
        }
    }

    fn show_failure(&self, error: &str) {
        self.title.set_subtitle(self.text.disconnected);
        self.banner.set_title(error);
        self.banner.set_revealed(true);
    }

    fn schedule_flush(self: &Rc<Self>) {
        if let Some(timer) = self.flush_timer.take() {
            timer.remove();
        }
        let weak = Rc::downgrade(self);
        let timer = glib::timeout_add_local_once(DEBOUNCE, move || {
            if let Some(ui) = weak.upgrade() {
                ui.flush_timer.take();
                ui.flush();
            }
        });
        self.flush_timer.replace(Some(timer));
    }

    fn flush(&self) {
        let (frames, config) = {
            let mut session = self.session.borrow_mut();
            (session.take_frames(), session.config.clone())
        };
        if let Some(path) = &self.save_path {
            self.report(save_config(&config, path));
        }
        if !frames.is_empty() {
            self.worker.borrow().send(frames);
        }
        self.show_active_profile();
    }

    fn on_worker(&self, event: Event) {
        match event {
            Event::Connected(bus) => {
                self.banner.set_revealed(false);
                match bus {
                    Some(bus) => self
                        .title
                        .set_subtitle(&format!("{}{bus}", self.text.connected)),
                    None => self.title.set_subtitle(self.text.dry_run),
                }
            }
            Event::Failed(error) => self.show_failure(&error),
        }
    }

    fn on_tray(self: &Rc<Self>, event: TrayEvent) {
        match event {
            TrayEvent::Show => {
                self.hold.take();
                self.window.present();
            }
            TrayEvent::ToggleLighting => {
                let on = !self.session.borrow().config.led;
                self.session.borrow_mut().set_led(on);
                self.load_state();
                self.schedule_flush();
            }
            TrayEvent::Profile(name) => self.apply_profile(&name),
            TrayEvent::Quit => self.quit(),
        }
    }

    fn enable_tray(&self, available: bool) {
        self.keep_row.set_sensitive(available);
        if !available {
            self.keep_row.set_subtitle(self.text.keep_no_tray);
        }
    }

    /// Sends what is still queued, then closes the bus and the tray.
    fn shutdown(&self) {
        if let Some(timer) = self.flush_timer.take() {
            timer.remove();
        }
        self.flush();
        self.worker.borrow_mut().finish();
        if let Some(tray) = self.tray.take() {
            tray.shutdown();
        }
        self.hold.take();
    }

    fn quit(&self) {
        self.shutdown();
        self.app.quit();
    }
}

fn activate(app: &adw::Application, dry_run: bool, allow_unknown: bool) {
    if let Some(window) = app.active_window() {
        window.present();
        return;
    }
    load_css();
    let path = config_path();
    let session = Session::new(load_config(&path));
    let startup_config = session.config.clone();
    let (event_sender, event_receiver) = async_channel::unbounded();
    let worker = Worker::spawn(dry_run, allow_unknown, event_sender);
    let save_path = (!dry_run).then_some(path);
    let ui = Ui::build(app, session, save_path, worker);

    let (tray_sender, tray_receiver) = async_channel::unbounded();
    ui.show_active_profile();
    let handle = tray::spawn(ui.text, tray_sender, ui.tray_state.borrow().clone());
    ui.enable_tray(handle.is_some());
    ui.tray.replace(handle);

    let listener = ui.clone();
    glib::spawn_future_local(async move {
        while let Ok(event) = event_receiver.recv().await {
            listener.on_worker(event);
        }
    });
    let listener = ui.clone();
    glib::spawn_future_local(async move {
        while let Ok(event) = tray_receiver.recv().await {
            listener.on_tray(event);
        }
    });

    ui.worker.borrow().connect(startup_config);
    ui.window.present();
}

pub fn run(dry_run: bool, allow_unknown: bool) -> ExitCode {
    let flags = if dry_run {
        // A dry run must not hand off to a real running instance.
        gio::ApplicationFlags::NON_UNIQUE
    } else {
        gio::ApplicationFlags::empty()
    };
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(flags)
        .build();
    app.connect_activate(move |app| activate(app, dry_run, allow_unknown));
    glib::set_application_name(APP_NAME);
    let code = app.run_with_args::<&str>(&[]);
    ExitCode::from(code.get())
}
