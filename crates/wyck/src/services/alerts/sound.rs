//! The sound an alert makes, and the desktop notification that goes with it.
//!
//! What to play and whether to show a notification is decided by [`plan`], a plain function.
//! Playing and notifying happen on a thread of their own ([`speaker`]): opening an audio device or
//! talking to the notification service can take a while, and neither may hold up the window. A
//! failure is logged and never reaches the user as an error: an alert that cannot be heard still
//! shows its notice in the app.
//!
//! The sounds are the system's own (each OS has its own), the ones built into the app (made by
//! `scripts/make_sounds.py`), or a file of the user's.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::mpsc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// The sounds built into the app.
#[derive(rust_embed::RustEmbed)]
#[folder = "assets/sounds"]
struct Builtin;

/// The largest file of the user's that is accepted.
pub const MAX_CUSTOM_BYTES: u64 = 4 * 1024 * 1024;
/// The longest a sound of the user's may last: an alert is a short signal.
pub const MAX_CUSTOM_SECONDS: f32 = 15.0;

/// Which sound to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundKind {
    /// Silence.
    Off,
    /// The notification sound of the operating system.
    System,
    #[default]
    Ping,
    Chime,
    Pulse,
    Drop,
    Alert,
    Glass,
    /// The file the user chose (see [`Output::custom`]).
    Custom,
}

impl SoundKind {
    pub const ALL: [Self; 9] = [
        Self::Off,
        Self::System,
        Self::Ping,
        Self::Chime,
        Self::Pulse,
        Self::Drop,
        Self::Alert,
        Self::Glass,
        Self::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Silent",
            Self::System => "System",
            Self::Ping => "Ping",
            Self::Chime => "Chime",
            Self::Pulse => "Pulse",
            Self::Drop => "Drop",
            Self::Alert => "Triple",
            Self::Glass => "Glass",
            Self::Custom => "My file",
        }
    }

    /// The file of a built-in sound.
    fn asset(self) -> Option<&'static str> {
        match self {
            Self::Ping => Some("ping.wav"),
            Self::Chime => Some("chime.wav"),
            Self::Pulse => Some("pulse.wav"),
            Self::Drop => Some("drop.wav"),
            Self::Alert => Some("alert.wav"),
            Self::Glass => Some("glass.wav"),
            Self::Off | Self::System | Self::Custom => None,
        }
    }
}

/// What the user chose for every alert: kept in the preferences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Output {
    pub sound: SoundKind,
    /// The volume, from 0 to 100.
    pub volume: u8,
    /// A desktop notification when an alert fires.
    pub notify: bool,
    /// The notification only shows when the window is not the one in use: the notice inside the
    /// app already says it.
    pub notify_in_background_only: bool,
    /// The file for [`SoundKind::Custom`]: a copy in the settings folder.
    pub custom: Option<String>,
}

impl Default for Output {
    fn default() -> Self {
        Self {
            sound: SoundKind::default(),
            volume: 70,
            notify: true,
            notify_in_background_only: true,
            custom: None,
        }
    }
}

impl Output {
    /// The choices put back in range.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.volume = self.volume.min(100);
        self.custom = self
            .custom
            .map(|c| c.trim().to_owned())
            .filter(|c| !c.is_empty());
        // Without a file, "my file" is not a sound: go back to the default one.
        if self.sound == SoundKind::Custom && self.custom.is_none() {
            self.sound = SoundKind::default();
        }
        self
    }
}

/// What to do when an alert fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub sound: Option<SoundKind>,
    pub notification: bool,
}

/// What to do for an alert. `own` is the sound the alert chose for itself; `window_in_use` is
/// whether one of the windows of the app is the one the user is working in.
pub fn plan(output: &Output, own: Option<SoundKind>, window_in_use: bool) -> Plan {
    let kind = own.unwrap_or(output.sound);
    let kind = if kind == SoundKind::Custom && output.custom.is_none() {
        SoundKind::default()
    } else {
        kind
    };
    Plan {
        sound: (kind != SoundKind::Off && output.volume > 0).then_some(kind),
        notification: output.notify && !(output.notify_in_background_only && window_in_use),
    }
}

/// The gain for a volume from 0 to 100. The ear follows the square of it more closely than the
/// line, so 50 sounds like half.
fn gain(volume: u8) -> f32 {
    let v = f32::from(volume.min(100)) / 100.0;
    v * v
}

/// The files the operating system keeps for its own notifications, best first.
fn system_files() -> Vec<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let root = std::env::var_os("SystemRoot")
            .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        let media = root.join("Media");
        [
            "Windows Notify System Generic.wav",
            "Windows Notify Messaging.wav",
            "Windows Background.wav",
            "notify.wav",
            "chimes.wav",
            "ding.wav",
        ]
        .iter()
        .map(|name| media.join(name))
        .collect()
    }
    #[cfg(target_os = "macos")]
    {
        ["Glass", "Ping", "Tink", "Pop"]
            .iter()
            .map(|name| PathBuf::from(format!("/System/Library/Sounds/{name}.aiff")))
            .collect()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let names = ["message", "complete", "dialog-warning", "bell"];
        let mut files = Vec::new();
        for dir in [
            "/usr/share/sounds/freedesktop/stereo",
            "/usr/share/sounds/ubuntu/stereo",
            "/usr/share/sounds/gnome/default/alerts",
        ] {
            for name in names {
                for ext in ["oga", "ogg", "wav"] {
                    files.push(Path::new(dir).join(format!("{name}.{ext}")));
                }
            }
        }
        files
    }
}

/// How a sound is played.
enum Source {
    /// Samples the audio library decodes.
    Bytes(Vec<u8>),
    /// A file the library cannot decode but the system can play (`afplay` on macOS).
    #[cfg(target_os = "macos")]
    System(PathBuf),
}

/// Finds what to play for a kind of sound. A sound that cannot be found (no system sound, a file
/// of the user's that was removed) gives the built-in ping, so an alert is never silent by
/// accident.
fn source(kind: SoundKind, custom: Option<&Path>) -> Option<Source> {
    let builtin = |name: &str| Builtin::get(name).map(|f| Source::Bytes(f.data.into_owned()));
    match kind {
        SoundKind::Off => None,
        SoundKind::System => system_files()
            .into_iter()
            .find(|f| f.is_file())
            .and_then(system_source)
            .or_else(|| builtin("ping.wav")),
        SoundKind::Custom => custom
            .and_then(|path| read_custom(path).ok())
            .map(Source::Bytes)
            .or_else(|| builtin("ping.wav")),
        other => other.asset().and_then(builtin),
    }
}

/// What plays a file of the system: macOS keeps its sounds as `.aiff`, which the audio library does
/// not decode, so `afplay` plays them.
#[cfg(target_os = "macos")]
fn system_source(file: PathBuf) -> Option<Source> {
    Some(Source::System(file))
}

/// What plays a file of the system: the audio library, from the bytes of the file.
#[cfg(not(target_os = "macos"))]
fn system_source(file: PathBuf) -> Option<Source> {
    std::fs::read(file).ok().map(Source::Bytes)
}

/// Reads a file the user chose, refusing what is too big.
fn read_custom(path: &Path) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_CUSTOM_BYTES {
        return Err("The file is over 4 MB".to_owned());
    }
    std::fs::read(path).map_err(|e| e.to_string())
}

/// Checks that a file can be used as an alert sound: it decodes, and it is short. Returns its
/// length in seconds. This is what the settings run before they keep a copy.
pub fn check_file(path: &Path) -> Result<f32, String> {
    use rodio::Source as _;
    let bytes = read_custom(path)?;
    let decoder = rodio::Decoder::new(Cursor::new(bytes))
        .map_err(|_| "This is not a sound the app can play (wav, ogg, mp3 or flac)".to_owned())?;
    // Some formats do not say how long they are: the length is then counted while playing, and
    // the player cuts the sound off at the limit.
    let seconds = decoder.total_duration().map_or(0.0, |d| d.as_secs_f32());
    if seconds > MAX_CUSTOM_SECONDS {
        return Err(format!(
            "The sound lasts {seconds:.0} s: an alert sound is at most {MAX_CUSTOM_SECONDS:.0} s"
        ));
    }
    Ok(seconds)
}

/// The work for the thread of [`speaker`].
enum Job {
    Sound {
        kind: SoundKind,
        custom: Option<PathBuf>,
        volume: u8,
    },
    Notify {
        title: String,
        body: String,
    },
}

/// The most notifications shown for a burst of alerts: more is noise.
const MAX_NOTIFICATIONS: usize = 4;

/// Plays sounds and shows notifications, one thing at a time, off the main thread.
pub struct Speaker {
    jobs: mpsc::Sender<Job>,
}

impl Speaker {
    fn start() -> Self {
        let (jobs, inbox) = mpsc::channel::<Job>();
        let spawned = std::thread::Builder::new()
            .name("wyck-alert-output".to_owned())
            .spawn(move || run(&inbox));
        if let Err(error) = spawned {
            tracing::warn!(%error, "could not start the thread of the alert sounds");
        }
        Self { jobs }
    }

    /// Plays a sound. Sounds asked while one plays are dropped but the last, which plays next: a
    /// burst of alerts is one sound, not a rattle.
    pub fn play(&self, kind: SoundKind, custom: Option<&str>, volume: u8) {
        let _ = self.jobs.send(Job::Sound {
            kind,
            custom: custom.map(PathBuf::from),
            volume,
        });
    }

    /// Shows a desktop notification.
    pub fn notify(&self, title: &str, body: &str) {
        let _ = self.jobs.send(Job::Notify {
            title: title.to_owned(),
            body: body.to_owned(),
        });
    }
}

/// The one speaker of the app, started when it is first needed.
pub fn speaker() -> &'static Speaker {
    static SPEAKER: OnceLock<Speaker> = OnceLock::new();
    SPEAKER.get_or_init(Speaker::start)
}

fn run(inbox: &mpsc::Receiver<Job>) {
    while let Ok(first) = inbox.recv() {
        let mut batch = vec![first];
        // What piled up while the last job ran.
        std::thread::sleep(Duration::from_millis(30));
        while let Ok(job) = inbox.try_recv() {
            batch.push(job);
        }
        let last_sound = batch
            .iter()
            .rposition(|job| matches!(job, Job::Sound { .. }));
        let mut shown = 0;
        for (index, job) in batch.into_iter().enumerate() {
            match job {
                Job::Notify { title, body } if shown < MAX_NOTIFICATIONS => {
                    shown += 1;
                    show(&title, &body);
                }
                Job::Notify { .. } => {}
                Job::Sound {
                    kind,
                    custom,
                    volume,
                } if Some(index) == last_sound => {
                    if let Err(error) = sound(kind, custom.as_deref(), volume) {
                        tracing::warn!(%error, "could not play an alert sound");
                    }
                }
                Job::Sound { .. } => {}
            }
        }
    }
}

/// Plays one sound to its end. The output device is opened for the sound and closed after it, so
/// a change of device (headphones, a bluetooth speaker going to sleep) never leaves it stuck.
fn sound(kind: SoundKind, custom: Option<&Path>, volume: u8) -> Result<(), String> {
    let Some(source) = source(kind, custom) else {
        return Ok(());
    };
    match source {
        Source::Bytes(bytes) => play_bytes(bytes, gain(volume)),
        #[cfg(target_os = "macos")]
        Source::System(file) => {
            let status = std::process::Command::new("afplay")
                .arg("-v")
                .arg(format!("{:.2}", gain(volume)))
                .arg(file)
                .status()
                .map_err(|e| e.to_string())?;
            if status.success() {
                Ok(())
            } else {
                Err("afplay failed".to_owned())
            }
        }
    }
}

fn play_bytes(bytes: Vec<u8>, gain: f32) -> Result<(), String> {
    use rodio::Source as _;
    let mut device = rodio::DeviceSinkBuilder::open_default_sink().map_err(|e| e.to_string())?;
    device.log_on_drop(false);
    let decoder = rodio::Decoder::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let player = rodio::Player::connect_new(device.mixer());
    player.set_volume(gain);
    // Whatever the file says about itself, the sound is cut at the limit.
    player.append(decoder.take_duration(Duration::from_secs_f32(MAX_CUSTOM_SECONDS)));
    player.sleep_until_end();
    Ok(())
}

fn show(title: &str, body: &str) {
    let mut notification = notify_rust::Notification::new();
    notification.summary(title).body(body).appname("Wyck");
    if let Err(error) = notification.show() {
        tracing::warn!(%error, "could not show a desktop notification");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output() -> Output {
        Output::default()
    }

    #[test]
    fn an_alert_uses_the_default_sound_unless_it_chose_its_own() {
        let out = output();
        assert_eq!(plan(&out, None, false).sound, Some(SoundKind::Ping));
        assert_eq!(
            plan(&out, Some(SoundKind::Chime), false).sound,
            Some(SoundKind::Chime)
        );
        assert_eq!(plan(&out, Some(SoundKind::Off), false).sound, None);
    }

    #[test]
    fn a_silent_default_or_a_volume_of_zero_plays_nothing() {
        let mut out = output();
        out.sound = SoundKind::Off;
        assert_eq!(plan(&out, None, false).sound, None);
        // An alert can still ask for a sound when the default is silence.
        assert_eq!(
            plan(&out, Some(SoundKind::Glass), false).sound,
            Some(SoundKind::Glass)
        );
        out.sound = SoundKind::Ping;
        out.volume = 0;
        assert_eq!(plan(&out, None, false).sound, None);
    }

    #[test]
    fn the_notification_waits_for_the_window_to_be_left() {
        let mut out = output();
        assert!(!plan(&out, None, true).notification);
        assert!(plan(&out, None, false).notification);
        out.notify_in_background_only = false;
        assert!(plan(&out, None, true).notification);
        out.notify = false;
        assert!(!plan(&out, None, false).notification);
    }

    #[test]
    fn my_file_without_a_file_falls_back_to_the_default_sound() {
        let mut out = output();
        out.sound = SoundKind::Custom;
        assert_eq!(plan(&out, None, false).sound, Some(SoundKind::Ping));
        assert_eq!(out.clone().normalized().sound, SoundKind::Ping);
        out.custom = Some("  ".into());
        assert_eq!(out.normalized().custom, None);
    }

    #[test]
    fn choices_read_from_a_file_are_put_in_range() {
        let out: Output = toml::from_str("volume = 250\nsound = \"chime\"").unwrap();
        assert_eq!(out.clone().normalized().volume, 100);
        assert_eq!(out.notify, Output::default().notify);
        let back: Output = toml::from_str(&toml::to_string(&out).unwrap()).unwrap();
        assert_eq!(back, out);
        // An older file has none of it.
        let none: Output = toml::from_str("").unwrap();
        assert_eq!(none, Output::default());
    }

    #[test]
    fn the_volume_follows_a_curve_that_keeps_the_ends() {
        assert_eq!(gain(0), 0.0);
        assert!((gain(100) - 1.0).abs() < 1e-6);
        assert!(gain(50) < 0.5 && gain(50) > 0.0);
        assert!((gain(255) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn every_built_in_sound_is_in_the_app_and_is_a_short_wav() {
        for kind in SoundKind::ALL {
            let Some(name) = kind.asset() else { continue };
            let file = Builtin::get(name).unwrap_or_else(|| panic!("{name} is missing"));
            assert!(file.data.len() < 100_000, "{name} is too big");
            assert_eq!(&file.data[..4], b"RIFF", "{name} is not a wav");
        }
    }

    #[test]
    fn a_built_in_sound_decodes_and_lasts_under_a_second() {
        use rodio::Source as _;
        for kind in SoundKind::ALL {
            let Some(name) = kind.asset() else { continue };
            let file = Builtin::get(name).unwrap();
            let decoder = rodio::Decoder::new(Cursor::new(file.data.into_owned())).unwrap();
            let seconds = decoder.total_duration().unwrap().as_secs_f32();
            assert!(seconds > 0.1 && seconds < 1.0, "{name} lasts {seconds} s");
        }
    }

    #[test]
    fn a_file_of_the_user_is_checked_before_it_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("mine.wav");
        std::fs::write(&good, Builtin::get("ping.wav").unwrap().data.as_ref()).unwrap();
        let seconds = check_file(&good).unwrap();
        assert!(seconds > 0.0 && seconds < MAX_CUSTOM_SECONDS);
        let bad = dir.path().join("mine.mp3");
        std::fs::write(&bad, b"not a sound at all").unwrap();
        assert!(check_file(&bad).is_err());
        assert!(check_file(&dir.path().join("missing.wav")).is_err());
    }
}
