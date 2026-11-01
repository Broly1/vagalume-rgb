# LED controller protocol

The Custom effect is confirmed on a real ONIX Lumi B580 (`8086:207e:a002`): `mode custom --color '#0000ff'`
lit the LEDs blue. Chase also looks right, and Sequential was tried across its sliders (see below). The
other effects are built from the same protocol but have not been individually checked here.

Slider ranges, defaults and the Wire column come from static analysis of the vendor's app, not from
watching it run. Findings marked † come from the measurements in
[b580-onix-lumi-linux](https://github.com/Arthur-Vassoler/b580-onix-lumi-linux) by Arthur Vassoler,
on the same card. They were not repeated here. No code is taken from that project.

## Bus

| | |
|---|---|
| Device | `/dev/i2c-12` on the test system, client `12-0028` |
| Address | `0x28`, named `amc` by the `xe` driver |
| Adapter | `i2c_designware.2048`, on the GPU |

The bus number can change, so it is found through sysfs rather than assumed.

## Transaction shape

Every command is one combined I2C transaction: write the frame, repeated start, read 1 byte. The
returned byte is an echo of the last value written, not a status†, and is not used. A busy device is
retried after 50 ms and again after 100 ms.

**Do not issue plain multi-byte reads.** Observed on the real card: a 1-byte read worked, the
next 4- and 16-byte reads timed out, and from then on the bus controller stopped responding to
everything (`controller timed out`, `timeout in disabling i2c adapter`). The rest of the GPU
kept working. Reboot to recover; if that does not clear it, cut power for 30 seconds.

If the bus sticks, resetting the `i2c_designware` driver as root may clear it without a reboot, but
if the controller itself is holding the line only a power cycle helps†. This is untested here.

A frame is a run of `(register, value)` pairs. Several pairs may share one transaction, but never a
mode change: the card accepts that packet and the LEDs go dark†. Send a mode change alone, and leave
50 ms after every frame. That gap is known to work†; whether it is the minimum is not known.

## Registers

| Reg | Purpose | Values |
|---|---|---|
| `0x0F` | Bypass (motherboard ARGB drives the LEDs) | 0 off, 1 on |
| `0x10` | Mode | see below |
| `0x14` | Direction | 0 left, 1 right |
| `0x3E` | Brightness | 0-255; LED off writes 0, default `0x88` |
| `0x27` | Strip length† | Always `0x0E` (14 LEDs), at connect |

### Modes

Wire = what is sent for setting value `v`.

| `0x10` | Name | Setting | Reg | Range | Default | Wire |
|---|---|---|---|---|---|---|
| 0 | Rainbow | Response | `0x18` | 2-13 | 2 | `v` |
| | | Speed | `0x19` | 0-16 | 5 | `v` |
| 4 | Chase | Interval | `0x12` | 0-9 | 1 | `v` |
| | | Response | `0x11` | 2-13 | 10 | `15-v` |
| | | Both directions | `0x29` | 0-1 | 1 | `v` |
| 5 | Gradient | Response | `0x13` | 2-13 | 10 | `15-v` |
| 3 | Sequential | Response | `0x16` | 0-5 | 2 | `5-v` |
| | | Speed | `0x17` | 0-16 | 16 | `16-v` |
| 6 | Stacking | Speed | `0x20` | 2-13 | 10 | `15-v` |
| 2 | Breathing | Tempo | `0xC8` | 2-13 | 6 | `15-v` |
| | | Colour | `0xC9 0xCA 0xCB` | RGB | white | one frame |
| 1 | Custom | Colour | `0x1A 0x1B 0x1C` | RGB | white | one frame |

Rainbow's `0x18` sets how fast it moves, and writing 0 stops it. `0x19` sets how tightly the colours
are packed, not their speed, and the card accepted `0x18` up to 255†. The labels above are the
vendor's.

Sequential, on our card: Response 5 (wire 0) stops the animation. Speed 16, the default (wire 0),
shows a single green flashing fast, and Speed 0 (wire 16) flashes very fast. This may be how the
effect looks in the vendor's app too, but that has not been compared.

Every mode also has brightness, default `0x88`. Rainbow, Chase, Sequential and Stacking have
direction, default left.

## Sequences

Connect, once: `3E <0x88 or 0>  0F <bypass>  27 0E` in a single frame. This is needed before a mode
change takes effect if the card was left in bypass.

Selecting a mode sends one frame per item:

- Rainbow: direction, mode, brightness, response, speed
- Chase: mode, direction, brightness, interval, response, both directions
- Gradient: mode, brightness, response
- Sequential: mode, direction, brightness, response, speed
- Stacking: mode, direction, brightness, speed
- Breathing: mode, brightness, tempo, colour
- Custom: mode, brightness, colour

## Settings with no hardware command

Keep Running in Tray only makes the window hide on close. Nothing is sent to the card. Theme and
language are also app-only.

## Unknown

- What the `0xFE` from a bare read means.
- Whether settings survive a power cycle.

Colour order is red, green, blue†.

## Safety

This controller also runs the card's fans and voltage regulator†, and an unknown write could stop
the cooling. The allowlist exists because of this. Do not search for per-LED control by writing to
unknown registers: the known search found nothing†.
