// Copyright (C) myl7
// SPDX-License-Identifier: Apache-2.0

//! Post consumers

use std::collections::{HashMap, VecDeque};

use anyhow::{anyhow, bail, ensure, Result};
use async_trait::async_trait;
use quick_xml::events::Event;
use quick_xml::name::QName;
use quick_xml::reader::Reader;
use reqwest::Url;
use teloxide::prelude::*;
use teloxide::types::{InputFile, InputMedia, InputMediaPhoto, MessageId, ParseMode};
use teloxide::RequestError;
use tokio::time;

use crate::as2::{Create, Page, Post};
use crate::db::DbConn;

pub type IdMap = HashMap<String, Vec<u8>>;

/// Consumer trait
#[async_trait]
pub trait Con {
    /// Send posts in the form of activities.
    /// Not send one-by-one directly in case collection-level cleaning is required.
    async fn send(&self, items: Vec<Create>) -> Result<IdMap>;

    /// Send a page of posts
    async fn send_page(&self, page: Page) -> Result<IdMap> {
        self.send(page.ordered_items).await
    }
}

pub struct TgCon {
    bot: Bot,
    tg_chan: String,
    db: DbConn,
}

impl TgCon {
    pub fn new(tg_chan: String, db: DbConn) -> Self {
        Self {
            bot: Bot::from_env(),
            tg_chan,
            db,
        }
    }
}

macro_rules! handle_reply {
    ($send:ident, $db:expr, $id_map:ident, $post:ident) => {
        if let Some(id) = $post.in_reply_to.as_ref() {
            let mut tg_id_opt = $id_map.get(id).cloned();
            if let None = tg_id_opt {
                tg_id_opt = $db.query_id_map(id.to_owned()).await?;
            }
            if let Some(tg_id) = tg_id_opt {
                let (_, msg_id) = de_tg_msg_id(&tg_id);
                $send = $send
                    .reply_to_message_id(MessageId(msg_id))
                    .allow_sending_without_reply(true);
            }
        }
    };
}

impl TgCon {
    async fn send_one(&self, id_map: &IdMap, mut act: Create) -> Result<Vec<u8>> {
        act.object.content = clean_body(&act.object.content)?;
        let post = &act.object;

        if post.attachment.is_empty() {
            let id = self.send_text(id_map, post).await?;
            return Ok(id);
        }

        if post.attachment.len() > 1 {
            ensure!(
                post.attachment
                    .iter()
                    .all(|att| att.media_type.starts_with("image/")),
                "media type not all images for multiple media"
            );
            let id = self.send_multi_grouped_images(id_map, post).await?;
            return Ok(id);
        }

        let att = &post.attachment[0];
        let media_type = &att.media_type[..att
            .media_type
            .find('/')
            .ok_or(anyhow!("invalid media type {}", att.media_type))?];
        let id = match media_type {
            "image" => self.send_image(id_map, post).await?,
            "video" => self.send_video(id_map, post).await?,
            "audio" => self.send_audio(id_map, post).await?,
            _ => bail!("unknown media type {}", att.media_type),
        };
        Ok(id)
    }

    async fn send_text(&self, id_map: &IdMap, post: &Post) -> Result<Vec<u8>> {
        let mut send = self
            .bot
            .send_message(self.tg_chan.clone(), &post.content)
            .parse_mode(ParseMode::Html);
        handle_reply!(send, self.db, id_map, post);
        let msg = send.await?;
        Ok(ser_tg_msg_id(&msg))
    }

    async fn send_multi_grouped_images(&self, id_map: &IdMap, post: &Post) -> Result<Vec<u8>> {
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
        let mut send = self.bot.send_media_group(self.tg_chan.clone(), photos);
        handle_reply!(send, self.db, id_map, post);
        let msgs = send.await?;
        Ok(ser_tg_msg_id(&msgs[0]))
    }

    async fn send_image(&self, id_map: &IdMap, post: &Post) -> Result<Vec<u8>> {
        let att = &post.attachment[0];
        let mut send = self
            .bot
            .send_photo(self.tg_chan.clone(), InputFile::url(Url::parse(&att.url)?))
            .caption(post.content.clone())
            .parse_mode(ParseMode::Html);
        handle_reply!(send, self.db, id_map, post);
        let msg = send.await?;
        Ok(ser_tg_msg_id(&msg))
    }

    async fn send_video(&self, id_map: &IdMap, post: &Post) -> Result<Vec<u8>> {
        let att = &post.attachment[0];
        let mut send = self
            .bot
            .send_video(self.tg_chan.clone(), InputFile::url(Url::parse(&att.url)?))
            .caption(post.content.clone())
            .parse_mode(ParseMode::Html);
        handle_reply!(send, self.db, id_map, post);
        let msg = send.await?;
        Ok(ser_tg_msg_id(&msg))
    }

    async fn send_audio(&self, id_map: &IdMap, post: &Post) -> Result<Vec<u8>> {
        let att = &post.attachment[0];
        let mut send = self
            .bot
            .send_audio(self.tg_chan.clone(), InputFile::url(Url::parse(&att.url)?))
            .caption(post.content.clone())
            .parse_mode(ParseMode::Html);
        handle_reply!(send, self.db, id_map, post);
        let msg = send.await?;
        Ok(ser_tg_msg_id(&msg))
    }
}

#[async_trait]
impl Con for TgCon {
    async fn send(&self, items: Vec<Create>) -> Result<IdMap> {
        let mut id_map = HashMap::new();
        let mut queue: VecDeque<_> = items.into_iter().rev().collect();
        while !queue.is_empty() {
            let item = if let Some(x) = queue.pop_front() {
                x
            } else {
                break;
            };

            match self.send_one(&id_map, item.clone()).await {
                Err(e) => {
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
                Ok(tg_id) => {
                    id_map.insert(item.object.id.clone(), tg_id);
                }
            }
        }
        Ok(id_map)
    }
}

/// Get the GUID from a Telegram msg
pub fn ser_tg_msg_id(msg: &Message) -> Vec<u8> {
    let chat_id = msg.chat.id.0;
    let msg_id = msg.id.0 as i64;
    [chat_id.to_be_bytes(), msg_id.to_be_bytes()].concat()
}

/// Extract the msg ID and chat ID from a Telegram msg GUID
pub fn de_tg_msg_id(id: &[u8]) -> (i64, i32) {
    assert_eq!(id.len(), 16);
    let chat_id = i64::from_be_bytes(id[..8].try_into().unwrap());
    let msg_id = i64::from_be_bytes(id[8..].try_into().unwrap()) as i32;
    (chat_id, msg_id)
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
