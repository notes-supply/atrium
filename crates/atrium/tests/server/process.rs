//! The real binary: it logs JSON lines and stops cleanly on SIGTERM.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde_json::Value;

#[tokio::test]
async fn binary_logs_each_request_and_stops_on_sigterm() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_atrium"))
        .args([
            "--addr",
            "127.0.0.1:0",
            "--viewer",
            "tailnet",
            "--mneme-url",
            "http://127.0.0.1:9",
        ])
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stderr = BufReader::new(child.stderr.take().unwrap());
    let (lines, received) = mpsc::channel();
    thread::spawn(move || {
        for line in stderr.lines().map_while(Result::ok) {
            let event: Value = serde_json::from_str(&line)
                .unwrap_or_else(|error| panic!("not a JSON line ({error}): {line}"));
            if lines.send(event).is_err() {
                break;
            }
        }
    });
    let next = || received.recv_timeout(Duration::from_secs(30)).unwrap();

    let listening = next();
    assert_eq!(listening["event"], "listening");
    let addr = listening["addr"].as_str().unwrap().to_owned();

    let reply = reqwest::get(format!("http://{addr}/healthz?q=secret"))
        .await
        .unwrap();
    assert_eq!(reply.status(), 200);
    let request = next();
    assert_eq!(request["event"], "request");
    assert_eq!(request["path"], "/healthz", "the query is left out");
    assert_eq!(request["status"], 200);

    let killed = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    assert_eq!(next()["event"], "stopped");
    assert!(child.wait().unwrap().success(), "exits 0 on SIGTERM");
}
