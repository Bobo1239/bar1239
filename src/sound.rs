use std::collections::HashMap;

use anyhow::{Error, Result};
use futures_async_stream::try_stream;
use pipewire_native::{
    context::Context,
    main_loop::MainLoop,
    properties::Properties,
    proxy::{
        metadata::{Metadata, MetadataEvents},
        node::{Node, NodeEvents},
        registry::RegistryEvents,
    },
    some_closure, types,
};
use pipewire_native_spa::{
    param::{props::Prop, ParamType},
    pod::parser::Parser,
};
use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::{Block, BlockData, BlockState};

#[derive(Debug)]
enum Update {
    DefaultSink(String),
    Sink { name: String, state: SinkState },
}

#[derive(Debug)]
struct SinkState {
    volume: f32,
    muted: bool,
}

pub struct SoundBlock {
    rx: Receiver<Update>,
}

impl SoundBlock {
    pub fn new() -> Result<SoundBlock> {
        let (tx, rx) = mpsc::channel(10);

        pipewire_native::init();

        let props = Properties::new();
        let ml = MainLoop::new(&props).unwrap();

        let ctx = Context::new(&ml, props)?;
        let core = ctx.connect(None)?;

        let reg = core.registry()?;

        reg.add_listener(RegistryEvents {
            global: some_closure!([reg ^(tx)] id, _perms, type_, version, props, {
                if type_ == types::interface::METADATA && props.get("metadata.name") == Some("default") {
                    let obj = reg.bind(id, type_, version).unwrap();
                    let metadata = obj.downcast::<Metadata>().unwrap();

                    metadata.add_listener(MetadataEvents {
                        property: some_closure!([^(tx)] _subject, key, _type, value, {
                            if key == Some("default.audio.sink") {
                                if let Some(value) = value {
                                    const PREFIX: &str = "{\"name\":\"";
                                    const SUFFIX: &str = "\"}";
                                    assert!(value.starts_with(PREFIX) && value.ends_with(SUFFIX));
                                    let new_default = &value[PREFIX.len()..value.len() - 2];
                                    send_update(tx, Update::DefaultSink(new_default.to_owned()));
                                }
                            }
                        }),
                    });
                } else if type_ == types::interface::NODE {
                    let node_name = if props.get("media.class") == Some("Audio/Sink") {
                        props.get("node.name").unwrap().to_owned()
                    } else {
                        return;
                    };

                    let obj = reg.bind(id, type_, version).unwrap();
                    let node = obj.downcast::<Node>().unwrap();

                    node.add_listener(NodeEvents {
                        info: None,
                        param: some_closure!([^(tx, node_name)] _id, _type, _index, _next, pod, {
                            let mut parser = Parser::new(pod.data());
                            parser
                                .pop_object::<Prop, ParamType, _>(|object_parser, _id| {
                                    let mut muted = None;
                                    let mut volume = None;
                                    for (prop, _flags, pod) in object_parser {
                                        match prop {
                                            Prop::Mute => {
                                                muted = Some(pod.decode::<bool>().unwrap());
                                            }
                                            Prop::SoftVolumes => {
                                                volume = Some(
                                                    pod.decode::<&[f32]>()
                                                        .unwrap()
                                                        .iter()
                                                        .copied()
                                                        .next()
                                                        .unwrap(),
                                                );
                                            }
                                            _ => {}
                                        }
                                    }
                                    if let (Some(volume), Some(muted)) = (volume, muted) {
                                        send_update(
                                            tx,
                                            Update::Sink {
                                                name: node_name.clone(),
                                                state: SinkState { volume, muted },
                                            },
                                        );
                                    }
                                    Ok(())
                                })
                                .unwrap();
                        }),
                    });
                    node.subscribe_params(&[ParamType::Props]).unwrap();
                }
            }),
            global_remove: None,
        });

        std::thread::spawn(move || {
            ml.run();
            // Capture these to keep them alive
            std::mem::drop(core);
            std::mem::drop(ctx);
        });

        Ok(SoundBlock { rx })
    }
}

impl Block for SoundBlock {
    #[try_stream(boxed, ok = BlockData, error = Error)]
    async fn block_data_stream(&mut self) {
        let mut default_sink = None;
        let mut sink_states = HashMap::new();

        while let Some(update) = self.rx.recv().await {
            let updated_state = match update {
                Update::DefaultSink(name) if default_sink.as_deref() != Some(&*name) => {
                    default_sink = Some(name.clone());
                    sink_states.get(&name)
                }
                Update::Sink { name, state } => {
                    sink_states.insert(name.clone(), state);
                    if Some(&*name) == default_sink.as_deref() {
                        sink_states.get(&name)
                    } else {
                        None
                    }
                }
                _ => None,
            };

            if let Some(state) = updated_state {
                let volume = if state.muted {
                    "--".to_string()
                } else {
                    ((linear_to_user(state.volume) * 100.0).round() as u8).to_string()
                };
                yield BlockData {
                    text: format!("S: {}%", volume),
                    state: BlockState::Normal,
                };
            }
        }
    }
}

fn send_update(tx: &Sender<Update>, update: Update) {
    let tx = tx.clone();
    futures::executor::block_on(async move {
        if tx.send(update).await.is_err() {
            panic!("receiver dropped");
        }
    });
}

fn linear_to_user(linear: f32) -> f32 {
    linear.clamp(0.0, 1.0).cbrt()
}
