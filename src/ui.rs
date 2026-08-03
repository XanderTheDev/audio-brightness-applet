use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{Align, Box as GtkBox, Button, DropDown, Label, Orientation, Scale, StringList};

use crate::audio::{AudioManager, Device};
use crate::brightness::{self, BacklightDevice};

/// One "Output"/"Input" row: a device dropdown, a volume slider, and a
/// mute toggle button, plus the bookkeeping needed to map the dropdown's
/// selected index back to a PulseAudio device and to avoid feedback loops
/// when we update widgets programmatically.
struct DeviceRow {
    dropdown: DropDown,
    scale: Scale,
    mute_button: Button,
    /// Devices currently shown in the dropdown, in display order.
    devices: Rc<RefCell<Vec<Device>>>,
}

fn build_device_row(
    title: &str,
    updating: Rc<Cell<bool>>,
    on_select: impl Fn(&Device) + 'static,
    on_volume: impl Fn(&Device, u8) + 'static,
    on_mute: impl Fn(&Device) + 'static,
) -> (GtkBox, DeviceRow) {
    let container = GtkBox::new(Orientation::Vertical, 6);
    container.add_css_class("abapplet-section");

    let label = Label::new(Some(title));
    label.set_halign(Align::Start);
    label.add_css_class("abapplet-heading");
    container.append(&label);

    let dropdown = DropDown::new(None::<StringList>, None::<gtk4::Expression>);
    dropdown.add_css_class("abapplet-dropdown");
    container.append(&dropdown);

    let volume_row = GtkBox::new(Orientation::Horizontal, 8);
    let scale = Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 1.0);
    scale.set_hexpand(true);
    scale.set_draw_value(true);
    scale.set_value_pos(gtk4::PositionType::Right);
    scale.add_css_class("abapplet-scale");

    let mute_button = Button::from_icon_name("audio-volume-muted-symbolic");
    mute_button.add_css_class("abapplet-mute-button");

    volume_row.append(&scale);
    volume_row.append(&mute_button);
    container.append(&volume_row);

    let devices: Rc<RefCell<Vec<Device>>> = Rc::new(RefCell::new(Vec::new()));

    // Dropdown selection -> set as default device.
    {
        let devices = devices.clone();
        let updating = updating.clone();
        dropdown.connect_selected_notify(move |dd| {
            if updating.get() {
                return;
            }
            let idx = dd.selected();
            if idx == gtk4::INVALID_LIST_POSITION {
                return;
            }
            if let Some(device) = devices.borrow().get(idx as usize) {
                on_select(device);
            }
        });
    }

    // Slider drag -> set volume on the currently selected device.
    {
        let devices = devices.clone();
        let dropdown_weak = dropdown.downgrade();
        let updating = updating.clone();
        scale.connect_value_changed(move |s| {
            if updating.get() {
                return;
            }
            let Some(dropdown) = dropdown_weak.upgrade() else {
                return;
            };
            let idx = dropdown.selected();
            if idx == gtk4::INVALID_LIST_POSITION {
                return;
            }
            if let Some(device) = devices.borrow().get(idx as usize) {
                on_volume(device, s.value().round() as u8);
            }
        });
    }

    // Mute button -> toggle mute on the currently selected device.
    {
        let devices = devices.clone();
        let dropdown_weak = dropdown.downgrade();
        mute_button.connect_clicked(move |_| {
            let Some(dropdown) = dropdown_weak.upgrade() else {
                return;
            };
            let idx = dropdown.selected();
            if idx == gtk4::INVALID_LIST_POSITION {
                return;
            }
            if let Some(device) = devices.borrow().get(idx as usize) {
                on_mute(device);
            }
        });
    }

    (
        container,
        DeviceRow {
            dropdown,
            scale,
            mute_button,
            devices,
        },
    )
}

/// Push a fresh device list + default selection + volume into a row's
/// widgets, without triggering the row's own change handlers.
fn sync_device_row(
    row: &DeviceRow,
    devices: &[Device],
    default_name: Option<&str>,
    updating: &Rc<Cell<bool>>,
) {
    updating.set(true);

    let labels: Vec<&str> = devices.iter().map(|d| d.description.as_str()).collect();
    let model = StringList::new(&labels);
    row.dropdown.set_model(Some(&model));

    let selected_idx = default_name
        .and_then(|name| devices.iter().position(|d| d.name == name))
        .unwrap_or(0);

    if !devices.is_empty() {
        row.dropdown.set_selected(selected_idx as u32);
        let current = &devices[selected_idx];
        row.scale.set_value(current.volume_percent as f64);
        let icon = if current.muted {
            "audio-volume-muted-symbolic"
        } else {
            "audio-volume-high-symbolic"
        };
        row.mute_button.set_icon_name(icon);
        row.scale.set_sensitive(true);
        row.mute_button.set_sensitive(true);
    } else {
        row.scale.set_sensitive(false);
        row.mute_button.set_sensitive(false);
    }

    *row.devices.borrow_mut() = devices.to_vec();

    updating.set(false);
}

/// Builds the full popup content: output row, input row, and (if a
/// backlight device was found) a brightness slider. Wires everything up to
/// the given `AudioManager` and returns the top-level widget to place in
/// the layer-shell window.
pub fn build_content(audio: Rc<AudioManager>) -> GtkBox {
    let root = GtkBox::new(Orientation::Vertical, 14);
    root.add_css_class("abapplet-root");
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);
    root.set_size_request(320, -1);

    let updating_output = Rc::new(Cell::new(false));
    let updating_input = Rc::new(Cell::new(false));

    let (output_box, output_row) = {
        let audio = audio.clone();
        let audio_vol = audio.clone();
        let audio_mute = audio.clone();
        build_device_row(
            "Audio Output",
            updating_output.clone(),
            move |device| audio.set_default_sink(&device.name),
            move |device, percent| {
                audio_vol.set_sink_volume(device.index, device.channel_count, percent)
            },
            move |device| audio_mute.set_sink_mute(device.index, !device.muted),
        )
    };
    root.append(&output_box);

    let (input_box, input_row) = {
        let audio = audio.clone();
        let audio_vol = audio.clone();
        let audio_mute = audio.clone();
        build_device_row(
            "Audio Input",
            updating_input.clone(),
            move |device| audio.set_default_source(&device.name),
            move |device, percent| {
                audio_vol.set_source_volume(device.index, device.channel_count, percent)
            },
            move |device| audio_mute.set_source_mute(device.index, !device.muted),
        )
    };
    root.append(&input_box);

    // --- Brightness ---
    if let Some(backlight) = brightness::detect_device() {
        let brightness_box = GtkBox::new(Orientation::Vertical, 6);
        brightness_box.add_css_class("abapplet-section");

        let label = Label::new(Some("Brightness"));
        label.set_halign(Align::Start);
        label.add_css_class("abapplet-heading");
        brightness_box.append(&label);

        let scale = Scale::with_range(Orientation::Horizontal, 1.0, 100.0, 1.0);
        scale.set_hexpand(true);
        scale.set_draw_value(true);
        scale.set_value_pos(gtk4::PositionType::Right);
        scale.add_css_class("abapplet-scale");

        if let Some(current) = brightness::get_percent(&backlight) {
            scale.set_value(current as f64);
        }

        let backlight_rc: Rc<BacklightDevice> = Rc::new(backlight);
        scale.connect_value_changed(move |s| {
            brightness::set_percent(&backlight_rc, s.value().round() as u8);
        });

        brightness_box.append(&scale);
        root.append(&brightness_box);
    } else {
        let warning = Label::new(Some(
            "No backlight device found (checked /sys/class/backlight via brightnessctl).",
        ));
        warning.set_wrap(true);
        warning.add_css_class("abapplet-warning");
        root.append(&warning);
    }

    // --- Wire audio state updates into the two device rows ---
    audio.set_on_update(move |state: &crate::audio::AudioState| {
        sync_device_row(
            &output_row,
            &state.sinks,
            state.default_sink.as_deref(),
            &updating_output,
        );
        sync_device_row(
            &input_row,
            &state.sources,
            state.default_source.as_deref(),
            &updating_input,
        );
    });

    root
}

/// Load the bundled stylesheet as the default CSS for the whole
/// application (touch-friendly sizing lives here).
pub fn load_css() {
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(include_str!("../style.css"));
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("no default display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
