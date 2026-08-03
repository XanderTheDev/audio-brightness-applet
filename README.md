# audio-brightness-applet

A lightweight GTK4 layer-shell popup applet for quick audio input/output switching, volume control, and screen brightness management on Wayland compositors (Hyprland, Sway, Wayfire, River, etc.).

## Features

* **Audio Control**: Select default playback and recording devices (sinks and sources) powered by PipeWire / PulseAudio, adjust volume levels, and toggle mutes.
* **Brightness Control**: Automatically detects backlight devices via `brightnessctl`.
* **Wayland Native**: Built with `gtk4-layer-shell` to attach directly to your screen edges or bars like Waybar.
* **Customizable Placement**: Move and position the popup easily using command line flags.

## Installation

### Using Nix Flakes

Add `audio-brightness-applet` to your `flake.nix` inputs:

```nix
inputs = {
  audio-brightness-applet.url = "github:XanderTheDev/audio-brightness-applet";
};

```

Then add the package to your `environment.systemPackages` or `home.packages`:

```nix
environment.systemPackages = with pkgs; [
  inputs.audio-brightness-applet.packages.${pkgs.system}.default
];

```

## Usage

Run the applet from your terminal or tie it to a Waybar module / keybinding:

```bash
audio-brightness-applet

```

If the window is already open, running the command again closes it (toggle behavior). You can also press `Escape` to close the window.

### Command Line Flags

| Flag | Default | Description |
| --- | --- | --- |
| `--anchor` | `right` | Horizontal position (`left`, `right`, or `center`) |
| `--vertical-anchor` | `top` | Vertical position (`top` or `bottom`) |
| `--margin-top` | `42` | Vertical margin in pixels from top/bottom |
| `--margin-side` | `12` | Horizontal margin in pixels from left/right |
| `--animation` | `fade` | Entry animation style (`none`, `fade`, or `slide`) |
| `--slide-direction` | `auto` | Direction to slide in from (`auto`, `top`, `bottom`, `left`, or `right`) |

#### Examples

**Top Right (Default for Waybar):**

```bash
audio-brightness-applet --anchor right --vertical-anchor top --margin-top 12 --margin-side 12

```

**Bottom Left:**

```bash
audio-brightness-applet --anchor left --vertical-anchor bottom --margin-top 16 --margin-side 16

```

## License

This project is licensed under the BSD 4-Clause License - see the [LICENSE](LICENSE)
