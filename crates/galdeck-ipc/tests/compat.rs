//! A CLI or editor built against this crate still has to read what an older
//! daemon says, since the two are upgraded separately, and the names on the
//! wire are what scripts match on.

use galdeck_ipc::{
    BuiltInInfo, EncoderInfo, Event, KeyInfo, KeyStateInfo, Layout, Patch, PresetInfo, Request,
    Response, Status,
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
    assert_eq!(status.capabilities.desktop, "");
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
    assert!(layout.warnings.is_empty());
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
    assert!(key.label_color.is_empty() && !key.label_color_is_own);
    assert!(key.states.is_empty() && key.state.is_none() && !key.state_known);
    assert!(key.status.is_none() && key.status_interval_ms.is_none());
    assert!(key.status_result.is_none());
}

#[test]
fn a_key_s_states_carry_their_match_under_the_name_config_gives_it() {
    let state = KeyStateInfo {
        name: "on".into(),
        matches: vec!["enabled".into()],
        ..KeyStateInfo::default()
    };
    let wire = serde_json::to_value(&state).unwrap();
    assert_eq!(wire["match"], serde_json::json!(["enabled"]));
    assert!(wire.get("matches").is_none());
    // A state from a daemon that leaves fields out still reads.
    let sparse: KeyStateInfo = serde_json::from_str(r#"{"name":"off"}"#).unwrap();
    assert_eq!(sparse.name, "off");
    assert!(sparse.matches.is_empty() && sparse.exec.is_none());
    let key: KeyInfo = serde_json::from_str(
        r##"{"key":0,"index":0,"label":"Wi-Fi","text":"Wi-Fi","widget":null,"animation":null,
            "icon":"network-wireless-symbolic","exec":null,"page":null,"profile":null,
            "back":false,"background":"#5e81ac","background_is_own":false,
            "states":[{"name":"off","match":["disabled"]},{"name":"on"}],
            "state":"on","state_known":true,"status":"nmcli radio wifi",
            "status_result":{"output":"enabled","ok":true,"age_ms":1500}}"##,
    )
    .unwrap();
    assert_eq!(key.states[0].matches, ["disabled"]);
    assert_eq!(key.state.as_deref(), Some("on"));
    let result = key.status_result.unwrap();
    assert!(result.ok && result.error.is_none());
    assert_eq!(result.age_ms, 1500);
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

#[test]
fn the_messages_for_keys_with_states_have_the_names_scripts_see() {
    let wire = |request: &Request| serde_json::to_string(request).unwrap();
    assert_eq!(
        wire(&Request::SetKeyState {
            key: 2,
            state: "on".into(),
            run: true
        }),
        r#"{"cmd":"set_key_state","key":2,"state":"on","run":true}"#
    );
    assert_eq!(wire(&Request::IconNames), r#"{"cmd":"icon_names"}"#);
    assert_eq!(
        wire(&Request::RenderKeyState {
            key: 2,
            state: None
        }),
        r#"{"cmd":"render_key_state","key":2,"state":null}"#
    );
    assert_eq!(
        wire(&Request::Which {
            names: vec!["nmcli".into()]
        }),
        r#"{"cmd":"which","names":["nmcli"]}"#
    );
    // Left out, `run` runs nothing: a script never starts a command it did
    // not ask for.
    let Request::SetKeyState { run, .. } =
        serde_json::from_str(r#"{"cmd":"set_key_state","key":0,"state":"off"}"#).unwrap()
    else {
        panic!("not set_key_state");
    };
    assert!(!run);

    assert_eq!(
        serde_json::to_string(&Response::IconNames {
            names: vec!["audio-volume-muted".into()]
        })
        .unwrap(),
        r#"{"result":"icon_names","names":["audio-volume-muted"]}"#
    );
    assert_eq!(
        serde_json::to_string(&Response::Which { found: Vec::new() }).unwrap(),
        r#"{"result":"which","found":[]}"#
    );
    let Response::Which { found } = serde_json::from_str(r#"{"result":"which"}"#).unwrap() else {
        panic!("not which");
    };
    assert!(found.is_empty());

    let changed = serde_json::to_string(&Event::KeyStateChanged {
        profile: "work".into(),
        page: "main".into(),
        key: 3,
        state: Some("on".into()),
        known: false,
    })
    .unwrap();
    assert_eq!(
        changed,
        r#"{"event":"key_state_changed","profile":"work","page":"main","key":3,"state":"on","known":false}"#
    );
}

#[test]
fn a_move_goes_over_the_wire_by_its_fields() {
    let patch: Patch =
        serde_json::from_str(r#"{"op":"move","path":"pages[0].keys[0].states","from":2,"to":0}"#)
            .unwrap();
    assert_eq!(
        patch,
        Patch::Move {
            path: "pages[0].keys[0].states".into(),
            from: 2,
            to: 0
        }
    );
}

#[test]
fn the_ui_sign_in_messages_have_the_names_scripts_see() {
    let login = serde_json::to_string(&Request::UiLogin { ttl_s: Some(300) }).unwrap();
    assert_eq!(login, r#"{"cmd":"ui_login","ttl_s":300}"#);
    let rotate = serde_json::to_string(&Request::UiRotateToken).unwrap();
    assert_eq!(rotate, r#"{"cmd":"ui_rotate_token"}"#);
    let reply = serde_json::to_string(&Response::UiLogin {
        port: 8787,
        code: "00ff".into(),
    })
    .unwrap();
    assert_eq!(reply, r#"{"result":"ui_login","port":8787,"code":"00ff"}"#);
    // Written by hand with socat, the time to live can be left out.
    let Request::UiLogin { ttl_s } = serde_json::from_str(r#"{"cmd":"ui_login"}"#).unwrap() else {
        panic!("not a ui login");
    };
    assert_eq!(ttl_s, None);
}

#[test]
fn a_download_goes_over_the_wire_by_its_link() {
    let request = serde_json::to_string(&Request::FetchAsset {
        url: "https://a.example/play.png".into(),
    })
    .unwrap();
    assert_eq!(
        request,
        r#"{"cmd":"fetch_asset","url":"https://a.example/play.png"}"#
    );
}

#[test]
fn keyboard_requests_read_as_an_editor_writes_them() {
    let layout: Request = serde_json::from_str(r#"{"cmd":"keyboard_layout"}"#).unwrap();
    assert!(matches!(layout, Request::KeyboardLayout));
    let preview: Request = serde_json::from_str(
        r#"{"cmd":"preview_lighting","lighting":"effect = \"wave\"","presses":[{"key":"G","at":0.5}]}"#,
    )
    .unwrap();
    let Request::PreviewLighting {
        lighting,
        theme,
        seconds,
        presses,
        ..
    } = preview
    else {
        panic!("not a preview");
    };
    assert_eq!(lighting, "effect = \"wave\"");
    assert_eq!((theme, seconds), (None, None));
    assert_eq!(presses[0].key, "G");
    // A daemon that is not lighting the keyboard says so with no frame.
    let quiet: Response = serde_json::from_str(r#"{"result":"keyboard_frame"}"#).unwrap();
    assert!(matches!(quiet, Response::KeyboardFrame { frame: None }));
}
