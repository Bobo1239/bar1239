#![feature(coroutines, stmt_expr_attributes, proc_macro_hygiene)]

mod backlight;
mod battery;
mod protocol;
mod sound;
mod time;

use std::time::Duration;

use anyhow::{Error, Result};
use futures::{stream, stream::StreamExt};
use futures_async_stream::{for_await, try_stream};

use crate::{
    backlight::BacklightBlock,
    battery::BatteryBlock,
    protocol::{SwaybarBlock, SwaybarHeader},
    sound::SoundBlock,
    time::TimeBlock,
};

pub trait Block {
    #[try_stream(boxed, ok = BlockData, error = Error)]
    async fn block_data_stream(&mut self);
}

pub trait PeriodicBlock {
    fn update_period(&self) -> Duration;
    fn block_data(&mut self) -> Result<BlockData>;
}

#[derive(Debug, Clone, Default)]
pub struct BlockData {
    text: String,
    state: BlockState,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum BlockState {
    #[default]
    Normal,
    Warning,
}

impl BlockState {
    fn color(&self) -> &str {
        match self {
            BlockState::Normal => "#d8d8d8",
            BlockState::Warning => "#ab4642",
        }
    }
}

#[tokio::main]
async fn main() {
    // TODO: stop cont signals?
    // TODO: How to handle clicks?
    // TODO: Remove all unwraps
    let mut blocks: Vec<Box<dyn Block>> = Vec::new();

    blocks.push(Box::new(SoundBlock::new().unwrap()));
    if let Ok(backlight) = BacklightBlock::new() {
        blocks.push(Box::new(backlight));
    }
    if let Ok(battery) = BatteryBlock::new() {
        blocks.push(Box::new(battery));
    }
    blocks.push(Box::new(TimeBlock::new()));

    println!(
        "{}",
        serde_json::to_string(&SwaybarHeader {
            version: 1,
            ..Default::default()
        })
        .unwrap()
    );
    println!("[");

    let blocks_len = blocks.len();
    let mut block_data = vec![BlockData::default(); blocks.len()];
    let combined_stream = stream::select_all(
        blocks
            .iter_mut()
            .enumerate()
            .map(|(i, b)| b.block_data_stream().map(move |block_data| (i, block_data))),
    );

    let mut last = String::new();
    #[for_await]
    for (i, new_block_data) in combined_stream {
        block_data[i] = new_block_data.unwrap(); // FIXME

        let blocks: Vec<_> = block_data
            .iter()
            .enumerate()
            .map(|(i, data)| SwaybarBlock {
                full_text: &data.text,
                color: Some(data.state.color()),
                separator: Some(false),
                separator_block_width: if i == blocks_len - 1 {
                    Some(10)
                } else {
                    Some(30)
                },
                // markup: Some("pango"),
                ..Default::default()
            })
            .collect();
        // let pango_example = format!(r#"<span foreground="blue">Blue text</span> is <i>cool</i>!"#);

        // TODO: Reuse String each iter via `serde_json::to_writer`
        let json = serde_json::to_string(&blocks).unwrap();
        if json != last {
            println!("{},", json);
            last = json;
        }
    }
}
