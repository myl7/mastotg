// Copyright (C) myl7
// SPDX-License-Identifier: Apache-2.0

use reqwest::Url;
use teloxide::prelude::*;
use teloxide::types::{InputFile, InputMedia, InputMediaPhoto, ParseMode};
use tokio::runtime;

use crate::post::{Media, MediaLayout, MediaMedium, Post};

pub trait Consumer {
    fn send_post(&self, post: &Post) -> anyhow::Result<()>;
}

pub struct TelegramConsumer {
    bot: Bot,
    tg_chan: String,
}

impl TelegramConsumer {
    pub fn new(tg_chan: String) -> Self {
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
    fn send_post(&self, post: &Post) -> anyhow::Result<()> {
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
                    .parse_mode(ParseMode::Html)
                    .await?;
                Ok(())
            }
            Some(Media { layout, items }) => {
                match *layout {
                    MediaLayout::Single => {
                        anyhow::ensure!(
                            items.len() == 1,
                            "Single media layout but multiple media items"
                        );
                        let item = &items[0];
                        match item.medium {
                            MediaMedium::Image => {
                                self.bot
                                    .send_photo(
                                        self.tg_chan.clone(),
                                        InputFile::url(Url::parse(&item.uri)?),
                                    )
                                    .caption(post.body.clone())
                                    .parse_mode(ParseMode::Html)
                                    .await?;
                                Ok(())
                            }
                            MediaMedium::Video => {
                                self.bot
                                    .send_video(
                                        self.tg_chan.clone(),
                                        InputFile::url(Url::parse(&item.uri)?),
                                    )
                                    .caption(post.body.clone())
                                    .parse_mode(ParseMode::Html)
                                    .await?;
                                Ok(())
                            }
                            MediaMedium::Audio => {
                                self.bot
                                    .send_audio(
                                        self.tg_chan.clone(),
                                        InputFile::url(Url::parse(&item.uri)?),
                                    )
                                    .caption(post.body.clone())
                                    .parse_mode(ParseMode::Html)
                                    .await?;
                                Ok(())
                            }
                        }
                    }
                    MediaLayout::Grouped => {
                        // TODO: Multiple grouped videos/audios
                        anyhow::ensure!(
                            items.iter().all(|item| item.medium == MediaMedium::Image),
                            "Media type not all images for multiple media items"
                        );
                        self.bot
                            .send_media_group(
                        self.tg_chan.clone(),
                        items.iter()
                            .enumerate()
                            .map(|(i, item)| {
                                anyhow::ensure!(
                                    item.medium == MediaMedium::Image,
                                    "Unsupported media type for multiple media items in the media item {}: {:?}",
                                    i,
                                    item.medium,
                                );
                                let photo = InputMediaPhoto::new(InputFile::url(Url::parse(&item.uri)?));
                                Ok(InputMedia::Photo(if i == 0 {
                                    photo.caption(post.body.clone()).parse_mode(ParseMode::Html)
                                } else {
                                    photo
                                }))
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                    )
                    .await?;
                        Ok(())
                    }
                    // TODO: See below
                    MediaLayout::Discrete => {
                        todo!("Discrete media layout has not been supported yet")
                    }
                }
            }
        }
    }
}
