//! Networked Play: the button beside Play says how the editor plays,
//! offline, as a host or as a dedicated server's first player, how many test
//! players' windows open, and how bad a network they all play through. Play
//! then starts the session, and each test player is a game of its own in a
//! window titled "Player 2", "Player 3" and on, joined to the editor's
//! session, its log in the Console. Stop closes them.

use std::io::{BufRead, BufReader, Read};
use std::net::{SocketAddr, UdpSocket};
use std::process::{Child, Command, Stdio};

use voxear_engine::{modules::Module, net::NetStart};
use voxear_ui::MenuItem;

use super::Editor;
use crate::settings::{PlayAs, TestNetwork};

/// The most test players' windows networked Play opens.
const MOST_TEST_PLAYERS: u32 = 7;
/// Set, test players run without windows, as the editor's own tests have them.
pub const HEADLESS_VARIABLE: &str = "VOXEAR_HEADLESS_TEST_PLAYERS";

/// The games networked Play started: a dedicated server, and the test
/// players' windows.
#[derive(Default)]
pub(super) struct TestPlayers {
    games: Vec<(String, Child)>,
}

impl Drop for TestPlayers {
    fn drop(&mut self) {
        self.close();
    }
}

impl TestPlayers {
    fn close(&mut self) {
        for (name, mut game) in self.games.drain(..) {
            let _ = game.kill();
            let _ = game.wait();
            tracing::debug!(target: "player", "closed {name}");
        }
    }

    /// Starts `vox` with `arguments` as a game named `name`, its log
    /// lines into the Console with its name before them.
    fn start(&mut self, name: &str, arguments: &[String]) -> std::io::Result<()> {
        let vox = voxear_engine::player_build::sibling_program("vox");
        let mut game = Command::new(&vox)
            .args(arguments)
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let pipes = [(game.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>), false)]
            .into_iter()
            .chain([(game.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>), true)]);
        for (pipe, errors) in pipes {
            let Some(pipe) = pipe else { continue };
            let name = name.to_string();
            std::thread::spawn(move || {
                for text in BufReader::new(pipe).lines().map_while(Result::ok) {
                    crate::build_window::log_named_line(&name, &text, errors);
                }
            });
        }
        self.games.push((name.to_string(), game));
        Ok(())
    }
}

/// A port free on this machine now, for a session of the editor's own.
fn free_port() -> u16 {
    UdpSocket::bind("127.0.0.1:0").and_then(|socket| socket.local_addr()).map_or(47777, |address| address.port())
}

impl Editor {
    /// The button's label, and its menu: how to play, how many test
    /// players, and through what network.
    pub(super) fn update_multiplayer(&mut self) {
        let button = self.nodes.multiplayer;
        // A project without the Networking module plays offline, with no button to say otherwise.
        let networking = self.game.project.modules().has(Module::Networking);
        if self.ui.is_visible(button) != networking {
            self.ui.set_visible(button, networking);
        }
        if !networking {
            return;
        }
        let settings = &self.memory.settings;
        let label = match settings.multiplayer {
            PlayAs::Offline => "Offline".to_string(),
            PlayAs::Host => format!("Host + {}", settings.test_players),
            PlayAs::Server => format!("Server + {}", settings.test_players + 1),
        };
        if self.ui.text(self.nodes.multiplayer_label) != label {
            self.ui.set_text(self.nodes.multiplayer_label, &label);
        }
        if self.ui.clicked(button) {
            let [x, y, _, height] = self.ui.rect(button);
            let mut items = vec![
                MenuItem::new("Offline").checked(settings.multiplayer == PlayAs::Offline),
                MenuItem::new("Host").checked(settings.multiplayer == PlayAs::Host),
                MenuItem::new("Dedicated server").checked(settings.multiplayer == PlayAs::Server),
                MenuItem::separator(),
            ];
            let networked = settings.multiplayer != PlayAs::Offline;
            for count in 0..=MOST_TEST_PLAYERS {
                let label = match count {
                    0 => "No test players".to_string(),
                    1 => "1 test player".to_string(),
                    n => format!("{n} test players"),
                };
                items.push(MenuItem::new(&label).checked(settings.test_players == count).enabled(networked));
            }
            items.push(MenuItem::separator());
            for network in TestNetwork::ALL {
                let label = format!("{} network", network.name());
                items.push(MenuItem::new(&label).checked(settings.test_network == network).enabled(networked));
            }
            self.ui.open_menu(button, (x, y + height + 4.0), &items);
        }
        let Some(choice) = self.ui.menu_choice(button) else { return };
        let players = MOST_TEST_PLAYERS as usize + 1;
        let settings = &mut self.memory.settings;
        match choice {
            0 => settings.multiplayer = PlayAs::Offline,
            1 => settings.multiplayer = PlayAs::Host,
            2 => settings.multiplayer = PlayAs::Server,
            c if (4..4 + players).contains(&c) => settings.test_players = (c - 4) as u32,
            c if c > 4 + players => {
                if let Some(&network) = TestNetwork::ALL.get(c - 5 - players) {
                    settings.test_network = network;
                }
            }
            _ => {}
        }
        self.memory.save();
    }

    /// How Play plays: offline or networked, with how many test players,
    /// through how bad a network.
    pub fn set_networked_play(&mut self, play_as: PlayAs, test_players: u32, network: TestNetwork) {
        let settings = &mut self.memory.settings;
        settings.multiplayer = play_as;
        settings.test_players = test_players.min(MOST_TEST_PLAYERS);
        settings.test_network = network;
    }

    /// Starts networked Play, once the level is in play: the editor's own
    /// session, as the host or joining a dedicated server it starts, and
    /// the test players' windows joining it.
    pub(super) fn start_multiplayer(&mut self) {
        let settings = &self.memory.settings;
        let (play_as, count, network) = (settings.multiplayer, settings.test_players, settings.test_network);
        if play_as == PlayAs::Offline || !self.game.project.modules().has(Module::Networking) {
            return;
        }
        let port = free_port();
        let address = SocketAddr::from(([127, 0, 0, 1], port));
        let root = self.game.project.root.to_string_lossy().into_owned();
        let shared = |more: &[String]| {
            let mut arguments = vec!["run".to_string(), root.clone(), "--no-build".into()];
            arguments.extend_from_slice(more);
            arguments.extend(["--network".into(), network.name().to_lowercase()]);
            arguments
        };
        let started = match play_as {
            PlayAs::Host => self.game.start_session(NetStart::Host(port), network.conditions()),
            _ => {
                let server = shared(&["--server".into(), "--port".into(), port.to_string()]);
                if let Err(error) = self.test_players.start("Server", &server) {
                    return self.networking_failed("The server could not start", error.into());
                }
                self.game.start_session(NetStart::Connect(address), network.conditions())
            }
        };
        if let Err(error) = started {
            return self.networking_failed("Networked play could not start", error);
        }
        let headless = std::env::var_os(HEADLESS_VARIABLE).is_some();
        for number in 2..count + 2 {
            let name = format!("Player {number}");
            let mut arguments = shared(&["--connect".into(), address.to_string(), "--title".into(), name.clone()]);
            if headless {
                arguments.push("--headless".into());
            }
            if let Err(error) = self.test_players.start(&name, &arguments) {
                tracing::error!(target: "player", "{name} could not start: {error}");
            }
        }
        let what = if play_as == PlayAs::Host { "hosting" } else { "joined to a dedicated server" };
        tracing::info!("playing networked, {what} on port {port}, with {count} test players");
    }

    /// Plays on offline, saying why it could not play networked.
    fn networking_failed(&mut self, what: &str, error: anyhow::Error) {
        tracing::error!("{what}: {error:#}");
        self.play.error = Some(what.to_string());
    }

    /// Closes the test players' windows and any dedicated server.
    pub(super) fn stop_multiplayer(&mut self) {
        self.test_players.close();
    }
}
