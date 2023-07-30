// Copyright (C) myl7
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use chrono::{DateTime, Utc};
use regex::Regex;
use rss::extension::Extension;
use rss::{Channel, Item};

use crate::post::{Media, MediaItem, MediaLayout, MediaMedium, Post};

pub trait Producer {
    fn fetch_posts(&self) -> anyhow::Result<(String, Vec<Post>)>;
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
    fn fetch_posts(&self) -> anyhow::Result<(String, Vec<Post>)> {
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
        let last_build_date = DateTime::parse_from_rfc2822(
            chan.last_build_date()
                .ok_or(anyhow::anyhow!("No last build date in the channel"))?,
        )
        .unwrap()
        .with_timezone(&Utc)
        .to_rfc3339();
        let items = chan
            .items()
            .iter()
            .rev()
            .map(Post::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok((last_build_date, items))
    }
}

impl Post {
    pub fn parse_media(items: &[Extension]) -> anyhow::Result<Media> {
        anyhow::ensure!(!items.is_empty(), "Empty media list. Wired.");
        if items.len() == 1 {
            let err_ctx = "in the media item 0";
            let url = items[0]
                .attrs()
                .get("url")
                .ok_or(anyhow::anyhow!("No URL {}", err_ctx))?;
            match items[0].attrs().get("medium").map(|s| s.as_str()) {
                Some(s) => Ok(Media {
                    layout: MediaLayout::Single,
                    items: vec![MediaItem {
                        uri: url.to_owned(),
                        medium: MediaMedium::try_from(s)?,
                        link: Some(url.to_owned()),
                    }],
                }),
                None => anyhow::bail!("No medium to indicate the media type {}", err_ctx),
            }
        } else {
            let items = items
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let t = item.attrs().get("medium").ok_or(anyhow::anyhow!(
                        "No medium to indicate the media type in the media item {}",
                        i
                    ))?;
                    anyhow::ensure!(
                        t == "image",
                        "Unsupported media type for multiple media items in the media item {}: {}",
                        i,
                        t,
                    );
                    let url = item
                        .attrs()
                        .get("url")
                        .ok_or(anyhow::anyhow!("No URL in the media item {}", i))?;
                    Ok(MediaItem {
                        uri: url.to_owned(),
                        medium: MediaMedium::Image,
                        link: Some(url.to_owned()),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Media {
                layout: MediaLayout::Grouped,
                items,
            })
        }
    }
}

impl TryFrom<&Item> for Post {
    type Error = anyhow::Error;

    fn try_from(item: &Item) -> Result<Self, Self::Error> {
        let media = item
            .extensions()
            .get("media")
            .and_then(|m| m.get("content"))
            .map(|items| Self::parse_media(items))
            .transpose()?;
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
            media,
            link: item.link().map(|s| s.to_owned()),
        })
    }
}

#[allow(dead_code)]
fn fname_from_url(url: &str) -> anyhow::Result<String> {
    let name = Regex::new(r"^[^/:]+?://[^/]+?/system/media_attachments/files/")
        .unwrap()
        .replace(url, "");
    if name.len() == url.len() {
        return Err(anyhow::anyhow!("URL unable to handle: {}", url));
    }
    Ok(name.replace('/', "_"))
}
