//! Atrium in front of a real `mneme serve`, with a synthetic wiki pushed through the stock
//! `git` command. Ignored in the gate, which needs no Mneme built; `mise run test:mneme` runs
//! it with the binary named by `ATRIUM_MNEME_BIN`.

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
use crate::support::Atrium;

const TOKEN: &str = "test-token-of-blackbox";

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
        r#"{{ "blackbox": "{}", "aeolus": "{}" }}"#,
        hash(TOKEN),
        hash("test-token-of-aeolus")
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
    let report = reqwest::Client::new()
        .post(format!("{}/git/status", mneme.base))
        .bearer_auth(TOKEN)
        .header("content-type", "application/json")
        .body(include_str!("../../../atrium-wiki/fixtures/report.json"))
        .send()
        .await
        .unwrap();
    assert_eq!(report.status(), 204);
    let atrium = Atrium::serve(Running::elsewhere(&mneme.base), &["--viewer", "tailnet"]).await;
    let setup = Setup {
        _dirs: [home, server, clone],
        _mneme: mneme,
    };
    (atrium, setup)
}

#[tokio::test]
#[ignore = "needs a mneme binary in ATRIUM_MNEME_BIN: mise run test:mneme"]
async fn atrium_reads_a_wiki_pushed_to_a_real_mneme() {
    let binary = std::env::var("ATRIUM_MNEME_BIN")
        .unwrap_or_else(|_| panic!("set ATRIUM_MNEME_BIN to a mneme binary"));
    let (atrium, _setup) = setup(&binary).await;

    let list = atrium.get("/wiki").await;
    assert_eq!(list.status, 200);
    assert_eq!(texts(&list.body, "tbody th"), ["Alpha", "Deep", "Home"]);
    let alpha = attrs(&list.body, "tbody a", "href")[0].clone();

    let index = atrium.get("/wiki/index.md").await;
    assert_eq!(index.status, 303);
    let home_page = atrium.get(index.header("location").unwrap()).await;
    assert_eq!(texts(&home_page.body, "h1"), ["Home"]);
    let links = attrs(&home_page.body, "article a:not(.anchor)", "href");
    assert_eq!(links, ["/wiki/greek-alpha.md", "/wiki/notes/deep.md"]);
    let followed = atrium.get(&links[0]).await;
    assert_eq!(
        followed.header("location"),
        Some(alpha.as_str()),
        "the renamed page"
    );

    let page = atrium.get(&alpha).await;
    assert_eq!(page.status, 200);
    assert_eq!(texts(&page.body, "h1"), ["Alpha"]);
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
    let text = texts(&page.body, "article p");
    assert!(
        text.contains(&"A Do not click link.".to_owned()),
        "the link's text stays as text: {text:?}"
    );
    assert_eq!(
        rows(&page.body, "#activity + table tbody tr")[0][..2],
        ["the last 7 days", "3"]
    );

    let history = atrium.get(&format!("{alpha}/history")).await;
    assert_eq!(history.status, 200);
    let events = texts(&history.body, "main ol li");
    assert_eq!(events.len(), 4);
    assert!(events[1].contains("renamed from alpha.md"), "{}", events[1]);

    let diff = atrium.get(&format!("{alpha}/diff")).await;
    assert_eq!(diff.status, 200);
    assert_eq!(texts(&diff.body, "ol.diff mark"), ["quiet"]);

    let search = atrium.get("/wiki/search?q=harbour*").await;
    assert_eq!(search.status, 200);
    assert_eq!(texts(&search.body, "main ol li > a"), ["Alpha"]);
    assert_eq!(texts(&search.body, "main ol li mark"), ["harbours"]);

    let machines = atrium.get("/wiki/machines").await;
    assert_eq!(machines.status, 200);
    let table = rows(&machines.body, "tbody tr");
    assert_eq!(table[0][..2], ["aeolus", "not reported"]);
    assert_eq!(
        table[1][..3],
        [
            "blackbox",
            "blocked: a merge is in progress",
            "no (faf510a)"
        ]
    );

    let changes = atrium.get("/wiki/changes").await;
    assert_eq!(texts(&changes.body, "main h2").len(), 4, "four days");

    let home_tile = atrium.get("/").await;
    assert_eq!(home_tile.status, 200);
}
