//! Unix-socket bridge between the core and the Swift helper (requirements NB-1, NB-2, NB-5, NB-6).
//!
//! The core listens; the helper connects. After the `hello` / `hello_ack` handshake every helper
//! event is passed to a callback. Commands are written with [`Bridge::send`]. Only std threads are
//! used here; the engines added later run on their own runtime.
//!
//! Security: the socket lives in a per-user directory (mode 0700) and the socket file is 0600.
//! Typed text arrives over this socket and is never logged.

use crate::protocol::{
    check_version, encode_line, parse_event, CoreCommand, Frame, LineDecoder, MicStatus,
    NativeEvent, PROTOCOL_VERSION,
};
use serde::Serialize;
use std::io::{self, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// What the UI and tray need to know about the helper.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct BridgeStatus {
    /// A helper has completed the handshake and is currently connected.
    pub connected: bool,
    /// Last reported Accessibility trust (None until the helper reports it).
    pub accessibility: Option<bool>,
    /// Last reported microphone status (None until the helper reports it).
    pub microphone: Option<MicStatus>,
    /// Last reported Screen Recording permission (only needed for screen context, PC-4).
    pub screen_recording: Option<bool>,
}

/// Default socket location: `$TMPDIR/bob-<uid>/native.sock`.
pub fn default_socket_path() -> PathBuf {
    let uid = current_uid();
    std::env::temp_dir()
        .join(format!("bob-{uid}"))
        .join("native.sock")
}

/// The current user's id, found without `unsafe`: the home directory is owned by the user on macOS.
/// Falls back to 0 only if `HOME` is unset or unreadable (the directory is still mode 0700).
fn current_uid() -> u32 {
    use std::os::unix::fs::MetadataExt;
    std::env::var_os("HOME")
        .and_then(|h| std::fs::metadata(h).ok())
        .map(|m| m.uid())
        .unwrap_or(0)
}

/// Bind the listening socket: create the parent directory with mode 0700, remove a stale socket,
/// then restrict the socket file to 0600 (NB-1).
pub fn bind_socket(path: &Path) -> io::Result<UnixListener> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Shared handle: current client stream plus the latest status.
#[derive(Clone, Default)]
pub struct Bridge {
    writer: Arc<Mutex<Option<UnixStream>>>,
    status: Arc<Mutex<BridgeStatus>>,
}

impl Bridge {
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of the helper status.
    pub fn status(&self) -> BridgeStatus {
        self.status.lock().expect("status lock").clone()
    }

    /// Send a command to the connected helper. `Err(NotConnected)` when none is connected.
    pub fn send(&self, cmd: &CoreCommand) -> io::Result<()> {
        let mut guard = self.writer.lock().expect("writer lock");
        match guard.as_mut() {
            Some(stream) => stream.write_all(encode_line(cmd).as_bytes()),
            None => Err(io::Error::from(io::ErrorKind::NotConnected)),
        }
    }

    /// Accept helper connections one at a time, forever. Blocks the calling thread.
    pub fn run<F: FnMut(&NativeEvent, &BridgeStatus)>(
        &self,
        listener: UnixListener,
        mut on_event: F,
    ) {
        for incoming in listener.incoming() {
            match incoming {
                Ok(stream) => {
                    if let Err(e) = self.serve_connection(stream, &mut on_event) {
                        log::warn!("native helper connection ended: {e}");
                    }
                }
                Err(e) => log::warn!("accept failed: {e}"),
            }
        }
    }

    /// Serve one helper connection until it closes. Performs the version handshake first (NB-2).
    pub fn serve_connection<F: FnMut(&NativeEvent, &BridgeStatus)>(
        &self,
        stream: UnixStream,
        on_event: &mut F,
    ) -> io::Result<()> {
        let mut reader = stream.try_clone()?;
        let mut writer = stream;
        let mut decoder = LineDecoder::new();
        let mut buf = [0u8; 8192];
        let mut handshaken = false;

        let result = loop {
            let n = match reader.read(&mut buf) {
                Ok(0) => break Ok(()),
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => break Err(e),
            };
            for frame in decoder.push(&buf[..n]) {
                let Frame::Line(line) = frame else {
                    log::warn!("dropped malformed frame from helper");
                    continue;
                };
                let event = match parse_event(&line) {
                    Ok(ev) => ev,
                    // NB-3: log the error class only, never the line (it may hold typed text).
                    Err(_) => {
                        log::warn!("dropped undecodable message from helper");
                        continue;
                    }
                };
                if !handshaken {
                    match &event {
                        NativeEvent::Hello { version, .. } => {
                            if let Err(reason) = check_version(*version) {
                                let _ = writer.write_all(
                                    encode_line(&CoreCommand::Reject { reason }).as_bytes(),
                                );
                                return Ok(());
                            }
                            writer.write_all(
                                encode_line(&CoreCommand::HelloAck {
                                    version: PROTOCOL_VERSION,
                                })
                                .as_bytes(),
                            )?;
                            *self.writer.lock().expect("writer lock") = Some(writer.try_clone()?);
                            self.status.lock().expect("status lock").connected = true;
                            handshaken = true;
                        }
                        // Anything before `hello` is a protocol violation: ignore it.
                        _ => continue,
                    }
                } else if let NativeEvent::Permissions {
                    accessibility,
                    microphone,
                    screen_recording,
                } = &event
                {
                    let mut st = self.status.lock().expect("status lock");
                    st.accessibility = Some(*accessibility);
                    st.microphone = Some(*microphone);
                    st.screen_recording = Some(*screen_recording);
                }
                if matches!(event, NativeEvent::Unknown) {
                    continue;
                }
                let snapshot = self.status();
                on_event(&event, &snapshot);
            }
        };

        // Connection closed: clear the writer and mark disconnected.
        *self.writer.lock().expect("writer lock") = None;
        let mut st = self.status.lock().expect("status lock");
        st.connected = false;
        drop(st);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{parse_command, Rect};
    use std::io::{BufRead, BufReader};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    type Harness = (
        UnixStream,
        Bridge,
        mpsc::Receiver<(NativeEvent, BridgeStatus)>,
        thread::JoinHandle<io::Result<()>>,
    );

    /// Spawn `serve_connection` on one end of a socket pair. Returns the client end, the bridge and
    /// a receiver of (event, status-at-that-time).
    fn start() -> Harness {
        let (server_end, client_end) = UnixStream::pair().unwrap();
        client_end
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let bridge = Bridge::new();
        let (tx, rx) = mpsc::channel();
        let b = bridge.clone();
        let handle = thread::spawn(move || {
            b.serve_connection(server_end, &mut |ev, st| {
                let _ = tx.send((ev.clone(), st.clone()));
            })
        });
        (client_end, bridge, rx, handle)
    }

    fn read_command(reader: &mut BufReader<UnixStream>) -> CoreCommand {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        parse_command(line.trim_end()).unwrap()
    }

    #[test]
    fn handshake_succeeds_and_events_flow() {
        let (mut client, bridge, rx, handle) = start();
        let mut reader = BufReader::new(client.try_clone().unwrap());

        client
            .write_all(
                encode_line(&NativeEvent::Hello {
                    version: PROTOCOL_VERSION,
                    pid: 7,
                })
                .as_bytes(),
            )
            .unwrap();
        assert_eq!(
            read_command(&mut reader),
            CoreCommand::HelloAck {
                version: PROTOCOL_VERSION
            }
        );

        client
            .write_all(
                encode_line(&NativeEvent::Permissions {
                    accessibility: true,
                    microphone: MicStatus::Denied,
                    screen_recording: false,
                })
                .as_bytes(),
            )
            .unwrap();
        // First callback is the Hello, second the Permissions with updated status.
        let (first, _) = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(first, NativeEvent::Hello { .. }));
        let (second, status) = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(second, NativeEvent::Permissions { .. }));
        assert_eq!(
            status,
            BridgeStatus {
                connected: true,
                accessibility: Some(true),
                microphone: Some(MicStatus::Denied),
                screen_recording: Some(false),
            }
        );
        assert_eq!(bridge.status(), status);

        // Core -> helper commands reach the client.
        bridge
            .send(&CoreCommand::ShowChip {
                text: " hi".into(),
                anchor: Rect {
                    x: 1.0,
                    y: 2.0,
                    width: 3.0,
                    height: 4.0,
                },
            })
            .unwrap();
        assert!(matches!(
            read_command(&mut reader),
            CoreCommand::ShowChip { .. }
        ));

        // Closing the client marks the bridge disconnected (NB-5 support).
        drop(reader);
        drop(client);
        handle.join().unwrap().unwrap();
        assert!(!bridge.status().connected);
        assert!(bridge.send(&CoreCommand::HideChip).is_err());
    }

    #[test]
    fn version_mismatch_is_rejected_and_never_connects() {
        let (mut client, bridge, rx, handle) = start();
        let mut reader = BufReader::new(client.try_clone().unwrap());
        client
            .write_all(
                encode_line(&NativeEvent::Hello {
                    version: PROTOCOL_VERSION + 1,
                    pid: 1,
                })
                .as_bytes(),
            )
            .unwrap();
        match read_command(&mut reader) {
            CoreCommand::Reject { reason } => assert!(reason.contains("mismatch")),
            other => panic!("expected reject, got {other:?}"),
        }
        handle.join().unwrap().unwrap();
        assert!(!bridge.status().connected);
        assert!(
            rx.try_recv().is_err(),
            "no event may be delivered after a rejected hello"
        );
    }

    #[test]
    fn events_before_hello_are_ignored() {
        let (mut client, bridge, rx, handle) = start();
        client
            .write_all(
                encode_line(&NativeEvent::Error {
                    message: "early".into(),
                })
                .as_bytes(),
            )
            .unwrap();
        client
            .write_all(
                encode_line(&NativeEvent::Permissions {
                    accessibility: true,
                    microphone: MicStatus::Granted,
                    screen_recording: true,
                })
                .as_bytes(),
            )
            .unwrap();
        drop(client);
        handle.join().unwrap().unwrap();
        assert!(rx.try_recv().is_err());
        assert_eq!(bridge.status(), BridgeStatus::default());
    }

    #[test]
    fn garbage_and_unknown_messages_do_not_break_the_session() {
        let (mut client, _bridge, rx, handle) = start();
        let mut reader = BufReader::new(client.try_clone().unwrap());
        client
            .write_all(
                encode_line(&NativeEvent::Hello {
                    version: PROTOCOL_VERSION,
                    pid: 1,
                })
                .as_bytes(),
            )
            .unwrap();
        let _ = read_command(&mut reader);
        client
            .write_all(b"this is not json\n{\"type\":\"future\"}\n")
            .unwrap();
        client
            .write_all(
                encode_line(&NativeEvent::Error {
                    message: "still alive".into(),
                })
                .as_bytes(),
            )
            .unwrap();
        let (hello, _) = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(hello, NativeEvent::Hello { .. }));
        let (err, _) = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(err, NativeEvent::Error { .. }));
        drop(reader);
        drop(client);
        handle.join().unwrap().unwrap();
    }

    #[test]
    fn socket_is_private_to_the_user() {
        let dir = std::env::temp_dir().join(format!("bob-test-{}", std::process::id()));
        let path = dir.join("native.sock");
        let _listener = bind_socket(&path).unwrap();
        let dir_mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        let sock_mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o700);
        assert_eq!(sock_mode, 0o600);
        // A stale socket file is replaced, not an error.
        drop(_listener);
        let _again = bind_socket(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_path_is_inside_a_per_user_directory() {
        let p = default_socket_path();
        assert!(p.ends_with("native.sock"));
        assert!(p
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("bob-"));
    }
}
