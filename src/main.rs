// Copyright (C) myl7
// SPDX-License-Identifier: Apache-2.0

mod as2;
mod cli;
mod cons;
mod db;
mod pro;
mod query;
mod utils;

use anyhow::Result;
use clap::Parser;
use reqwest::Url;
use rusqlite::Connection;
use tokio::time::{self, Duration};

use crate::as2::Page;
use crate::cli::{Cli, CliInput, CliOutput};
use crate::cons::{Con, TgCon};
use crate::db::{migration, DbConn, State};
use crate::pro::{Pro, UriPro};
use crate::query::query_outbox_url;
use crate::utils::int_id;

fn main() -> Result<()> {
    env_logger::init();

    let mut cli = Cli::parse();
    cli.clean()?;

    let mut conn = Connection::open(&cli.db_file)?;
    init_db(&mut conn)?;
    let db = DbConn::new(conn);

    let ctx = Ctx { cli, db };
    run(&ctx)?;
    Ok(())
}

struct Ctx {
    cli: Cli,
    db: DbConn,
}

#[tokio::main]
async fn run(ctx: &Ctx) -> Result<()> {
    let cli = &ctx.cli;
    let db = &ctx.db;

    let init_state = if cli.min_id >= 0 {
        State::new(cli.min_id)
    } else {
        db.load_state()
            .await?
            .map(|s| {
                log::debug!("Loaded state min_id {} from the database", s.min_id);
                s
            })
            .unwrap_or_else(|| {
                log::debug!("No state loaded from the database");
                State::default()
            })
    };

    let mut state = init_state;
    loop {
        state = run_round(ctx, state).await?;
        db.save_state(state.clone()).await?;

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
    // Use the mode to get the `min_id` that ignores all previous posts.
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
            let min_id_query = if !ff_latest {
                Some(("min_id", min_id.to_string()))
            } else {
                None
            };
            let max_id_query = ctx.cli.max_id.map(|id| ("max_id", id.to_string()));
            let mut u = Url::parse(&base_url)?;
            {
                let mut q = u.query_pairs_mut();
                if let Some((k, v)) = min_id_query {
                    q.append_pair(k, &v);
                }
                if let Some((k, v)) = max_id_query {
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
        if post_len == 0 {
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

        if ctx.cli.no_follow_paging {
            break;
        }
    }

    log::info!("Finished running a round with min_id {next_min_id}");
    Ok(State {
        min_id: next_min_id,
    })
}

fn init_db(conn: &mut Connection) -> Result<()> {
    let report = migration::migrations::runner().run(conn)?;
    let migs = report.applied_migrations();
    if !migs.is_empty() {
        let s = migs
            .iter()
            .map(|m| format!("{m}"))
            .collect::<Vec<_>>()
            .join(", ");
        log::info!("Applied migrations: {s}");
    } else {
        log::debug!("No migrations applied");
    }
    Ok(())
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
            let con = TgCon::new(ctx.cli.tg_chan.clone().unwrap(), ctx.db.clone());
            let id_map = con.send_page(page).await?;
            ctx.db.save_id_map(id_map).await?;
            log::info!("Sent {post_len} posts to the Telegram channel");
        }
    }
    Ok(())
}
