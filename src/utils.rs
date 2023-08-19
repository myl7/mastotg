// Copyright (C) myl7
// SPDX-License-Identifier: Apache-2.0

//! Helpers of which you do not need to check the code to know the meaning

use anyhow::{anyhow, Result};
use regex::Regex;
use reqwest::Response;

/// Check if the response is a success
pub async fn check_res(res: Response) -> Result<Response> {
    if res.status().is_success() {
        Ok(res)
    } else {
        let url = res.url().as_str().to_owned();
        Err(anyhow!(
            "request to {url} failed with status code {} and body {}",
            res.status(),
            res.text().await?,
        ))
    }
}

/// Extract the integer ID from the activity/note GUID
pub fn int_id(guid: &str) -> Result<i64> {
    let m = Regex::new(r"/(\d+?)(?:/activity)?$")
        .unwrap()
        .captures(guid)
        .ok_or(anyhow!("no integer id in the activity guid"))?;
    let int: i64 = m.get(1).unwrap().as_str().parse()?;
    Ok(int)
}

/// De a test fixture
#[cfg(test)]
#[macro_export]
macro_rules! check_de {
    ($t:ty, $fname:literal) => {{
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join($fname.to_owned() + ".json");
        let s = std::fs::read(path).unwrap();
        let res: $t = serde_json::from_slice(&s)?;
        res
    }};
}
