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

    let ctx = Ctx { cli: Box::new(cli) };
    run(&ctx)?;
    Ok(())
}

struct Ctx {
    cli: Box<Cli>,
}

#[tokio::main]
async fn run(ctx: &Ctx) -> Result<()> {
    let cli = &ctx.cli;
    if let Some(interval) = cli.loop_interval {
        let mut min_id = cli.min_id;
        loop {
            min_id = run_round(ctx, min_id).await?;
            time::sleep(Duration::from_secs(interval)).await;
        }
    } else {
        run_round(ctx, cli.min_id).await?;
    }
    Ok(())
}

async fn run_round(ctx: &Ctx, min_id: i64) -> Result<i64> {
    log::debug!("Starts to run a round");

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
            let id_range_query = if min_id >= 0 {
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
        log::info!("Fetched {post_len} posts from the page");
        let iid = int_id(page.ordered_items.first().unwrap().id.as_ref())?;
        consume(ctx, page).await?;
        next_min_id = iid;
    }

    log::info!("Finished running a round with min_id {next_min_id}");
    Ok(next_min_id)
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
