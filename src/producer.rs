// Copyright (C) myl7
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use chrono::{DateTime, Utc};
use regex::Regex;
use rss::extension::Extension;
use rss::{Channel, Item};

use crate::post::{Fid, Media, Post};

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
    pub fn parse_media(items: &[&Extension]) -> anyhow::Result<Media> {
        if items.is_empty() {
            return Err(anyhow::anyhow!("Empty media list. Wired."));
        }
        if items.len() == 1 {
            let url = items[0]
                .attrs()
                .get("url")
                .ok_or(anyhow::anyhow!("No URL in the media item"))?;
            match items[0].attrs().get("medium").map(|s| s.as_str()) {
                Some("image") => Ok(Media::Photos(vec![Fid {
                    value: fid_value_from_url(url)?,
                    link: Some(url.to_owned()),
                }])),
                Some("video") => Ok(Media::Video(Fid {
                    value: fid_value_from_url(url)?,
                    link: Some(url.to_owned()),
                })),
                Some("audio") => Ok(Media::Audio(Fid {
                    value: fid_value_from_url(url)?,
                    link: Some(url.to_owned()),
                })),
                Some(t) => Err(anyhow::anyhow!("Unsupported media type: {}", t)),
                None => Err(anyhow::anyhow!("No medium to indicate the media type")),
            }
        } else {
            let fids = items
                .iter()
                .map(|item| {
                    let t = item
                        .attrs()
                        .get("medium")
                        .ok_or(anyhow::anyhow!("No medium to indicate the media type"))?;
                    if t == "image" {
                        let url = item
                            .attrs()
                            .get("url")
                            .ok_or(anyhow::anyhow!("No URL in the media item"))?;
                        Ok(Fid {
                            value: fid_value_from_url(url)?,
                            link: Some(url.to_owned()),
                        })
                    } else {
                        Err(anyhow::anyhow!(
                            "Unsupported media type for multiple media files: {}",
                            t
                        ))
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Media::Photos(fids))
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
            .map(|items| Self::parse_media(&items.iter().collect::<Vec<_>>()))
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

fn fid_value_from_url(url: &str) -> anyhow::Result<String> {
    let name = Regex::new(r"^[^/:]+?://[^/]+?/system/media_attachments/files/")
        .unwrap()
        .replace(url, "");
    if name.len() == url.len() {
        return Err(anyhow::anyhow!("URL unable to handle: {}", url));
    }
    Ok(name.replace('/', "_"))
}
