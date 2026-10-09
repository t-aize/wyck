//! Checking, downloading and installing signed application updates.

use std::path::Path;
use std::time::Duration;

use cargo_packager_updater::semver::Version;
use cargo_packager_updater::{
    Config, Update, UpdaterBuilder, WindowsConfig, WindowsUpdateInstallMode,
};
use gpui::{App, BorrowAppContext, Entity, Global, Subscription};

use crate::app::appearance;
use crate::infra::platform::build_info::{BuildMode, VERSION};
use crate::infra::platform::runtime;
use crate::ui::features::multichart::MultiChart;
use crate::ui::kit::toast;

const UPDATE_ENDPOINT: &str = "https://github.com/t-aize/wyck/releases/latest/download/latest.json";
pub const RELEASES_URL: &str = "https://github.com/t-aize/wyck/releases/latest";
const UPDATE_PUBLIC_KEY: &str = include_str!("../../../assets/update.pubkey");
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// What the application currently knows or is doing about updates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateState {
    Disabled,
    Idle,
    Checking,
    UpToDate,
    Available {
        version: String,
        notes: Option<String>,
        automatic: bool,
    },
    Downloading {
        version: String,
    },
    Installing {
        version: String,
    },
    Failed {
        message: String,
    },
}

impl UpdateState {
    pub fn status(&self) -> String {
        match self {
            Self::Disabled => "Disabled in development builds".to_owned(),
            Self::Idle => "Not checked yet".to_owned(),
            Self::Checking => "Checking for updates...".to_owned(),
            Self::UpToDate => format!("Wyck {VERSION} is up to date"),
            Self::Available { version, .. } => format!("Wyck {version} is available"),
            Self::Downloading { version } => format!("Downloading Wyck {version}..."),
            Self::Installing { version } => format!("Installing Wyck {version}..."),
            Self::Failed { message } => format!("Update check failed: {message}"),
        }
    }

    pub fn busy(&self) -> bool {
        matches!(
            self,
            Self::Checking | Self::Downloading { .. } | Self::Installing { .. }
        )
    }
}

struct Service {
    state: UpdateState,
    update: Option<Update>,
}

impl Global for Service {}

pub fn init(cx: &mut App) {
    cx.set_global(Service {
        state: if BuildMode::CURRENT.is_production() {
            UpdateState::Idle
        } else {
            UpdateState::Disabled
        },
        update: None,
    });
}

pub fn state(cx: &App) -> UpdateState {
    cx.try_global::<Service>()
        .map(|service| service.state.clone())
        .unwrap_or(UpdateState::Disabled)
}

pub fn observe<T: 'static>(
    cx: &mut gpui::Context<T>,
    on_change: impl FnMut(&mut T, &mut gpui::Context<T>) + 'static,
) -> Subscription {
    cx.observe_global::<Service>(on_change)
}

fn set(state: UpdateState, update: Option<Update>, cx: &mut App) {
    if cx.has_global::<Service>() {
        cx.update_global::<Service, _>(|service, _| {
            service.state = state;
            service.update = update;
        });
    }
}

/// Checks the stable release manifest. Startup checks only announce an available update.
pub fn check(cx: &mut App, announce_available: bool) {
    if !BuildMode::CURRENT.is_production()
        || cx
            .try_global::<Service>()
            .is_none_or(|service| service.state.busy())
    {
        return;
    }
    set(UpdateState::Checking, None, cx);
    cx.spawn(async move |cx| {
        let checked = blocking(check_now).await;
        cx.update(|cx| match checked {
            Ok(Some(update)) => {
                let version = update.version.clone();
                let notes = update.body.clone();
                let automatic = automatic_install_supported();
                set(
                    UpdateState::Available {
                        version: version.clone(),
                        notes,
                        automatic,
                    },
                    Some(update),
                    cx,
                );
                if announce_available {
                    toast::show(
                        cx,
                        toast::Kind::Info,
                        "Wyck update available",
                        format!("Version {version} can be installed from Settings > About."),
                    );
                }
            }
            Ok(None) => set(UpdateState::UpToDate, None, cx),
            Err(message) => set(UpdateState::Failed { message }, None, cx),
        });
    })
    .detach();
}

fn check_now() -> Result<Option<Update>, String> {
    let current = Version::parse(VERSION).map_err(|error| error.to_string())?;
    let endpoint = UPDATE_ENDPOINT
        .parse()
        .map_err(|error: cargo_packager_updater::url::ParseError| error.to_string())?;
    let config = Config {
        endpoints: vec![endpoint],
        pubkey: UPDATE_PUBLIC_KEY.trim().to_owned(),
        windows: Some(WindowsConfig {
            installer_args: None,
            install_mode: Some(WindowsUpdateInstallMode::Passive),
        }),
    };
    UpdaterBuilder::new(current, config)
        .timeout(REQUEST_TIMEOUT)
        .version_comparator(|current, release| stable_newer(&current, &release.version))
        .build()
        .and_then(|updater| updater.check())
        .map_err(|error| error.to_string())
}

fn stable_newer(current: &Version, candidate: &Version) -> bool {
    candidate.pre.is_empty() && candidate > current
}

/// Downloads, verifies and installs the update kept by the last successful check.
pub fn install(multi: Entity<MultiChart>, cx: &mut App) {
    let Some(update) = cx
        .try_global::<Service>()
        .and_then(|service| service.update.clone())
    else {
        return;
    };
    if !automatic_install_supported() {
        cx.open_url(RELEASES_URL);
        return;
    }

    let version = update.version.clone();
    set(
        UpdateState::Downloading {
            version: version.clone(),
        },
        Some(update.clone()),
        cx,
    );
    cx.spawn(async move |cx| {
        let downloader = update.clone();
        let downloaded = blocking(move || downloader.download()).await;
        let bytes = match downloaded {
            Ok(bytes) => bytes,
            Err(message) => {
                cx.update(|cx| set(UpdateState::Failed { message }, None, cx));
                return;
            }
        };

        cx.update(|cx| {
            multi.read(cx).flush_documents(cx);
            appearance::save_now(cx);
            set(
                UpdateState::Installing {
                    version: version.clone(),
                },
                Some(update.clone()),
                cx,
            );
        });

        let installer = update;
        let installed = blocking(move || installer.install(bytes)).await;
        cx.update(|cx| match installed {
            Ok(()) => cx.restart(),
            Err(message) => set(UpdateState::Failed { message }, None, cx),
        });
    })
    .detach();
}

pub fn open_releases(cx: &mut App) {
    cx.open_url(RELEASES_URL);
}

async fn blocking<T, E, F>(work: F) -> Result<T, String>
where
    T: Send + 'static,
    E: ToString + Send + 'static,
    F: FnOnce() -> Result<T, E> + Send + 'static,
{
    runtime::spawn(async move {
        tokio::task::spawn_blocking(work)
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

fn automatic_install_supported() -> bool {
    let in_app_bundle = std::env::current_exe()
        .ok()
        .is_some_and(|path| path.ancestors().any(is_app_bundle));
    supports_automatic_install(
        std::env::consts::OS,
        std::env::var_os("APPIMAGE").is_some(),
        in_app_bundle,
    )
}

fn is_app_bundle(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "app")
}

fn supports_automatic_install(os: &str, appimage: bool, app_bundle: bool) -> bool {
    match os {
        "windows" => true,
        "macos" => app_bundle,
        "linux" => appimage,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn serve(bodies: Vec<(&'static str, String)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            for (content_type, body) in bodies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0_u8; 2048];
                let _ = stream.read(&mut request);
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });
        format!("http://{address}")
    }

    fn updater(endpoint: &str) -> cargo_packager_updater::Updater {
        UpdaterBuilder::new(
            Version::parse("0.3.0").unwrap(),
            Config {
                endpoints: vec![endpoint.parse().unwrap()],
                pubkey: UPDATE_PUBLIC_KEY.trim().to_owned(),
                windows: None,
            },
        )
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
    }

    #[test]
    fn only_newer_stable_versions_are_offered() {
        let current = Version::parse("0.3.0").unwrap();
        assert!(stable_newer(&current, &Version::parse("0.3.1").unwrap()));
        assert!(!stable_newer(&current, &Version::parse("0.3.0").unwrap()));
        assert!(!stable_newer(&current, &Version::parse("0.2.9").unwrap()));
        assert!(!stable_newer(
            &current,
            &Version::parse("0.4.0-beta.1").unwrap()
        ));
    }

    #[test]
    fn automatic_install_needs_a_managed_linux_or_macos_package() {
        assert!(supports_automatic_install("windows", false, false));
        assert!(supports_automatic_install("linux", true, false));
        assert!(!supports_automatic_install("linux", false, false));
        assert!(supports_automatic_install("macos", false, true));
        assert!(!supports_automatic_install("macos", false, false));
    }

    #[test]
    fn a_missing_platform_is_reported() {
        let manifest = serde_json::json!({
            "version": "0.3.1",
            "platforms": {
                "other-x86_64": {
                    "signature": "unused",
                    "url": "http://127.0.0.1/unused",
                    "format": "appimage"
                }
            }
        });
        let endpoint = serve(vec![("application/json", manifest.to_string())]);
        let error = updater(&endpoint).check().unwrap_err().to_string();
        assert!(error.contains("platform"));
    }

    #[test]
    fn an_invalid_download_signature_is_rejected() {
        let target = cargo_packager_updater::target().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let mut platforms = serde_json::Map::new();
        platforms.insert(
            target,
            serde_json::json!({
                "signature": "not-base64",
                "url": format!("http://{address}/payload"),
                "format": "appimage"
            }),
        );
        let manifest = serde_json::json!({
            "version": "0.3.1",
            "platforms": platforms
        });
        thread::spawn(move || {
            for body in [manifest.to_string(), "not an update".to_owned()] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0_u8; 2048];
                let _ = stream.read(&mut request);
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });

        let update = updater(&format!("http://{address}"))
            .check()
            .unwrap()
            .unwrap();
        assert!(update.download().is_err());
    }

    #[test]
    fn a_network_error_is_reported() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        assert!(updater(&format!("http://{address}")).check().is_err());
    }
}
