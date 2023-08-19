// Copyright (C) myl7
// SPDX-License-Identifier: Apache-2.0

//! CLI definitions with its cleaning

use anyhow::{anyhow, Result};
use clap::{Parser, ValueEnum};
use regex::Regex;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Where to get the ActivityPub outbox JSON
    #[clap(short, long)]
    pub input: Option<CliInput>,
    /// According to `--input`, outbox JSON URL or web domain of the server,
    /// e.g., `social.myl.moe/users/myl/outbox` or `mastodon.social`.
    /// The protocol head default to `https://`.
    #[clap(short = 's', long)]
    pub host: Option<String>,
    /// Webfinger account URI of the user to be fetched,
    /// e.g., `myl@myl.moe` or `myl`.
    /// The leading `@` is optional.
    /// The domain default to the value of `--host` without the protocol head.
    #[clap(short = 'u', long)]
    pub acct: Option<String>,
    /// Where to output the parsed posts
    #[clap(short, long)]
    pub output: Option<CliOutput>,
    /// Telegram channel ID to send to, e.g., @myl7s.
    /// The leading `@` is optional.
    #[clap(long)]
    pub tg_chan: Option<String>,
    /// Path to the JSON file to persist states.
    /// If not specified, do not write to a file.
    /// Then users should use the log to trace the states and pass them manually.
    #[clap(short, long)]
    pub file: Option<String>,
    /// Use builtin loop runner to run the program every fixed interval. Unit: Seconds.
    #[clap(long)]
    pub loop_interval: Option<u64>,
    /// Minimum integer ID of the posts to fetch. The newer posts have larger IDs.
    /// If not specified or set to < 0, ignore all previous posts.
    /// If set to 0, fetch all existing posts.
    /// The stdin input is not affected.
    /// This overrides the states given by `--file`.
    #[clap(short, long)]
    pub min_id: Option<i64>,
    /// The program follows the paging link `prev` to fetch more pending posts.
    /// Set this flag to disable the behavior.
    #[clap(long)]
    pub no_follow_paging: bool,
    // TODO: Post command
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum CliInput {
    /// From the stdin (default)
    Stdin,
    /// Fetch from the outbox JSON URL
    Fetch,
    /// Get the outbox JSON URL from the WebFinger API and then fetch it
    QueryFetch,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum CliOutput {
    /// Print to stdout (default)
    Print,
    /// Send to the Telegram channel
    TgSend,
}

impl Cli {
    pub fn clean(&mut self) -> Result<()> {
        self.tg_chan = self.tg_chan.as_ref().map(|s| {
            if !s.starts_with('@') {
                format!("@{}", s)
            } else {
                s.to_owned()
            }
        });

        self.host = self.host.as_ref().map(|s| match self.input {
            Some(CliInput::Fetch) | Some(CliInput::QueryFetch) => {
                if !s.starts_with("https://") && !s.starts_with("http://") {
                    format!("https://{}", s)
                } else {
                    s.to_owned()
                }
            }
            _ => s.to_owned(),
        });

        self.acct = self.acct.as_ref().map(|s| {
            let s = s.strip_prefix('@').unwrap_or(s);
            if !s.contains('@') {
                let re_proto = Regex::new(r"^[^:/]+://").unwrap();
                let h = re_proto.replace(self.host.as_ref().unwrap(), "");
                s.to_owned() + "@" + &h[..h.find('/').unwrap_or(h.len())]
            } else {
                s.to_owned()
            }
        });

        match self.input.as_ref() {
            Some(CliInput::Fetch) => {
                self.host
                    .as_ref()
                    .ok_or(anyhow!("option host is required when input=fetch"))?;
            }
            Some(CliInput::QueryFetch) => {
                let err = || anyhow!("options host and acct are required when input=query-fetch");
                self.host.as_ref().ok_or(err())?;
                self.acct.as_ref().ok_or(err())?;
            }
            _ => (),
        }

        Ok(())
    }
}
