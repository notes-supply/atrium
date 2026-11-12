//! A Mneme of Atrium's own for `--wiki-folder`: `mneme serve --folder` as a child process,
//! which reads the folder's Git history on this machine.
//!
//! The child logs JSON lines on standard error. It builds its index first, then writes one
//! line with `"event":"listening"` and the `"addr"` it bound, which is how Atrium learns its
//! port. Every line it writes is passed on to Atrium's own standard error.

use std::io::ErrorKind;
use std::net::SocketAddr;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStderr, Command};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// How long Mneme may take to build its index and say where it listens.
const READY: Duration = Duration::from_secs(120);

/// How long a Mneme that has closed its standard error may take to exit, and its last lines
/// to arrive once it has.
const SETTLE: Duration = Duration::from_secs(2);

/// Where to get Mneme.
const GET_MNEME: &str = "https://github.com/notes-supply/mneme";

/// A running `mneme serve --folder`. Dropping it kills the process.
pub struct Mneme {
    child: Child,
    url: String,
    last: Last,
    forwarding: JoinHandle<()>,
}

/// The latest line the child wrote on standard error.
type Last = Arc<Mutex<String>>;

impl Mneme {
    /// Starts `binary serve --folder folder` on a free loopback port, and waits until it says
    /// where it listens.
    ///
    /// # Errors
    ///
    /// Fails when the folder is not one, the binary cannot be started, or it exits or stays
    /// silent before it is ready, with the last line it wrote.
    pub async fn start(binary: &Path, folder: &Path) -> Result<Self> {
        if !folder.is_dir() {
            bail!("--wiki-folder {} is not a folder", folder.display());
        }
        let mut child = Command::new(binary)
            .arg("serve")
            .arg("--folder")
            .arg(folder)
            .args(["--addr", "127.0.0.1:0", "--exit-with-parent"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| not_started(binary, &error))?;
        let stderr = child
            .stderr
            .take()
            .context("reading Mneme's standard error")?;
        let last = Last::default();
        let (ready, listening) = oneshot::channel();
        let forwarding = tokio::spawn(forward(stderr, Arc::clone(&last), ready));
        let mut mneme = Self {
            child,
            url: String::new(),
            last,
            forwarding,
        };
        let ready = tokio::time::timeout(READY, async {
            tokio::select! {
                addr = listening => addr.ok(),
                _ = mneme.child.wait() => None,
            }
        })
        .await;
        match ready {
            Ok(Some(addr)) => {
                mneme.url = format!("http://{addr}");
                Ok(mneme)
            }
            Ok(None) => Err(mneme.ended("exited before it was ready").await),
            Err(_) => {
                let _ = mneme.child.kill().await;
                Err(anyhow!(
                    "{} said nothing of where it listens within {} s{}",
                    binary.display(),
                    READY.as_secs(),
                    mneme.said()
                ))
            }
        }
    }

    /// Its base URL, such as `http://127.0.0.1:41234`.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Waits until the process stops, and says how, with the last line it wrote.
    pub async fn stopped(&mut self) -> anyhow::Error {
        let _ = self.child.wait().await;
        self.ended("stopped while Atrium ran").await
    }

    /// Kills the process and waits for it to go.
    pub async fn stop(mut self) {
        let _ = self.child.kill().await;
    }

    /// Why a process that has stopped, or is stopping, stopped.
    async fn ended(&mut self, what: &str) -> anyhow::Error {
        let status = match tokio::time::timeout(SETTLE, self.child.wait()).await {
            Ok(Ok(status)) => status.to_string(),
            Ok(Err(error)) => error.to_string(),
            Err(_) => {
                // It closed its standard error but did not exit.
                let _ = self.child.kill().await;
                "killed".to_owned()
            }
        };
        let _ = tokio::time::timeout(SETTLE, &mut self.forwarding).await;
        anyhow!("Mneme {what} ({status}){}", self.said())
    }

    /// The last line the process wrote, as the end of a message.
    fn said(&self) -> String {
        let last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        if last.is_empty() {
            String::new()
        } else {
            format!(": {last}")
        }
    }
}

/// Why a binary did not start. A missing one says how to get Mneme.
fn not_started(binary: &Path, error: &std::io::Error) -> anyhow::Error {
    if error.kind() != ErrorKind::NotFound {
        return anyhow!("starting {}: {error}", binary.display());
    }
    let missing = if binary.components().count() > 1 {
        format!("there is no {}", binary.display())
    } else {
        format!("{} is not on PATH", binary.display())
    };
    anyhow!(
        "--wiki-folder runs Mneme, but {missing}. Install Mneme from {GET_MNEME}, \
         or name its binary with --mneme-bin or ATRIUM_MNEME_BIN."
    )
}

/// Passes every line of `stderr` on to Atrium's own, keeps the latest in `last`, and sends
/// the address of the first `listening` line to `ready`.
async fn forward(stderr: ChildStderr, last: Last, ready: oneshot::Sender<SocketAddr>) {
    let mut ready = Some(ready);
    let mut lines = BufReader::new(stderr).split(b'\n');
    while let Ok(Some(line)) = lines.next_segment().await {
        let line = String::from_utf8_lossy(&line);
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        eprintln!("{line}");
        if let Some(addr) = listening(line)
            && let Some(ready) = ready.take()
        {
            let _ = ready.send(addr);
        }
        line.clone_into(&mut last.lock().unwrap_or_else(PoisonError::into_inner));
    }
}

/// The address a `listening` line names.
fn listening(line: &str) -> Option<SocketAddr> {
    let event: Value = serde_json::from_str(line).ok()?;
    if event["event"] != "listening" {
        return None;
    }
    event["addr"].as_str()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_listening_line_names_the_port() {
        assert_eq!(
            listening(r#"{"time":1,"event":"listening","addr":"127.0.0.1:41234"}"#),
            Some(SocketAddr::from(([127, 0, 0, 1], 41234)))
        );
        assert_eq!(listening(r#"{"event":"index","addr":"127.0.0.1:1"}"#), None);
        assert_eq!(listening("listening on 127.0.0.1:1"), None);
    }

    #[test]
    fn a_missing_mneme_names_the_flag_and_where_to_get_it() {
        let missing = std::io::Error::from(ErrorKind::NotFound);
        let on_path = not_started(Path::new("mneme"), &missing).to_string();
        assert!(on_path.contains("mneme is not on PATH"), "{on_path}");
        assert!(on_path.contains("--mneme-bin"), "{on_path}");
        assert!(on_path.contains(GET_MNEME), "{on_path}");
        let named = not_started(Path::new("/opt/mneme"), &missing).to_string();
        assert!(named.contains("there is no /opt/mneme"), "{named}");
    }
}
