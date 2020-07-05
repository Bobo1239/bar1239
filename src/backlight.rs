// inotify isn't actually supported for sysfs[1] (which makes sense) but it does work in
// this specific case so we'll use it
// [1]: https://www.spinics.net/lists/linux-fsdevel/msg73924.html

use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
};

use anyhow::{anyhow, Error, Result};
use futures::stream::StreamExt;
use futures_async_stream::try_stream;
use inotify::{Inotify, WatchMask};

use crate::{Block, BlockData, BlockState};

pub struct BacklightBlock {
    inotify: Inotify,
    file: File,
    max_brightness: f32,
}

impl BacklightBlock {
    pub fn new() -> Result<BacklightBlock> {
        let path = fs::read_dir("/sys/class/backlight/")?
            .flat_map(|entry_result| entry_result.map(|entry| entry.path()))
            .next();
        match path {
            None => Err(anyhow!("No backlight available")),
            Some(path) => {
                let mut inotify = Inotify::init()?;
                inotify.add_watch(&path, WatchMask::MODIFY)?;
                Ok(BacklightBlock {
                    inotify,
                    file: File::open(path.join("actual_brightness"))?,
                    max_brightness: fs::read_to_string(path.join("max_brightness"))?
                        .trim()
                        .parse::<f32>()?,
                })
            }
        }
    }
}

impl BacklightBlock {
    fn new_block_data(&mut self) -> Result<BlockData> {
        let mut brightness = String::new();
        self.file.seek(SeekFrom::Start(0))?;
        self.file.read_to_string(&mut brightness)?;

        let percentage = brightness.trim().parse::<f32>()? / self.max_brightness;
        Ok(BlockData {
            text: format!("L: {}%", (percentage * 100.0).round() as u8),
            state: BlockState::Normal,
        })
    }
}

impl Block for BacklightBlock {
    #[try_stream(boxed, ok = BlockData, error = Error)]
    async fn block_data_stream(&mut self) {
        let mut buffer = [0u8; 1024];
        let mut stream = self.inotify.event_stream(&mut buffer[..]).unwrap();

        yield self.new_block_data()?;
        while stream.next().await.is_some() {
            yield self.new_block_data()?;
        }
        unreachable!("inotify's stream is infinite")
    }
}
