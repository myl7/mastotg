// Copyright (C) myl7
// SPDX-License-Identifier: Apache-2.0

mod as2;
mod cli;
mod con;
mod pro;
mod query;
mod utils;

use anyhow::Result;
use clap::Parser;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::time::{self, Duration};

use crate::as2::Page;
use crate::cli::{Cli, CliInput, CliOutput};
use crate::con::{Con, TgCon};
use crate::pro::{Pro, UriPro};
use crate::query::query_outbox_url;
use crate::utils::int_id;

fn main() -> Result<()> {
    env_logger::init();

    let mut cli = Cli::parse();
    cli.clean()?;

    let ctx = Ctx { cli };
    run(&ctx)?;
    Ok(())
}

struct Ctx {
    cli: Cli,
}

#[tokio::main]
async fn run(ctx: &Ctx) -> Result<()> {
    let cli = &ctx.cli;
    let init_state = match ctx.cli.file.as_ref() {
        None => State::default(),
        Some(path) => load_state(path).await.unwrap_or(State::default()),
    };

    let mut state = init_state;
    loop {
        state = run_round(ctx, state).await?;
        if let Some(path) = cli.file.as_ref() {
            save_state(path, &state).await?;
        }

        if let Some(interval) = cli.loop_interval {
            time::sleep(Duration::from_secs(interval)).await;
        } else {
            break;
        }
    }
    Ok(())
}

async fn run_round(ctx: &Ctx, state: State) -> Result<State> {
    log::debug!("Starts to run a round");

    let min_id = state.min_id;
    // Whether to fast forward to the latest post without sending.
    // Use the mode go get the min_id that ignores all previous posts.
    let ff_latest = min_id < 0;
    let uri = match ctx.cli.input.as_ref() {
        None | Some(CliInput::Stdin) => r"stdio://in".to_owned(),
        input => {
            let base_url = match input {
                Some(CliInput::Fetch) => ctx.cli.host.as_ref().unwrap().to_owned(),
                Some(CliInput::QueryFetch) => {
                    let host = ctx.cli.host.as_ref().unwrap();
                    let acct = ctx.cli.acct.as_ref().unwrap();
                    query_outbox_url(host, acct).await?
                }
                _ => unreachable!(),
            };
            let id_range_query = if !ff_latest {
                Some(("min_id", min_id.to_string()))
            } else {
                None
            };
            let mut u = Url::parse(&base_url)?;
            {
                let mut q = u.query_pairs_mut();
                if let Some((k, v)) = id_range_query {
                    q.append_pair(k, &v);
                }
                q.append_pair("page", "true");
            }
            let url = u.to_string();
            log::debug!("The page is at {url}");
            url
        }
    };

    let mut pro = UriPro::new(uri);
    let mut next_min_id = min_id;
    loop {
        let page = pro.fetch().await?;
        let post_len = page.ordered_items.len();
        if post_len == 0 || ctx.cli.no_follow_paging {
            break;
        }

        if ff_latest {
            next_min_id = int_id(page.ordered_items.first().unwrap().id.as_ref())?;
            log::info!("Ignore from the latest min_id {next_min_id}");
            break;
        }

        log::info!("Fetched {post_len} posts from the page");
        let iid = int_id(page.ordered_items.first().unwrap().id.as_ref())?;
        consume(ctx, page).await?;
        next_min_id = iid;
    }

    log::info!("Finished running a round with min_id {next_min_id}");
    Ok(State {
        min_id: next_min_id,
    })
}

async fn consume(ctx: &Ctx, page: Page) -> Result<()> {
    match ctx.cli.output.as_ref() {
        None | Some(CliOutput::Print) => {
            page.ordered_items.iter().try_for_each(|post| {
                println!("{}", serde_json::to_string_pretty(post)?);
                anyhow::Ok(())
            })?;
        }
        Some(CliOutput::TgSend) => {
            let post_len = page.ordered_items.len();
            let con = TgCon::new(ctx.cli.tg_chan.clone().unwrap());
            con.send_page(page).await?;
            log::info!("Sent {post_len} posts to the Telegram channel");
        }
    }
    Ok(())
}

async fn load_state(path: &str) -> Result<State> {
    let buf = fs::read(path).await?;
    let state: State = serde_json::from_slice(&buf)?;
    Ok(state)
}

async fn save_state(path: &str, state: &State) -> Result<()> {
    let buf = serde_json::to_vec(state)?;
    fs::write(path, buf).await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct State {
    #[serde(default = "min_id_default")]
    min_id: i64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            min_id: min_id_default(),
        }
    }
}

fn min_id_default() -> i64 {
    -1
}
