//! The real binary: it logs JSON lines, stops cleanly on SIGTERM, and reads times in the
//! system's zone unless told another.

use std::ffi::OsStr;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::html::attrs;

/// How long the binary may take to say anything.
const PATIENCE: Duration = Duration::from_secs(30);

/// The binary as a child process, with its standard error read line by line.
pub struct Binary {
    child: Child,
    lines: mpsc::Receiver<String>,
}

impl Binary {
    /// Starts the binary with `args` and `env` added to the environment.
    pub fn start<A: AsRef<OsStr>>(args: &[A], env: &[(&str, &str)]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_atrium"))
            .args(args)
            .envs(env.iter().copied())
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stderr = BufReader::new(child.stderr.take().unwrap());
        let (lines, received) = mpsc::channel();
        thread::spawn(move || {
            for line in stderr.lines().map_while(Result::ok) {
                if lines.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            lines: received,
        }
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// The next line, or `None` once the binary has closed its standard error.
    pub fn line(&self) -> Option<String> {
        match self.lines.recv_timeout(PATIENCE) {
            Ok(line) => Some(line),
            Err(mpsc::RecvTimeoutError::Disconnected) => None,
            Err(mpsc::RecvTimeoutError::Timeout) => panic!("the binary said nothing for 30 s"),
        }
    }

    /// The next line, which must be a JSON object.
    pub fn next(&self) -> Value {
        let line = self
            .line()
            .unwrap_or_else(|| panic!("the binary closed its standard error"));
        serde_json::from_str(&line)
            .unwrap_or_else(|error| panic!("not a JSON line ({error}): {line}"))
    }

    /// Every line until the next with `"event": event`, which it returns, with the lines
    /// before it.
    pub fn until(&self, event: &str) -> (Value, Vec<String>) {
        let mut before = Vec::new();
        loop {
            let line = self
                .line()
                .unwrap_or_else(|| panic!("no {event:?} line before the end: {before:#?}"));
            if let Ok(found) = serde_json::from_str::<Value>(&line)
                && found["event"] == event
            {
                return (found, before);
            }
            before.push(line);
        }
    }

    /// The address of Atrium's own listener, from its `listening` line, which names the
    /// viewer.
    pub fn listening(&self) -> String {
        loop {
            let (line, _) = self.until("listening");
            if line.get("viewer").is_some() {
                return line["addr"].as_str().unwrap().to_owned();
            }
        }
    }

    pub fn terminate(&self) {
        signal("TERM", self.pid());
    }

    /// Its exit status, waiting at most 30 s for it.
    pub fn exit(&mut self) -> ExitStatus {
        let deadline = Instant::now() + PATIENCE;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < deadline, "the binary did not exit");
            thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Binary {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Sends `SIG<name>` to `pid` with the stock `kill` command.
pub fn signal(name: &str, pid: u32) {
    let sent = Command::new("kill")
        .args([&format!("-{name}"), &pid.to_string()])
        .status()
        .unwrap();
    assert!(sent.success(), "kill -{name} {pid}");
}

#[tokio::test]
async fn binary_logs_each_request_and_stops_on_sigterm() {
    let mut atrium = Binary::start(
        &[
            "--addr",
            "127.0.0.1:0",
            "--viewer",
            "tailnet",
            "--mneme-url",
            "http://127.0.0.1:9",
        ],
        &[],
    );
    let listening = atrium.next();
    assert_eq!(listening["event"], "listening");
    let addr = listening["addr"].as_str().unwrap().to_owned();

    let reply = reqwest::get(format!("http://{addr}/healthz?q=secret"))
        .await
        .unwrap();
    assert_eq!(reply.status(), 200);
    let request = atrium.next();
    assert_eq!(request["event"], "request");
    assert_eq!(request["path"], "/healthz", "the query is left out");
    assert_eq!(request["status"], 200);

    atrium.terminate();
    assert_eq!(atrium.next()["event"], "stopped");
    assert!(atrium.exit().success(), "exits 0 on SIGTERM");
}

/// The masthead clock's time on a page of a binary started with `env` and `args`.
async fn clock(env: &[(&str, &str)], args: &[&str]) -> String {
    let fixed = [
        "--addr",
        "127.0.0.1:0",
        "--viewer",
        "tailnet",
        "--allowed-hosts",
        "127.0.0.1",
        "--mneme-url",
        "http://127.0.0.1:9",
    ];
    let atrium = Binary::start(&[&fixed[..], args].concat(), env);
    let addr = atrium.listening();
    let page = reqwest::get(format!("http://{addr}/no-such-page"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    attrs(&page, "time.clock", "datetime").remove(0)
}

#[tokio::test]
async fn times_are_read_in_the_systems_zone_unless_another_is_named() {
    let tokyo = clock(&[("TZ", "Asia/Tokyo")], &[]).await;
    assert!(tokyo.ends_with("+09:00"), "{tokyo}");
    let utc = clock(&[("TZ", "")], &[]).await;
    assert!(utc.ends_with("+00:00"), "an empty TZ is UTC: {utc}");
    let named = clock(&[("TZ", "Asia/Tokyo")], &["--time-zone", "Asia/Kolkata"]).await;
    assert!(named.ends_with("+05:30"), "{named}");
}
