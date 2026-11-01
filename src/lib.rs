//! ARGB lighting control for graphics cards on Linux.

pub mod cli;
pub mod color;
pub mod config;
pub mod controller;
pub mod device;
pub mod error;
pub mod gui;
pub mod i18n;
pub mod protocol;
pub mod session;
pub mod tray;

pub const APP_ID: &str = "org.vagalumergb.VagalumeRgb";
pub const APP_NAME: &str = "Vagalume";
