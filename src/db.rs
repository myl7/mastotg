// Copyright (C) myl7
// SPDX-License-Identifier: Apache-2.0

//! Database wrappers.
//! Since the application is async and database operations are blocking,
//! you should only use the methods here to interact with the database.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use tokio::task;

use crate::con::IdMap;

pub mod migration {
    refinery::embed_migrations!();
}

pub struct DbConn {
    conn: Arc<Mutex<Connection>>,
}

macro_rules! conn_blocking {
    ($conn:expr, $var:ident, $b:block) => {{
        let conn = $conn.clone();
        task::spawn_blocking(move || {
            let $var = conn.lock().unwrap();
            $b
        })
        .await??
    }};
}

impl DbConn {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    pub async fn save_state(&self, state: State) -> Result<()> {
        conn_blocking!(self.conn, conn, {
            conn.execute(SQL_REPLACE_STATE, (state.min_id,))?;
            anyhow::Ok(())
        });
        Ok(())
    }

    pub async fn load_state(&self) -> Result<Option<State>> {
        let state = conn_blocking!(self.conn, conn, {
            conn.query_row(SQL_SELECT_STATE, (), |row| {
                Ok(State {
                    min_id: row.get(0)?,
                })
            })
            .optional()
        });
        Ok(state)
    }

    pub async fn save_id_map(&self, id_map: IdMap) -> Result<()> {
        conn_blocking!(self.conn, conn, {
            let mut stmt = conn.prepare_cached(SQL_INSERT_ID_PAIR)?;
            for (id, tg_id) in id_map.iter() {
                stmt.execute((id, tg_id))?;
            }
            anyhow::Ok(())
        });
        Ok(())
    }

    pub async fn query_id_map(&self, id: String) -> Result<Option<Vec<u8>>> {
        let tg_id: Option<Vec<u8>> = conn_blocking!(self.conn, conn, {
            conn.query_row(SQL_SELECT_ID_PAIR, (&id,), |row| row.get(0))
                .optional()
        });
        Ok(tg_id)
    }
}

#[derive(Debug, Clone)]
pub struct State {
    pub min_id: i64,
}

impl State {
    pub fn new(min_id: i64) -> Self {
        Self { min_id }
    }
}

impl Default for State {
    fn default() -> Self {
        Self { min_id: -1 }
    }
}

const SQL_REPLACE_STATE: &str = r#"INSERT OR REPLACE INTO state (pk, min_id) VALUES (1, ?1)"#;
const SQL_SELECT_STATE: &str = r#"SELECT min_id FROM state WHERE pk = 1"#;
const SQL_INSERT_ID_PAIR: &str = r#"INSERT INTO id_map (id, tg_id) VALUES (?1, ?2)"#;
const SQL_SELECT_ID_PAIR: &str = r#"SELECT tg_id FROM id_map WHERE id = ?1"#;
