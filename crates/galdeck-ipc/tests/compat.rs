//! A CLI or editor built against this crate still has to read what an older
//! daemon says, since the two are upgraded separately, and the names on the
//! wire are what scripts match on.

use galdeck_ipc::{
    BuiltInInfo, EncoderInfo, Event, KeyInfo, Layout, PresetInfo, Request, Response, Status,
};

#[test]
fn a_status_from_a_daemon_without_the_mixer_still_reads() {
    let status: Status = serde_json::from_str(
        r#"{"connected":true,"firmware":null,"serial":null,"page":"main","pages":["main"],
            "brightness":60,
            "capabilities":{"virtual_input":"ready","audio":"wpctl","media":"ok"}}"#,
    )
    .unwrap();
    assert_eq!(status.capabilities.audio, "wpctl");
    assert_eq!(status.capabilities.mixer, "");
}

#[test]
fn a_catalog_from_before_slot_rules_and_titles_still_reads() {
    let built_in: BuiltInInfo = serde_json::from_str(
        r#"{"name":"volume_up","group":"Sound","label":"volume up","step_unit":"percent",
            "step_default":2.0,"step_min":1.0,"step_max":20.0,"takes_target":true,
            "needs_virtual_input":false,"keys_only":false}"#,
    )
    .unwrap();
    assert!(!built_in.knobs_only);
    assert_eq!(built_in.target_kind, None);
    assert!(!built_in.needs_mixer);
    let preset: PresetInfo = serde_json::from_str(
        r#"{"name":"volume","label":"Turn for volume","press":null,"cw":null,"ccw":null,
            "ring_shows":"output_level","needs_virtual_input":false}"#,
    )
    .unwrap();
    assert_eq!(preset.title, "");
    assert_eq!(preset.target_kind, None);
    assert!(!preset.needs_mixer);
}

#[test]
fn a_layout_from_before_the_outputs_list_still_reads() {
    let layout: Layout = serde_json::from_str(
        r#"{"profile":"p","file":"profiles/p.toml","page":"one","page_index":0,
            "pages":["one"],"keys":[],"encoders":[],"can_go_back":false}"#,
    )
    .unwrap();
    assert!(layout.outputs.is_empty());
}

#[test]
fn a_layout_from_before_modes_and_timers_still_reads() {
    let encoder: EncoderInfo = serde_json::from_str(
        r##"{"encoder":0,"index":null,"press":null,"cw":null,"ccw":null,"ring":"#88c0d0",
            "ring_is_own":false,"animation":null,
            "layers":[{"layer":"global","file":"galdeck.toml","path":"encoders[0]",
                       "preset":"volume","step":null,
                       "gestures":{"press":null,"cw":null,"ccw":null,"hold":null}}]}"##,
    )
    .unwrap();
    assert!(encoder.modes.is_empty() && encoder.mode.is_none());
    assert!(encoder.layers[0].modes.is_empty() && encoder.layers[0].target.is_none());
    let key: KeyInfo = serde_json::from_str(
        r##"{"key":0,"index":0,"label":null,"text":null,"widget":null,"animation":null,
            "icon":null,"exec":null,"page":null,"profile":null,"back":false,
            "background":"#000000","background_is_own":false}"##,
    )
    .unwrap();
    assert!(key.timer.is_none());
}

#[test]
fn the_new_messages_have_the_names_scripts_see() {
    let request = serde_json::to_string(&Request::AudioTargets).unwrap();
    assert_eq!(request, r#"{"cmd":"audio_targets"}"#);
    let response = serde_json::to_string(&Response::AudioTargets {
        outputs: Vec::new(),
        apps: Vec::new(),
    })
    .unwrap();
    assert_eq!(
        response,
        r#"{"result":"audio_targets","outputs":[],"apps":[]}"#
    );
    let done = serde_json::to_string(&Event::TimerDone {
        profile: "work".into(),
        page: "main".into(),
        key: 3,
    })
    .unwrap();
    assert_eq!(
        done,
        r#"{"event":"timer_done","profile":"work","page":"main","key":3}"#
    );
    let mode = serde_json::to_string(&Event::ModeChanged {
        encoder: 1,
        mode: 2,
    })
    .unwrap();
    assert_eq!(mode, r#"{"event":"mode_changed","encoder":1,"mode":2}"#);
    // An empty answer from a daemon that found nothing still reads.
    let Response::AudioTargets { outputs, apps } =
        serde_json::from_str(r#"{"result":"audio_targets"}"#).unwrap()
    else {
        panic!("not audio targets");
    };
    assert!(outputs.is_empty() && apps.is_empty());
}
