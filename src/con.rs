// Copyright (C) myl7
// SPDX-License-Identifier: Apache-2.0

//! Post consumers

use std::collections::VecDeque;

use anyhow::{anyhow, bail, ensure, Result};
use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::name::QName;
use quick_xml::reader::Reader;
use reqwest::Url;
use teloxide::prelude::*;
use teloxide::types::{InputFile, InputMedia, InputMediaPhoto, ParseMode};
use teloxide::RequestError;
use tokio::time;

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
    async fn send_one(&self, mut act: Create) -> Result<()> {
        act.object.content = clean_body(&act.object.content)?;
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
        let mut queue: VecDeque<_> = items.into_iter().rev().collect();
        while !queue.is_empty() {
            let item = if let Some(x) = queue.pop_front() {
                x
            } else {
                break;
            };
            if let Err(e) = self.send_one(item.clone()).await {
                if let Some(req_e) = e.downcast_ref::<RequestError>() {
                    if let RequestError::RetryAfter(du) = req_e {
                        log::warn!("Retry after {} seconds due to flood control", du.as_secs());
                        queue.push_front(item);
                        time::sleep(*du).await;
                    }
                } else {
                    bail!(e)
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check_de;

    #[test]
    fn test_body_text() -> Result<()> {
        let post = check_de!(Post, "post_text");
        let body = clean_body(&post.content)?;
        let body_expected = concat!(
            "哈哈哈哈，追番的乐趣原来就是这样啊੭ ᐕ)੭\n",
            "虽然还是没有更多的信息，但是实在是名场面啊，很很的破防！\n",
            "mygo 好！"
        );
        assert_eq!(body, body_expected);
        Ok(())
    }

    #[test]
    fn test_body_link() -> Result<()> {
        let post = check_de!(Post, "post_link");
        let body = clean_body(&post.content)?;
        let body_expected = concat!(
            r#"已经 deploy <a href="https://github.com/myl7/mastotg">https://github.com/myl7/mastotg</a> 了，应该是 generally available 了"#,
            "\n",
            "功能的话还差个 reply 关系（这条作为样例试试再看怎么处理"
        );
        assert_eq!(body, body_expected);
        Ok(())
    }

    #[test]
    fn test_body_tag() -> Result<()> {
        let post = check_de!(Post, "post_tag");
        let body = clean_body(&post.content)?;
        let body_expected = concat!(
            "另：信息已经不重要了，具体的前因后果就等 ave mujica 里讲吧，或许可以 mygo 结尾留个引子？\n",
            "#mygo"
        );
        assert_eq!(body, body_expected);
        Ok(())
    }
}
