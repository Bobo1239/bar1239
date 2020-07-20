use std::{cell::RefCell, collections::HashMap, mem, rc::Rc, string::ToString};

use anyhow::{anyhow, Error, Result};
use futures_async_stream::try_stream;
use libpulse_binding::{
    callbacks::ListResult::Item,
    context,
    context::{
        subscribe::{subscription_masks, Facility, Operation},
        Context, State,
    },
    mainloop::threaded::Mainloop,
    volume::VOLUME_NORM,
};
use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::{Block, BlockData, BlockState};

#[derive(Debug)]
enum Update {
    DefaultSink(u32),
    Sink { idx: u32, state: SinkState },
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
        // TODO: Unsure if this is the most optimal channel...
        let (tx, rx) = mpsc::channel(10);

        // See the libpulse_binding's `Mainloop` docs for more context here

        let main_loop = Rc::new(RefCell::new(
            Mainloop::new().ok_or_else(|| anyhow!("failed to create mainloop"))?,
        ));

        let context = Rc::new(RefCell::new(
            Context::new(&*main_loop.borrow(), "bar1239")
                .ok_or_else(|| anyhow!("failed to create context"))?,
        ));

        {
            let ml_ref = Rc::clone(&main_loop);
            let context_ref = Rc::clone(&context);
            context
                .borrow_mut()
                .set_state_callback(Some(Box::new(move || {
                    let state = unsafe { (*context_ref.as_ptr()).get_state() };
                    match state {
                        State::Ready | State::Failed | State::Terminated => unsafe {
                            (*ml_ref.as_ptr()).signal(false);
                        },
                        _ => {}
                    }
                })));
        }

        context
            .borrow_mut()
            .connect(None, context::flags::NOFLAGS, None)
            .map_err(|e| anyhow!(e))?;

        main_loop.borrow_mut().lock();
        main_loop.borrow_mut().start().map_err(|e| anyhow!(e))?;

        loop {
            match context.borrow().get_state() {
                State::Ready => {
                    break;
                }
                State::Failed | State::Terminated => {
                    main_loop.borrow_mut().unlock();
                    main_loop.borrow_mut().stop();
                    return Err(anyhow!("context state failed"));
                }
                _ => {
                    main_loop.borrow_mut().wait();
                }
            }
        }
        context.borrow_mut().set_state_callback(None);

        let interest_mask = subscription_masks::SERVER | subscription_masks::SINK;
        context.borrow_mut().subscribe(interest_mask, |_| {});

        {
            let tx = tx.clone();
            let context_ref = Rc::clone(&context);
            context.borrow_mut().set_subscribe_callback(Some(Box::new(
                move |facility, operation, idx| {
                    if let Some(Operation::Changed) = operation {
                        match facility {
                            Some(Facility::Sink) => send_sink_info_for_idx(&context_ref, idx, &tx),
                            Some(Facility::Server) => {
                                send_default_sink(&context_ref, &tx);
                            }
                            _ => {}
                        }
                    }
                },
            )));
        }

        // Send the default sink once at the start so we know which sink state to display
        send_default_sink(&context, &tx);
        // Send initial sink states
        {
            let introspector = context.borrow().introspect();
            let context_ref = Rc::clone(&context);
            introspector.get_sink_info_list(move |list_result| {
                if let Item(sink_info) = list_result {
                    send_sink_info_for_idx(&context_ref, sink_info.index, &tx)
                }
            });
        }

        main_loop.borrow_mut().unlock();

        // Leak the values so we don't have to take care of the Send + Sync requirements for our
        // async generator; Maybe there's a cleaner way...
        mem::forget(main_loop);
        mem::forget(context);

        Ok(SoundBlock { rx })
    }
}

impl Block for SoundBlock {
    #[try_stream(boxed, ok = BlockData, error = Error)]
    async fn block_data_stream(&mut self) {
        // Just a temporary idx until we get the real default sink
        let mut default_sink = 1337;
        let mut cached_sink_state = HashMap::new();

        while let Some(update) = self.rx.recv().await {
            let updated_state = match update {
                Update::DefaultSink(idx) if idx != default_sink => {
                    default_sink = idx;
                    cached_sink_state.get(&idx)
                }
                Update::Sink { idx, state } => {
                    cached_sink_state.insert(idx, state);
                    if idx == default_sink {
                        cached_sink_state.get(&idx)
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
                    ((state.volume * 100.0).round() as u8).to_string()
                };
                yield BlockData {
                    text: format!("S: {}%", volume),
                    state: BlockState::Normal,
                };
            }
        }
    }
}

fn send_sink_info_for_idx(context: &Rc<RefCell<Context>>, idx: u32, tx: &Sender<Update>) {
    let introspector = context.borrow().introspect();
    let tx = tx.clone();
    introspector.get_sink_info_by_index(idx, move |list_result| {
        if let Item(sink_info) = list_result {
            send_update(
                &tx,
                Update::Sink {
                    idx,
                    state: SinkState {
                        volume: sink_info.volume.avg().0 as f32 / VOLUME_NORM.0 as f32,
                        muted: sink_info.mute,
                    },
                },
            )
        }
    });
}

fn send_default_sink(context: &Rc<RefCell<Context>>, tx: &Sender<Update>) {
    let introspector = context.borrow().introspect();
    let context_ref = Rc::clone(&context);
    let tx = tx.clone();
    introspector.get_server_info(move |server_info| {
        if let Some(default_sink_name) = &server_info.default_sink_name {
            let introspector = context_ref.borrow().introspect();
            let tx = tx.clone();
            introspector.get_sink_info_by_name(&default_sink_name, move |list_result| {
                if let Item(sink_info) = list_result {
                    send_update(&tx, Update::DefaultSink(sink_info.index))
                }
            });
        }
    });
}

fn send_update(tx: &Sender<Update>, update: Update) {
    let mut tx = tx.clone();
    futures::executor::block_on(async move {
        if tx.send(update).await.is_err() {
            panic!("receiver dropped");
        }
    });
}
