//! A running game's console, reached from outside it: `vox console "God
//! true"` runs a line in a game window that has its tools, as typing it into
//! the console would, and prints what the line logged.
//!
//! The game listens on the loopback address only, on a port the system
//! picks, and writes the address and a secret into a file only its user can
//! read ([`FILE`], in the project's `.voxear` folder or a development build's
//! data folder), which it removes when it closes. A connection sends the
//! secret and a line, gets back what the line logged, and closes.

use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{Ipv4Addr, Shutdown, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};

use anyhow::{Context, Result, bail};

/// The file a game keeps its console's address and secret in.
pub const FILE: &str = "Console";
/// How long either side waits for the other.
const WAIT: Duration = Duration::from_secs(5);

/// A line asked for, and where its answer goes.
struct Request {
    line: String,
    reply: Sender<Vec<String>>,
}

/// The game's side: lines arrive on a thread of their own, and run on the
/// game's own thread when it asks for them, each frame.
pub struct RemoteConsole {
    requests: Receiver<Request>,
    file: PathBuf,
}

impl RemoteConsole {
    /// Listens, and writes where to `folder`.
    pub fn listen(folder: &Path) -> Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).context("listening for the console")?;
        let address = listener.local_addr()?;
        let secret = uuid::Uuid::new_v4().simple().to_string();
        std::fs::create_dir_all(folder).with_context(|| format!("creating {}", folder.display()))?;
        let file = folder.join(FILE);
        write_private(&file, &format!("{address}\n{secret}\n"))?;
        let (send, requests) = mpsc::channel();
        std::thread::Builder::new()
            .name("remote console".into())
            .spawn(move || serve(&listener, &secret, &send))
            .context("starting the console's thread")?;
        Ok(Self { requests, file })
    }

    /// Runs each line asked for since the last call with `run`, which
    /// returns what the line logged, to send back.
    pub fn poll(&self, mut run: impl FnMut(&str) -> Vec<String>) {
        while let Ok(request) = self.requests.try_recv() {
            let _ = request.reply.send(run(&request.line));
        }
    }
}

impl Drop for RemoteConsole {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.file);
    }
}

/// Answers connections until the game is gone. The thread ends with the
/// process; a connection after the game stopped asking is answered with
/// nothing.
fn serve(listener: &TcpListener, secret: &str, requests: &Sender<Request>) {
    for stream in listener.incoming().flatten() {
        if let Err(error) = answer(stream, secret, requests) {
            tracing::debug!("the remote console turned a connection away: {error:#}");
        }
    }
}

fn answer(stream: TcpStream, secret: &str, requests: &Sender<Request>) -> Result<()> {
    stream.set_read_timeout(Some(WAIT))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut given = String::new();
    reader.read_line(&mut given)?;
    if given.trim_end() != secret {
        bail!("it did not know the secret");
    }
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let (reply, replies) = mpsc::channel();
    requests.send(Request { line: line.trim_end().to_string(), reply }).context("the game has stopped")?;
    let lines = replies.recv_timeout(WAIT).context("the game did not run the line in time")?;
    let mut stream = stream;
    for line in lines {
        writeln!(stream, "{line}")?;
    }
    Ok(())
}

/// Runs `line` in the game whose console file is in `folder`, and returns
/// what it logged.
pub fn send(folder: &Path, line: &str) -> Result<Vec<String>> {
    let file = folder.join(FILE);
    let text = std::fs::read_to_string(&file)
        .with_context(|| format!("no game with its tools is running from here ({} is missing)", file.display()))?;
    let mut parts = text.lines();
    let (Some(address), Some(secret)) = (parts.next(), parts.next()) else {
        bail!("{} is not a console's", file.display())
    };
    let address = address.parse().with_context(|| format!("{} names no address", file.display()))?;
    let mut stream = TcpStream::connect_timeout(&address, WAIT)
        .context("the game did not answer; it may have closed without tidying up")?;
    stream.set_read_timeout(Some(WAIT * 2))?;
    writeln!(stream, "{secret}\n{line}")?;
    stream.shutdown(Shutdown::Write)?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply).context("reading the game's answer")?;
    Ok(reply.lines().map(String::from).collect())
}

/// Writes a file only its user can read, where the system has such a thing.
fn write_private(file: &Path, text: &str) -> Result<()> {
    // A file left by an earlier run keeps its permissions, so it goes first.
    let _ = std::fs::remove_file(file);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut opened = options.open(file).with_context(|| format!("writing {}", file.display()))?;
    opened.write_all(text.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Polls `console` as a game's frames would until `asked` finishes.
    fn answer_while<T>(console: &RemoteConsole, asked: std::thread::JoinHandle<T>) -> T {
        let started = std::time::Instant::now();
        while !asked.is_finished() {
            console.poll(|line| vec![format!("ran {line}"), "and logged this".into()]);
            assert!(started.elapsed() < Duration::from_secs(10), "the line was never answered");
            std::thread::sleep(Duration::from_millis(5));
        }
        asked.join().unwrap()
    }

    #[test]
    fn a_line_sent_runs_in_the_game_and_its_log_comes_back() {
        let folder = tempfile::tempdir().unwrap();
        let console = RemoteConsole::listen(folder.path()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(folder.path().join(FILE)).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "only its user can read the secret");
        }
        let path = folder.path().to_path_buf();
        let asked = std::thread::spawn(move || send(&path, "God true").unwrap());
        assert_eq!(answer_while(&console, asked), ["ran God true", "and logged this"]);

        // A connection without the secret gets nothing run and nothing back.
        let text = std::fs::read_to_string(folder.path().join(FILE)).unwrap();
        let address = text.lines().next().unwrap().to_string();
        let asked = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(address).unwrap();
            writeln!(stream, "guess\nquit").unwrap();
            stream.shutdown(Shutdown::Write).unwrap();
            let mut reply = String::new();
            stream.read_to_string(&mut reply).unwrap();
            reply
        });
        assert_eq!(answer_while(&console, asked), "");

        drop(console);
        assert!(!folder.path().join(FILE).exists(), "closing tidies up");
        assert!(send(folder.path(), "help").is_err(), "and nothing answers then");
    }
}
