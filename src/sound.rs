// TODO: Consider using pulseaudio once https://github.com/jnqnfe/pulse-binding-rust/issues/20 is
//       resolved

use alsa::{
    mixer::{Selem, SelemChannelId, SelemId},
    Mixer,
};
use anyhow::{Error, Result};
use futures_async_stream::try_stream;
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
        // TODO: Unsure if this is the most optimal channel...
        let (tx, rx) = mpsc::channel(10);
        task::spawn_blocking(move || {
            // TODO: Click to swap output device
            // TODO: Show battery level for bluetooth device?
            loop {
                let mut mixer = Mixer::new("default", false).unwrap();
                let selem_id = SelemId::new("Master", 0);
                loop {
                    // Terrible hack...; This provokes an error from mixer.handle_events() whenever
                    // an event happens (default device change; volume change); We then create a
                    // `Mixer` to track default device changes which doesn't seem possible to do
                    // with a single `Mixer`. There may be a better way though...
                    mixer
                        .attach(&std::ffi::CString::new("default").unwrap())
                        .unwrap();

                    let selem = mixer.find_selem(&selem_id).unwrap();

                    if mixer.handle_events().is_err() {
                        break;
                    }
                    send_current_state(&selem, &tx);
                    mixer.wait(None).unwrap();
                }
            }
        });
        Ok(SoundBlock { rx })
    }
}

impl Block for SoundBlock {
    #[try_stream(boxed, ok = BlockData, error = Error)]
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
