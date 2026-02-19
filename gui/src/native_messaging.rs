/// Chrome Native Messaging protocol — 4-byte LE length-prefixed JSON on stdin/stdout.
///
/// The browser extension sends requests, the GUI responds. Each message is a
/// JSON object preceded by its byte-length as a little-endian u32.
///
/// Single-instance relay: when Chrome spawns a second process and a GUI is already
/// listening on `~/.iota-wallet/gui.sock`, the new process acts as a headless
/// stdin↔socket byte forwarder instead of opening a duplicate window.
use std::io::{self, Read, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::messages::Message;

// -- Protocol types --

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct NativeRequest {
    pub(crate) id: String,
    pub(crate) method: String,
    #[serde(default)]
    pub(crate) params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct NativeResponse {
    pub(crate) id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<NativeError>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct NativeError {
    pub(crate) code: String,
    pub(crate) message: String,
}

impl NativeResponse {
    pub(crate) fn ok(id: String, result: serde_json::Value) -> Self {
        Self {
            id,
            result: Some(result),
            error: None,
        }
    }

    pub(crate) fn err(id: String, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id,
            result: None,
            error: Some(NativeError {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

// -- Wire format IO --

/// Read one native messaging frame from `reader`.
/// Returns `Ok(None)` on clean EOF (stdin closed).
pub(crate) fn read_native_message(reader: &mut impl Read) -> io::Result<Option<NativeRequest>> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }

    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 || len > 1_048_576 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid native message length: {len}"),
        ));
    }

    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;

    let req: NativeRequest =
        serde_json::from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Some(req))
}

/// Write one native messaging frame to `writer`.
pub(crate) fn write_native_message(
    writer: &mut impl Write,
    response: &NativeResponse,
) -> io::Result<()> {
    let json =
        serde_json::to_vec(response).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let len = json.len() as u32;
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(&json)?;
    writer.flush()
}

// -- Iced subscription --

/// Returns true when stdin looks like a Chrome Native Messaging pipe (not a TTY).
pub(crate) fn is_native_messaging_host() -> bool {
    !atty_stdin()
}

fn atty_stdin() -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        unsafe { libc::isatty(io::stdin().as_raw_fd()) != 0 }
    }
    #[cfg(not(unix))]
    {
        true // assume TTY on non-unix — disable native messaging
    }
}

/// Iced subscription that reads native messaging requests from stdin.
/// Only produces items when stdin is a pipe (launched by Chrome).
pub(crate) fn native_messaging_subscription() -> iced::Subscription<Message> {
    if !is_native_messaging_host() {
        return iced::Subscription::none();
    }

    iced::Subscription::run(create_native_stream)
}

fn create_native_stream() -> impl iced::futures::Stream<Item = Message> {
    iced::stream::channel(
        32,
        |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
            use iced::futures::SinkExt;

            // Read stdin in a blocking thread
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            std::thread::spawn(move || {
                let mut stdin = io::stdin().lock();
                loop {
                    match read_native_message(&mut stdin) {
                        Ok(Some(req)) => {
                            if tx.send(req).is_err() {
                                break; // receiver dropped
                            }
                        }
                        Ok(None) => break, // EOF
                        Err(e) => {
                            eprintln!("native messaging read error: {e}");
                            break;
                        }
                    }
                }
            });

            // Forward to iced
            while let Some(req) = rx.recv().await {
                let _ = output.send(Message::NativeRequest(req)).await;
            }

            // Keep alive (iced drops the subscription if this future completes)
            std::future::pending::<()>().await;
        },
    )
}

/// Spawn a writer thread that consumes responses from the channel and writes
/// them to stdout. Returns the sender half.
pub(crate) fn spawn_native_response_writer() -> std::sync::mpsc::Sender<NativeResponse> {
    let (tx, rx) = std::sync::mpsc::channel::<NativeResponse>();
    std::thread::spawn(move || {
        let mut stdout = io::stdout().lock();
        for response in rx {
            if let Err(e) = write_native_message(&mut stdout, &response) {
                eprintln!("native messaging write error: {e}");
                break;
            }
        }
    });
    tx
}

// -- Extension ID detection --

/// Extract the extension ID from CLI args. Chrome passes `chrome-extension://ID/`
/// as an argument when launching a native messaging host.
pub(crate) fn detect_extension_id() -> Option<String> {
    std::env::args().find_map(|arg| {
        arg.strip_prefix("chrome-extension://")
            .and_then(|rest| rest.strip_suffix('/'))
            .map(|id| id.to_string())
    })
}

// -- Native host installation --

/// Install the native messaging host manifest for all detected Chromium-based browsers.
/// Returns the list of paths where manifests were written.
pub(crate) fn install_native_host(extension_id: &str) -> io::Result<Vec<std::path::PathBuf>> {
    // Chrome extension IDs are exactly 32 lowercase alphabetic characters
    if extension_id.len() != 32 || !extension_id.bytes().all(|b| b.is_ascii_lowercase()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid extension ID format (expected 32 lowercase characters)",
        ));
    }

    let binary = std::env::current_exe()?;
    let binary_str = binary
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Binary path is not UTF-8"))?;

    let manifest = serde_json::json!({
        "name": "org.iota.wallet",
        "description": "IOTA Desktop Wallet - Native Messaging Bridge",
        "path": binary_str,
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{extension_id}/")]
    });
    let manifest = serde_json::to_string_pretty(&manifest)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let home = dirs::home_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Could not determine home directory",
        )
    })?;

    #[cfg(target_os = "macos")]
    let browser_dirs = [
        home.join("Library/Application Support/Chromium/NativeMessagingHosts"),
        home.join("Library/Application Support/Google/Chrome/NativeMessagingHosts"),
        home.join("Library/Application Support/BraveSoftware/Brave-Browser/NativeMessagingHosts"),
    ];
    #[cfg(not(target_os = "macos"))]
    let browser_dirs = [
        home.join(".config/chromium/NativeMessagingHosts"),
        home.join(".config/google-chrome/NativeMessagingHosts"),
        home.join(".config/BraveSoftware/Brave-Browser/NativeMessagingHosts"),
    ];

    let mut installed = Vec::new();
    for dir in &browser_dirs {
        // Only install if the parent browser config dir exists
        if let Some(parent) = dir.parent() {
            if parent.exists() {
                std::fs::create_dir_all(dir)?;
                let path = dir.join("org.iota.wallet.json");
                std::fs::write(&path, &manifest)?;
                installed.push(path);
            }
        }
    }

    if installed.is_empty() {
        // Fallback: create for Chromium anyway
        let dir = &browser_dirs[0];
        std::fs::create_dir_all(dir)?;
        let path = dir.join("org.iota.wallet.json");
        std::fs::write(&path, &manifest)?;
        installed.push(path);
    }

    Ok(installed)
}

// -- Unix socket relay (single-instance) --

/// Path to the Unix domain socket used for single-instance relay.
pub(crate) fn socket_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".iota-wallet")
        .join("gui.sock")
}

/// Try to connect to an existing GUI's socket. Returns `Some(stream)` if a GUI
/// is listening, `None` otherwise (missing file, stale socket, etc.).
#[cfg(unix)]
pub(crate) fn try_connect_relay() -> Option<std::os::unix::net::UnixStream> {
    std::os::unix::net::UnixStream::connect(socket_path()).ok()
}

/// Read one 4-byte LE length-prefixed frame from `reader` and write it verbatim
/// to `writer`. Returns `Ok(true)` on success, `Ok(false)` on EOF.
fn forward_frame(reader: &mut impl Read, writer: &mut impl Write) -> io::Result<bool> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(false),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len == 0 || len > 1_048_576 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid frame length: {len}"),
        ));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    writer.write_all(&len_buf)?;
    writer.write_all(&buf)?;
    writer.flush()?;
    Ok(true)
}

/// Headless byte forwarder: stdin↔socket. Divergent — never returns normally.
/// Called from `main()` when a GUI is already listening on the socket.
#[cfg(unix)]
pub(crate) fn run_relay(stream: std::os::unix::net::UnixStream) -> ! {
    use std::net::Shutdown;

    let mut socket_writer = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("relay: failed to clone socket: {e}");
            std::process::exit(1);
        }
    };
    let mut socket_reader = stream;

    // stdin → socket
    let writer_handle = std::thread::spawn(move || {
        let mut stdin = io::stdin().lock();
        loop {
            match forward_frame(&mut stdin, &mut socket_writer) {
                Ok(true) => continue,
                Ok(false) => break, // Chrome closed stdin
                Err(e) => {
                    eprintln!("relay stdin→socket: {e}");
                    break;
                }
            }
        }
        let _ = socket_writer.shutdown(Shutdown::Write);
    });

    // socket → stdout
    let reader_handle = std::thread::spawn(move || {
        let mut stdout = io::stdout().lock();
        loop {
            match forward_frame(&mut socket_reader, &mut stdout) {
                Ok(true) => continue,
                Ok(false) => break,
                Err(e) => {
                    eprintln!("relay socket→stdout: {e}");
                    break;
                }
            }
        }
    });

    let _ = writer_handle.join();
    let _ = reader_handle.join();
    std::process::exit(0);
}

/// Iced subscription that listens on `gui.sock` for relay connections.
/// Always active — the socket is available as soon as the GUI starts.
pub(crate) fn socket_listener_subscription() -> iced::Subscription<Message> {
    iced::Subscription::run(create_socket_listener_stream)
}

#[cfg(unix)]
fn create_socket_listener_stream() -> impl iced::futures::Stream<Item = Message> {
    iced::stream::channel(
        32,
        |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
            use iced::futures::SinkExt;
            use std::os::unix::net::UnixListener;

            let path = socket_path();

            // Remove stale socket if it exists
            let _ = std::fs::remove_file(&path);

            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            let listener = match UnixListener::bind(&path) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("socket listener: failed to bind {}: {e}", path.display());
                    std::future::pending::<()>().await;
                    unreachable!();
                }
            };

            // Blocking accept loop in a dedicated thread
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Message>();
            std::thread::spawn(move || {
                for conn in listener.incoming() {
                    let mut stream = match conn {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("socket listener: accept error: {e}");
                            continue;
                        }
                    };

                    // Per-connection response channel
                    let (resp_tx, resp_rx) = std::sync::mpsc::channel::<NativeResponse>();

                    // Notify iced of new client
                    if tx.send(Message::NativeClientConnected(resp_tx)).is_err() {
                        break; // iced dropped
                    }

                    // Writer thread: responses → socket
                    let mut writer = match stream.try_clone() {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("socket listener: clone error: {e}");
                            let _ = tx.send(Message::NativeClientDisconnected);
                            continue;
                        }
                    };
                    let writer_handle = std::thread::spawn(move || {
                        for response in resp_rx {
                            if let Err(e) = write_native_message(&mut writer, &response) {
                                eprintln!("socket writer: {e}");
                                break;
                            }
                        }
                    });

                    // Read loop: socket → iced messages
                    loop {
                        match read_native_message(&mut stream) {
                            Ok(Some(req)) => {
                                if tx.send(Message::NativeRequest(req)).is_err() {
                                    break;
                                }
                            }
                            Ok(None) => break, // relay closed
                            Err(e) => {
                                eprintln!("socket reader: {e}");
                                break;
                            }
                        }
                    }

                    let _ = tx.send(Message::NativeClientDisconnected);
                    let _ = writer_handle.join();
                }
            });

            // Forward to iced
            while let Some(msg) = rx.recv().await {
                let _ = output.send(msg).await;
            }

            std::future::pending::<()>().await;
        },
    )
}

#[cfg(not(unix))]
fn create_socket_listener_stream() -> impl iced::futures::Stream<Item = Message> {
    iced::stream::channel(
        1,
        |_output: iced::futures::channel::mpsc::Sender<Message>| async move {
            // Unix sockets not available — just stay alive
            std::future::pending::<()>().await;
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let req_json = r#"{"id":"1","method":"connect","params":{}}"#;
        let len = req_json.len() as u32;
        let mut buf = Vec::new();
        buf.extend_from_slice(&len.to_le_bytes());
        buf.extend_from_slice(req_json.as_bytes());

        let mut cursor = io::Cursor::new(&buf);
        let req = read_native_message(&mut cursor).unwrap().unwrap();
        assert_eq!(req.id, "1");
        assert_eq!(req.method, "connect");

        let resp = NativeResponse::ok("1".into(), serde_json::json!({ "accounts": [] }));
        let mut out = Vec::new();
        write_native_message(&mut out, &resp).unwrap();

        // Verify we can read back the response as raw JSON
        let resp_len = u32::from_le_bytes(out[..4].try_into().unwrap()) as usize;
        let resp_json: serde_json::Value = serde_json::from_slice(&out[4..4 + resp_len]).unwrap();
        assert_eq!(resp_json["id"], "1");
        assert!(resp_json["result"]["accounts"].is_array());
    }

    #[test]
    fn eof_returns_none() {
        let mut cursor = io::Cursor::new(Vec::<u8>::new());
        assert!(read_native_message(&mut cursor).unwrap().is_none());
    }

    #[test]
    fn error_response_serializes() {
        let resp = NativeResponse::err("2".into(), "USER_REJECTED", "User declined");
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["error"]["code"], "USER_REJECTED");
        assert!(json.get("result").is_none());
    }
}
