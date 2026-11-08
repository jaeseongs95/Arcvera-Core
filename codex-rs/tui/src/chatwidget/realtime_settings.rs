//! Voice settings hierarchy with parent navigation; selections apply to subsequent conversations.

use super::*;
use crate::bottom_pane::MultiSelectItem;
use crate::bottom_pane::MultiSelectPicker;
use codex_config::config_toml::MicrophoneChannels;
use codex_protocol::protocol::RealtimeVoice;
use codex_protocol::protocol::RealtimeVoicesList;
use codex_realtime_webrtc::AudioDevice;
use codex_realtime_webrtc::AudioDeviceKind;

fn back_item(parent: fn() -> AppEvent) -> SelectionItem {
    SelectionItem {
        name: crate::i18n::static_message("controls-back").to_string(),
        actions: vec![Box::new(move |tx| tx.send(parent()))],
        dismiss_on_select: true,
        ..Default::default()
    }
}

impl ChatWidget {
    pub(super) fn realtime_audio_settings(
        &mut self,
    ) -> Option<codex_config::config_toml::RealtimeAudioToml> {
        match self.local_settings.audio.clone() {
            Ok(audio) => Some(audio),
            Err(error) => {
                self.add_error_message(error);
                None
            }
        }
    }

    pub(crate) fn open_realtime_settings(&mut self) {
        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some(crate::i18n::static_message("controls-voice-settings").to_string()),
            items: vec![
                SelectionItem {
                    name: crate::i18n::static_message("controls-voice-devices").to_string(),
                    actions: vec![Box::new(|tx| tx.send(AppEvent::OpenRealtimeSoundDevices))],
                    dismiss_on_select: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: crate::i18n::static_message("controls-voice-choose").to_string(),
                    actions: vec![Box::new(|tx| tx.send(AppEvent::OpenRealtimeVoices))],
                    dismiss_on_select: true,
                    ..Default::default()
                },
            ],
            footer_hint: Some(standard_popup_hint_line()),
            ..SelectionViewParams::picker()
        });
    }

    pub(crate) fn open_realtime_voices(
        &mut self,
        current: Option<RealtimeVoice>,
        voices: RealtimeVoicesList,
    ) {
        // The TUI uses V3, which shares the V1 voice catalog.
        let mut items: Vec<SelectionItem> = voices
            .v1
            .into_iter()
            .map(|voice| SelectionItem {
                name: voice.wire_name().to_string(),
                is_current: Some(voice) == current,
                actions: vec![Box::new(move |tx| {
                    tx.send(AppEvent::PersistRealtimeVoiceSelection { voice });
                })],
                dismiss_on_select: true,
                ..Default::default()
            })
            .collect();
        items.push(back_item(|| AppEvent::OpenRealtimeSettings));
        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some(crate::i18n::static_message("controls-voice-select").to_string()),
            subtitle: Some(crate::i18n::static_message("controls-voice-next").to_string()),
            footer_hint: Some(standard_popup_hint_line()),
            items,
            on_cancel: Some(Box::new(|tx| tx.send(AppEvent::OpenRealtimeSettings))),
            ..SelectionViewParams::picker()
        });
    }

    pub(crate) fn open_realtime_sound_devices(&mut self) {
        let Some(audio) = self.realtime_audio_settings() else {
            return;
        };
        let mut items = Vec::new();
        for (kind, label, selected) in [
            (
                AudioDeviceKind::Input,
                crate::i18n::static_message("controls-voice-input-device"),
                audio.microphone,
            ),
            (
                AudioDeviceKind::Output,
                crate::i18n::static_message("controls-voice-output-device"),
                audio.speaker,
            ),
        ] {
            items.push(SelectionItem {
                name: label.into(),
                description: Some(selected.unwrap_or_else(|| {
                    crate::i18n::static_message("controls-voice-system-default").into()
                })),
                actions: vec![Box::new(move |tx| {
                    tx.send(AppEvent::OpenRealtimeDevicePicker { kind })
                })],
                dismiss_on_select: true,
                ..Default::default()
            });
        }
        items.push(back_item(|| AppEvent::OpenRealtimeSettings));
        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some(crate::i18n::static_message("controls-voice-sound-devices").to_string()),
            subtitle: Some(crate::i18n::static_message("controls-voice-next").into()),
            footer_hint: Some(standard_popup_hint_line()),
            items,
            on_cancel: Some(Box::new(|tx| tx.send(AppEvent::OpenRealtimeSettings))),
            ..SelectionViewParams::picker()
        });
    }

    pub(crate) fn open_realtime_device_picker(
        &mut self,
        kind: AudioDeviceKind,
        devices: Vec<AudioDevice>,
    ) {
        let Some(audio) = self.realtime_audio_settings() else {
            return;
        };
        let (title, current) = match kind {
            AudioDeviceKind::Input => (
                crate::i18n::static_message("controls-voice-input-device"),
                audio.microphone,
            ),
            AudioDeviceKind::Output => (
                crate::i18n::static_message("controls-voice-output-device"),
                audio.speaker,
            ),
        };
        let selected = devices
            .iter()
            .find(|device| {
                current
                    .as_ref()
                    .map_or(device.is_default, |name| *name == device.name)
            })
            .cloned();
        let ambiguous: std::collections::HashSet<_> = devices
            .iter()
            .filter(|device| {
                devices
                    .iter()
                    .filter(|other| other.name == device.name)
                    .count()
                    > 1
            })
            .map(|device| device.name.clone())
            .collect();
        let mut items: Vec<SelectionItem> = std::iter::once(/*value*/ None)
            .chain(devices.into_iter().map(Some))
            .map(|device| {
                let name = device.map(|device| device.name);
                let is_current = name == current;
                let disabled_reason = name
                    .as_ref()
                    .filter(|name| ambiguous.contains(*name))
                    .map(|_| crate::i18n::static_message("controls-voice-ambiguous").into());
                SelectionItem {
                    name: name.clone().unwrap_or_else(|| {
                        crate::i18n::static_message("controls-voice-system-default").into()
                    }),
                    is_current,
                    disabled_reason,
                    actions: vec![Box::new(move |tx| {
                        tx.send(AppEvent::PersistRealtimeDevice {
                            kind,
                            name: name.clone(),
                        })
                    })],
                    dismiss_on_select: true,
                    ..Default::default()
                }
            })
            .collect();
        if kind == AudioDeviceKind::Input
            && let Some(device) = selected.filter(|device| device.channels >= 3)
        {
            let parent = items.iter().position(|item| item.is_current).unwrap_or(0);
            items.insert(
                parent + 1,
                SelectionItem {
                    name: crate::i18n::static_message("controls-voice-input-channels").into(),
                    child_label: Some('a'),
                    description: Some(audio.microphone_channel.as_ref().map_or_else(
                        || crate::i18n::static_message("controls-voice-all-channels").into(),
                        |channels| {
                            crate::i18n::message(
                                "controls-voice-inputs",
                                &[(
                                    "value",
                                    channels
                                        .as_slice()
                                        .iter()
                                        .map(ToString::to_string)
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                        .as_str(),
                                )],
                            )
                        },
                    )),
                    actions: vec![Box::new(move |tx| {
                        tx.send(AppEvent::OpenRealtimeInputChannels {
                            device: device.clone(),
                        })
                    })],
                    dismiss_on_select: true,
                    ..Default::default()
                },
            );
        }
        items.push(back_item(|| AppEvent::OpenRealtimeSoundDevices));
        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some(title.into()),
            subtitle: Some(crate::i18n::static_message("controls-voice-next").into()),
            items,
            footer_hint: Some(standard_popup_hint_line()),
            on_cancel: Some(Box::new(|tx| tx.send(AppEvent::OpenRealtimeSoundDevices))),
            ..SelectionViewParams::picker()
        });
    }

    pub(crate) fn open_realtime_input_channels(&mut self, microphone: AudioDevice) {
        let Some(audio) = self.realtime_audio_settings() else {
            return;
        };
        let current = audio.microphone_channel;
        let count = microphone.channels;
        let picker = MultiSelectPicker::builder(
            crate::i18n::message(
                "controls-voice-microphone",
                &[("value", microphone.name.as_str())],
            ),
            Some(crate::i18n::static_message("controls-voice-channel-help").into()),
            self.app_event_tx.clone(),
        )
        .list_keymap(self.bottom_pane.list_keymap())
        .items(
            (1..=count)
                .map(|number| MultiSelectItem {
                    id: number.to_string(),
                    name: crate::i18n::message(
                        "controls-voice-input",
                        &[("value", number.to_string().as_str())],
                    ),
                    enabled: current.as_ref().is_none_or(|channels| {
                        channels
                            .as_slice()
                            .iter()
                            .any(|channel| channel.get() == number)
                    }),
                    ..Default::default()
                })
                .collect(),
        )
        .require_selection()
        .on_preview(|items| {
            items.iter().all(|item| !item.enabled).then(|| {
                crate::i18n::static_message("controls-voice-channel-required")
                    .red()
                    .into()
            })
        })
        .on_confirm(move |ids, tx| {
            let selected: Vec<_> = (1..=count)
                .filter(|number| ids.contains(&number.to_string()))
                .filter_map(std::num::NonZeroU16::new)
                .collect();
            let channel = if selected.len() == usize::from(count) {
                None
            } else if selected.len() == 1 {
                selected.first().copied().map(MicrophoneChannels::Single)
            } else {
                Some(MicrophoneChannels::Multiple(selected))
            };
            tx.send(AppEvent::PersistRealtimeInputChannel { channel });
        })
        .on_cancel(|tx| {
            tx.send(AppEvent::OpenRealtimeDevicePicker {
                kind: AudioDeviceKind::Input,
            })
        })
        .build();
        self.bottom_pane.show_view(Box::new(picker));
    }

    pub(crate) fn set_realtime_voice(&mut self, voice: Option<RealtimeVoice>) {
        self.config.realtime.voice = voice;
    }

    pub(crate) fn on_realtime_voice_saved(&mut self, voice: RealtimeVoice) {
        self.set_realtime_voice(Some(voice));
        self.add_info_message(
            crate::i18n::message("controls-voice-saved", &[("value", voice.wire_name())]),
            /*hint*/ None,
        );
    }
}

#[cfg(test)]
#[path = "realtime_settings_tests.rs"]
mod tests;

#[cfg(test)]
mod localization_tests {
    use super::*;
    use crate::chatwidget::tests::make_chatwidget_manual_with_sender;
    use crate::chatwidget::tests::render_bottom_popup;
    use crossterm::event::KeyCode;
    use crossterm::event::KeyEvent;
    use crossterm::event::KeyModifiers;

    #[tokio::test]
    async fn voice_device_picker_localizes_labels_and_preserves_selected_device_payload() {
        let name = "English Mic { $value } 한글";
        for (locale, title, default) in [
            (
                crate::i18n::Locale::English,
                "Input device",
                "System default",
            ),
            (crate::i18n::Locale::Korean, "입력 장치", "시스템 기본값"),
        ] {
            let (mut chat, _sender, mut events, mut ops) =
                make_chatwidget_manual_with_sender().await;
            while events.try_recv().is_ok() {}
            chat.local_settings.audio.as_mut().unwrap().microphone = None;
            crate::i18n::with_locale(locale, || {
                chat.open_realtime_device_picker(
                    AudioDeviceKind::Input,
                    vec![AudioDevice {
                        name: name.to_string(),
                        channels: 2,
                        is_default: true,
                    }],
                );
                let screen = render_bottom_popup(&chat, 100);
                assert!(screen.contains(title), "{screen}");
                assert!(screen.contains(default), "{screen}");
                assert!(screen.contains(name), "{screen}");
                for code in [KeyCode::Home, KeyCode::Down, KeyCode::Enter] {
                    chat.bottom_pane
                        .handle_key_event(KeyEvent::new(code, KeyModifiers::NONE));
                }
            });
            assert!(std::iter::from_fn(|| events.try_recv().ok()).any(|event| matches!(event,
                AppEvent::PersistRealtimeDevice { kind: AudioDeviceKind::Input, name: Some(selected) }
                if selected == name)));
            assert!(ops.try_recv().is_err());
            assert!(!chat.realtime_conversation_is_running());
        }
    }
}
