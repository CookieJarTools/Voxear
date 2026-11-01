//! Files dragged from the desktop onto the editor's window on Wayland, which
//! winit does not pass on there (it does on X11, Windows and macOS). The
//! editor binds a data device for each seat on winit's own connection, in an
//! event queue of its own, takes drags that carry files (`text/uri-list`),
//! and tells the window where a drag moves and what was dropped, so the
//! window can hand them to the editor as the pointer moving and files
//! dropped. A drop's list of files is read on a thread of its own, as the
//! program it came from writes it.

use std::{
    ffi::OsString,
    io::Read,
    os::unix::ffi::OsStringExt,
    path::PathBuf,
    sync::mpsc::{self, Receiver, TryRecvError},
};

use anyhow::{Context, Result};
use raw_window_handle::RawDisplayHandle;
use smithay_client_toolkit::{
    data_device_manager::{
        DataDeviceManagerState, WritePipe,
        data_device::{DataDevice, DataDeviceHandler},
        data_offer::{DataOfferHandler, DragOffer},
        data_source::DataSourceHandler,
    },
    delegate_data_device, delegate_registry, delegate_seat,
    reexports::client::{
        Connection, EventQueue, QueueHandle,
        backend::Backend,
        globals::registry_queue_init,
        protocol::{
            wl_data_device::WlDataDevice, wl_data_device_manager::DndAction, wl_data_source::WlDataSource,
            wl_seat::WlSeat, wl_surface::WlSurface,
        },
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{Capability, SeatHandler, SeatState},
};

/// What a file manager offers a dragged file as.
const URI_LIST: &str = "text/uri-list";

/// What a drag of files over the window did since the last frame.
#[derive(Debug, PartialEq)]
pub enum Drag {
    /// It is over the window here, in logical pixels from its top left.
    Over(f64, f64),
    /// These files were dropped where it was last over.
    Dropped(Vec<PathBuf>),
}

/// Drags of files onto the window, heard on its Wayland connection.
pub struct WaylandDrops {
    queue: EventQueue<State>,
    state: State,
}

struct State {
    registry: RegistryState,
    seats: SeatState,
    manager: DataDeviceManagerState,
    devices: Vec<DataDevice>,
    drags: Vec<Drag>,
    /// Dropped offers whose list of files is being read, and where it comes.
    reading: Vec<(DragOffer, Receiver<Vec<PathBuf>>)>,
}

impl WaylandDrops {
    /// Listens for drags of files on the window's Wayland connection, or
    /// gives none when the window is not on Wayland.
    pub fn new(display: RawDisplayHandle) -> Result<Option<Self>> {
        let RawDisplayHandle::Wayland(display) = display else { return Ok(None) };
        // The window's own connection, which winit keeps open until the window
        // closes; the window drops this before then.
        let backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
        let connection = Connection::from_backend(backend);
        let (globals, queue) = registry_queue_init::<State>(&connection).context("reading the Wayland globals")?;
        let handle = queue.handle();
        let manager = DataDeviceManagerState::bind(&globals, &handle).context("finding the data device manager")?;
        let seats = SeatState::new(&globals, &handle);
        let devices = seats.seats().map(|seat| manager.get_data_device(&handle, &seat)).collect();
        let registry = RegistryState::new(&globals);
        let state = State { registry, seats, manager, devices, drags: Vec::new(), reading: Vec::new() };
        Ok(Some(Self { queue, state }))
    }

    /// What drags did since the last call. Call it once a frame.
    pub fn poll(&mut self) -> Vec<Drag> {
        if let Err(error) = self.queue.dispatch_pending(&mut self.state) {
            tracing::warn!("could not hear files dragged onto the window: {error}");
        }
        let State { reading, drags, .. } = &mut self.state;
        reading.retain(|(offer, read)| {
            let done = match read.try_recv() {
                Ok(paths) => {
                    drags.push(Drag::Dropped(paths));
                    true
                }
                Err(TryRecvError::Empty) => false,
                Err(TryRecvError::Disconnected) => true,
            };
            if done {
                offer.finish();
                offer.destroy();
            }
            !done
        });
        if let Err(error) = self.queue.flush() {
            tracing::warn!("could not answer a drag: {error}");
        }
        std::mem::take(&mut self.state.drags)
    }
}

impl State {
    /// The drag over the window through `device`, when it carries files.
    fn files(&self, device: &WlDataDevice) -> Option<DragOffer> {
        let device = self.devices.iter().find(|d| d.inner() == device)?;
        let offer = device.data().drag_offer()?;
        offer.with_mime_types(|types| types.iter().any(|t| t == URI_LIST)).then_some(offer)
    }
}

impl DataDeviceHandler for State {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice, x: f64, y: f64, _: &WlSurface) {
        let Some(offer) = self.files(device) else { return };
        offer.accept_mime_type(offer.serial, Some(URI_LIST.to_string()));
        offer.set_actions(DndAction::Copy, DndAction::Copy);
        self.drags.push(Drag::Over(x, y));
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice, x: f64, y: f64) {
        if self.files(device).is_some() {
            self.drags.push(Drag::Over(x, y));
        }
    }

    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

    fn drop_performed(&mut self, connection: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice) {
        let Some(offer) = self.files(device) else { return };
        let mut pipe = match offer.receive(URI_LIST.to_string()) {
            Ok(pipe) => pipe,
            Err(error) => return tracing::warn!("could not read the files dropped: {error}"),
        };
        // The program the drag came from writes the list once it hears the request.
        let _ = connection.flush();
        let (send, read) = mpsc::channel();
        let reader = std::thread::Builder::new().name("dropped files".into()).spawn(move || {
            let mut list = Vec::new();
            if pipe.read_to_end(&mut list).is_ok() {
                let _ = send.send(paths(&String::from_utf8_lossy(&list)));
            }
        });
        match reader {
            Ok(_) => self.reading.push((offer, read)),
            Err(error) => tracing::warn!("could not read the files dropped: {error}"),
        }
    }
}

impl DataOfferHandler for State {
    fn source_actions(&mut self, _: &Connection, _: &QueueHandle<Self>, offer: &mut DragOffer, _: DndAction) {
        offer.set_actions(DndAction::Copy, DndAction::Copy);
    }

    fn selected_action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}
}

/// The editor drags nothing out of its window, so its sources hear nothing.
impl DataSourceHandler for State {
    fn accept_mime(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: Option<String>) {}

    fn send_request(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: String, _: WritePipe) {}

    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }

    fn new_seat(&mut self, _: &Connection, handle: &QueueHandle<Self>, seat: WlSeat) {
        if !self.devices.iter().any(|device| device.data().seat() == &seat) {
            self.devices.push(self.manager.get_data_device(handle, &seat));
        }
    }

    fn new_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat, _: Capability) {}

    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat, _: Capability) {}

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, seat: WlSeat) {
        self.devices.retain(|device| device.data().seat() != &seat);
    }
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    registry_handlers![SeatState];
}

delegate_registry!(State);
delegate_seat!(State);
delegate_data_device!(State);

/// The files in a `text/uri-list`: one URI a line, `#` lines being
/// comments, and only `file:` URIs naming files.
fn paths(list: &str) -> Vec<PathBuf> {
    let lines = list.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#'));
    lines
        .filter_map(|uri| uri.strip_prefix("file://"))
        // After `file://` comes the host, often none, then the path.
        .filter_map(|rest| rest.find('/').map(|slash| &rest[slash..]))
        .map(|path| PathBuf::from(OsString::from_vec(unescape(path))))
        .collect()
}

/// A URI's `%20`-style escapes as the bytes they stand for.
fn unescape(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = bytes.get(at + 1..at + 3).and_then(|h| std::str::from_utf8(h).ok());
        match (bytes[at], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(byte)) => {
                out.push(byte);
                at += 3;
            }
            (byte, _) => {
                out.push(byte);
                at += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_uri_list_names_the_files_dropped() {
        let list = "# from Files\r\nfile:///home/jo/My%20Models/Tree.glb\r\nfile://laptop/tmp/Rock.png\r\n\
                    https://example.com/not-a-file.png\r\nfile:///tmp/caf%C3%A9.png\r\n";
        let found = paths(list);
        assert_eq!(
            found,
            [
                PathBuf::from("/home/jo/My Models/Tree.glb"),
                PathBuf::from("/tmp/Rock.png"),
                PathBuf::from("/tmp/café.png")
            ]
        );
        assert_eq!(unescape("100%"), b"100%", "a lone percent stays as it is");
    }
}
