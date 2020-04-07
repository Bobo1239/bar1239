use std::time::Duration;

use anyhow::Error;
use chrono::Local;
use futures_async_stream::async_try_stream;

use crate::{Block, BlockData, BlockState};

pub struct TimeBlock;

impl TimeBlock {
    pub fn new() -> TimeBlock {
        TimeBlock
    }
}

impl Block for TimeBlock {
    #[async_try_stream(boxed, ok = BlockData, error = Error)]
    async fn block_data_stream(&mut self) {
        loop {
            yield BlockData {
                text: Local::now().format("%a %Y-%m-%d %H:%M:%S").to_string(),
                state: BlockState::Normal,
            };

            tokio::time::delay_for(Duration::from_millis(1000)).await;
        }
    }
}
