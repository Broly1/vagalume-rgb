use std::process::ExitCode;

use clap::Parser;
use vagalume_rgb::cli::{self, Cli, Command};
use vagalume_rgb::{config, gui};

fn main() -> ExitCode {
    let args = Cli::parse();
    if matches!(args.command, None | Some(Command::Gui)) {
        return gui::run(args.dry_run, args.allow_unknown_board);
    }
    match cli::run(&args, &config::config_path(), std::path::Path::new("/sys")) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("vagalume-rgb: {error}");
            ExitCode::FAILURE
        }
    }
}
