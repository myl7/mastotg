// Copyright (C) myl7
// SPDX-License-Identifier: Apache-2.0

//! Query the outbox JSON URL from the WebFinger API

use std::path::Path;

use anyhow::{anyhow, Result};
use reqwest::Url;
use serde::Deserialize;
use serde_with::{serde_as, DefaultOnError};

use crate::utils::check_res;

pub async fn query_outbox_url(host: &str, acct: &str) -> Result<String> {
    let mut webfinger_u = Url::parse(host)?;
    let webfinger_path = Path::new(webfinger_u.path()).join(".well-known/webfinger");
    webfinger_u.set_path(webfinger_path.to_str().unwrap());
    webfinger_u
        .query_pairs_mut()
        .append_pair("resource", &format!("acct:{}", acct));
    let webfinger_info: WebFinger = check_res(reqwest::get(webfinger_u).await?)
        .await?
        .json()
        .await?;
    let ctx_type = "application/activity+json";
    let profile_url = webfinger_info
        .links
        .iter()
        .find_map(|link| {
            if link.r#type == ctx_type && link.rel == "self" {
                Some(link.href.clone())
            } else {
                None
            }
        })
        .ok_or(anyhow!(
            "profile link with context type {ctx_type} not found"
        ))?;

    let client = reqwest::Client::new();
    let profile: Profile = check_res(
        client
            .get(profile_url)
            .header("accept", ctx_type)
            .send()
            .await?,
    )
    .await?
    .json()
    .await?;
    let url = profile.outbox;
    Ok(url)
}

#[serde_as]
#[derive(Deserialize)]
struct WebFinger {
    #[serde_as(as = "Vec<DefaultOnError>")]
    links: Vec<WebFingerLink>,
}

#[derive(Deserialize, Default)]
struct WebFingerLink {
    rel: String,
    r#type: String,
    href: String,
}

#[derive(Deserialize)]
struct Profile {
    outbox: String,
}
