//! Files dragged from the desktop onto the editor's windows on Wayland, which
//! winit does not pass on there (it does on X11, Windows and macOS). The
//! editor binds a data device for each seat on winit's own connection, in an
//! event queue of its own, takes drags that carry files (`text/uri-list`),
//! and tells the windows where a drag moves and what was dropped, so they
//! can hand them to the editor as the pointer moving and files dropped. A
//! drop's list of files is read on a thread of its own, as the program it
//! came from writes it.
//!
//! The same drags carry a panel's tab from one of the editor's windows to
//! another, as Wayland never says where windows are: a tab dragged out of
//! its window becomes the desktop's drag and drop (`drag_panel`), of a type
//! of the editor's own (`application/x-voxear-panel`), which each window it
//! passes over hears of, from its own top left. Starting one needs the press
//! the drag began with, so the editor also listens to the pointer on this
//! connection, for the presses' serials.

use std::{
    ffi::{OsString, c_void},
    io::Read,
    os::unix::ffi::OsStringExt,
    path::PathBuf,
    ptr::NonNull,
    sync::mpsc::{self, Receiver, TryRecvError},
};

use anyhow::{Context, Result};
use raw_window_handle::RawDisplayHandle;
use smithay_client_toolkit::{
    data_device_manager::{
        DataDeviceManagerState, WritePipe,
        data_device::{DataDevice, DataDeviceHandler},
        data_offer::{DataOfferHandler, DragOffer},
        data_source::{DataSourceHandler, DragSource},
    },
    delegate_data_device, delegate_pointer, delegate_registry, delegate_seat,
    reexports::client::{
        Connection, EventQueue, Proxy, QueueHandle,
        backend::Backend,
        globals::registry_queue_init,
        protocol::{
            wl_data_device::WlDataDevice, wl_data_device_manager::DndAction, wl_data_source::WlDataSource,
            wl_pointer::WlPointer, wl_seat::WlSeat, wl_surface::WlSurface,
        },
    },
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        pointer::{PointerData, PointerEvent, PointerEventKind, PointerHandler},
    },
};

/// What a file manager offers a dragged file as.
const URI_LIST: &str = "text/uri-list";
/// What a panel's tab dragged between the editor's windows is offered as.
const PANEL: &str = "application/x-voxear-panel";

/// A window's surface, as its handle names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Surface(pub usize);

impl Surface {
    fn of(surface: &WlSurface) -> Self {
        Self(surface.id().as_ptr() as usize)
    }
}

/// What a drag over the windows did since the last frame.
#[derive(Debug, PartialEq)]
pub enum Drag {
    /// Files dragged over a window, here, in logical pixels from its top left.
    Over(Surface, f64, f64),
    /// These files were dropped where they were last over.
    Dropped(Vec<PathBuf>),
    /// A panel's tab dragged out of its window ([`WaylandDrops::drag_panel`])
    /// is over a window, here.
    PanelOver(Surface, f64, f64),
    /// It went off every window of the editor's.
    PanelAway,
    /// It was let go over the window it was last over.
    PanelDropped,
    /// It was let go anywhere else, or the drag was given up.
    PanelCancelled,
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
    /// The pointers heard, the button held on a window with its press's
    /// serial and seat, and the drag of a panel's tab under way.
    pointers: Vec<WlPointer>,
    pressed: Option<(WlSurface, u32, WlSeat)>,
    panel: Option<DragSource>,
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
        let state = State {
            registry,
            seats,
            manager,
            devices,
            drags: Vec::new(),
            reading: Vec::new(),
            pointers: Vec::new(),
            pressed: None,
            panel: None,
        };
        Ok(Some(Self { queue, state }))
    }

    /// Makes the press held on the window whose surface is `surface` (from
    /// its handle) the desktop's drag and drop of a panel's tab, which every
    /// window it passes over then hears of. False when no button is held
    /// there, or one is under way already.
    pub fn drag_panel(&mut self, surface: NonNull<c_void>) -> bool {
        // Heard since the last frame, the press may not have come yet.
        let _ = self.queue.dispatch_pending(&mut self.state);
        let State { pressed, manager, devices, panel, .. } = &mut self.state;
        let held = pressed.as_ref().filter(|(on, ..)| Surface::of(on) == Surface(surface.as_ptr() as usize));
        let Some((origin, serial, seat)) = held else { return false };
        let Some(device) = devices.iter().find(|device| device.data().seat() == seat) else { return false };
        if panel.is_some() {
            return false;
        }
        let source = manager.create_drag_and_drop_source(&self.queue.handle(), [PANEL], DndAction::Move);
        source.start_drag(device, origin, None, *serial);
        *panel = Some(source);
        if let Err(error) = self.queue.flush() {
            tracing::warn!("could not start dragging the tab: {error}");
        }
        true
    }

    /// A surface of a window, from its handle, to compare with a drag's.
    pub fn surface(surface: NonNull<c_void>) -> Surface {
        Surface(surface.as_ptr() as usize)
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
    /// The drag over a window through `device`, when it carries `kind`.
    fn offered(&self, device: &WlDataDevice, kind: &str) -> Option<DragOffer> {
        let device = self.devices.iter().find(|d| d.inner() == device)?;
        let offer = device.data().drag_offer()?;
        offer.with_mime_types(|types| types.iter().any(|t| t == kind)).then_some(offer)
    }

    fn files(&self, device: &WlDataDevice) -> Option<DragOffer> {
        self.offered(device, URI_LIST)
    }

    /// The drag over a window through `device`, when it is a panel's tab.
    fn panel(&self, device: &WlDataDevice) -> Option<DragOffer> {
        self.offered(device, PANEL)
    }
}

impl DataDeviceHandler for State {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        device: &WlDataDevice,
        x: f64,
        y: f64,
        surface: &WlSurface,
    ) {
        let (kind, action, drag) = if let Some(offer) = self.panel(device) {
            (offer, PANEL, Drag::PanelOver(Surface::of(surface), x, y))
        } else if let Some(offer) = self.files(device) {
            (offer, URI_LIST, Drag::Over(Surface::of(surface), x, y))
        } else {
            return;
        };
        kind.accept_mime_type(kind.serial, Some(action.to_string()));
        let wanted = if action == PANEL { DndAction::Move } else { DndAction::Copy };
        kind.set_actions(wanted, wanted);
        self.drags.push(drag);
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice) {
        if self.panel(device).is_some() {
            self.drags.push(Drag::PanelAway);
        }
    }

    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice, x: f64, y: f64) {
        let Some(offer) = self.panel(device).or_else(|| self.files(device)) else { return };
        let surface = Surface::of(&offer.surface);
        let panel = offer.with_mime_types(|types| types.iter().any(|t| t == PANEL));
        self.drags.push(if panel { Drag::PanelOver(surface, x, y) } else { Drag::Over(surface, x, y) });
    }

    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

    fn drop_performed(&mut self, connection: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice) {
        if let Some(offer) = self.panel(device) {
            // The editor knows which tab it is; nothing needs reading.
            offer.finish();
            offer.destroy();
            self.drags.push(Drag::PanelDropped);
            return;
        }
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

/// A panel's tab is the only thing the editor drags out, and it carries
/// nothing to read: the drop's window knows which tab it is.
impl DataSourceHandler for State {
    fn accept_mime(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: Option<String>) {}

    fn send_request(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: String, _: WritePipe) {}

    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        if self.panel.take_if(|panel| panel.inner() == source).is_some() {
            self.pressed = None;
            self.drags.push(Drag::PanelCancelled);
        }
    }

    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        if self.panel.take_if(|panel| panel.inner() == source).is_some() {
            self.pressed = None;
        }
    }

    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
}

/// Only the presses matter: their serials start a panel's drag.
impl PointerHandler for State {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, pointer: &WlPointer, events: &[PointerEvent]) {
        let Some(seat) = pointer.data::<PointerData>().map(|data| data.seat().clone()) else { return };
        for event in events {
            match event.kind {
                PointerEventKind::Press { serial, .. } => {
                    self.pressed = Some((event.surface.clone(), serial, seat.clone()));
                }
                PointerEventKind::Release { .. } if self.panel.is_none() => self.pressed = None,
                _ => {}
            }
        }
    }
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

    fn new_capability(&mut self, _: &Connection, handle: &QueueHandle<Self>, seat: WlSeat, capability: Capability) {
        if capability == Capability::Pointer {
            match self.seats.get_pointer(handle, &seat) {
                Ok(pointer) => self.pointers.push(pointer),
                Err(error) => tracing::debug!("tabs cannot be dragged between windows: {error}"),
            }
        }
    }

    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat, capability: Capability) {
        if capability == Capability::Pointer {
            for pointer in self.pointers.drain(..) {
                if pointer.version() >= 3 {
                    pointer.release();
                }
            }
        }
    }

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
delegate_pointer!(State);

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
