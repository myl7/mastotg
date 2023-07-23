// Copyright (C) myl7
// SPDX-License-Identifier: Apache-2.0

use reqwest::Url;
use teloxide::prelude::*;
use teloxide::types::{InputFile, InputMedia, InputMediaPhoto};
use tokio::runtime;

use crate::post::{Media, Post, Repo};

pub trait Consumer {
    fn send_post(&self, post: &Post, repo: &impl Repo) -> anyhow::Result<()>;
}

pub struct TelegramConsumer {
    bot: Bot,
    tg_chan: String,
}

impl TelegramConsumer {
    pub fn new(tg_chan: String) -> Self {
        let tg_chan = if tg_chan.starts_with('@') {
            tg_chan
        } else {
            "@".to_owned() + &tg_chan
        };
        Self {
            bot: Bot::from_env(),
            tg_chan,
        }
    }
}

lazy_static::lazy_static! {
    static ref RUNTIME: runtime::Runtime = runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
}

impl Consumer for TelegramConsumer {
    fn send_post(&self, post: &Post, _repo: &impl Repo) -> anyhow::Result<()> {
        // Current implementation does not read files from the repo
        RUNTIME.block_on(self.send_post(post))
    }
}

impl TelegramConsumer {
    async fn send_post(&self, post: &Post) -> anyhow::Result<()> {
        match post.media.as_ref() {
            None => {
                self.bot
                    .send_message(self.tg_chan.clone(), &post.body)
                    .await?;
                Ok(())
            }
            Some(Media::Photos(fids)) => {
                let urls = fids
                    .iter()
                    .map(|fid| {
                        fid.link.as_ref().ok_or(anyhow::anyhow!(
                            "Current implementation requires media to have a URL"
                        ))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                self.bot
                    .send_media_group(
                        self.tg_chan.clone(),
                        urls.iter()
                            .enumerate()
                            .map(|(i, url)| {
                                let photo = InputMediaPhoto::new(InputFile::url(Url::parse(url)?));
                                let photo = if i == 0 {
                                    photo.caption(post.body.clone())
                                } else {
                                    photo
                                };
                                Ok(InputMedia::Photo(photo))
                            })
                            .collect::<anyhow::Result<Vec<_>>>()?,
                    )
                    .await?;
                Ok(())
            }
            Some(Media::Video(fid)) => {
                let url = fid.link.as_ref().ok_or(anyhow::anyhow!(
                    "Current implementation requires media to have a URL"
                ))?;
                self.bot
                    .send_video(self.tg_chan.clone(), InputFile::url(Url::parse(url)?))
                    .caption(post.body.clone())
                    .await?;
                Ok(())
            }
            Some(Media::Audio(fid)) => {
                let url = fid.link.as_ref().ok_or(anyhow::anyhow!(
                    "Current implementation requires media to have a URL"
                ))?;
                self.bot
                    .send_audio(self.tg_chan.clone(), InputFile::url(Url::parse(url)?))
                    .caption(post.body.clone())
                    .await?;
                Ok(())
            }
        }
    }
}
