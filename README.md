# Vagalume

ARGB lighting control for graphics cards on Linux, with a native window and a command line.
Unofficial, and not affiliated with any hardware vendor.

Confirmed board: `8086:207e:a002`. Other boards are refused unless you pass
`--allow-unknown-board`.

## Features

- Seven effects: Rainbow, Chase, Gradient, Sequential, Stacking, Breathing, Custom
- Colour wheel, RGB and hex entry, preset swatches
- Brightness, speed, direction and per-effect settings
- Lighting on/off, and Motherboard Control to let the board's ARGB header drive the LEDs
- Saved settings, restore at login, tray icon, English and Chinese
- Follows the desktop's light/dark setting and accent colour on KDE Plasma and GNOME

The tray needs a StatusNotifier host. KDE Plasma has one built in; on GNOME install the
AppIndicator extension. Without one, Keep Running in Tray is simply disabled.

## Build

Needs current stable Rust, GTK 4.12+, libadwaita 1.7+, and at runtime the `xe` driver and `i2c-dev` module.

```sh
sudo pacman -S rust gtk4 libadwaita        # Arch
sudo dnf install rust cargo gtk4-devel libadwaita-devel      # Fedora
sudo apt install rustc cargo libgtk-4-dev libadwaita-1-dev   # Debian, Ubuntu

cargo build --release
```

## Run

```sh
vagalume-rgb                               # window
vagalume-rgb status                        # detect the card; sends nothing
vagalume-rgb --dry-run mode breathing --color '#6d4aff'
vagalume-rgb mode custom --color '#0000ff'
vagalume-rgb mode chase --direction right --set response=8
vagalume-rgb led off
vagalume-rgb list                          # every effect's settings
```

`--dry-run` also works with the window: frames are printed and nothing is saved.

To install for your user, adding the menu entry and icon:

```sh
./install.sh            # add --restore to re-apply lighting at login
```

If you can't open `/dev/i2c-*`, install the udev rule:

```sh
sudo install -m644 packaging/70-vagalume-rgb.rules /etc/udev/rules.d/
```

## Flatpak

Build a single-file bundle (this needs `flatpak-builder`, `cargo` and the Flathub remote, and
downloads the GNOME SDK on first run), then install it:

```sh
./packaging/flatpak/build.sh
flatpak install --user --bundle flatpak-out/vagalume-rgb.flatpak
flatpak run org.vagalumergb.VagalumeRgb
```

The sandbox has `--device=all`, because `/dev/i2c-*` has no narrower permission. The `i2c-dev` module
and the udev rule below stay on the host; a Flatpak can't install either. Settings live in
`~/.var/app/org.vagalumergb.VagalumeRgb/`, separate from a native install.

To restore lighting at login, use `flatpak run org.vagalumergb.VagalumeRgb restore` as the
service's `ExecStart`.

## Safety

The controller at `0x28` is the card's management controller, not just an LED driver, so:

- Only allowlisted registers can be written.
- Only one transaction shape exists: write, then read one byte. A plain multi-byte read locked up
  this bus during development, so never probe it with `i2cdump` or similar.

Details are in [docs/PROTOCOL.md](docs/PROTOCOL.md).

## Tests

```sh
cargo test
```

These use no hardware.

## License

MIT. See [LICENSE](LICENSE).
