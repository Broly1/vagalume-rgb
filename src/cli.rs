//! Command line interface.

use std::path::Path;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::color::{parse_color, to_hex};
use crate::config::{load_config, save_config, Config};
use crate::controller::{led_frame, restore_frames, send_all};
use crate::device::{find_device, open_transport, DryRunTransport, Transport, I2C_ADDRESS};
use crate::error::{Error, Result};
use crate::protocol::{bypass_frame, find_mode, Frame, DIRECTION_LEFT, DIRECTION_RIGHT, MODES};

#[derive(Parser)]
#[command(
    name = "vagalume-rgb",
    version,
    about = "ARGB lighting control for graphics cards"
)]
pub struct Cli {
    /// Print frames; touch no hardware
    #[arg(long, global = true)]
    pub dry_run: bool,

    /// Send even if this card is not a confirmed board
    #[arg(long, global = true)]
    pub allow_unknown_board: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Open the window (default)
    Gui,
    /// Show the detected device and saved settings; sends nothing
    Status,
    /// Send the saved settings to the card
    Restore,
    /// List effects and their settings
    List,
    /// Return every setting to default and apply
    Reset,
    /// Select an effect and adjust it
    Mode(ModeArgs),
    /// Turn the LEDs on or off
    Led { state: Switch },
    /// Let the motherboard ARGB header drive the LEDs
    Bypass { state: Switch },
    /// List, apply, save or delete profiles
    Profile {
        #[command(subcommand)]
        action: ProfileAction,
    },
}

#[derive(Subcommand)]
pub enum ProfileAction {
    /// List saved profiles
    List,
    /// Apply a profile to the card
    Apply { name: String },
    /// Save the current effect as a profile
    Save { name: String },
    /// Delete a profile
    Delete { name: String },
}

#[derive(Args)]
pub struct ModeArgs {
    #[arg(value_parser = clap::builder::PossibleValuesParser::new(MODES.iter().map(|m| m.key)))]
    name: String,
    /// 0-255
    #[arg(long)]
    brightness: Option<u8>,
    #[arg(long, value_enum)]
    direction: Option<Direction>,
    /// #RRGGBB
    #[arg(long)]
    color: Option<String>,
    /// An effect setting; see the list command
    #[arg(long = "set", value_name = "KEY=VALUE")]
    set: Vec<String>,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Switch {
    On,
    Off,
}

impl Switch {
    fn enabled(self) -> bool {
        matches!(self, Switch::On)
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Direction {
    Left,
    Right,
}

fn update_mode(config: &mut Config, args: &ModeArgs) -> Result<()> {
    let mode = find_mode(&args.name).expect("clap restricts this to known modes");
    let invalid = |text: String| Error::Invalid(text);
    config.mode = args.name.clone();
    let settings = config.settings_mut();
    if let Some(brightness) = args.brightness {
        settings.brightness = brightness;
    }
    if let Some(direction) = args.direction {
        if !mode.has_direction {
            return Err(invalid(format!("{} has no direction", mode.name)));
        }
        settings.direction = Some(match direction {
            Direction::Left => DIRECTION_LEFT,
            Direction::Right => DIRECTION_RIGHT,
        });
    }
    if let Some(color) = &args.color {
        if mode.color_regs.is_none() {
            return Err(invalid(format!("{} has no colour", mode.name)));
        }
        settings.color = Some(to_hex(parse_color(color)?));
    }
    for item in &args.set {
        let (key, value) = item.split_once('=').unwrap_or((item, ""));
        let param = mode.params.iter().find(|param| param.key == key);
        let (Some(param), Ok(value)) = (param, value.parse::<i32>()) else {
            let keys: Vec<_> = mode.params.iter().map(|param| param.key).collect();
            let keys = if keys.is_empty() {
                "none".to_string()
            } else {
                keys.join(", ")
            };
            return Err(invalid(format!(
                "{} settings are: {keys} (KEY=NUMBER)",
                mode.name
            )));
        };
        if !(param.min..=param.max).contains(&value) {
            return Err(invalid(format!(
                "{key} must be {}-{}",
                param.min, param.max
            )));
        }
        settings.params.insert(key.to_string(), value);
    }
    Ok(())
}

fn print_profiles(config: &Config) {
    let active = config.active_profile();
    for (name, profile) in &config.profiles {
        let mark = if active == Some(name.as_str()) {
            "*"
        } else {
            " "
        };
        println!(
            "{mark} {name}: {}",
            find_mode(&profile.mode).expect("validated").name
        );
    }
}

fn print_modes() {
    for mode in &MODES {
        println!("{}: {}", mode.key, mode.name);
        println!("    brightness 0-255");
        if mode.has_direction {
            println!("    direction");
        }
        if mode.color_regs.is_some() {
            println!("    color");
        }
        for param in mode.params {
            println!(
                "    {} {}-{} (default {})",
                param.key, param.min, param.max, param.default
            );
        }
    }
}

fn print_status(config: &Config, sysfs: &Path) {
    match find_device(sysfs) {
        Ok(info) => println!(
            "device: i2c-{} address {I2C_ADDRESS:#04x}, GPU {}, board {} ({})",
            info.bus,
            info.pci_address,
            info.board_text(),
            if info.supported() {
                "confirmed"
            } else {
                "NOT confirmed"
            },
        ),
        Err(error) => println!("{error}"),
    }
    let state = |on: bool| if on { "on" } else { "off" };
    let settings = config.settings();
    println!("mode: {}", find_mode(&config.mode).expect("validated").name);
    println!("brightness: {}", settings.brightness);
    if let Some(color) = &settings.color {
        println!("color: {color}");
    }
    for (key, value) in &settings.params {
        println!("{key}: {value}");
    }
    println!(
        "LED: {}, bypass: {}",
        state(config.led),
        state(config.bypass)
    );
}

fn save_unless_dry(args: &Cli, config: &Config, path: &Path) -> Result<()> {
    if args.dry_run {
        return Ok(());
    }
    save_config(config, path)
}

pub fn run(args: &Cli, config_path: &Path, sysfs: &Path) -> Result<()> {
    let mut config = load_config(config_path);
    let command = args
        .command
        .as_ref()
        .expect("the window handles no command");
    let frames: Vec<Frame> = match command {
        Command::Gui => unreachable!("handled before the command line runs"),
        Command::List => {
            print_modes();
            return Ok(());
        }
        Command::Status => {
            print_status(&config, sysfs);
            return Ok(());
        }
        Command::Restore => restore_frames(&config)?,
        Command::Reset => {
            config.reset_lighting();
            restore_frames(&config)?
        }
        Command::Mode(mode) => {
            update_mode(&mut config, mode)?;
            restore_frames(&config)?
        }
        Command::Led { state } => {
            config.led = state.enabled();
            vec![led_frame(&config)]
        }
        Command::Bypass { state } => {
            config.bypass = state.enabled();
            vec![bypass_frame(config.bypass)]
        }
        Command::Profile { action } => match action {
            ProfileAction::List => {
                print_profiles(&config);
                return Ok(());
            }
            ProfileAction::Apply { name } => {
                config.apply_profile(name)?;
                restore_frames(&config)?
            }
            ProfileAction::Save { name } => {
                config.save_profile(name)?;
                return save_unless_dry(args, &config, config_path);
            }
            ProfileAction::Delete { name } => {
                config.delete_profile(name)?;
                return save_unless_dry(args, &config, config_path);
            }
        },
    };

    let mut transport: Box<dyn Transport> = if args.dry_run {
        Box::new(DryRunTransport::new(true))
    } else {
        open_transport(args.allow_unknown_board, sysfs)?
    };
    send_all(transport.as_mut(), &frames)?;
    if !args.dry_run && !matches!(command, Command::Restore) {
        save_config(&config, config_path)?;
    }
    Ok(())
}
