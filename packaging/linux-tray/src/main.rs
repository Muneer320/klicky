#![cfg(target_os = "linux")]

mod client;
mod state;

use anyhow::{bail, Context, Result};
use client::Client;
use ksni::{menu::*, Handle, MenuItem, ToolTip, TrayMethods};
use state::{label, Audio, Device, Outputs, Playback, Preference};
use std::{env, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::{mpsc, Notify};

#[derive(Clone, Debug)]
enum Action {
    Pause,
    Resume,
    Volume(u8),
    Pack(String),
    Start,
    Stop,
    Autostart(bool),
    FollowDefault,
    Select(Device),
    RetryAudio,
    CheckUpdates,
}

impl Action {
    fn arguments(&self) -> Vec<String> {
        match self {
            Self::Pause => vec!["pause".into()],
            Self::Resume => vec!["resume".into()],
            Self::Volume(percent) => {
                vec!["volume".into(), (f64::from(*percent) / 100.0).to_string()]
            }
            Self::Pack(name) => vec!["switch".into(), "--".into(), name.clone()],
            Self::Start => vec!["service".into(), "start".into()],
            Self::Stop => vec!["stop".into()],
            Self::Autostart(enabled) => vec![
                "service".into(),
                "autostart".into(),
                if *enabled { "enable" } else { "disable" }.into(),
            ],
            Self::FollowDefault => vec!["audio".into(), "follow-default".into()],
            Self::Select(device) => vec![
                "audio".into(),
                "select".into(),
                "--backend".into(),
                device.backend.replace('_', "-"),
                format!("--id={}", device.id),
            ],
            Self::RetryAudio => vec!["audio".into(), "retry".into()],
            Self::CheckUpdates => vec!["update".into(), "--check".into()],
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Snapshot {
    playback: Option<Playback>,
    status_error: String,
    audio: Option<Audio>,
    outputs: Vec<Device>,
    audio_error: String,
}

async fn read_snapshot(client: &Client) -> Snapshot {
    let mut snapshot = Snapshot::default();
    match client
        .invoke(&["status".into(), "--json".into()], Duration::from_secs(5))
        .await
        .and_then(|text| Playback::parse(&text))
    {
        Ok(playback) => snapshot.playback = Some(playback),
        Err(error) => {
            snapshot.status_error = error.to_string();
            return snapshot;
        }
    }
    if !snapshot.playback.as_ref().is_some_and(|p| p.running) {
        return snapshot;
    }
    // Older engines and default-only Linux backends retain all basic controls.
    if let Ok(audio) = client
        .invoke(&["audio".into(), "status".into()], Duration::from_secs(5))
        .await
        .and_then(|text| Audio::parse(&text))
    {
        if audio.supported() {
            if audio.can_enumerate && audio.can_select {
                match client
                    .invoke(&["audio".into(), "outputs".into()], Duration::from_secs(5))
                    .await
                    .and_then(|text| Outputs::parse(&text))
                {
                    Ok(outputs) => snapshot.outputs = outputs.devices,
                    Err(error) => snapshot.audio_error = error.to_string(),
                }
            }
            snapshot.audio = Some(audio);
        }
    }
    snapshot
}

struct App {
    snapshot: Snapshot,
    busy: bool,
    action_error: String,
    update_text: Option<String>,
    icon: String,
    actions: mpsc::Sender<Action>,
    refresh: Arc<Notify>,
    quit: Arc<Notify>,
}

impl App {
    fn submit(&mut self, action: Action) {
        if self.busy {
            return;
        }
        if self.actions.try_send(action).is_ok() {
            self.busy = true;
            self.action_error.clear();
        } else {
            self.action_error = "Another request is pending; refresh before retrying.".into();
        }
    }

    fn command(&self, title: &str, enabled: bool, action: Action) -> MenuItem<Self> {
        StandardItem {
            label: title.into(),
            enabled: enabled && !self.busy,
            activate: Box::new(move |app: &mut Self| app.submit(action.clone())),
            ..Default::default()
        }
        .into()
    }

    fn choice(
        &self,
        title: String,
        checked: bool,
        enabled: bool,
        action: Action,
    ) -> MenuItem<Self> {
        CheckmarkItem {
            label: title,
            checked,
            enabled: enabled && !checked && !self.busy,
            activate: Box::new(move |app: &mut Self| app.submit(action.clone())),
            ..Default::default()
        }
        .into()
    }

    fn audio_menu(&self, audio: &Audio) -> MenuItem<Self> {
        let confirmed = audio.requested.is_some();
        let mut items = vec![info(&format!("Output: {}", audio.availability))];
        if audio.can_select {
            items.push(self.choice(
                "Follow system default".into(),
                audio.requested == Some(Preference::SystemDefault),
                confirmed,
                Action::FollowDefault,
            ));
            for (index, device) in self.snapshot.outputs.iter().enumerate() {
                let selected = audio.requested
                    == Some(Preference::Device {
                        backend: device.backend.clone(),
                        id: device.id.clone(),
                    });
                let name = device.name.as_deref().unwrap_or("Unnamed output");
                let duplicate = self
                    .snapshot
                    .outputs
                    .iter()
                    .filter(|d| d.name == device.name)
                    .count()
                    > 1;
                let title = if duplicate {
                    format!("{} ({})", label(name), index + 1)
                } else {
                    label(name)
                };
                items.push(self.choice(title, selected, confirmed, Action::Select(device.clone())));
            }
            if let Some(Preference::Device { backend, id }) = &audio.requested {
                if audio.can_enumerate
                    && self.snapshot.audio_error.is_empty()
                    && !self
                        .snapshot
                        .outputs
                        .iter()
                        .any(|device| device.backend == *backend && device.id == *id)
                {
                    items.push(info("Remembered output is not in the current list"));
                }
            }
        }
        if audio.can_retry {
            items.push(self.command("Retry audio (keep Pause)", true, Action::RetryAudio));
        }
        if !self.snapshot.audio_error.is_empty() {
            items.push(info(&self.snapshot.audio_error));
        }
        items.push(
            StandardItem {
                label: "Refresh outputs".into(),
                activate: Box::new(|app: &mut Self| app.refresh.notify_one()),
                ..Default::default()
            }
            .into(),
        );
        SubMenu {
            label: "Audio output".into(),
            submenu: items,
            ..Default::default()
        }
        .into()
    }
}

fn info<T>(text: &str) -> MenuItem<T> {
    StandardItem {
        label: label(text),
        enabled: false,
        ..Default::default()
    }
    .into()
}

impl ksni::Tray for App {
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        "io.github.muneer320.klicky-tray".into()
    }
    fn title(&self) -> String {
        "Klicky controls".into()
    }
    fn icon_name(&self) -> String {
        self.icon.clone()
    }
    fn tool_tip(&self) -> ToolTip {
        let state = self
            .snapshot
            .playback
            .as_ref()
            .map_or("Unavailable", Playback::title);
        ToolTip {
            title: format!("Klicky: {state}"),
            description: if self.action_error.is_empty() {
                &self.snapshot.status_error
            } else {
                &self.action_error
            }
            .chars()
            .filter(|c| !c.is_control())
            .take(512)
            .collect::<String>()
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;"),
            ..Default::default()
        }
    }
    fn menu_about_to_show(&mut self) {
        self.refresh.notify_one();
    }
    fn watcher_online(&self) {
        self.refresh.notify_one();
    }
    fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
        eprintln!(
            "Tray host unavailable ({reason:?}); waiting for it to return. Playback is unchanged."
        );
        true
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let playback = self.snapshot.playback.as_ref();
        let running = playback.is_some_and(|p| p.running);
        let paused = playback.is_some_and(|p| p.paused);
        let configurable = playback.is_some_and(Playback::can_configure);
        let state = playback.map_or("Unavailable", Playback::title);
        let mut menu = vec![
            info(&format!(
                "Klicky: {state}{}",
                if self.busy { " (working…)" } else { "" }
            )),
            self.command("Pause", running && !paused, Action::Pause),
            self.command("Resume", running && paused, Action::Resume),
            MenuItem::Separator,
        ];
        let mut volume = Vec::new();
        if let Some(state) = playback {
            volume.push(info(&format!("Current: {:.0}%", state.volume * 100.0)));
        }
        for percent in [0, 10, 25, 50, 75, 100] {
            let checked =
                playback.is_some_and(|p| (p.volume - f64::from(percent) / 100.0).abs() < 0.00001);
            volume.push(self.choice(
                format!("{percent}%"),
                checked,
                configurable,
                Action::Volume(percent),
            ));
        }
        menu.push(
            SubMenu {
                label: "Volume".into(),
                submenu: volume,
                ..Default::default()
            }
            .into(),
        );
        let mut packs = Vec::new();
        if let Some(state) = playback {
            for name in &state.packs {
                packs.push(self.choice(
                    label(name),
                    *name == state.sound_pack,
                    configurable,
                    Action::Pack(name.clone()),
                ));
            }
            if !state.packs.contains(&state.sound_pack) {
                packs.push(info(&format!("{} (Unavailable)", state.sound_pack)));
            }
        }
        menu.push(
            SubMenu {
                label: "Sound packs".into(),
                submenu: packs,
                enabled: configurable,
                ..Default::default()
            }
            .into(),
        );
        if let Some(audio) = &self.snapshot.audio {
            menu.push(self.audio_menu(audio));
        }
        menu.extend([
            MenuItem::Separator,
            self.command(
                "Start playback",
                playback.is_some_and(|p| !p.running && !p.process_exists),
                Action::Start,
            ),
            self.command(
                "Stop playback",
                playback.is_some_and(|p| p.process_exists),
                Action::Stop,
            ),
            CheckmarkItem {
                label: "Start playback at login".into(),
                checked: playback.is_some_and(|p| p.autostart),
                enabled: playback.is_some() && !self.busy,
                activate: Box::new(|app: &mut Self| {
                    if let Some(state) = &app.snapshot.playback {
                        app.submit(Action::Autostart(!state.autostart));
                    }
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            self.command("Check for updates", true, Action::CheckUpdates),
        ]);
        if let Some(text) = &self.update_text {
            let lines = text
                .lines()
                .filter(|line| !line.trim().is_empty())
                .take(12)
                .map(info)
                .collect();
            menu.push(
                SubMenu {
                    label: "Update check result".into(),
                    submenu: lines,
                    ..Default::default()
                }
                .into(),
            );
        }
        if !self.snapshot.status_error.is_empty() {
            menu.push(info(&self.snapshot.status_error));
        }
        if !self.action_error.is_empty() {
            menu.push(info(&self.action_error));
        }
        menu.extend([
            StandardItem {
                label: "Refresh status".into(),
                activate: Box::new(|app: &mut Self| app.refresh.notify_one()),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit tray helper (keep playback)".into(),
                activate: Box::new(|app: &mut Self| app.quit.notify_one()),
                ..Default::default()
            }
            .into(),
        ]);
        menu
    }
}

async fn worker(
    client: Client,
    handle: &Handle<App>,
    mut actions: mpsc::Receiver<Action>,
    refresh: Arc<Notify>,
    seconds: u64,
) -> Result<()> {
    let mut timer = tokio::time::interval(Duration::from_secs(seconds));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut pending = None;
    let mut previous = None;
    loop {
        let action = if pending.is_some() {
            pending.take()
        } else {
            tokio::select! {
                biased;
                action = actions.recv() => Some(action.context("Tray service closed")?),
                _ = refresh.notified() => None,
                _ = timer.tick() => None,
            }
        };
        if let Some(action) = action {
            let duration = if matches!(action, Action::CheckUpdates) {
                25
            } else {
                10
            };
            let result = client
                .invoke(&action.arguments(), Duration::from_secs(duration))
                .await
                .and_then(|output| {
                    if matches!(action, Action::CheckUpdates) && output.trim().is_empty() {
                        bail!("Update check returned no information; nothing was installed");
                    }
                    Ok(output)
                });
            let snapshot = read_snapshot(&client).await;
            previous = Some(snapshot.clone());
            handle
                .update(move |app| {
                    app.snapshot = snapshot;
                    app.busy = false;
                    match result {
                        Ok(output) => {
                            app.action_error.clear();
                            if matches!(action, Action::CheckUpdates) {
                                app.update_text = Some(output);
                            }
                        }
                        Err(error) => {
                            app.action_error = error.to_string();
                            if matches!(action, Action::CheckUpdates) {
                                app.update_text = None;
                            }
                        }
                    }
                })
                .await
                .context("Tray service closed")?;
        } else {
            // An explicit action cancels a background refresh rather than
            // waiting behind a slow or unresponsive CLI.
            let snapshot = tokio::select! {
                biased;
                action = actions.recv() => { pending = Some(action.context("Tray service closed")?); None },
                snapshot = read_snapshot(&client) => Some(snapshot),
            };
            if let Some(snapshot) = snapshot {
                if previous.as_ref() != Some(&snapshot) {
                    previous = Some(snapshot.clone());
                    handle
                        .update(move |app| app.snapshot = snapshot)
                        .await
                        .context("Tray service closed")?;
                }
            }
        }
    }
}

struct Options {
    cli: PathBuf,
    icon: String,
    seconds: u64,
    wait: bool,
}

fn options() -> Result<Option<Options>> {
    let source_cli = env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".local/bin/klicky"));
    let mut cli = env::var_os("KLICKY_TRAY_CLI")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            source_cli
                .filter(|path| path.is_file())
                .unwrap_or_else(|| "klicky".into())
        });
    let mut icon = "input-keyboard".to_string();
    let mut seconds = 10;
    let mut wait = false;
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--cli") => {
                cli = args
                    .next()
                    .context("--cli requires an executable path")?
                    .into()
            }
            Some("--icon") => {
                icon = args
                    .next()
                    .context("--icon requires an icon name or path")?
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("Icon path must be UTF-8"))?
            }
            Some("--refresh-seconds") => {
                seconds = args
                    .next()
                    .context("--refresh-seconds requires a number")?
                    .to_string_lossy()
                    .parse()?;
                if !(2..=60).contains(&seconds) {
                    bail!("Refresh interval must be 2-60 seconds");
                }
            }
            Some("--wait-for-tray") => wait = true,
            Some("--version") => {
                println!("klicky-tray {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            Some("--help") | Some("-h") => {
                println!("Klicky Linux tray controls\n\nUsage: klicky-tray [--cli PATH] [--icon NAME_OR_PATH] [--refresh-seconds 2..60] [--wait-for-tray]\n\nDefault refresh: 10 seconds. --wait-for-tray waits for desktop tray support at login.\nRequires a graphical session with StatusNotifierItem/D-BusMenu support.\nQuit closes only this helper. Playback and login startup are separate menu controls.");
                return Ok(None);
            }
            _ => bail!("Unknown argument: {} (use --help)", arg.to_string_lossy()),
        }
    }
    Ok(Some(Options {
        cli,
        icon,
        seconds,
        wait,
    }))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let Some(options) = options()? else {
        return Ok(());
    };
    // Owning a session-bus name prevents duplicate icons without PID files,
    // daemon ownership changes or writes to the user's Klicky configuration.
    let instance = zbus::Connection::session()
        .await
        .context("Cannot start tray helper: session bus unavailable")?;
    let ownership = instance
        .request_name_with_flags(
            "io.github.muneer320.KlickyTray",
            zbus::fdo::RequestNameFlags::DoNotQueue.into(),
        )
        .await
        .context("Cannot claim the tray helper session name")?;
    if ownership != zbus::fdo::RequestNameReply::PrimaryOwner {
        bail!("Another Klicky tray helper is already running in this session");
    }
    let (sender, receiver) = mpsc::channel(1);
    let refresh = Arc::new(Notify::new());
    let quit = Arc::new(Notify::new());
    let app = App {
        snapshot: Snapshot::default(),
        busy: false,
        action_error: String::new(),
        update_text: None,
        icon: options.icon,
        actions: sender,
        refresh: refresh.clone(),
        quit: quit.clone(),
    };
    let handle = app.assume_sni_available(options.wait).spawn().await
        .context("No compatible tray host: enable StatusNotifierItem support (GNOME needs an extension), or use --wait-for-tray at login")?;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let result = tokio::select! {
        result = worker(Client { binary: options.cli }, &handle, receiver, refresh, options.seconds) => result,
        _ = quit.notified() => Ok(()),
        _ = tokio::signal::ctrl_c() => Ok(()),
        _ = terminate.recv() => Ok(()),
    };
    handle.shutdown().await;
    result
}
