//! Atrium in this process on a free port, and plain HTTP to it.

use clap::Parser;
use reqwest::header::HeaderMap;
use reqwest::{Client, redirect};
use tokio::net::TcpListener;

pub struct Atrium {
    pub url: String,
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
    /// Serves a tailnet listener with `args` added to the flags.
    pub async fn tailnet(args: &[&str]) -> Self {
        Self::start(&[&["--viewer", "tailnet"], args].concat()).await
    }

    /// Serves with exactly `args`, besides the address and the allowed host.
    pub async fn start(args: &[&str]) -> Self {
        let flags = [
            &[
                "atrium",
                "--addr",
                "127.0.0.1:0",
                "--allowed-hosts",
                "127.0.0.1",
            ],
            args,
        ];
        let config = atrium::Config::try_parse_from(flags.concat()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let app = atrium::app(&config);
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = Client::builder()
            .redirect(redirect::Policy::none())
            .build()
            .unwrap();
        Self { url, client }
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
