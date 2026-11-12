//! Atrium in front of a real `mneme serve`, with a synthetic wiki pushed through the stock
//! `git` command, and in front of the `mneme serve --folder` it starts for a wiki folder.
//! Ignored in the gate, which needs no Mneme built; `mise run test:mneme` runs them with the
//! binary named by `ATRIUM_MNEME_BIN`, or `mneme` on `PATH`.

use std::fmt::Write as _;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use atrium_wiki::stub::Running;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use crate::html::{attrs, rows, texts};
use crate::process::{Binary, children, stops};
use crate::support::Atrium;

const TOKEN: &str = "test-token-of-ironbark";

/// Empty configuration for every `git` and `mneme` the test runs. The machine's global
/// gitignore would otherwise hide files such as `AGENTS.md`.
fn isolated<'a>(command: &'a mut Command, home: &Path) -> &'a mut Command {
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("xdg"))
        .env("GIT_CONFIG_GLOBAL", home.join("gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "Writer")
        .env("GIT_AUTHOR_EMAIL", "writer@example.invalid")
        .env("GIT_COMMITTER_NAME", "Writer")
        .env("GIT_COMMITTER_EMAIL", "writer@example.invalid")
}

/// The Mneme binary to test against.
fn mneme_binary() -> String {
    std::env::var("ATRIUM_MNEME_BIN").unwrap_or_else(|_| "mneme".to_owned())
}

struct Mneme {
    child: Child,
    base: String,
}

impl Drop for Mneme {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start_mneme(binary: &str, dir: &Path) -> Mneme {
    let hash = |token: &str| {
        Sha256::digest(token.as_bytes())
            .iter()
            .fold(String::new(), |mut out, byte| {
                let _ = write!(out, "{byte:02x}");
                out
            })
    };
    let tokens = dir.join("tokens.json");
    let machines = format!(
        r#"{{ "ironbark": "{}", "willow": "{}" }}"#,
        hash(TOKEN),
        hash("test-token-of-willow")
    );
    fs::write(&tokens, machines).unwrap();
    let mut command = Command::new(binary);
    command
        .args(["serve", "--addr", "127.0.0.1:0", "--tokens"])
        .arg(&tokens)
        .arg("--repo")
        .arg(dir.join("wiki.git"))
        .stderr(Stdio::piped());
    let mut child = isolated(&mut command, dir).spawn().unwrap();
    let stderr = BufReader::new(child.stderr.take().unwrap());
    let (ready, listening) = mpsc::channel();
    thread::spawn(move || {
        for line in stderr.lines().map_while(Result::ok) {
            if let Ok(event) = serde_json::from_str::<Value>(&line)
                && event["event"] == "listening"
            {
                let _ = ready.send(event["addr"].as_str().unwrap().to_owned());
            }
        }
    });
    let addr = listening.recv_timeout(Duration::from_secs(60)).unwrap();
    Mneme {
        child,
        base: format!("http://{addr}"),
    }
}

struct Work<'a> {
    dir: &'a Path,
    home: &'a Path,
}

impl Work<'_> {
    fn git(&self, args: &[&str], time: i64) -> String {
        let date = format!("@{time} +0000");
        let mut command = Command::new("git");
        command
            .current_dir(self.dir)
            .args(args)
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date);
        let output = isolated(&mut command, self.home).output().unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn write(&self, path: &str, text: &str) {
        let file = self.dir.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }

    fn commit(&self, message: &str, time: i64) {
        self.git(&["add", "-A"], time);
        self.git(&["commit", "-q", "-m", message], time);
    }
}

/// A small wiki over four days that end an hour ago: three pages, an edit, a rename that
/// keeps the links working, and an edit that tries raw HTML and a `javascript:` link.
fn write_wiki(work: &Work, now: i64) {
    let day = 86_400;
    let start = now - 4 * day;
    work.git(&["init", "-q", "-b", "main"], start);
    work.write(
        "index.md",
        "# Home\n\nStart at [Alpha](alpha.md) or [the deep notes](notes/deep.md).\n",
    );
    work.write(
        "alpha.md",
        "# Alpha\n\nThe first page talks about boats and harbours.\n",
    );
    work.write(
        "notes/deep.md",
        "# Deep\n\nBack [home](../index.md). Nothing about ships.\n",
    );
    work.commit("Start the wiki", start);
    work.write(
        "alpha.md",
        "# Alpha\n\nThe first page talks about boats and harbours.\n\nNow with a second line.\n",
    );
    work.commit("Add a line", start + day);
    work.git(&["mv", "alpha.md", "greek-alpha.md"], start + 2 * day);
    work.write(
        "index.md",
        "# Home\n\nStart at [Alpha](greek-alpha.md) or [the deep notes](notes/deep.md).\n",
    );
    work.commit("Rename alpha", start + 2 * day);
    work.write(
        "greek-alpha.md",
        "# Alpha\n\nThe first page talks about boats and quiet harbours.\n\nNow with a second line.\n\n\
         <script>alert(1)</script>\n\nA [Do not click](javascript:alert(1)) link.\n",
    );
    work.commit("Try a script", now - 3600);
}

async fn until_indexed(base: &str, head: &str) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let body = reqwest::get(format!("{base}/v1/status"))
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        let status: Value = serde_json::from_str(&body).unwrap();
        if status["head"] == head {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Mneme never indexed {head}: {status}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Standard base64 with padding, for the report header.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0_u32, |n, (i, byte)| n | u32::from(*byte) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[(n >> (18 - 6 * i) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The directories and the server the test needs to keep while it runs.
struct Setup {
    _dirs: [TempDir; 3],
    _mneme: Mneme,
}

/// Starts Mneme, pushes the wiki to it, reports a blocked machine, and serves Atrium in
/// front of it.
async fn setup(binary: &str) -> (Atrium, Setup) {
    let home = TempDir::new().unwrap();
    fs::create_dir(home.path().join("xdg")).unwrap();
    fs::write(home.path().join("gitconfig"), "").unwrap();
    let server = TempDir::new().unwrap();
    let mneme = start_mneme(binary, server.path());
    let clone = TempDir::new().unwrap();
    let work = Work {
        dir: clone.path(),
        home: home.path(),
    };
    let now = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap();
    write_wiki(&work, now);
    let header = format!("http.extraHeader=Authorization: Bearer {TOKEN}");
    let remote = format!("{}/git/wiki.git", mneme.base);
    work.git(&["-c", &header, "push", "-q", &remote, "main"], now);
    let head = work.git(&["rev-parse", "HEAD"], now);
    until_indexed(&mneme.base, head.trim()).await;
    // A report rides on a Git request in a header, as the autosave service sends it.
    let report = base64(include_str!("../../../atrium-wiki/fixtures/report.json").as_bytes());
    let carried = format!("http.extraHeader=Mneme-Report: {report}");
    work.git(&["-c", &header, "-c", &carried, "ls-remote", &remote], now);
    let atrium = Atrium::serve(Running::elsewhere(&mneme.base), &["--viewer", "member"]).await;
    let setup = Setup {
        _dirs: [home, server, clone],
        _mneme: mneme,
    };
    (atrium, setup)
}

#[tokio::test]
#[ignore = "needs a mneme binary, in ATRIUM_MNEME_BIN or on PATH: mise run test:mneme"]
#[expect(
    clippy::too_many_lines,
    reason = "one walk through every route, which its output reads as"
)]
async fn atrium_reads_a_wiki_pushed_to_a_real_mneme() {
    let (atrium, _setup) = setup(&mneme_binary()).await;

    let say = |what: &str, lines: Vec<String>| {
        println!("── {what}");
        for line in lines {
            println!("   {line}");
        }
    };

    let home = atrium.get("/").await;
    assert_eq!(home.status, 200);
    let sentence = texts(&home.body, "p.sentence");
    say("GET / sentence", sentence.clone());
    assert!(sentence[0].contains("ironbark is blocked"), "{sentence:?}");
    say("GET / places", texts(&home.body, ".dock-card .dk-state"));
    assert_eq!(
        attrs(&home.body, ".dock-card", "data-want"),
        ["2", "0", "2"]
    );
    say(
        "GET / recent changes",
        texts(&home.body, ".tile.changes li"),
    );
    say("GET / this week", texts(&home.body, ".week-note"));

    let list = atrium.get("/wiki").await;
    assert_eq!(list.status, 200);
    say("GET /wiki", texts(&list.body, "tbody tr"));
    let mut titles = texts(&list.body, "tbody a.ttl");
    titles.sort();
    assert_eq!(titles, ["Alpha", "Deep", "Home"]);
    let alpha = attrs(&list.body, "tbody a.ttl", "href")
        .into_iter()
        .zip(texts(&list.body, "tbody a.ttl"))
        .find(|(_, title)| title == "Alpha")
        .unwrap()
        .0;

    let index = atrium.get("/wiki/index.md").await;
    assert_eq!(index.status, 303);
    let home_page = atrium.get(index.header("location").unwrap()).await;
    assert_eq!(texts(&home_page.body, "h1"), ["Home"]);
    let links = attrs(&home_page.body, "article.paper > p a", "href");
    assert_eq!(links, ["/wiki/greek-alpha.md", "/wiki/notes/deep.md"]);
    let followed = atrium.get(&links[0]).await;
    assert_eq!(
        followed.header("location"),
        Some(alpha.as_str()),
        "the renamed page"
    );
    let old = atrium.get("/wiki/alpha.md").await;
    say(
        "GET /wiki/alpha.md, renamed away",
        vec![format!(
            "{} {}",
            old.status,
            old.header("location").unwrap_or_default()
        )],
    );
    assert_eq!(
        old.header("location"),
        Some(alpha.as_str()),
        "the path before the rename"
    );

    let page = atrium.get(&alpha).await;
    assert_eq!(page.status, 200);
    assert_eq!(texts(&page.body, "h1"), ["Alpha"]);
    say(
        "GET the page",
        texts(&page.body, ".doc-meta, article.paper > p"),
    );
    assert!(
        !page.body.contains("<script>alert"),
        "raw HTML is not rendered"
    );
    assert!(
        !attrs(&page.body, "a", "href")
            .iter()
            .any(|href| href.contains("javascript")),
        "the javascript: link is not a link"
    );
    let text = texts(&page.body, "article.paper p");
    assert!(
        text.contains(&"A Do not click link.".to_owned()),
        "the link's text stays as text: {text:?}"
    );
    assert_eq!(
        rows(&page.body, "table.windows tbody tr")[0][..2],
        ["7 days", "3"]
    );

    let history = atrium.get(&format!("{alpha}/history")).await;
    assert_eq!(history.status, 200);
    let events = rows(&history.body, "tbody tr");
    say(
        "GET history",
        events.iter().map(|row| row.join(" | ")).collect(),
    );
    assert_eq!(events.len(), 4);
    assert_eq!(events[1][2], "renamedfrom alpha");

    let diff = atrium.get(&format!("{alpha}/diff")).await;
    assert_eq!(diff.status, 200);
    say("GET diff", texts(&diff.body, "#diff-h, .diff-body p"));
    let added = texts(&diff.body, ".diff-body ins");
    assert_eq!(added[0], "quiet", "the changed word, marked");
    assert!(
        added.contains(&"<script>alert(1)</script>".to_owned()),
        "added raw HTML reads as text"
    );

    let search = atrium.get("/wiki/search?q=harbour*").await;
    assert_eq!(search.status, 200);
    say(
        "GET search harbour*",
        texts(&search.body, "#find-count, .result-list h3, .snip"),
    );
    assert_eq!(texts(&search.body, ".result-list h3"), ["Alpha"]);
    assert_eq!(texts(&search.body, ".result-list mark"), ["harbours"]);

    let machines = atrium.get("/wiki/machines").await;
    assert_eq!(machines.status, 200);
    say(
        "GET /wiki/machines",
        texts(
            &machines.body,
            "section.mach .mach-head h2, section.mach .word, section.mach .why, .skips li",
        ),
    );
    assert_eq!(
        texts(&machines.body, "#ironbark .why"),
        ["a merge is in progress"]
    );
    assert_eq!(
        texts(&machines.body, "#ironbark .skips code"),
        ["notes/scan.pdf"]
    );

    let changes = atrium.get("/wiki/changes").await;
    say("GET /wiki/changes", texts(&changes.body, "h2.day"));
    assert_eq!(texts(&changes.body, "h2.day").len(), 4, "four days");

    // The offset is past every event, so only Mneme's `before` can fill the page.
    let days = attrs(&changes.body, "h2.day time", "datetime");
    let before: jiff::Timestamp = days[1].parse().unwrap();
    let older = atrium
        .get(&format!(
            "/wiki/changes?before={}&offset=999",
            before.as_second()
        ))
        .await;
    let shown = attrs(&older.body, "h2.day time", "datetime");
    say(
        "GET /wiki/changes?before=, the days older than the second",
        shown.clone(),
    );
    assert_ne!(shown, Vec::<String>::new());
    assert!(
        shown.iter().all(|day| day.as_str() < days[1].as_str()),
        "{shown:?}"
    );
}

/// The titles the wiki's list shows, sorted.
async fn titles(base: &str) -> Vec<String> {
    let list = reqwest::get(format!("{base}/wiki"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let mut titles = texts(&list, "tbody a.ttl");
    titles.sort();
    titles
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a mneme binary with serve --folder, in ATRIUM_MNEME_BIN or on PATH: mise run test:mneme"]
async fn atrium_reads_a_wiki_folder_through_a_real_mneme_serve_folder() {
    let binary = mneme_binary();
    let help = Command::new(&binary)
        .args(["serve", "--help"])
        .output()
        .unwrap_or_else(|error| panic!("{binary}: {error}"));
    assert!(
        String::from_utf8_lossy(&help.stdout).contains("--folder"),
        "{binary} has no `serve --folder`, which --wiki-folder runs: name a Mneme that has it \
         in ATRIUM_MNEME_BIN"
    );
    let home = TempDir::new().unwrap();
    fs::create_dir(home.path().join("xdg")).unwrap();
    fs::write(home.path().join("gitconfig"), "").unwrap();
    let folder = TempDir::new().unwrap();
    let work = Work {
        dir: folder.path(),
        home: home.path(),
    };
    let now = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap();
    write_wiki(&work, now);

    let path = |path: &Path| path.to_str().unwrap().to_owned();
    let (xdg, gitconfig) = (
        path(&home.path().join("xdg")),
        path(&home.path().join("gitconfig")),
    );
    let env = [
        ("HOME", path(home.path())),
        ("XDG_CONFIG_HOME", xdg),
        ("GIT_CONFIG_GLOBAL", gitconfig),
        ("GIT_CONFIG_NOSYSTEM", "1".to_owned()),
    ];
    let env: Vec<(&str, &str)> = env
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    let wiki = path(folder.path());
    let mut atrium = Binary::start(
        &[
            "--wiki-folder",
            &wiki,
            "--mneme-bin",
            &binary,
            "--addr",
            "127.0.0.1:0",
        ],
        &env,
    );
    let base = format!("http://{}", atrium.listening());
    assert_eq!(titles(&base).await, ["Alpha", "Deep", "Home"]);
    let search = reqwest::get(format!("{base}/wiki/search?q=harbour*"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(texts(&search, ".result-list h3"), ["Alpha"]);
    let machines = reqwest::get(format!("{base}/wiki/machines")).await.unwrap();
    assert_eq!(machines.status(), 200);

    // A commit on main after Atrium started shows up without a restart.
    work.write("gamma.md", "# Gamma\n\nWritten while Atrium ran.\n");
    work.commit("Add gamma", now);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !titles(&base).await.contains(&"Gamma".to_owned()) {
        assert!(Instant::now() < deadline, "the new page never showed");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    let mneme = children(atrium.pid());
    assert_eq!(mneme.len(), 1, "{mneme:?}");
    atrium.terminate();
    assert!(atrium.exit().success(), "exits 0 on SIGTERM");
    assert!(stops(mneme[0]), "its Mneme stops with it");
}
