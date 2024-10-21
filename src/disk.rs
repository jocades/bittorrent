use std::{
    fs::{self, File},
    io::{prelude::*, SeekFrom, Write},
    path::Path,
};

use anyhow::Result;

use crate::{download::PieceDownload, metainfo::Metainfo, Storage};

struct Disk {
    files: Vec<File>,
    dest: Path,
}

impl Disk {
    pub fn allocate(&mut self, meta: &Metainfo) -> Result<()> {
        fs::create_dir("test_download_dir")?;

        for index in 0..meta.piece_len() {
            let file = File::create(format!("piece_{index}"))?;
            self.files.push(file)
        }

        todo!()
    }

    pub fn write(download: PieceDownload) -> Result<()> {
        todo!()
    }
}
