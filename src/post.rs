// Copyright (C) myl7
// SPDX-License-Identifier: Apache-2.0

use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::{env, io};

#[derive(Debug)]
pub struct Post {
    /// GUID
    pub id: String,
    /// Content text
    pub body: String,
    /// Media attachments
    pub media: Option<Media>,
    /// Original URL
    pub link: Option<String>,
}

#[derive(Debug)]
pub enum Media {
    Photos(Vec<Fid>),
    Video(Fid),
    Audio(Fid),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Fid {
    pub value: String,
    /// Original URL
    pub link: Option<String>,
}

pub trait Repo {
    fn put(&mut self, id: &Fid, file: impl Read) -> anyhow::Result<()>;
    fn get(&self, id: &Fid) -> anyhow::Result<Option<Box<dyn Read>>>;
}

pub struct FsRepo {
    dir: PathBuf,
}

impl FsRepo {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }
}

impl Default for FsRepo {
    fn default() -> Self {
        let dir = env::temp_dir().join("mastotg/media");
        Self::new(dir)
    }
}

impl Repo for FsRepo {
    fn put(&mut self, id: &Fid, mut file: impl Read) -> anyhow::Result<()> {
        let path = self.dir.join(&id.value);
        let mut f = File::create(path)?;
        io::copy(&mut file, &mut f)?;
        Ok(())
    }

    fn get(&self, id: &Fid) -> anyhow::Result<Option<Box<dyn Read>>> {
        let path = self.dir.join(&id.value);
        let f = File::open(path).map(Some).or_else(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                Ok(None)
            } else {
                Err(e)
            }
        })?;
        Ok(f.map(|f| Box::new(f) as Box<dyn Read>))
    }
}
