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
    param::{ParamType, props::Prop},
    pod::{RawPodOwned, parser::Parser},
};
use serde::Deserialize;
use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::{Block, BlockData, BlockState};

#[derive(Debug)]
enum Update {
    DefaultSink(String),
    DefaultSource(String),
    Node { name: String, state: NodeState },
}

#[derive(Debug)]
struct NodeState {
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
                if type_ == types::interface::METADATA &&
                    props.get("metadata.name") == Some("default") {
                    let obj = reg.bind(id, type_, version).unwrap();
                    let metadata = obj.downcast::<Metadata>().unwrap();

                    metadata.add_listener(MetadataEvents {
                        property: some_closure!([^(tx)] _subject, key, _type, value, {
                            if let Some(value) = value {
                                match key {
                                    Some("default.audio.sink") => {
                                        let new_default =
                                            serde_json::from_str::<PwDevice>(value).unwrap().name;
                                        send_update(tx, Update::DefaultSink(new_default.to_owned()));
                                    },
                                    Some("default.audio.source") => {
                                        let new_default =
                                            serde_json::from_str::<PwDevice>(value).unwrap().name;
                                        send_update(tx, Update::DefaultSource(new_default.to_owned()));
                                    },
                                    _ => {},
                                }
                            }
                        }),
                    });
                } else if type_ == types::interface::NODE {
                    let node_name = match props.get("media.class") {
                        Some("Audio/Sink") | Some("Audio/Source") => {
                            props.get("node.name").unwrap().to_owned()
                        }
                        _ => return
                    };

                    let obj = reg.bind(id, type_, version).unwrap();
                    let node = obj.downcast::<Node>().unwrap();

                    node.add_listener(NodeEvents {
                        info: None,
                        param: some_closure!([^(tx, node_name)] _id, _type, _index, _next, pod, {
                            if let Some((volume, muted)) = parse_props(pod) {
                                send_update(
                                    tx,
                                    Update::Node {
                                        name: node_name.clone(),
                                        state: NodeState { volume, muted },
                                    },
                                );
                            }
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
        let mut default_source = None;
        let mut node_states = HashMap::new();

        while let Some(update) = self.rx.recv().await {
            match update {
                Update::DefaultSink(name) => {
                    default_sink = Some(name);
                }
                Update::DefaultSource(name) => {
                    default_source = Some(name);
                }
                Update::Node { name, state } => {
                    node_states.insert(name, state);
                }
            }

            let sink_state = default_sink.as_ref().and_then(|ds| node_states.get(ds));
            let source_state = default_source.as_ref().and_then(|ds| node_states.get(ds));
            let sink_volume = match sink_state {
                Some(ss) => {
                    if ss.muted {
                        "--".to_string()
                    } else {
                        ((ss.volume.cbrt() * 100.0).round() as u8).to_string()
                    }
                }
                None => "???".to_owned(),
            };
            let mic_state = match source_state {
                Some(ss) => {
                    if ss.muted {
                        "M"
                    } else {
                        "L"
                    }
                }
                None => "???",
            };
            yield BlockData {
                text: format!("S: {}%  M: {}", sink_volume, mic_state),
                state: BlockState::Normal,
            };
        }
    }
}

// Not entirely what the right Pipewire terminology is for this
#[derive(Deserialize)]
pub struct PwDevice<'a> {
    name: &'a str,
}

fn send_update(tx: &Sender<Update>, update: Update) {
    let tx = tx.clone();
    futures::executor::block_on(async move {
        if tx.send(update).await.is_err() {
            panic!("receiver dropped");
        }
    });
}

fn parse_props(pod: &RawPodOwned) -> Option<(f32, bool)> {
    let mut parser = Parser::new(pod.data());

    let mut muted = None;
    let mut volume = None;

    parser
        .pop_object::<Prop, ParamType, _>(|object_parser, _id| {
            for (prop, _flags, pod) in object_parser {
                match prop {
                    Prop::Mute => {
                        muted = Some(pod.decode::<bool>().unwrap());
                    }
                    Prop::ChannelVolumes => {
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
            Ok(())
        })
        .unwrap();

    Some((volume?, muted?))
}
