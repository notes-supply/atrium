//! Atrium in this process on a free port, in front of a stand-in for Mneme, and plain HTTP
//! to it.

use atrium_wiki::stub::{self, Running, Stub};
use clap::Parser;
use reqwest::header::HeaderMap;
use reqwest::{Client, redirect};
use tokio::net::TcpListener;

pub struct Atrium {
    pub url: String,
    pub mneme: Running,
    client: Client,
}

pub struct Reply {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: String,
}

impl Reply {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }
}

impl Atrium {
    /// A tailnet listener in front of Mneme's example responses.
    pub async fn tailnet() -> Self {
        Self::start(Stub::Fixtures, &["--viewer", "tailnet"]).await
    }

    /// A listener with `args` added to the flags, in front of `mneme`.
    pub async fn start(mneme: Stub, args: &[&str]) -> Self {
        let mneme = stub::serve(mneme).await;
        Self::serve(mneme, args)
    }

    /// A listener with `args` added to the flags, in front of a running Mneme.
    pub fn serve(mneme: Running, args: &[&str]) -> Self {
        let fixed = [
            "atrium",
            "--addr",
            "127.0.0.1:0",
            "--allowed-hosts",
            "127.0.0.1",
            "--mneme-timeout-ms",
            "1000",
            "--mneme-url",
            &mneme.base,
        ];
        let config = atrium::Config::try_parse_from([&fixed[..], args].concat()).unwrap();
        let app = atrium::app(&config).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let listener = TcpListener::from_std(listener).unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = Client::builder()
            .redirect(redirect::Policy::none())
            .build()
            .unwrap();
        Self { url, mneme, client }
    }

    pub async fn get(&self, path: &str) -> Reply {
        self.get_with(path, &[]).await
    }

    pub async fn get_with(&self, path: &str, headers: &[(&str, &str)]) -> Reply {
        let mut request = self.client.get(format!("{}{path}", self.url));
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let response = request.send().await.unwrap();
        Reply {
            status: response.status().as_u16(),
            headers: response.headers().clone(),
            body: response.text().await.unwrap(),
        }
    }
}
