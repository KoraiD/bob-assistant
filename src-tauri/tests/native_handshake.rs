//! End-to-end check of the Rust <-> Swift protocol: the real `bob-native` binary connects to a real
//! core bridge, completes the handshake and reports permissions (requirements NB-1, NB-2, NB-6).
//!
//! Needs the helper built first (`make native` / `swift build` in native/bob-native). When the binary
//! is missing the test is skipped with a message, so `cargo test` still works on a fresh checkout;
//! CI builds the helper before running it.

use bob_lib::native_ipc::{bind_socket, Bridge};
use bob_lib::protocol::{CoreCommand, NativeEvent};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn helper_binary() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    ["debug", "release"]
        .iter()
        .map(|p| {
            root.join("native/bob-native/.build")
                .join(p)
                .join("bob-native")
        })
        .find(|p| p.is_file())
}

struct KillOnDrop(Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn swift_helper_handshakes_and_reports_permissions() {
    let Some(helper) = helper_binary() else {
        eprintln!("SKIPPED: build the helper first (cd native/bob-native && swift build)");
        return;
    };

    let dir = std::env::temp_dir().join(format!("bob-e2e-{}", std::process::id()));
    let socket = dir.join("n.sock");
    let listener = bind_socket(&socket).expect("bind socket");

    let bridge = Bridge::new();
    let (tx, rx) = mpsc::channel::<NativeEvent>();
    let serve = bridge.clone();
    thread::spawn(move || {
        serve.run(listener, |event, _| {
            let _ = tx.send(event.clone());
        })
    });

    let child = Command::new(helper)
        .env("BOB_SOCKET", &socket)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn helper");
    let mut child = KillOnDrop(child);

    // 1. hello with the matching protocol version
    match rx.recv_timeout(Duration::from_secs(15)).expect("hello") {
        NativeEvent::Hello { version, pid } => {
            assert_eq!(version, bob_lib::protocol::PROTOCOL_VERSION);
            assert_eq!(pid, child.0.id());
        }
        other => panic!("expected hello, got {other:?}"),
    }
    // 2. permissions report after the ack (values depend on the machine, so only the shape is checked)
    match rx
        .recv_timeout(Duration::from_secs(15))
        .expect("permissions")
    {
        NativeEvent::Permissions { .. } => {}
        other => panic!("expected permissions, got {other:?}"),
    }
    let status = bridge.status();
    assert!(status.connected);
    assert!(status.accessibility.is_some() && status.microphone.is_some());

    // 3. commands reach the helper without breaking the session
    bridge.send(&CoreCommand::HideChip).expect("send hide_chip");

    // 4. quit makes the helper exit on its own
    bridge.send(&CoreCommand::Quit).expect("send quit");
    let mut exited = false;
    for _ in 0..100 {
        if child.0.try_wait().expect("try_wait").is_some() {
            exited = true;
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    assert!(exited, "helper did not exit after quit");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn helper_exits_cleanly_when_the_core_goes_away() {
    let Some(helper) = helper_binary() else {
        eprintln!("SKIPPED: build the helper first (cd native/bob-native && swift build)");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bob-e2e2-{}", std::process::id()));
    let socket = dir.join("n.sock");
    let listener = bind_socket(&socket).expect("bind socket");

    let child = Command::new(helper)
        .env("BOB_SOCKET", &socket)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn helper");
    let mut child = KillOnDrop(child);

    // Accept the helper, wait for its hello line, then close the connection like a dying core.
    let (stream, _) = listener.accept().expect("accept helper");
    let mut reader = std::io::BufReader::new(stream);
    let mut hello = String::new();
    std::io::BufRead::read_line(&mut reader, &mut hello).expect("read hello");
    assert!(hello.contains("\"type\":\"hello\""));
    drop(reader);

    // The helper must notice EOF and exit with status 0 on its own (NB-5).
    let mut status = None;
    for _ in 0..100 {
        if let Some(s) = child.0.try_wait().expect("try_wait") {
            status = Some(s);
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let status = status.expect("helper did not exit after the core closed the socket");
    assert!(status.success(), "helper exited with {status}");
    let _ = std::fs::remove_dir_all(&dir);
}
