//! CLI parsing is effect-free. All offline/export/help modes short-circuit connection discovery.
use medousa_desktop_host::connection::{LocalConnection, Overrides};
use rndr_ui::{Appearance, Contrast, UiTheme};
use std::path::PathBuf;

pub const HELP: &str = "Medousa (rndr)
  [--data-dir PATH] [--daemon http://127.0.0.1:7419] [--session ID]
  Normal startup uses the configured local workshop (same paths/registry as Tauri).
  A missing local engine may be started; readiness is checked before credentials are loaded.
  --daemon is attach-only; supply its matching --data-dir for another workshop.
  --offline  No config/credential discovery, network or bootstrap
  --light / --high-contrast  Explicit window appearance (default: dark)
  --export DIRECTORY  Offline chat GPU PNG evidence
  --export-notes DIRECTORY  Synthetic Notes GPU matrix
  --export-desktop DIRECTORY  Synthetic settings/admin GPU matrix
  --observe-offline  Synthetic live capture, 15s exit (requires feature observe)
Remote workshops are unsupported. Sending remains gated by native-input safety and can incur inference charges.";

pub struct Cli {
    pub origin: Option<String>,
    pub data_dir: Option<PathBuf>,
    pub session: Option<String>,
    pub export: Option<PathBuf>,
    pub desktop_export: Option<PathBuf>,
    pub notes_export: Option<PathBuf>,
    pub offline: bool,
    pub observe_offline: bool,
    pub theme: UiTheme,
    pub help: bool,
}
impl Cli {
    pub fn parse(
        args: impl IntoIterator<Item = String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut origin = None;
        let mut data_dir: Option<PathBuf> = None;
        let mut session = None;
        let mut export: Option<PathBuf> = None;
        let mut desktop_export: Option<PathBuf> = None;
        let mut notes_export: Option<PathBuf> = None;
        let mut offline = false;
        let mut observe_offline = false;
        let mut theme = UiTheme {
            appearance: Appearance::Dark,
            ..Default::default()
        };
        let mut help = false;
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--daemon" => origin = Some(args.next().ok_or("--daemon needs an origin")?),
                "--data-dir" => {
                    data_dir = Some(args.next().ok_or("--data-dir needs a directory")?.into())
                }
                "--session" => session = Some(args.next().ok_or("--session needs an id")?),
                "--export" => {
                    export = Some(args.next().ok_or("--export needs a directory")?.into());
                    offline = true;
                }
                "--offline" => offline = true,
                "--export-notes" => {
                    notes_export = Some(
                        args.next()
                            .ok_or("--export-notes needs a directory")?
                            .into(),
                    );
                    offline = true;
                }
                "--export-desktop" => {
                    desktop_export = Some(
                        args.next()
                            .ok_or("--export-desktop needs a directory")?
                            .into(),
                    );
                    offline = true;
                }
                "--light" => theme.appearance = Appearance::Light,
                "--high-contrast" => theme.contrast = Contrast::High,
                "--observe-offline" => {
                    offline = true;
                    observe_offline = true;
                }
                "--help" | "-h" => {
                    help = true;
                    break;
                }
                _ if arg.starts_with("-psn_") => {}
                _ => return Err(format!("Unknown argument: {arg}").into()),
            }
        }

        Ok(Self {
            origin,
            data_dir,
            session,
            export,
            desktop_export,
            notes_export,
            offline,
            observe_offline,
            theme,
            help,
        })
    }
    pub fn connection_with(
        &self,
        discover: impl FnOnce(Overrides) -> Result<LocalConnection, String>,
    ) -> Result<Option<LocalConnection>, String> {
        if self.offline || self.help {
            return Ok(None);
        }
        discover(Overrides {
            data_dir: self.data_dir.clone(),
            daemon: self.origin.clone(),
        })
        .map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_export_observe_and_help_never_discover() {
        for args in [
            vec!["--offline"],
            vec!["--export", "out"],
            vec!["--export-notes", "out"],
            vec!["--export-desktop", "out"],
            vec!["--observe-offline"],
            vec!["--help"],
            vec!["-h"],
        ] {
            let mut args: Vec<String> = args.into_iter().map(str::to_string).collect();
            // Deliberately invalid connection overrides cannot enter discovery.
            args.splice(
                0..0,
                [
                    "--data-dir".into(),
                    "/do-not-read".into(),
                    "--daemon".into(),
                    "https://user:secret@remote.invalid/path".into(),
                ],
            );
            let cli = Cli::parse(args).unwrap();
            assert!(
                cli.connection_with(|_| panic!("offline discovery called"))
                    .unwrap()
                    .is_none()
            );
        }
    }
    #[test]
    fn no_flags_and_overrides_reach_shared_policy_and_preserve_session() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let cli = Cli::parse(Vec::<String>::new()).unwrap();
        let conn = cli
            .connection_with(|o| medousa_desktop_host::connection::resolve_local(root, o))
            .unwrap()
            .unwrap();
        assert_eq!(conn.data_dir, root);
        assert_eq!(conn.origin, "http://127.0.0.1:7419/");
        let cli = Cli::parse(
            [
                "--data-dir",
                "/synthetic/override",
                "--daemon",
                "http://127.0.0.1:9876",
                "--session",
                "session-x",
            ]
            .map(str::to_string),
        )
        .unwrap();
        assert_eq!(cli.session.as_deref(), Some("session-x"));
        let conn = cli
            .connection_with(|o| medousa_desktop_host::connection::resolve_local(root, o))
            .unwrap()
            .unwrap();
        assert_eq!(conn.data_dir, std::path::Path::new("/synthetic/override"));
        assert_eq!(conn.origin, "http://127.0.0.1:9876/");
        assert!(!conn.allow_bootstrap);
    }
}
