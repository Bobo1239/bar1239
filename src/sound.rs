// TODO: Consider using pulseaudio once https://github.com/jnqnfe/pulse-binding-rust/issues/20 is
//       resolved

use alsa::{
    mixer::{Selem, SelemChannelId, SelemId},
    Mixer,
};
use anyhow::{Error, Result};
use futures_async_stream::async_try_stream;
use tokio::{
    sync::mpsc::{self, Receiver, Sender},
    task,
};

use crate::{Block, BlockData, BlockState};

pub struct SoundBlock {
    rx: Receiver<SoundState>,
}

struct SoundState {
    volume: f32,
    muted: bool,
}

impl SoundBlock {
    pub fn new() -> Result<SoundBlock> {
        let mixer = Mixer::new("default", false)?;

        // TODO: Unsure if this is the most optimal channel...
        let (tx, rx) = mpsc::channel(10);
        task::spawn_blocking(move || {
            // Not `Send` so must be done here
            let selem_id = SelemId::new("Master", 0);
            let selem = mixer.find_selem(&selem_id).unwrap();

            // Needs to be called once before we enter the loop (`assert` fails otherwise)
            mixer.handle_events().unwrap();
            send_current_state(&selem, &tx);

            loop {
                mixer.wait(None).unwrap();
                let events = mixer.handle_events().unwrap();
                assert!(events > 0);

                send_current_state(&selem, &tx);
            }
        });
        Ok(SoundBlock { rx })
    }
}

impl Block for SoundBlock {
    #[async_try_stream(boxed, ok = BlockData, error = Error)]
    async fn block_data_stream(&mut self) {
        while let Some(sound_state) = self.rx.recv().await {
            let volume = if sound_state.muted {
                "--".to_string()
            } else {
                ((sound_state.volume * 100.0).round() as u8).to_string()
            };
            yield BlockData {
                text: format!("S: {}%", volume),
                state: BlockState::Normal,
            };
        }
        panic!("alsa thread panicked")
    }
}

fn send_current_state(selem: &Selem, tx: &Sender<SoundState>) {
    let vol = selem.get_playback_volume(SelemChannelId::mono()).unwrap();
    let (min_vol, max_vol) = selem.get_playback_volume_range();
    let range = max_vol - min_vol;
    let volume = vol as f32 / range as f32;
    let muted = selem.get_playback_switch(SelemChannelId::mono()).unwrap() == 0;

    // TODO: Can we do this without spawning a new task?
    let mut tx = tx.clone();
    tokio::spawn(async move {
        if tx.send(SoundState { volume, muted }).await.is_err() {
            panic!("receiver dropped");
        }
    });
}
