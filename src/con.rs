// Copyright (C) myl7
// SPDX-License-Identifier: Apache-2.0

//! Post consumers

use anyhow::{anyhow, bail, ensure, Result};
use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::name::QName;
use quick_xml::reader::Reader;
use reqwest::Url;
use teloxide::prelude::*;
use teloxide::types::{InputFile, InputMedia, InputMediaPhoto, ParseMode};

use crate::as2::{Create, Page, Post};

/// Consumer trait
#[async_trait]
pub trait Con {
    /// Send posts in the form of activities.
    /// Not send one-by-one directly in case collection-level cleaning is required.
    async fn send(&self, items: Vec<Create>) -> Result<()>;

    /// Send a page of posts
    async fn send_page(&self, page: Page) -> Result<()> {
        self.send(page.ordered_items).await
    }
}

pub struct TgCon {
    bot: Bot,
    tg_chan: String,
}

impl TgCon {
    pub fn new(tg_chan: String) -> Self {
        Self {
            bot: Bot::from_env(),
            tg_chan,
        }
    }
}

impl TgCon {
    async fn send_one(&self, act: &Create) -> Result<()> {
        let post = &act.object;

        if post.attachment.is_empty() {
            self.send_text(post).await?;
            return Ok(());
        }

        if post.attachment.len() > 1 {
            ensure!(
                post.attachment
                    .iter()
                    .all(|att| att.media_type.starts_with("image/")),
                "media type not all images for multiple media"
            );
            self.send_multi_grouped_images(post).await?;
            return Ok(());
        }

        let att = &post.attachment[0];
        let media_type = &att.media_type[..att
            .media_type
            .find('/')
            .ok_or(anyhow!("invalid media type {}", att.media_type))?];
        match media_type {
            "image" => {
                self.send_image(post).await?;
            }
            "video" => {
                self.send_video(post).await?;
            }
            "audio" => {
                self.send_audio(post).await?;
            }
            _ => bail!("unknown media type {}", att.media_type),
        }
        Ok(())
    }

    async fn send_text(&self, post: &Post) -> Result<()> {
        self.bot
            .send_message(self.tg_chan.clone(), &post.content)
            .parse_mode(ParseMode::Html)
            .await?;
        Ok(())
    }

    async fn send_multi_grouped_images(&self, post: &Post) -> Result<()> {
        let photos = post
            .attachment
            .iter()
            .enumerate()
            .map(|(i, att)| {
                let photo = InputMediaPhoto::new(InputFile::url(Url::parse(&att.url)?));
                Ok(InputMedia::Photo(if i == 0 {
                    photo
                        .caption(post.content.clone())
                        .parse_mode(ParseMode::Html)
                } else {
                    photo
                }))
            })
            .collect::<Result<Vec<_>>>()?;
        self.bot
            .send_media_group(self.tg_chan.clone(), photos)
            .await?;
        Ok(())
    }

    async fn send_image(&self, post: &Post) -> Result<()> {
        let att = &post.attachment[0];
        self.bot
            .send_photo(self.tg_chan.clone(), InputFile::url(Url::parse(&att.url)?))
            .caption(post.content.clone())
            .parse_mode(ParseMode::Html)
            .await?;
        Ok(())
    }

    async fn send_video(&self, post: &Post) -> Result<()> {
        let att = &post.attachment[0];
        self.bot
            .send_video(self.tg_chan.clone(), InputFile::url(Url::parse(&att.url)?))
            .caption(post.content.clone())
            .parse_mode(ParseMode::Html)
            .await?;
        Ok(())
    }

    async fn send_audio(&self, post: &Post) -> Result<()> {
        let att = &post.attachment[0];
        self.bot
            .send_audio(self.tg_chan.clone(), InputFile::url(Url::parse(&att.url)?))
            .caption(post.content.clone())
            .parse_mode(ParseMode::Html)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl Con for TgCon {
    async fn send(&self, items: Vec<Create>) -> Result<()> {
        for mut item in items.into_iter().rev() {
            item.object.content = clean_body(&item.object.content)?;
            self.send_one(&item).await?;
        }
        Ok(())
    }
}

fn clean_body(body: &str) -> Result<String> {
    let mut texts = String::new();
    let mut reader = Reader::from_str(body);
    // In a <a>. Texts inside ignored.
    let mut in_link = false;
    // In a <a> as a hashtag.
    let mut in_hashtag = false;
    loop {
        #[allow(clippy::single_match)]
        match reader.read_event()? {
            Event::Eof => break,
            Event::Start(elem) => match elem.name().as_ref() {
                b"a" => {
                    let mut is_hashtag = false;
                    let mut href_opt = None;
                    elem.html_attributes().try_for_each(|res| {
                        let attr = res?;
                        match attr.key {
                            QName(b"class") => {
                                is_hashtag = attr
                                    .decode_and_unescape_value(&reader)?
                                    .find("hashtag")
                                    .is_some()
                            }
                            QName(b"href") => {
                                href_opt = Some(attr.decode_and_unescape_value(&reader)?)
                            }
                            _ => (),
                        }
                        anyhow::Ok(())
                    })?;
                    if is_hashtag && !in_hashtag {
                        in_hashtag = true;
                    } else if !in_link {
                        let href = href_opt.ok_or(anyhow!("no href in the <a> tag"))?;
                        texts += &format!(r#"<a href="{}">{href}"#, href);
                        in_link = true;
                    } else {
                        bail!("unknown <a> tag");
                    }
                }
                _ => (),
            },
            Event::Text(elem) => {
                if !in_link {
                    texts += &elem.unescape()?;
                }
            }
            Event::End(elem) => match elem.name().as_ref() {
                b"a" => {
                    if in_hashtag {
                        in_hashtag = false;
                    } else if in_link {
                        texts += "</a>";
                        in_link = false;
                    } else {
                        anyhow::bail!("unknown <a> tag");
                    }
                }
                _ => (),
            },
            Event::Empty(elem) => match elem.name().as_ref() {
                b"br" => texts += "\n",
                _ => (),
            },
            _ => (),
        }
    }
    Ok(texts)
}