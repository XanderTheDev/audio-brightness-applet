//! Thin wrapper around libpulse-binding, integrated with the GLib main loop
//! that GTK is already running. Because the PulseAudio context is driven by
//! the same main loop as the UI, callbacks are dispatched back safely to
//! the GLib main loop.

use std::cell::RefCell;
use std::rc::Rc;

use libpulse_binding as pulse;
use libpulse_glib_binding as pulse_glib;

use pulse::callbacks::ListResult;
use pulse::context::introspect::{ServerInfo, SinkInfo, SourceInfo};
use pulse::context::subscribe::{Facility, InterestMaskSet};
use pulse::context::{Context, FlagSet as ContextFlagSet, State as ContextState};
use pulse::proplist::Proplist;
use pulse::volume::{ChannelVolumes, Volume};

#[derive(Clone, Debug)]
pub struct Device {
    pub index: u32,
    pub name: String,
    pub description: String,
    pub volume_percent: u8,
    pub muted: bool,
    pub channel_count: u8,
}

#[derive(Clone, Debug)]
pub struct AudioState {
    pub sinks: Vec<Device>,
    pub sources: Vec<Device>,
    pub default_sink: Option<String>,
    pub default_source: Option<String>,
}

impl AudioState {
    fn empty() -> Self {
        Self {
            sinks: Vec::new(),
            sources: Vec::new(),
            default_sink: None,
            default_source: None,
        }
    }
}

pub struct AudioManager {
    context: Rc<RefCell<Context>>,
    pub state: Rc<RefCell<AudioState>>,
    pub on_update: Rc<RefCell<Option<Rc<dyn Fn(&AudioState)>>>>,
}

fn percent_to_volume(percent: u8) -> Volume {
    let raw = (Volume::NORMAL.0 as f64) * (percent as f64 / 100.0);
    Volume(raw.round() as u32)
}

fn volume_to_percent(cv: &ChannelVolumes) -> u8 {
    let avg = cv.avg().0 as f64;
    let normal = Volume::NORMAL.0 as f64;
    ((avg / normal) * 100.0).round().clamp(0.0, 150.0) as u8
}

impl AudioManager {
    /// Connect to the PulseAudio server using the given GLib-backed main loop.
    pub fn new(glib_mainloop: &pulse_glib::Mainloop) -> Rc<Self> {
        let mut proplist = Proplist::new().expect("failed to create pulse proplist");
        proplist
            .set_str(
                pulse::proplist::properties::APPLICATION_NAME,
                "Audio & Brightness Applet",
            )
            .ok();

        let context = Context::new_with_proplist(glib_mainloop, "audio-brightness-applet", &proplist)
            .expect("failed to create pulse context");

        let manager = Rc::new(Self {
            context: Rc::new(RefCell::new(context)),
            state: Rc::new(RefCell::new(AudioState::empty())),
            on_update: Rc::new(RefCell::new(None)),
        });

        manager.clone().setup_state_callback();

        {
            let mut ctx = manager.context.borrow_mut();
            ctx.connect(None, ContextFlagSet::NOAUTOSPAWN, None)
                .expect("failed to connect to pulseaudio/pipewire-pulse");
        }

        let is_ready = manager.context.borrow().get_state() == ContextState::Ready;
        if is_ready {
            manager.clone().subscribe_to_events();
            manager.clone().refresh_all();
        }

        manager
    }

    /// Register a closure to run every time device/volume state changes.
    pub fn set_on_update<F>(&self, callback: F)
    where
        F: Fn(&AudioState) + 'static,
    {
        *self.on_update.borrow_mut() = Some(Rc::new(callback));
    }

    fn fire_update(&self) {
        let state_clone = (*self.state.borrow()).clone();

        if let Some(cb) = self.on_update.borrow().as_ref() {
            let cb = cb.clone();
            glib::idle_add_local(move || {
                cb(&state_clone);
                glib::ControlFlow::Break
            });
        }
    }

    fn setup_state_callback(self: Rc<Self>) {
        let me = self.clone();

        let mut ctx = self.context.borrow_mut();
        ctx.set_state_callback(Some(Box::new(move || {
            let state = me.context.try_borrow().map(|c| c.get_state());

            if let Ok(ContextState::Ready) = state {
                me.clone().subscribe_to_events();
                me.clone().refresh_all();
            } else if matches!(state, Ok(ContextState::Failed | ContextState::Terminated)) {
                eprintln!("pulseaudio/pipewire-pulse connection lost");
            }
        })));
    }

    fn subscribe_to_events(self: Rc<Self>) {
        let me = self.clone();

        {
            let mut ctx = self.context.borrow_mut();
            ctx.set_subscribe_callback(Some(Box::new(move |facility, _operation, _index| {
                if matches!(
                    facility,
                    Some(Facility::Sink) | Some(Facility::Source) | Some(Facility::Server)
                ) {
                    me.clone().refresh_all();
                }
            })));
        }

        self.context.borrow_mut().subscribe(
            InterestMaskSet::SINK | InterestMaskSet::SOURCE | InterestMaskSet::SERVER,
            |_success| {},
        );
    }

    fn refresh_all(self: Rc<Self>) {
        self.clone().refresh_server_info();
        self.clone().refresh_sinks();
        self.clone().refresh_sources();
    }

    fn refresh_server_info(self: Rc<Self>) {
        let me = self.clone();
        let context_rc = me.context.clone();
        context_rc
            .borrow()
            .introspect()
            .get_server_info(move |info: &ServerInfo| {
                let mut state = me.state.borrow_mut();
                state.default_sink = info.default_sink_name.as_ref().map(|c| c.to_string());
                state.default_source = info.default_source_name.as_ref().map(|c| c.to_string());
                drop(state);
                me.fire_update();
            });
    }

    fn refresh_sinks(self: Rc<Self>) {
        let me = self.clone();
        let pending_sinks = Rc::new(RefCell::new(Vec::new()));
        let context_rc = me.context.clone();

        context_rc
            .borrow()
            .introspect()
            .get_sink_info_list(move |result: ListResult<&SinkInfo>| match result {
                ListResult::Item(info) => {
                    let device = Device {
                        index: info.index,
                        name: info.name.as_ref().map(|c| c.to_string()).unwrap_or_default(),
                        description: info
                            .description
                            .as_ref()
                            .map(|c| c.to_string())
                            .unwrap_or_default(),
                        volume_percent: volume_to_percent(&info.volume),
                        muted: info.mute,
                        channel_count: info.volume.len() as u8,
                    };
                    pending_sinks.borrow_mut().push(device);
                }
                ListResult::End => {
                    me.state.borrow_mut().sinks = pending_sinks.borrow().clone();
                    me.fire_update();
                }
                ListResult::Error => {}
            });
    }

    fn refresh_sources(self: Rc<Self>) {
        let me = self.clone();
        let pending_sources = Rc::new(RefCell::new(Vec::new()));
        let context_rc = me.context.clone();

        context_rc
            .borrow()
            .introspect()
            .get_source_info_list(move |result: ListResult<&SourceInfo>| match result {
                ListResult::Item(info) => {
                    let name = info.name.as_ref().map(|c| c.to_string()).unwrap_or_default();
                    if name.contains(".monitor") {
                        return;
                    }
                    let device = Device {
                        index: info.index,
                        name,
                        description: info
                            .description
                            .as_ref()
                            .map(|c| c.to_string())
                            .unwrap_or_default(),
                        volume_percent: volume_to_percent(&info.volume),
                        muted: info.mute,
                        channel_count: info.volume.len() as u8,
                    };
                    pending_sources.borrow_mut().push(device);
                }
                ListResult::End => {
                    me.state.borrow_mut().sources = pending_sources.borrow().clone();
                    me.fire_update();
                }
                ListResult::Error => {}
            });
    }

    pub fn set_sink_volume(&self, index: u32, channel_count: u8, percent: u8) {
        let mut cv = ChannelVolumes::default();
        cv.set(channel_count.max(1), percent_to_volume(percent));
        self.context
            .borrow_mut()
            .introspect()
            .set_sink_volume_by_index(index, &cv, None);
    }

    pub fn set_source_volume(&self, index: u32, channel_count: u8, percent: u8) {
        let mut cv = ChannelVolumes::default();
        cv.set(channel_count.max(1), percent_to_volume(percent));
        self.context
            .borrow_mut()
            .introspect()
            .set_source_volume_by_index(index, &cv, None);
    }

    pub fn set_sink_mute(&self, index: u32, muted: bool) {
        self.context
            .borrow_mut()
            .introspect()
            .set_sink_mute_by_index(index, muted, None);
    }

    pub fn set_source_mute(&self, index: u32, muted: bool) {
        self.context
            .borrow_mut()
            .introspect()
            .set_source_mute_by_index(index, muted, None);
    }

    pub fn set_default_sink(&self, name: &str) {
        self.context.borrow_mut().set_default_sink(name, |_| {});
    }

    pub fn set_default_source(&self, name: &str) {
        self.context.borrow_mut().set_default_source(name, |_| {});
    }
}
