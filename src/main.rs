// Copyright (C) myl7
// SPDX-License-Identifier: Apache-2.0

use std::thread;
use std::time::Duration;

use clap::Parser;
use rusqlite::Connection;

mod db;
mod post;
mod producer;

use post::FsRepo;
use producer::{MastodonProducer, Producer};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let conn = Connection::open(&cli.db_path)?;
    conn.execute_batch(SQL_INIT_TABLES)?;

    if let Some(interval) = cli.loop_interval {
        loop {
            run(&cli, &conn)?;
            thread::sleep(Duration::from_secs(interval));
        }
    }

    run(&cli, &conn)?;
    Ok(())
}

fn run(cli: &Cli, conn: &Connection) -> anyhow::Result<()> {
    let producer = MastodonProducer::new(cli.rss_url.clone());
    let (last_build_date, posts) = producer.fetch_posts()?;
    let posts = db::dedup_posts(conn, &last_build_date, posts)?;
    let repo = FsRepo::new(cli.media_dir.clone().into());

    // TODO: Send the posts
    println!("last_build_date: {}", last_build_date);
    println!("posts: {:?}", posts);

    db::save_posts(conn, &last_build_date, &posts.iter().collect::<Vec<_>>())?;
    Ok(())
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
    /// Path to the SQLite database file to persist states
    #[clap(long, default_value = "mastotg.sqlite")]
    db_path: String,
    /// Dir to store media files
    #[clap(long, default_value = "mastotg/media")]
    media_dir: String,
    /// Keep media files after sending
    // With this we will use the `media` table in the database.
    #[clap(long)]
    keep_media: bool,
    /// Use builtin loop runner to run the program every fixed interval. Unit: seconds.
    #[clap(long)]
    loop_interval: Option<u64>,
}

const SQL_INIT_TABLES: &str = r#"
CREATE TABLE IF NOT EXISTS posts (
    id TEXT PRIMARY KEY,
    body TEXT NOT NULL,
    link TEXT NOT NULL,
    created_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_build_date TEXT NOT NULL,
    FOREIGN KEY(last_build_date) REFERENCES last_build_dates(value)
);
CREATE TABLE IF NOT EXISTS media (
    fid TEXT PRIMARY KEY,
    type TEXT NOT NULL,
    post_id TEXT NOT NULL,
    FOREIGN KEY(post_id) REFERENCES posts(id)
);
CREATE TABLE IF NOT EXISTS last_build_dates (
    value TEXT PRIMARY KEY
);
"#;
