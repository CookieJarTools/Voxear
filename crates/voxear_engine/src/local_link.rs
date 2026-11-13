//! A link between two programs on the same computer, as `vox editor call`
//! reaches a running editor: calls by name with JSON arguments, answered
//! with JSON, as JSON-RPC 2.0 has them, a call to a line.
//!
//! The side that answers listens on the loopback address only, on a port the
//! system picks, and writes where it listens and a secret into a file only
//! its user can read, which it removes when it closes. A connection sends
//! the secret on its first line, then calls, and gets back an answer to
//! each, in order. Calls are run on the answering side's own thread when it
//! asks for them, as [`LinkServer::poll`] does each frame; `link.hello` is
//! answered at once, whatever that thread is doing.

use std::{
    io::{BufRead, BufReader, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    time::Duration,
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// How long a connection has to give its secret, and to connect.
const WAIT: Duration = Duration::from_secs(5);
/// The call every link answers itself, with what it is.
pub const HELLO: &str = "link.hello";

/// Why a call was not done, as a tool reads it to decide what to do next.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// No call has that name.
    NoSuchCall,
    /// The arguments are missing something or are not what the call takes.
    BadArguments,
    NoSuchObj,
    NoSuchComponent,
    NoSuchField,
    NoSuchAsset,
    /// A value is not one the field takes.
    WrongValue,
    /// Edits are not made while the game plays.
    RefusedWhilePlaying,
    /// It would lose edits not saved.
    UnsavedConflict,
    /// The editor's settings do not let tools change the project.
    NotAllowed,
    /// It was tried and did not work; the message says why.
    Failed,
    /// The other side closed before answering.
    Gone,
}

impl ErrorKind {
    /// The code JSON-RPC gives it: its own for an unknown call and bad
    /// arguments, and one of the codes left for programs for the rest.
    pub fn code(self) -> i64 {
        match self {
            ErrorKind::NoSuchCall => -32601,
            ErrorKind::BadArguments => -32602,
            ErrorKind::NoSuchObj => -32001,
            ErrorKind::NoSuchComponent => -32002,
            ErrorKind::NoSuchField => -32003,
            ErrorKind::NoSuchAsset => -32004,
            ErrorKind::WrongValue => -32005,
            ErrorKind::RefusedWhilePlaying => -32006,
            ErrorKind::UnsavedConflict => -32007,
            ErrorKind::NotAllowed => -32008,
            ErrorKind::Failed => -32009,
            ErrorKind::Gone => -32010,
        }
    }
}

/// A call not done: why, as a kind, and in words.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkError {
    pub kind: ErrorKind,
    pub message: String,
}

impl LinkError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into() }
    }

    /// As a JSON-RPC error object.
    pub fn to_json(&self) -> Value {
        json!({ "code": self.kind.code(), "message": self.message, "data": { "kind": self.kind } })
    }

    /// Back from a JSON-RPC error object.
    pub fn from_json(error: &Value) -> Self {
        let kind = error.pointer("/data/kind").and_then(|kind| serde_json::from_value(kind.clone()).ok());
        let message = error.get("message").and_then(Value::as_str).unwrap_or("the call failed");
        Self::new(kind.unwrap_or(ErrorKind::Failed), message)
    }
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for LinkError {}

/// What a call gives back.
pub type Answer = Result<Value, LinkError>;

/// One call asked for, and where its answer goes. Dropping it unanswered
/// answers [`ErrorKind::Gone`].
pub struct Request {
    pub call: String,
    pub args: Value,
    reply: Sender<Answer>,
}

impl Request {
    /// A call made in the same program, as `vox editor call --headless`
    /// makes one, and where its answer arrives.
    pub fn local(call: &str, args: Value) -> (Self, Receiver<Answer>) {
        let (reply, answer) = mpsc::channel();
        (Self { call: call.to_string(), args, reply }, answer)
    }

    pub fn answer(self, answer: Answer) {
        let _ = self.reply.send(answer);
    }
}

/// What a link's file says about it: where it listens, its secret, and what
/// is answering, which `link.hello` gives back without the secret.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct About {
    pub address: SocketAddr,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub secret: String,
    pub pid: u32,
    /// What answers, as `editor`.
    pub kind: String,
    /// No window: a headless editor that a windowed one may take over from.
    #[serde(default)]
    pub headless: bool,
    pub version: String,
}

/// The answering side: calls arrive on threads of their own, a thread a
/// connection, and are run on the program's own thread when it asks.
pub struct LinkServer {
    requests: Receiver<Request>,
    file: PathBuf,
    connections: Arc<AtomicUsize>,
}

impl LinkServer {
    /// Listens, and writes where to `folder`/`name`. Refuses when a link
    /// answering there is open already.
    pub fn listen(folder: &Path, name: &str, kind: &str, headless: bool) -> Result<Self> {
        if let Ok(other) = LinkClient::connect(folder, name) {
            bail!("another {} (process {}) has the link already", other.about.kind, other.about.pid);
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).context("listening for tools")?;
        let about = About {
            address: listener.local_addr()?,
            secret: uuid::Uuid::new_v4().simple().to_string(),
            pid: std::process::id(),
            kind: kind.to_string(),
            headless,
            version: crate::engine_version().to_string(),
        };
        std::fs::create_dir_all(folder).with_context(|| format!("creating {}", folder.display()))?;
        let file = folder.join(name);
        write_private(&file, &serde_json::to_string_pretty(&about)?)?;
        let (send, requests) = mpsc::channel();
        let connections = Arc::new(AtomicUsize::new(0));
        let counted = connections.clone();
        std::thread::Builder::new()
            .name("link".into())
            .spawn(move || serve(&listener, &about, &send, &counted))
            .context("starting the link's thread")?;
        Ok(Self { requests, file, connections })
    }

    /// The calls asked for since the last time, to run and answer.
    pub fn poll(&self) -> Vec<Request> {
        self.requests.try_iter().collect()
    }

    /// How many programs are connected now.
    pub fn connections(&self) -> usize {
        self.connections.load(Ordering::Relaxed)
    }

    /// The file it wrote.
    pub fn file(&self) -> &Path {
        &self.file
    }
}

impl Drop for LinkServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.file);
    }
}

/// Takes connections until the program is gone, each on a thread of its own.
fn serve(listener: &TcpListener, about: &About, requests: &Sender<Request>, connections: &Arc<AtomicUsize>) {
    for stream in listener.incoming().flatten() {
        let (about, requests, connections) = (about.clone(), requests.clone(), connections.clone());
        let started = std::thread::Builder::new().name("link connection".into()).spawn(move || {
            connections.fetch_add(1, Ordering::Relaxed);
            if let Err(error) = answer(stream, &about, &requests) {
                tracing::debug!("a link connection ended: {error:#}");
            }
            connections.fetch_sub(1, Ordering::Relaxed);
        });
        if let Err(error) = started {
            tracing::warn!("could not take a link connection: {error}");
        }
    }
}

fn answer(stream: TcpStream, about: &About, requests: &Sender<Request>) -> Result<()> {
    stream.set_read_timeout(Some(WAIT))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    let mut given = String::new();
    reader.read_line(&mut given)?;
    if given.trim_end() != about.secret {
        let refused = LinkError::new(ErrorKind::NotAllowed, "the secret was not the link's");
        writeln!(writer, "{}", json!({ "jsonrpc": "2.0", "id": null, "error": refused.to_json() }))?;
        bail!("it did not know the secret");
    }
    // A tool may wait as long as it likes between calls.
    reader.get_ref().set_read_timeout(None)?;
    let public = About { secret: String::new(), ..about.clone() };
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(call) => {
                let id = call.get("id").cloned().unwrap_or(Value::Null);
                let method = call.get("method").and_then(Value::as_str).unwrap_or_default().to_string();
                let args = call.get("params").cloned().unwrap_or_else(|| json!({}));
                let answered = match method.as_str() {
                    HELLO => Ok(serde_json::to_value(&public)?),
                    "" => Err(LinkError::new(ErrorKind::BadArguments, "a call needs a `method`")),
                    _ => ask(requests, method, args),
                };
                match answered {
                    Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
                    Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error.to_json() }),
                }
            }
            Err(error) => json!({
                "jsonrpc": "2.0",
                "id": null,
                "error": { "code": -32700, "message": format!("not JSON: {error}") },
            }),
        };
        writeln!(writer, "{reply}")?;
    }
    Ok(())
}

/// Hands a call to the program's thread and waits for its answer.
fn ask(requests: &Sender<Request>, call: String, args: Value) -> Answer {
    let (reply, answer) = mpsc::channel();
    let gone = || LinkError::new(ErrorKind::Gone, "the editor closed before answering");
    requests.send(Request { call, args, reply }).map_err(|_| gone())?;
    answer.recv().map_err(|_| gone())?
}

/// Why no link could be reached.
#[derive(Debug)]
pub enum Unreachable {
    /// No program has written its link file there.
    Missing(PathBuf),
    /// The file is there but nothing answers: the program that wrote it
    /// closed without tidying up. The file has been removed.
    Stale(PathBuf),
}

impl std::fmt::Display for Unreachable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unreachable::Missing(file) => write!(f, "nothing is listening ({} is missing)", file.display()),
            Unreachable::Stale(file) => {
                write!(f, "nothing is listening; {} was left by one that closed, and is gone now", file.display())
            }
        }
    }
}

impl std::error::Error for Unreachable {}

/// The calling side, connected.
pub struct LinkClient {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    next: u64,
    /// What answers, as it said.
    pub about: About,
}

impl LinkClient {
    /// Connects to the link whose file is `folder`/`name`. An error that is
    /// [`Unreachable`] means no program is there to answer.
    pub fn connect(folder: &Path, name: &str) -> Result<Self> {
        let file = folder.join(name);
        let Ok(text) = std::fs::read_to_string(&file) else { return Err(Unreachable::Missing(file).into()) };
        let about: About = serde_json::from_str(&text).with_context(|| format!("{} is not a link's", file.display()))?;
        let stale = |file: PathBuf| {
            // Only the file as it was read: a new program may have written its own since.
            if std::fs::read_to_string(&file).is_ok_and(|now| now == text) {
                let _ = std::fs::remove_file(&file);
            }
            anyhow::Error::from(Unreachable::Stale(file))
        };
        let Ok(stream) = TcpStream::connect_timeout(&about.address, WAIT) else { return Err(stale(file)) };
        stream.set_read_timeout(Some(WAIT))?;
        let mut client =
            Self { reader: BufReader::new(stream.try_clone()?), writer: stream, next: 0, about: about.clone() };
        writeln!(client.writer, "{}", about.secret)?;
        // Something else may listen on that port now: what answers must be a link.
        let Ok(Ok(hello)) = client.call(HELLO, json!({})) else { return Err(stale(file)) };
        client.about = serde_json::from_value(hello).context("the link's hello")?;
        client.reader.get_ref().set_read_timeout(None)?;
        Ok(client)
    }

    /// Makes a call and waits for its answer. The outer error is the link
    /// failing; the inner one the call refused.
    pub fn call(&mut self, call: &str, args: Value) -> Result<Answer> {
        self.next += 1;
        let request = json!({ "jsonrpc": "2.0", "id": self.next, "method": call, "params": args });
        writeln!(self.writer, "{request}").context("sending the call")?;
        let mut line = String::new();
        if self.reader.read_line(&mut line).context("reading the answer")? == 0 {
            return Ok(Err(LinkError::new(ErrorKind::Gone, "the editor closed before answering")));
        }
        let reply: Value = serde_json::from_str(&line).context("the answer is not JSON")?;
        Ok(match reply.get("error") {
            Some(error) => Err(LinkError::from_json(error)),
            None => Ok(reply.get("result").cloned().unwrap_or(Value::Null)),
        })
    }
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

    /// Answers calls as a program's frames would until `asked` is done.
    fn answering<T>(server: &LinkServer, asked: std::thread::JoinHandle<T>) -> T {
        while !asked.is_finished() {
            for request in server.poll() {
                let answer = match request.call.as_str() {
                    "add" => {
                        let (a, b) = (request.args["a"].as_i64().unwrap_or(0), request.args["b"].as_i64().unwrap_or(0));
                        Ok(json!(a + b))
                    }
                    _ => Err(LinkError::new(ErrorKind::NoSuchCall, "no such call")),
                };
                request.answer(answer);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        asked.join().unwrap()
    }

    #[test]
    fn calls_are_answered_in_order_over_one_connection() {
        let folder = tempfile::tempdir().unwrap();
        let server = LinkServer::listen(folder.path(), "Editor", "editor", true).unwrap();
        let path = folder.path().to_path_buf();
        let asked = std::thread::spawn(move || {
            let mut client = LinkClient::connect(&path, "Editor").unwrap();
            assert_eq!(client.about.kind, "editor");
            assert!(client.about.secret.is_empty(), "hello gives the secret away");
            let sums: Vec<Answer> = (0..3).map(|i| client.call("add", json!({ "a": i, "b": 10 })).unwrap()).collect();
            let unknown = client.call("nothing", json!({})).unwrap();
            (sums, unknown)
        });
        let (sums, unknown) = answering(&server, asked);
        assert_eq!(sums, [Ok(json!(10)), Ok(json!(11)), Ok(json!(12))]);
        assert_eq!(unknown.unwrap_err().kind, ErrorKind::NoSuchCall);
    }

    #[test]
    fn a_wrong_secret_runs_nothing() {
        let folder = tempfile::tempdir().unwrap();
        let server = LinkServer::listen(folder.path(), "Editor", "editor", false).unwrap();
        let about: About = serde_json::from_str(&std::fs::read_to_string(server.file()).unwrap()).unwrap();
        let mut stream = TcpStream::connect(about.address).unwrap();
        writeln!(stream, "not the secret").unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut reply = String::new();
        reader.read_line(&mut reply).unwrap();
        let refused: Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(LinkError::from_json(&refused["error"]).kind, ErrorKind::NotAllowed);
        // The connection is closed: a call after it goes nowhere.
        let _ = writeln!(stream, "{}", json!({ "jsonrpc": "2.0", "id": 1, "method": "add" }));
        let mut more = String::new();
        assert!(reader.read_line(&mut more).map_or(true, |read| read == 0), "answered after a wrong secret: {more}");
        std::thread::sleep(Duration::from_millis(50));
        assert!(server.poll().is_empty(), "a call with a wrong secret reached the editor");
    }

    #[test]
    fn the_link_is_on_loopback_in_a_file_only_its_user_reads() {
        let folder = tempfile::tempdir().unwrap();
        let server = LinkServer::listen(folder.path(), "Editor", "editor", false).unwrap();
        let about: About = serde_json::from_str(&std::fs::read_to_string(server.file()).unwrap()).unwrap();
        assert!(about.address.ip().is_loopback());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(server.file()).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let file = server.file().to_path_buf();
        drop(server);
        assert!(!file.exists(), "the link's file outlived it");
    }

    #[test]
    fn a_second_server_is_refused_and_a_stale_file_is_tidied_away() {
        let folder = tempfile::tempdir().unwrap();
        let first = LinkServer::listen(folder.path(), "Editor", "editor", false).unwrap();
        let path = folder.path().to_path_buf();
        let second = std::thread::spawn(move || LinkServer::listen(&path, "Editor", "editor", false).err());
        let refused = answering(&first, second).expect("a second link opened over the first");
        assert!(refused.to_string().contains("has the link already"), "{refused}");
        // A file left by a program that closed without tidying up.
        let text = std::fs::read_to_string(first.file()).unwrap();
        drop(first);
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let mut about: About = serde_json::from_str(&text).unwrap();
        about.address = listener.local_addr().unwrap();
        drop(listener);
        std::fs::write(folder.path().join("Editor"), serde_json::to_string(&about).unwrap()).unwrap();
        let error = LinkClient::connect(folder.path(), "Editor").err().unwrap();
        assert!(matches!(error.downcast_ref::<Unreachable>(), Some(Unreachable::Stale(_))), "{error}");
        assert!(!folder.path().join("Editor").exists());
    }
}
