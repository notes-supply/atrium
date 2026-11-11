use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    atrium::run(atrium::Config::parse()).await
}
