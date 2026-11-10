//! One process per user: the first instance owns a socket in the cache dir, a second launch
//! forwards its flags (`scrme-gpui --prefs`) and exits. No crate, no polling — the listener
//! thread just blocks on accept, so it costs a stack and nothing else.

use std::io::{BufRead, BufReader, Write};

use crate::action::Action;

/// True when this process should keep running. When another instance already owns the
/// socket, the CLI flags are forwarded to it and this returns false.
#[cfg(unix)]
pub(crate) fn claim(tx: &async_channel::Sender<Action>) -> bool {
    claim_unix(tx, &std::env::args().skip(1).collect::<Vec<_>>())
}

/// Windows: single-instance lands with the engine swap (phase 4).
#[cfg(not(unix))]
pub(crate) fn claim(_tx: &async_channel::Sender<Action>) -> bool {
    true
}

#[cfg(unix)]
fn claim_unix(tx: &async_channel::Sender<Action>, args: &[String]) -> bool {
    use std::os::unix::net::{UnixListener, UnixStream};

    let Some(path) = crate::paths::cache_dir().map(|d| d.join("scrme.sock")) else {
        return true;
    };
    if let Ok(mut stream) = UnixStream::connect(&path) {
        // Hand the flags over, then let the caller exit: the running app acts on them.
        let _ = stream.write_all(args.join("\n").as_bytes());
        let _ = stream.flush();
        return false;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::remove_file(&path); // a socket left behind by a crash
    let Ok(listener) = UnixListener::bind(&path) else {
        return true;
    };
    let tx = tx.clone();
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let lines = BufReader::new(conn).lines().map_while(Result::ok);
            for line in lines {
                if let Some(action) = Action::from_flag(line.trim()) {
                    let _ = tx.send_blocking(action);
                }
            }
        }
    });
    true
}
