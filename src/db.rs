// Copyright (C) myl7
// SPDX-License-Identifier: Apache-2.0

use rusqlite::{Connection, OptionalExtension};

use crate::post::Post;

const SQL_SELECT_LAST_BUILD_DATE: &str = "SELECT value FROM last_build_dates WHERE value = ?1";
const SQL_INSERT_LAST_BUILD_DATE: &str = "INSERT INTO last_build_dates (value) VALUES (?1)";
const SQL_SELECT_POST: &str = "SELECT id FROM posts WHERE id = ?1";
const SQL_INSERT_POST: &str =
    "INSERT INTO posts (id, body, link, last_build_date) VALUES (?1, ?2, ?3, ?4)";

pub fn dedup_posts(
    conn: &Connection,
    last_build_date: &str,
    posts: Vec<Post>,
) -> anyhow::Result<Vec<Post>> {
    let date: Option<String> = conn
        .query_row(SQL_SELECT_LAST_BUILD_DATE, (last_build_date,), |row| {
            row.get(0)
        })
        .optional()?;
    if date.is_some() {
        return Ok(Vec::new());
    }

    let mut stmt = conn.prepare(SQL_SELECT_POST)?;
    Ok(posts
        .into_iter()
        .filter_map(|post| {
            stmt.query_row((&post.id,), |row| row.get(0))
                .optional()
                .map(|id_opt: Option<String>| match id_opt {
                    Some(_) => None,
                    None => Some(post),
                })
                .transpose()
        })
        .collect::<Result<_, _>>()?)
}

pub fn save_posts(conn: &Connection, last_build_date: &str, posts: &[&Post]) -> anyhow::Result<()> {
    conn.execute(SQL_INSERT_LAST_BUILD_DATE, (last_build_date,))?;

    let mut stmt = conn.prepare(SQL_INSERT_POST)?;
    posts
        .iter()
        .map(|post| {
            stmt.execute((
                &post.id,
                &post.body,
                post.link.as_deref().unwrap_or(""),
                last_build_date,
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(())
}

// TODO: Save media
