// Copyright (C) myl7
// SPDX-License-Identifier: Apache-2.0

#[derive(Debug)]
pub struct Post {
    /// GUID
    pub id: String,
    /// Content text
    pub body: String,
    /// Media attachments
    pub media: Option<Media>,
    /// Relationship with other posts
    pub rel: Vec<Rel>,
    /// Optional original URL
    pub link: Option<String>,
}

#[derive(Debug)]
pub struct Media {
    pub layout: MediaLayout,
    pub items: Vec<MediaItem>,
}

#[derive(Debug)]
pub struct MediaItem {
    /// URI as the handle
    pub uri: String,
    /// Media type, e.g., images, videos, audios, etc.
    pub medium: MediaMedium,
    /// Optional original URL
    pub link: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MediaLayout {
    Single,
    Grouped,
    #[allow(dead_code)]
    Discrete,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MediaMedium {
    Image,
    Video,
    Audio,
}

impl TryFrom<&str> for MediaMedium {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "image" => Ok(MediaMedium::Image),
            "video" => Ok(MediaMedium::Video),
            "audio" => Ok(MediaMedium::Audio),
            _ => anyhow::bail!("Unsupported media type {}", value),
        }
    }
}

#[derive(Debug)]
pub enum Rel {
    ReplyTo { id: String },
    // TODO: Impl
    // FwdFrom { id: String },
}
