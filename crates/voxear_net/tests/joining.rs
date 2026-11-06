//! Servers and clients joining, talking and leaving, over a network in
//! memory with packets lost, late and doubled, and over real UDP.

use std::net::SocketAddr;

use voxear_net::*;

const PROTOCOL: u64 = 0x766f_7865_6172;
const TICK: f64 = 1.0 / 60.0;

fn server_at(network: &MemoryNetwork, key: Key, max_clients: usize, conditions: Conditions) -> Server {
    let address: SocketAddr = "10.0.0.1:7777".parse().unwrap();
    let config = ServerConfig {
        protocol_id: PROTOCOL,
        private_key: key,
        max_clients,
        public_addresses: vec![address],
        connection: ConnectionConfig::default(),
    };
    Server::new(config, Box::new(Simulated::new(network.bind(address), conditions, 11)), 0.0)
}

fn token(key: &Key, client_id: u64, servers: Vec<SocketAddr>) -> ConnectToken {
    let mut user_data = [0u8; USER_DATA_BYTES];
    user_data[0] = client_id as u8;
    ConnectToken::generate(TokenRequest {
        protocol_id: PROTOCOL,
        private_key: key,
        client_id,
        user_data,
        servers,
        expire_seconds: 30,
        timeout_seconds: 2,
        now_unix: unix_seconds() as u64,
    })
    .unwrap()
}

fn client_at(network: &MemoryNetwork, token: ConnectToken, n: u8, conditions: Conditions) -> Client {
    let address: SocketAddr = format!("10.0.1.{n}:5000").parse().unwrap();
    let transport = Simulated::new(network.bind(address), conditions, u64::from(n) * 7919);
    Client::new(token, Box::new(transport), ConnectionConfig::default(), 0.0)
}

/// Ticks everything from `from` for `seconds`.
fn run(server: &mut Server, clients: &mut [Client], from: f64, seconds: f64) -> f64 {
    let mut now = from;
    while now < from + seconds {
        now += TICK;
        server.update(now);
        for client in clients.iter_mut() {
            client.update(now);
        }
        server.flush(now);
        for client in clients.iter_mut() {
            client.flush(now);
        }
    }
    now
}

#[test]
fn clients_join_and_talk_through_a_bad_network() {
    let network = MemoryNetwork::new();
    let key = random_key();
    let mut server = server_at(&network, key, 8, Conditions::BAD);
    let address = server.local_addr();
    let mut clients: Vec<Client> =
        (1..=4).map(|n| client_at(&network, token(&key, u64::from(n), vec![address]), n, Conditions::BAD)).collect();
    let mut now = run(&mut server, &mut clients, 0.0, 1.5);
    assert!(clients.iter().all(Client::is_connected), "{:?}", clients.iter().map(Client::state).collect::<Vec<_>>());
    let joined: Vec<u8> = server
        .take_events()
        .into_iter()
        .filter_map(|e| match e {
            ServerEvent::Connected { user_data, .. } => Some(user_data[0]),
            _ => None,
        })
        .collect();
    assert_eq!(joined.len(), 4, "each told once, with its own data");

    for (i, client) in clients.iter_mut().enumerate() {
        for k in 0..50u32 {
            client.send(Channel::RELIABLE, &(i as u32 * 1000 + k).to_le_bytes()).unwrap();
        }
    }
    for id in server.clients() {
        server.send(id, Channel::RELIABLE, b"welcome").unwrap();
    }
    now = run(&mut server, &mut clients, now, 3.0);
    for (i, id) in server.clients().into_iter().enumerate() {
        let got: Vec<u32> = std::iter::from_fn(|| server.receive(id, Channel::RELIABLE))
            .map(|m| u32::from_le_bytes(m[..].try_into().unwrap()))
            .collect();
        let first = (id.0 as u32 - 1) * 1000;
        assert_eq!(got, (first..first + 50).collect::<Vec<_>>(), "client {i}'s messages, in order");
        let rtt = server.stats(id).unwrap().rtt;
        assert!(rtt > 0.15 && rtt < 0.4, "round trip on a bad network: {rtt}");
    }
    for client in &mut clients {
        assert_eq!(client.receive(Channel::RELIABLE).as_deref(), Some(&b"welcome"[..]));
    }

    clients[0].disconnect();
    run(&mut server, &mut clients, now, 0.5);
    let left: Vec<ServerEvent> = server.take_events();
    assert!(
        left.iter()
            .any(|e| matches!(e, ServerEvent::Disconnected { client: ClientId(1), reason: DisconnectReason::Closed })),
        "{left:?}"
    );
    assert_eq!(server.clients().len(), 3);
}

#[test]
fn a_full_server_turns_clients_away_and_a_quiet_one_is_dropped() {
    let network = MemoryNetwork::new();
    let key = random_key();
    let mut server = server_at(&network, key, 1, Conditions::PERFECT);
    let address = server.local_addr();
    let mut clients = vec![
        client_at(&network, token(&key, 1, vec![address]), 1, Conditions::PERFECT),
        client_at(&network, token(&key, 2, vec![address]), 2, Conditions::PERFECT),
    ];
    let now = run(&mut server, &mut clients, 0.0, 1.0);
    assert!(clients[0].is_connected());
    assert_eq!(clients[1].state(), ClientState::Failed(ConnectFailure::Denied));

    // The first goes quiet: it stops being ticked, and is dropped after its timeout.
    let mut rest: Vec<Client> = Vec::new();
    run(&mut server, &mut rest, now, 2.5);
    let events = server.take_events();
    assert!(
        events.iter().any(|e| matches!(e, ServerEvent::Disconnected { reason: DisconnectReason::TimedOut, .. })),
        "{events:?}"
    );
}

#[test]
fn a_token_works_once_only_from_one_address_and_only_unexpired() {
    let network = MemoryNetwork::new();
    let key = random_key();
    let mut server = server_at(&network, key, 4, Conditions::PERFECT);
    let address = server.local_addr();
    let shared = token(&key, 1, vec![address]);
    let mut clients = vec![
        client_at(&network, shared.clone(), 1, Conditions::PERFECT),
        client_at(&network, shared, 2, Conditions::PERFECT),
    ];
    run(&mut server, &mut clients, 0.0, 1.0);
    assert!(clients[0].is_connected());
    assert!(!clients[1].is_connected(), "the same token from another address");

    let mut expired = token(&key, 3, vec![address]);
    expired.expires = unix_seconds() as u64 - 1;
    let mut late = vec![client_at(&network, expired, 3, Conditions::PERFECT)];
    run(&mut server, &mut late, 0.0, 0.5);
    assert_eq!(late[0].state(), ClientState::Failed(ConnectFailure::TokenExpired));

    let stranger = token(&random_key(), 4, vec![address]);
    let mut forged = vec![client_at(&network, stranger, 4, Conditions::PERFECT)];
    run(&mut server, &mut forged, 0.0, 2.5);
    assert_eq!(forged[0].state(), ClientState::Failed(ConnectFailure::TimedOut), "sealed with another key");
}

/// A transport that notes the size of everything it receives.
struct Recording {
    inner: MemoryTransport,
    received: std::sync::Arc<std::sync::Mutex<Vec<usize>>>,
}

impl Transport for Recording {
    fn send(&mut self, to: SocketAddr, packet: &[u8]) {
        self.inner.send(to, packet);
    }

    fn receive(&mut self, buffer: &mut [u8]) -> Option<(usize, SocketAddr)> {
        let got = self.inner.receive(buffer)?;
        self.received.lock().unwrap().push(got.0);
        Some(got)
    }

    fn flush(&mut self) {
        self.inner.flush();
    }

    fn local_addr(&self) -> SocketAddr {
        self.inner.local_addr()
    }
}

#[test]
fn nothing_answers_more_than_it_was_sent_before_joining() {
    let network = MemoryNetwork::new();
    let key = random_key();
    let mut server = server_at(&network, key, 4, Conditions::PERFECT);
    let address = server.local_addr();
    // Garbage and wrong sizes get no answer at all.
    let mut probe = network.bind("10.0.9.9:1".parse().unwrap());
    for packet in [vec![0u8; 10], vec![0u8; REQUEST_BYTES - 1], vec![0u8; REQUEST_BYTES], vec![5u8; 300]] {
        probe.send(address, &packet);
    }
    probe.flush();
    // A real client gets answers, each smaller than its request.
    let received = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let transport = Recording { inner: network.bind("10.0.9.10:1".parse().unwrap()), received: received.clone() };
    let mut clients =
        vec![Client::new(token(&key, 1, vec![address]), Box::new(transport), ConnectionConfig::default(), 0.0)];
    run(&mut server, &mut clients, 0.0, 0.5);
    assert!(clients[0].is_connected());
    assert!(probe.receive(&mut [0u8; MAX_DATAGRAM]).is_none(), "nothing to garbage");
    let sizes = received.lock().unwrap().clone();
    assert!(!sizes.is_empty());
    assert!(sizes.iter().all(|&size| size < REQUEST_BYTES / 3), "{sizes:?}");
}

#[test]
fn over_real_udp_on_this_machine() {
    let key = random_key();
    let transport = UdpTransport::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let address = transport.local_addr();
    let config = ServerConfig {
        protocol_id: PROTOCOL,
        private_key: key,
        max_clients: 2,
        public_addresses: vec![address],
        connection: ConnectionConfig::default(),
    };
    let mut server = Server::new(config, Box::new(transport), 0.0);
    let client_transport = UdpTransport::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let mut clients =
        vec![Client::new(token(&key, 9, vec![address]), Box::new(client_transport), ConnectionConfig::default(), 0.0)];
    let start = std::time::Instant::now();
    while !clients[0].is_connected() && start.elapsed().as_secs() < 3 {
        let now = start.elapsed().as_secs_f64();
        run(&mut server, &mut clients, now, TICK);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(clients[0].is_connected(), "{:?}", clients[0].state());
    clients[0].send(Channel::UNRELIABLE, b"ping").unwrap();
    let id = server.clients()[0];
    let mut got = None;
    while got.is_none() && start.elapsed().as_secs() < 5 {
        let now = start.elapsed().as_secs_f64();
        run(&mut server, &mut clients, now, TICK);
        got = server.receive(id, Channel::UNRELIABLE);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(got.as_deref(), Some(&b"ping"[..]));
}
