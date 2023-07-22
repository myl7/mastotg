// Copyright (C) myl7
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use chrono::{DateTime, Utc};
use rss::{Channel, Item};

use crate::post::Post;

pub trait Producer {
    fn fetch_posts(&self) -> anyhow::Result<(Option<String>, Vec<Post>)>;
}

pub struct MastodonProducer {
    rss_url: String,
}

impl MastodonProducer {
    pub fn new(rss_url: String) -> Self {
        Self { rss_url }
    }
}

impl Producer for MastodonProducer {
    fn fetch_posts(&self) -> anyhow::Result<(Option<String>, Vec<Post>)> {
        let res = reqwest::blocking::get(&self.rss_url)?;
        if !res.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to request Mastodon RSS: {} {}",
                res.status(),
                res.text()?
            ));
        }
        let body = res.text()?;
        let chan = Channel::from_str(&body)?;
        let last_build_date = chan.last_build_date().map(|s| {
            DateTime::parse_from_rfc2822(s)
                .unwrap()
                .with_timezone(&Utc)
                .to_rfc3339()
        });
        let items = chan
            .items()
            .iter()
            .map(|item| Post::try_from(item))
            .collect::<Result<Vec<_>, _>>()?;
        Ok((last_build_date, items))
    }
}

impl TryFrom<&Item> for Post {
    type Error = anyhow::Error;

    fn try_from(item: &Item) -> Result<Self, Self::Error> {
        // TODO: Download media
        Ok(Self {
            id: item
                .guid()
                .ok_or(anyhow::anyhow!("No GUID in the item"))?
                .value
                .clone(),
            body: item
                .description()
                .ok_or(anyhow::anyhow!("No description in the item"))?
                .to_owned(),
            media: None,
            link: item.link().map(|s| s.to_owned()),
        })
    }
}
