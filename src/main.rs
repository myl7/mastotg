// Copyright (C) myl7
// SPDX-License-Identifier: Apache-2.0

mod post;
mod producer;

use std::thread;
use std::time::Duration;

use clap::Parser;

use crate::producer::{MastodonProducer, Producer};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if let Some(interval) = cli.loop_interval {
        loop {
            run(&cli)?;
            thread::sleep(Duration::from_secs(interval));
        }
    }

    run(&cli)?;
    Ok(())
}

fn run(cli: &Cli) -> anyhow::Result<()> {
    let producer = MastodonProducer::new(cli.rss_url.clone());
    let posts = producer.fetch_posts()?;
    todo!()
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// URL to the Mastodon public user RSS, e.g., https://social.myl.moe/@myl.rss
    #[clap(long)]
    rss_url: String,
    /// Telegram bot token
    #[clap(long, env)]
    bot_token: String,
    /// Use builtin loop runner to run the program every fixed interval. Unit: seconds.
    #[clap(long)]
    loop_interval: Option<u64>,
}
