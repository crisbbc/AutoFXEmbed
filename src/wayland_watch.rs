//! Event-driven Wayland selection watcher (Linux only).
//!
//! Binds `ext-data-control` (or `wlr-data-control` v2) and reports when the
//! CLIPBOARD or PRIMARY selection changes owner with a text MIME type offered.
//! No selection data is ever read here: offers are destroyed as soon as they
//! arrive, and the monitor loop reads the text itself when notified.

use std::collections::HashSet;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};

use wayland_client::{
    backend::ObjectId,
    event_created_child,
    globals::{registry_queue_init, GlobalListContents},
    protocol::{wl_callback::WlCallback, wl_registry::WlRegistry, wl_seat::WlSeat},
    Connection, Dispatch, Proxy, QueueHandle,
};
use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1::{self as ext_device, ExtDataControlDeviceV1},
    ext_data_control_manager_v1::ExtDataControlManagerV1,
    ext_data_control_offer_v1::{self as ext_offer, ExtDataControlOfferV1},
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1::{self as wlr_device, ZwlrDataControlDeviceV1},
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
    zwlr_data_control_offer_v1::{self as wlr_offer, ZwlrDataControlOfferV1},
};

/// Which selection changed owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Clipboard,
    Primary,
}

fn is_text_mime(mime: &str) -> bool {
    mime.starts_with("text/plain")
        || mime.eq_ignore_ascii_case("UTF8_STRING")
        || mime.eq_ignore_ascii_case("STRING")
        || mime.eq_ignore_ascii_case("TEXT")
}

struct State {
    on_change: Box<dyn Fn(Change) + Send>,
    /// Offers that advertised at least one plain-text MIME type before
    /// `selection` / `primary_selection` fired.
    text_offers: HashSet<ObjectId>,
}

impl State {
    fn emit(&self, change: Change) {
        (self.on_change)(change);
    }

    fn record_mime(&mut self, id: ObjectId, mime: &str) {
        if is_text_mime(mime) {
            // Bound memory in case a compositor emits orphaned offers.
            if self.text_offers.len() > 256 {
                self.text_offers.clear();
            }
            self.text_offers.insert(id);
        }
    }

    fn take_has_text(&mut self, id: &ObjectId) -> bool {
        self.text_offers.remove(id)
    }
}

pub struct WaylandWatcher {
    stop: Arc<AtomicBool>,
    conn: Connection,
    qh: QueueHandle<State>,
    thread: JoinHandle<()>,
}

impl WaylandWatcher {
    /// Connect and start watching. Calls `on_change` on each text selection
    /// change and `on_disconnect` if the Wayland connection closes unexpectedly.
    /// Returns `None` when there is no Wayland session or the compositor offers
    /// no data-control protocol.
    pub fn start(
        on_change: impl Fn(Change) + Send + 'static,
        on_disconnect: impl FnOnce() + Send + 'static,
    ) -> Option<Self> {
        std::env::var_os("WAYLAND_DISPLAY")?;
        let conn = Connection::connect_to_env().ok()?;
        let (globals, mut queue) = registry_queue_init::<State>(&conn).ok()?;
        let qh = queue.handle();

        let seat: WlSeat = globals.bind(&qh, 1..=1, ()).ok()?;
        // Keep the device proxy alive for the lifetime of the thread.
        let device =
            if let Ok(manager) = globals.bind::<ExtDataControlManagerV1, _, _>(&qh, 1..=1, ()) {
                Device::Ext(manager.get_data_device(&seat, &qh, ()))
            } else {
                // `primary_selection` events need wlr-data-control v2.
                let manager: ZwlrDataControlManagerV1 = globals.bind(&qh, 2..=2, ()).ok()?;
                Device::Wlr(manager.get_data_device(&seat, &qh, ()))
            };

        let mut state = State {
            on_change: Box::new(on_change),
            text_offers: HashSet::new(),
        };
        // Fail here, synchronously, if the compositor rejects the requests.
        queue.roundtrip(&mut state).ok()?;

        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            let _device = device;
            while !thread_stop.load(Ordering::Acquire) {
                if queue.blocking_dispatch(&mut state).is_err() {
                    if !thread_stop.load(Ordering::Acquire) {
                        on_disconnect();
                    }
                    break;
                }
            }
        });
        Some(Self {
            stop,
            conn,
            qh,
            thread,
        })
    }

    /// Wake the dispatch thread with a sync callback on its own queue, then join.
    pub fn shutdown(self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.conn.display().sync(&self.qh, ());
        let _ = self.conn.flush();
        let _ = self.thread.join();
    }

    /// Join a thread that already exited (e.g. the compositor went away).
    pub fn join(self) {
        let _ = self.thread.join();
    }
}

#[allow(dead_code)]
enum Device {
    Ext(ExtDataControlDeviceV1),
    Wlr(ZwlrDataControlDeviceV1),
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as wayland_client::Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

macro_rules! noop_dispatch {
    ($($iface:ty),*) => {
        $(
            impl Dispatch<$iface, ()> for State {
                fn event(
                    _: &mut Self,
                    _: &$iface,
                    _: <$iface as wayland_client::Proxy>::Event,
                    _: &(),
                    _: &Connection,
                    _: &QueueHandle<Self>,
                ) {
                }
            }
        )*
    };
}

noop_dispatch!(
    WlSeat,
    WlCallback,
    ExtDataControlManagerV1,
    ZwlrDataControlManagerV1
);

impl Dispatch<ExtDataControlOfferV1, ()> for State {
    fn event(
        state: &mut Self,
        offer: &ExtDataControlOfferV1,
        event: ext_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_offer::Event::Offer { mime_type } = event {
            state.record_mime(offer.id(), &mime_type);
        }
    }
}

impl Dispatch<ZwlrDataControlOfferV1, ()> for State {
    fn event(
        state: &mut Self,
        offer: &ZwlrDataControlOfferV1,
        event: wlr_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wlr_offer::Event::Offer { mime_type } = event {
            state.record_mime(offer.id(), &mime_type);
        }
    }
}

impl Dispatch<ExtDataControlDeviceV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ExtDataControlDeviceV1,
        event: ext_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_device::Event::Selection { id: Some(offer) } => {
                let has_text = state.take_has_text(&offer.id());
                offer.destroy();
                if has_text {
                    state.emit(Change::Clipboard);
                }
            }
            ext_device::Event::PrimarySelection { id: Some(offer) } => {
                let has_text = state.take_has_text(&offer.id());
                offer.destroy();
                if has_text {
                    state.emit(Change::Primary);
                }
            }
            _ => {}
        }
    }

    event_created_child!(State, ExtDataControlDeviceV1, [
        ext_device::EVT_DATA_OFFER_OPCODE => (ExtDataControlOfferV1, ()),
    ]);
}

impl Dispatch<ZwlrDataControlDeviceV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ZwlrDataControlDeviceV1,
        event: wlr_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wlr_device::Event::Selection { id: Some(offer) } => {
                let has_text = state.take_has_text(&offer.id());
                offer.destroy();
                if has_text {
                    state.emit(Change::Clipboard);
                }
            }
            wlr_device::Event::PrimarySelection { id: Some(offer) } => {
                let has_text = state.take_has_text(&offer.id());
                offer.destroy();
                if has_text {
                    state.emit(Change::Primary);
                }
            }
            _ => {}
        }
    }

    event_created_child!(State, ZwlrDataControlDeviceV1, [
        wlr_device::EVT_DATA_OFFER_OPCODE => (ZwlrDataControlOfferV1, ()),
    ]);
}
