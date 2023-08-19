// Copyright (C) myl7
// SPDX-License-Identifier: Apache-2.0

//! [ActivityStreams 2.0 types] supported by mastotg
//!
//! The first-class support list: Mastodon
//!
//! Extensions that are not part of the ActivityStreams 2.0 spec
//! are explicitly marked as "Extension" and given a default value when not provided.
//!
//! [ActivityStreams 2.0 types]: https://www.w3.org/TR/activitystreams-vocabulary/

use std::fmt;

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use serde_with::SerializeDisplay;

/// Page of the outbox of a user.
/// Many unused fields are ignored.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    /// Always includes "https://www.w3.org/ns/activitystreams"
    #[serde(rename = "@context")]
    pub context: Context,
    /// URL of the outbox page
    pub id: String,
    /// Always "OrderedCollectionPage"
    pub r#type: String,
    /// The next page of the posts, a.k.a. the older posts.
    /// The last one is the oldest one.
    /// So the order is kinda reversed.
    /// Bounded by an empty page.
    pub next: Option<String>,
    /// The previous page of the posts, a.k.a. the newer posts.
    /// The first one is the newest one.
    /// So the order is kinda reversed.
    /// Bounded by an empty page.
    pub prev: Option<String>,
    /// Posts in the page
    pub ordered_items: Vec<Create>,
}

/// Activity of a status. Only accept `Create`.
/// Many unused fields are ignored.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Create {
    /// GUID of the activity.
    /// Ignored when the ID of the post can work.
    pub id: String,
    /// Always "Create"
    pub r#type: String,
    /// Created post. Only accept `Note`.
    pub object: Post,
}

/// `Note` in the spec
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Post {
    /// GUID of the post
    pub id: String,
    /// Always "Note"
    pub r#type: String,
    // summary: Option<String>, // Always null
    /// GUID of the replied post
    pub in_reply_to: Option<String>,
    /// `xsd:dateTime` in the spec.
    /// RFC3339 like "2014-12-12T12:12:12Z" without omitting in Mastodon.
    pub published: String,
    /// URL of the post. Different from `id`.
    pub url: String,
    // attributed_to: String,
    // to: Vec<String>,
    // cc: Vec<String>,
    /// Extension.
    // TODO: Can it be used for spoiler?
    #[serde(default)]
    pub sensitive: bool,
    // atom_uri: // Extension
    // in_reply_to_atom_uri: // Extension
    // conversation: // Extension
    /// The original post text content
    pub content: String,
    // content_map: HashMap<String, String>, // I18n, ignored
    /// Media attachments.
    /// Multiple grouped images, a video, or a audio.
    pub attachment: Vec<Document>,
    /// List of hashtags
    pub tag: Vec<Tag>,
    // replies: Vec<Reply>, // Comments, ignored
}

/// Inherits all props from `Object`
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    /// Always "Hashtag"
    pub r#type: String,
    // href: String,
    /// Tag name incluing the leading `#`
    pub name: String,
}

/// Attachment of a post. Only accept `Document`.
/// See [`Post`] for the limitations of Mastodon.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    /// Always "Document"
    pub r#type: String,
    /// MIME media type like `A/B`. `A` is `(image|video|audio)`.
    /// `B` is ignored. We use the extension of the URL instead.
    pub media_type: String,
    /// URL of the attachment file
    pub url: String,
    /// Used as the alt text by Mastodon.
    /// However, Telegram does not support alt texts so it is included but unused.
    pub name: Option<String>,
    // blurhash: String, // Extension
    // `width` and `height` are only valid for `Link`.
    // `Document`, `Image`, `Audio`, `Video` all can not have them.
    // So it is weird to see Mastodon uses them here. Or they may be extensions.
    // width: u32, // Ignored
    // height: u32, // Ignored
}

const TYPES: &[&str] = &[
    "OrderedCollectionPage",
    "Create",
    "Note",
    "Hashtag",
    "Document",
];

pub trait CheckType<const TYPE_IDX: usize> {
    fn check_type(&self) -> Result<()>;
}

macro_rules! impl_check_type {
    ($t:ty, $idx:literal) => {
        impl CheckType<$idx> for $t {
            fn check_type(&self) -> Result<()> {
                if self.r#type == TYPES[$idx] {
                    Ok(())
                } else {
                    Err(anyhow!(
                        "invalid type {} (expected {})",
                        self.r#type.clone(),
                        TYPES[$idx],
                    ))
                }
            }
        }
    };
}

impl_check_type!(Page, 0);
impl_check_type!(Create, 1);
impl_check_type!(Post, 2);
impl_check_type!(Tag, 3);
impl_check_type!(Document, 4);

const AS2_SCHEMA: &str = "https://www.w3.org/ns/activitystreams";

pub trait CheckContext {
    fn check_context(&self) -> Result<()>;
}

macro_rules! impl_check_context {
    ($t:ty) => {
        impl CheckContext for $t {
            fn check_context(&self) -> Result<()> {
                let ctx = match &self.context {
                    Context::Str(value) => value,
                    Context::List(items) => {
                        if items.len() <= 1 {
                            bail!("invalid context that is a too short list")
                        } else {
                            match &items[0] {
                                CtxItem::Str(value) => value,
                                CtxItem::Obj(_) => {
                                    bail!("invalid context of which the first item is an object")
                                }
                            }
                        }
                    }
                };
                if ctx == AS2_SCHEMA {
                    Ok(())
                } else {
                    Err(anyhow!("invalid context that does not contain as2"))
                }
            }
        }
    };
}

impl_check_context!(Page);

#[derive(Deserialize, SerializeDisplay)]
#[serde(untagged)]
pub enum Context {
    Str(String),
    List(Vec<CtxItem>),
}

impl fmt::Display for Context {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", AS2_SCHEMA)
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum CtxItem {
    Str(String),
    #[serde(skip_serializing)]
    Obj(CtxItemObj),
}

#[derive(Deserialize)]
pub struct CtxItemObj {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check_de;

    #[test]
    fn test_de_page() -> Result<()> {
        check_de!(Page, "page");
        Ok(())
    }

    #[test]
    fn test_de_create() -> Result<()> {
        check_de!(Create, "create");
        Ok(())
    }

    #[test]
    fn test_de_post() -> Result<()> {
        check_de!(Post, "post_text");
        Ok(())
    }

    #[test]
    fn test_de_post_link() -> Result<()> {
        check_de!(Post, "post_link");
        Ok(())
    }

    #[test]
    fn test_de_tag() -> Result<()> {
        check_de!(Post, "post_tag");
        Ok(())
    }

    #[test]
    fn test_de_multi_grouped_images() -> Result<()> {
        check_de!(Post, "post_multi_grouped_images");
        Ok(())
    }
}
