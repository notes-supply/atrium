//! `--wiki-folder`: Atrium starts a Mneme of its own as a child process, reads the wiki
//! through it, and stops with it. The child here is a stand-in, a shell script that says it
//! listens where a stub of Mneme's example responses does.

use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use atrium_core::Viewer;
use atrium_wiki::stub::{self, Running, Stub};
use clap::Parser;
use tempfile::TempDir;

use crate::process::{Binary, children, signal, stops};

/// A wiki folder, and a stand-in for `mneme` beside it.
struct Local {
    dir: TempDir,
    wiki: PathBuf,
    mneme: PathBuf,
}

impl Local {
    /// A stand-in that writes its arguments to `args`, one a line, then runs `then`.
    fn new(then: &str) -> Self {
        let dir = TempDir::new().unwrap();
        let wiki = dir.path().join("wiki");
        fs::create_dir(&wiki).unwrap();
        let mneme = dir.path().join("mneme");
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n{then}\n",
            dir.path().join("args").display()
        );
        fs::write(&mneme, script).unwrap();
        fs::set_permissions(&mneme, Permissions::from_mode(0o755)).unwrap();
        Self { dir, wiki, mneme }
    }

    /// Atrium for the folder, with `args` added.
    fn atrium(&self, args: &[&str]) -> Binary {
        let fixed = [
            "--wiki-folder",
            path(&self.wiki),
            "--mneme-bin",
            path(&self.mneme),
            "--addr",
            "127.0.0.1:0",
        ];
        Binary::start(&[&fixed[..], args].concat(), &[])
    }

    /// The arguments the stand-in was started with, if it was.
    fn args(&self) -> Option<Vec<String>> {
        let args = fs::read_to_string(self.dir.path().join("args")).ok()?;
        Some(args.lines().map(str::to_owned).collect())
    }
}

fn path(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// What a Mneme says once its index is built, as listening where `mneme` does, and then it
/// runs until it is killed.
fn ready(mneme: &Running) -> String {
    let addr = mneme.base.strip_prefix("http://").unwrap();
    format!(
        r#"echo '{{"time":1,"event":"indexed","pages":5}}' >&2
echo '{{"time":2,"event":"listening","addr":"{addr}"}}' >&2
exec sleep 600"#
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn atrium_reads_the_folder_through_the_mneme_it_starts_and_stops_it() {
    let mneme = stub::serve(Stub::Fixtures).await;
    let local = Local::new(&ready(&mneme));
    let mut atrium = local.atrium(&[]);

    let (said, before) = atrium.until("listening");
    assert_eq!(
        said["addr"].as_str(),
        mneme.base.strip_prefix("http://"),
        "Mneme's own line, passed on"
    );
    assert!(
        before
            .iter()
            .any(|line| line.contains(r#""event":"indexed""#)),
        "every line Mneme writes is passed on: {before:?}"
    );
    let addr = atrium.listening();
    assert_eq!(
        local.args().unwrap(),
        [
            "serve",
            "--folder",
            path(&local.wiki),
            "--addr",
            "127.0.0.1:0",
            "--exit-with-parent"
        ]
    );

    let wiki = reqwest::get(format!("http://{addr}/wiki")).await.unwrap();
    assert_eq!(wiki.status(), 200, "a member sees the wiki");
    assert!(
        mneme
            .requests()
            .iter()
            .any(|asked| asked.starts_with("/v1/pages?")),
        "the pages come from the address Mneme named: {:?}",
        mneme.requests()
    );

    let started = children(atrium.pid());
    assert_eq!(started.len(), 1, "{started:?}");
    atrium.terminate();
    let rest = atrium.rest();
    assert!(atrium.exit().success(), "exits 0 on SIGTERM: {rest:#?}");
    assert!(
        rest.last()
            .is_some_and(|line| line.contains(r#""event":"stopped""#)),
        "{rest:#?}"
    );
    assert!(stops(started[0]), "its Mneme stops with it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_mneme_that_exits_before_it_is_ready_stops_atrium_with_its_last_line() {
    let local = Local::new(
        r#"echo '{"time":1,"event":"indexing"}' >&2
echo 'mneme: no Git repository in the folder' >&2
exit 3"#,
    );
    let mut atrium = local.atrium(&[]);
    let said = atrium.rest();
    assert!(!atrium.exit().success());
    assert!(
        said.contains(&r#"{"time":1,"event":"indexing"}"#.to_owned()),
        "{said:#?}"
    );
    let error = said.join("\n");
    assert!(
        error.contains(
            "Mneme exited before it was ready (exit status: 3): \
             mneme: no Git repository in the folder"
        ),
        "{error}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn atrium_stops_with_an_error_when_its_mneme_dies() {
    let mneme = stub::serve(Stub::Fixtures).await;
    let local = Local::new(&ready(&mneme));
    let mut atrium = local.atrium(&[]);
    let addr = atrium.listening();
    assert_eq!(
        reqwest::get(format!("http://{addr}/"))
            .await
            .unwrap()
            .status(),
        200
    );

    signal("KILL", children(atrium.pid())[0]);
    let said = atrium.rest().join("\n");
    assert!(
        !atrium.exit().success(),
        "so that a supervisor restarts both"
    );
    assert!(
        said.contains("Mneme stopped while Atrium ran (signal: 9"),
        "{said}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stop_while_mneme_builds_its_index_stops_both() {
    let local = Local::new(
        r#"echo '{"time":1,"event":"indexing"}' >&2
exec sleep 600"#,
    );
    let mut atrium = local.atrium(&[]);
    assert!(atrium.line().unwrap().contains("indexing"));
    let started = children(atrium.pid());
    assert_eq!(started.len(), 1, "{started:?}");
    atrium.terminate();
    assert!(atrium.exit().success());
    assert!(stops(started[0]), "its Mneme stops with it");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wiki_folder_is_served_on_loopback_only() {
    let local = Local::new("exit 0");
    let mut atrium = Binary::start(
        &["--mneme-bin", path(&local.mneme), "--addr", "0.0.0.0:0"],
        &[("ATRIUM_WIKI_FOLDER", path(&local.wiki))],
    );
    let said = atrium.rest().join("\n");
    assert!(!atrium.exit().success());
    assert!(
        said.contains("so --addr must be a loopback address such as 127.0.0.1:8080, not 0.0.0.0:0"),
        "{said}"
    );
    assert_eq!(local.args(), None, "Mneme was never started");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_mneme_says_where_to_get_it() {
    let local = Local::new("exit 0");
    let empty = TempDir::new().unwrap();
    let mut atrium = Binary::start(
        &["--wiki-folder", path(&local.wiki), "--addr", "127.0.0.1:0"],
        &[("PATH", path(empty.path()))],
    );
    let said = atrium.rest().join("\n");
    assert!(!atrium.exit().success());
    for part in [
        "mneme is not on PATH",
        "https://github.com/notes-supply/mneme",
        "--mneme-bin",
    ] {
        assert!(said.contains(part), "{part}: {said}");
    }
}

#[test]
fn a_wiki_folder_takes_no_mneme_url_or_demo_and_serves_members() {
    let parse = |args: &[&str]| atrium::Config::try_parse_from([&["atrium"][..], args].concat());
    let local = parse(&["--wiki-folder", "/wiki"]).unwrap();
    assert_eq!(local.viewer, Viewer::Member);
    assert_eq!(local.wiki_folder.as_deref(), Some(Path::new("/wiki")));
    let anonymous = parse(&["--wiki-folder", "/wiki", "--viewer", "anonymous"]).unwrap();
    assert_eq!(anonymous.viewer, Viewer::Anonymous);
    for other in [&["--mneme-url", "http://mneme"][..], &["--demo"]] {
        let refused = parse(&[&["--wiki-folder", "/wiki"][..], other].concat()).unwrap_err();
        assert_eq!(
            refused.kind(),
            clap::error::ErrorKind::ArgumentConflict,
            "{other:?}"
        );
    }
}
