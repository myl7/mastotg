// Copyright (C) myl7
// SPDX-License-Identifier: Apache-2.0

//! Post produers

use std::io::{self, BufReader};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use regex::Regex;
use tokio::task;

use crate::as2::{CheckContext, CheckType, Page};
use crate::utils::check_res;

/// Producer trait
#[async_trait]
pub trait Pro {
    /// Fetch a page of posts.
    /// Returns a page with no posts to indicate no more posts currently.
    async fn fetch(&mut self) -> Result<Page>;
}

/// URI producer.
/// Make HTTP requests for `http(s)://`.
/// Read the stdin for `stdio://in`.
pub struct UriPro {
    uri: String,
}

impl UriPro {
    pub fn new(uri: String) -> Self {
        Self { uri }
    }
}

impl UriPro {
    async fn fetch_http(url: &str) -> Result<Page> {
        let page: Page = check_res(reqwest::get(url).await?).await?.json().await?;
        Ok(page)
    }

    async fn fetch_stdin() -> Result<Page> {
        task::spawn_blocking(move || {
            let r = BufReader::new(io::stdin());
            let page: Page = serde_json::from_reader(r)?;
            Ok(page)
        })
        .await?
    }
}

#[async_trait]
impl Pro for UriPro {
    async fn fetch(&mut self) -> Result<Page> {
        let re = Regex::new(r"^[^:/]+?(?:://)").unwrap();
        let proto = re.find(&self.uri).map(|m| m.as_str());
        let err = || anyhow!("invalid uri {}", self.uri);
        let page = match proto {
            Some("http://") | Some("https://") => Self::fetch_http(&self.uri).await,
            Some("stdio://") => {
                if self.uri == "stdio://in" {
                    Self::fetch_stdin().await
                } else {
                    Err(err())
                }
            }
            _ => Err(err()),
        }?;

        page.check_context()?;
        page.check_type()?;
        page.ordered_items.iter().try_for_each(|item| {
            item.check_type()?;
            let post = &item.object;
            post.check_type()?;
            post.attachment
                .iter()
                .try_for_each(|att| att.check_type())?;
            post.tag.iter().try_for_each(|tag| tag.check_type())?;
            anyhow::Ok(())
        })?;

        if let Some(next_uri) = page.prev.as_ref() {
            self.uri = next_uri.clone()
        }

        Ok(page)
    }
}
