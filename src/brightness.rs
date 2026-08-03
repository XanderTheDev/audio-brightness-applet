//! Brightness control via `brightnessctl`.
//!
//! We deliberately shell out to `brightnessctl` instead of poking
//! `/sys/class/backlight` directly: it already solves device autodetection
//! (backlight vs. leds, picking a sane default), and it ships its own udev
//! rule granting the `video` group write access, so we don't need to manage
//! permissions ourselves.
//!
//! `acpi_video1` (the generic ACPI backlight interface referenced in the
//! user's old waybar config) is frequently non-functional on modern
//! Intel/AMD graphics. The real control usually lives under a
//! vendor-specific name such as `intel_backlight` or `amdgpu_bl0`. Rather
//! than hardcoding one, we ask `brightnessctl --list` for every backlight
//! class device on the system and use the first one that reports a class
//! of `backlight` (falling back to any device at all if none matches).

use std::process::Command;

#[derive(Debug, Clone)]
pub struct BacklightDevice {
    pub name: String,
}

/// Autodetect a usable backlight device by asking brightnessctl to list
/// every device it knows about (machine-readable mode).
///
/// Output format per line: `device,class,current,percent%,max`
pub fn detect_device() -> Option<BacklightDevice> {
    let output = Command::new("brightnessctl")
        .args(["--list", "--machine-readable"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut fallback: Option<String> = None;

    for line in stdout.lines() {
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() < 2 {
            continue;
        }
        let name = fields[0].trim().to_string();
        let class = fields[1].trim();

        if fallback.is_none() {
            fallback = Some(name.clone());
        }

        if class == "backlight" {
            return Some(BacklightDevice { name });
        }
    }

    fallback.map(|name| BacklightDevice { name })
}

/// Current brightness as a 0-100 percentage, for the given device.
pub fn get_percent(device: &BacklightDevice) -> Option<u8> {
    let output = Command::new("brightnessctl")
        .args(["--device", &device.name, "--machine-readable", "info"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?;
    let fields: Vec<&str> = line.split(',').collect();
    let percent_field = fields.get(3)?;
    let percent_str = percent_field.trim().trim_end_matches('%');
    percent_str.parse::<u8>().ok()
}

/// Set brightness to an absolute percentage (0-100) on the given device.
pub fn set_percent(device: &BacklightDevice, percent: u8) {
    let percent = percent.clamp(0, 100);
    let _ = Command::new("brightnessctl")
        .args(["--device", &device.name, "set", &format!("{percent}%")])
        .output();
}
