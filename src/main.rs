use std::cell::RefCell;

use clap::{Parser, ValueEnum};
use gtk4::prelude::*;
use gtk4::{gio, glib, Application, ApplicationWindow};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use libpulse_glib_binding::Mainloop;

mod audio;
mod brightness;
mod ui;

thread_local! {
    static PULSE_MAINLOOP: RefCell<Option<Mainloop>> = const { RefCell::new(None) };
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationType {
    None,
    Fade,
    Slide,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlideDirection {
    Auto,
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Horizontal anchor position ("left", "right", or "center")
    #[arg(long, default_value = "right")]
    pub anchor: String,

    /// Vertical margin in pixels (distance from top/bottom bar)
    #[arg(long, default_value_t = 42)]
    pub margin_top: i32,

    /// Horizontal margin/offset in pixels
    #[arg(long, default_value_t = 12)]
    pub margin_side: i32,

    /// Vertical anchor position ("top" or "bottom")
    #[arg(long, default_value = "top")]
    pub vertical_anchor: String,

    /// Choose entry animation style ("none", "fade", or "slide")
    #[arg(long, value_enum, default_value_t = AnimationType::Fade)]
    pub animation: AnimationType,

    /// Direction to slide in from ("auto", "top", "bottom", "left", "right")
    #[arg(long, value_enum, default_value_t = SlideDirection::Auto)]
    pub slide_direction: SlideDirection,
}

fn main() -> glib::ExitCode {
    let args = Cli::parse();

    let app = Application::builder()
        .application_id("com.github.audio_brightness_applet")
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    let cli_args = args.clone();
    app.connect_command_line(move |app, _cmdline| {
        let windows = app.windows();
        if !windows.is_empty() {
            for win in windows {
                win.close();
            }
            return 0.into();
        }

        build_ui(app, &cli_args);
        0.into()
    });

    app.run()
}

fn build_ui(app: &Application, args: &Cli) {
    ui::load_css();

    let pulse_mainloop = Mainloop::new(None).expect("Failed to create PulseAudio GLib mainloop");
    let audio_manager = audio::AudioManager::new(&pulse_mainloop);

    PULSE_MAINLOOP.with(|m| {
        *m.borrow_mut() = Some(pulse_mainloop);
    });

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Audio & Brightness")
        .build();

    window.connect_destroy(|_| {
        PULSE_MAINLOOP.with(|m| {
            *m.borrow_mut() = None;
        });
    });

    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::OnDemand);

    // --- Vertical Placement ---
    let anchor_top = args.vertical_anchor.to_lowercase() != "bottom";
    window.set_anchor(Edge::Top, anchor_top);
    window.set_anchor(Edge::Bottom, !anchor_top);
    if anchor_top {
        window.set_margin(Edge::Top, args.margin_top);
    } else {
        window.set_margin(Edge::Bottom, args.margin_top);
    }

    // --- Horizontal Placement ---
    match args.anchor.to_lowercase().as_str() {
        "left" => {
            window.set_anchor(Edge::Left, true);
            window.set_anchor(Edge::Right, false);
            window.set_margin(Edge::Left, args.margin_side);
        }
        "center" | "middle" => {
            window.set_anchor(Edge::Left, false);
            window.set_anchor(Edge::Right, false);
        }
        _ => {
            window.set_anchor(Edge::Right, true);
            window.set_anchor(Edge::Left, false);
            window.set_margin(Edge::Right, args.margin_side);
        }
    }

    let (root_widget, revealer) = ui::build_content(
        audio_manager,
        args.animation,
        args.slide_direction,
        anchor_top,
    );
    window.set_child(Some(&root_widget));

    // Escape key listener to close
    let key_controller = gtk4::EventControllerKey::new();
    let window_weak_esc = window.downgrade();
    key_controller.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk4::gdk::Key::Escape {
            if let Some(win) = window_weak_esc.upgrade() {
                win.close();
            }
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(key_controller);

    window.present();

    if let Some(rev) = revealer {
        rev.set_reveal_child(true);
    }
}
