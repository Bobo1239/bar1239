use std::{convert::TryFrom, u32};

use anyhow::Result;
use pipewire_native::{
    context::Context,
    core::CoreEvents,
    main_loop::{MainLoop, MainLoopEvents},
    properties::Properties,
    proxy::{
        metadata::{Metadata, MetadataEvents},
        node::{Node, NodeEvents},
        registry::RegistryEvents,
        HasProxy, ProxyEvents,
    },
    some_closure,
    types::{self, params::ParamBuilder},
};
use pipewire_native_spa::{
    param::{props::Prop, ParamType},
    pod::{parser::Parser, types::Property},
};

fn main() -> Result<()> {
    pipewire_native::init();

    // TODO
    let default_sink =
        "alsa_output.usb-Focusrite_Scarlett_2i2_USB_Y8QTJR70954754-00.HiFi__Line__sink";

    let props = Properties::new();
    let ml = MainLoop::new(&props).unwrap();

    let ctx = Context::new(&ml, props)?;
    let core = ctx.connect(None)?;

    let mut core_events = CoreEvents::default();
    core_events.info = Some(Box::new(|a| {
        dbg!(a.props);
    }));
    core.add_listener(core_events);

    let reg = core.registry()?;
    // FIXME: Binding somehow causes error when changing volume; probably upstream bug?
    let obj = reg.bind(40, "PipeWire:Interface:Metadata", 3).unwrap();
    std::mem::drop(obj);

    // let events = RegistryEvents {
    //     global: some_closure!([reg] id, _perms, type_, version, _props, {
    //         // dbg!(type_);
    //         // dbg!(_props);
    //         if type_ == types::interface::METADATA && _props.get("metadata.name") == Some("default") {
    //             dbg!(id, type_, version);
    //             // let obj = reg.bind(id, type_, version).unwrap();
    //             // let metadata = obj.downcast::<Metadata>().unwrap();

    //             // metadata.add_listener(MetadataEvents {
    //             //     property: some_closure!([metadata] subject, key, type_, value, {
    //             //         dbg!(key, value);
    //             //         // state.metadata_property(&metadata, subject, key, type_, value);
    //             //     }),
    //             // });

    //             // std::mem::drop(metadata);
    //             // std::mem::drop(obj);

    //         } else if type_ == types::interface::NODE {
    //             if _props.get("node.name") != Some(default_sink) {
    //                 // dbg!(_props);
    //                 return;
    //             }
    //             let obj = reg.bind(id, type_, version).unwrap();
    //             let node = obj.downcast::<Node>().unwrap();
    //             // dbg!(_props);
    //             node.subscribe_params(&[ParamType::Props]).unwrap();

    //             node.add_listener(NodeEvents {
    //                 info: None,
    //                 param: Some(Box::new(|id, type_, _index, _next, pod| {
    //                     let mut parser = Parser::new(pod.data());
    //                     parser
    //                         .pop_object_raw::<ParamType, _>(|object_parser, _, _id| {
    //                             for (id, _flags, pod) in object_parser {
    //                                 if let Ok(known_prop) = Prop::try_from(id) {
    //                                     match known_prop {
    //                                         Prop::Mute => {
    //                                             dbg!(pod.decode::<bool>().unwrap());
    //                                         }
    //                                         Prop::SoftVolumes => {
    //                                             dbg!(pod
    //                                                 .decode::<&[f32]>()
    //                                                 .unwrap()
    //                                                 .iter()
    //                                                 .map(|x| linear_to_user(*x))
    //                                                 .collect::<Vec<_>>());
    //                                         }
    //                                         _ => {}
    //                                     }
    //                                 }
    //                             }
    //                             Ok(())
    //                         })
    //                         .unwrap();
    //                 })),
    //             });
    //             // node.enum_params(0, None, 0, u32::MAX, ParamBuilder).unwrap();
    //         }
    //     }),
    //     global_remove: None,
    // };
    // reg.add_listener(events);

    ml.run();

    Ok(())
}

fn linear_to_user(linear: f32) -> f32 {
    linear.clamp(0.0, 1.0).cbrt()
}
