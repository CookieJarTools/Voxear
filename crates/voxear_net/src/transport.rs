//! Where packets go: a [`Transport`] sends datagrams and gives back those
//! that came. [`UdpTransport`] is a real socket; [`MemoryNetwork`] joins
//! ends in one process, for tests. A relay service would be another.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};

/// The largest datagram anything sends.
pub const MAX_DATAGRAM: usize = 1200;

pub trait Transport: Send + Sync {
    /// Queues a datagram to `to`; it goes by the next [`Transport::flush`].
    fn send(&mut self, to: SocketAddr, packet: &[u8]);
    /// The next datagram that came, into `buffer`: its length and sender.
    fn receive(&mut self, buffer: &mut [u8]) -> Option<(usize, SocketAddr)>;
    /// Sends what is queued.
    fn flush(&mut self) {}
    /// Where it can be reached.
    fn local_addr(&self) -> SocketAddr;
    /// The time now, in seconds, for transports that hold packets back.
    fn update(&mut self, _now: f64) {}
}

/// A boxed transport, so one chosen while running can be wrapped, as in a
/// [`Simulated`](crate::Simulated) network.
impl Transport for Box<dyn Transport> {
    fn send(&mut self, to: SocketAddr, packet: &[u8]) {
        (**self).send(to, packet);
    }

    fn receive(&mut self, buffer: &mut [u8]) -> Option<(usize, SocketAddr)> {
        (**self).receive(buffer)
    }

    fn flush(&mut self) {
        (**self).flush();
    }

    fn local_addr(&self) -> SocketAddr {
        (**self).local_addr()
    }

    fn update(&mut self, now: f64) {
        (**self).update(now);
    }
}

/// A UDP socket that never blocks, with large buffers so a burst of packets
/// is not dropped by the system. On Linux, a flush sends everything queued
/// in one system call, and receiving takes up to 64 datagrams in one.
pub struct UdpTransport {
    socket: UdpSocket,
    queue: Vec<(SocketAddr, Vec<u8>)>,
    #[cfg(target_os = "linux")]
    batch: linux::Batch,
}

/// The size asked for each of a socket's buffers; the system may give less.
const SOCKET_BUFFER: usize = 4 * 1024 * 1024;

impl UdpTransport {
    /// A socket bound to `address`; port 0 lets the system choose one.
    pub fn bind(address: SocketAddr) -> io::Result<Self> {
        use socket2::{Domain, Protocol, Socket, Type};
        let socket = Socket::new(Domain::for_address(address), Type::DGRAM, Some(Protocol::UDP))?;
        if address.is_ipv6() {
            // Both kinds of address on one socket, where the system allows.
            let _ = socket.set_only_v6(false);
        }
        let _ = socket.set_recv_buffer_size(SOCKET_BUFFER);
        let _ = socket.set_send_buffer_size(SOCKET_BUFFER);
        socket.set_nonblocking(true)?;
        socket.bind(&address.into())?;
        Ok(Self {
            socket: socket.into(),
            queue: Vec::new(),
            #[cfg(target_os = "linux")]
            batch: linux::Batch::default(),
        })
    }
}

impl Transport for UdpTransport {
    fn send(&mut self, to: SocketAddr, packet: &[u8]) {
        self.queue.push((to, packet.to_vec()));
    }

    fn receive(&mut self, buffer: &mut [u8]) -> Option<(usize, SocketAddr)> {
        #[cfg(target_os = "linux")]
        {
            self.batch.receive(&self.socket, buffer)
        }
        #[cfg(not(target_os = "linux"))]
        {
            loop {
                match self.socket.recv_from(buffer) {
                    Ok(got) => return Some(got),
                    // A port the last send went to was closed; carry on.
                    Err(e) if e.kind() == io::ErrorKind::ConnectionReset => continue,
                    Err(_) => return None,
                }
            }
        }
    }

    fn flush(&mut self) {
        #[cfg(target_os = "linux")]
        linux::send_all(&self.socket, &self.queue);
        #[cfg(not(target_os = "linux"))]
        for (to, packet) in &self.queue {
            let _ = self.socket.send_to(packet, to);
        }
        self.queue.clear();
    }

    fn local_addr(&self) -> SocketAddr {
        self.socket.local_addr().expect("a bound socket has an address")
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::net::{SocketAddr, UdpSocket};
    use std::os::fd::AsRawFd;

    use socket2::SockAddr;

    const BATCH: usize = 64;

    /// Sends every queued datagram with as few `sendmmsg` calls as it takes.
    pub fn send_all(socket: &UdpSocket, queue: &[(SocketAddr, Vec<u8>)]) {
        for chunk in queue.chunks(BATCH) {
            let addresses: Vec<SockAddr> = chunk.iter().map(|(to, _)| SockAddr::from(*to)).collect();
            let mut iovecs: Vec<libc::iovec> = chunk
                .iter()
                .map(|(_, packet)| libc::iovec { iov_base: packet.as_ptr() as *mut _, iov_len: packet.len() })
                .collect();
            let mut headers: Vec<libc::mmsghdr> = (0..chunk.len())
                .map(|i| {
                    // SAFETY: an all-zero msghdr is a valid empty one.
                    let mut header: libc::msghdr = unsafe { std::mem::zeroed() };
                    header.msg_name = addresses[i].as_ptr() as *mut _;
                    header.msg_namelen = addresses[i].len();
                    header.msg_iov = &mut iovecs[i];
                    header.msg_iovlen = 1;
                    libc::mmsghdr { msg_hdr: header, msg_len: 0 }
                })
                .collect();
            let mut sent = 0;
            while sent < headers.len() {
                // SAFETY: every header points at an address and buffer that outlive the call.
                let n = unsafe {
                    libc::sendmmsg(socket.as_raw_fd(), headers[sent..].as_mut_ptr(), (headers.len() - sent) as u32, 0)
                };
                if n <= 0 {
                    // The rest go one at a time, so one bad address does not lose the others.
                    for (to, packet) in &chunk[sent + 1..] {
                        let _ = socket.send_to(packet, to);
                    }
                    break;
                }
                sent += n as usize;
            }
        }
    }

    /// Datagrams taken in one `recvmmsg` call, handed out one at a time.
    pub struct Batch {
        buffers: Vec<[u8; super::MAX_DATAGRAM + 64]>,
        got: Vec<(usize, SocketAddr)>,
        next: usize,
    }

    impl Default for Batch {
        fn default() -> Self {
            Self { buffers: vec![[0; super::MAX_DATAGRAM + 64]; BATCH], got: Vec::new(), next: 0 }
        }
    }

    impl Batch {
        pub fn receive(&mut self, socket: &UdpSocket, out: &mut [u8]) -> Option<(usize, SocketAddr)> {
            if self.next >= self.got.len() {
                self.fill(socket);
            }
            let (length, from) = *self.got.get(self.next)?;
            let index = self.next;
            self.next += 1;
            let length = length.min(out.len());
            out[..length].copy_from_slice(&self.buffers[index][..length]);
            Some((length, from))
        }

        fn fill(&mut self, socket: &UdpSocket) {
            self.got.clear();
            self.next = 0;
            // SAFETY: zeroed sockaddr_storage values are valid to be written into.
            let mut names: Vec<libc::sockaddr_storage> = vec![unsafe { std::mem::zeroed() }; BATCH];
            let mut iovecs: Vec<libc::iovec> = self
                .buffers
                .iter_mut()
                .map(|b| libc::iovec { iov_base: b.as_mut_ptr() as *mut _, iov_len: b.len() })
                .collect();
            let mut headers: Vec<libc::mmsghdr> = (0..BATCH)
                .map(|i| {
                    // SAFETY: an all-zero msghdr is a valid empty one.
                    let mut header: libc::msghdr = unsafe { std::mem::zeroed() };
                    header.msg_name = &mut names[i] as *mut _ as *mut _;
                    header.msg_namelen = size_of::<libc::sockaddr_storage>() as u32;
                    header.msg_iov = &mut iovecs[i];
                    header.msg_iovlen = 1;
                    libc::mmsghdr { msg_hdr: header, msg_len: 0 }
                })
                .collect();
            // SAFETY: the headers point at buffers and names that outlive the call; the socket never blocks.
            let n = unsafe {
                libc::recvmmsg(
                    socket.as_raw_fd(),
                    headers.as_mut_ptr(),
                    BATCH as u32,
                    libc::MSG_DONTWAIT,
                    std::ptr::null_mut(),
                )
            };
            for (i, header) in headers.iter().enumerate().take(n.max(0) as usize) {
                if let Some(from) = socket_addr(&names[i]) {
                    self.got.push((header.msg_len as usize, from));
                }
            }
        }
    }

    /// The address the system wrote into `storage`.
    fn socket_addr(storage: &libc::sockaddr_storage) -> Option<SocketAddr> {
        use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
        match i32::from(storage.ss_family) {
            libc::AF_INET => {
                // SAFETY: the family says the storage holds a sockaddr_in.
                let a = unsafe { &*(storage as *const _ as *const libc::sockaddr_in) };
                let ip = Ipv4Addr::from(u32::from_be(a.sin_addr.s_addr));
                Some(SocketAddr::V4(SocketAddrV4::new(ip, u16::from_be(a.sin_port))))
            }
            libc::AF_INET6 => {
                // SAFETY: the family says the storage holds a sockaddr_in6.
                let a = unsafe { &*(storage as *const _ as *const libc::sockaddr_in6) };
                let ip = Ipv6Addr::from(a.sin6_addr.s6_addr);
                let port = u16::from_be(a.sin6_port);
                Some(SocketAddr::V6(SocketAddrV6::new(ip, port, a.sin6_flowinfo, a.sin6_scope_id)))
            }
            _ => None,
        }
    }
}

/// Datagrams waiting for an address, with who sent each.
type Inbox = VecDeque<(SocketAddr, Vec<u8>)>;

/// Ends joined in one process, for tests: each bound address has an inbox.
#[derive(Clone, Default)]
pub struct MemoryNetwork {
    inboxes: Arc<Mutex<HashMap<SocketAddr, Inbox>>>,
}

impl MemoryNetwork {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind(&self, address: SocketAddr) -> MemoryTransport {
        self.inboxes.lock().expect("the network's lock").insert(address, VecDeque::new());
        MemoryTransport { network: self.clone(), address, queue: Vec::new() }
    }
}

pub struct MemoryTransport {
    network: MemoryNetwork,
    address: SocketAddr,
    queue: Vec<(SocketAddr, Vec<u8>)>,
}

impl Transport for MemoryTransport {
    fn send(&mut self, to: SocketAddr, packet: &[u8]) {
        self.queue.push((to, packet.to_vec()));
    }

    fn receive(&mut self, buffer: &mut [u8]) -> Option<(usize, SocketAddr)> {
        let mut inboxes = self.network.inboxes.lock().expect("the network's lock");
        let (from, packet) = inboxes.get_mut(&self.address)?.pop_front()?;
        let length = packet.len().min(buffer.len());
        buffer[..length].copy_from_slice(&packet[..length]);
        Some((length, from))
    }

    fn flush(&mut self) {
        let mut inboxes = self.network.inboxes.lock().expect("the network's lock");
        for (to, packet) in self.queue.drain(..) {
            if let Some(inbox) = inboxes.get_mut(&to) {
                inbox.push_back((self.address, packet));
            }
        }
    }

    fn local_addr(&self) -> SocketAddr {
        self.address
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn udp_sends_a_batch_and_receives_it() {
        let mut a = UdpTransport::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let mut b = UdpTransport::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        for i in 0..100u8 {
            a.send(b.local_addr(), &[i; 100]);
        }
        a.flush();
        let mut buffer = [0u8; MAX_DATAGRAM];
        let mut got = Vec::new();
        let start = std::time::Instant::now();
        while got.len() < 100 && start.elapsed().as_secs() < 2 {
            if let Some((length, from)) = b.receive(&mut buffer) {
                assert_eq!(from, a.local_addr());
                assert_eq!(length, 100);
                got.push(buffer[0]);
            }
        }
        assert_eq!(got, (0..100).collect::<Vec<u8>>(), "every one, in order on loopback");
    }

    #[test]
    fn memory_ends_reach_each_other() {
        let network = MemoryNetwork::new();
        let mut a = network.bind("10.0.0.1:1".parse().unwrap());
        let mut b = network.bind("10.0.0.2:1".parse().unwrap());
        a.send(b.local_addr(), b"hello");
        assert!(b.receive(&mut [0; 16]).is_none(), "nothing until flushed");
        a.flush();
        let mut buffer = [0u8; 16];
        assert_eq!(b.receive(&mut buffer), Some((5, a.local_addr())));
        assert_eq!(&buffer[..5], b"hello");
    }
}
