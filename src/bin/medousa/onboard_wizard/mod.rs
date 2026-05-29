use std::{env, io, path::PathBuf, time::Duration};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use image::ImageReader;
use ratatui::{Terminal, backend::CrosstermBackend};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};

mod model;
mod ui;

pub(crate) use model::{WizardBootstrap, WizardOutput};
use model::{WizardState, WizardTransition};

struct WizardMedia {
    logo_protocol: Option<StatefulProtocol>,
    logo_notice: Option<String>,
}

impl WizardMedia {
    fn load() -> Self {
        let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());

        let Some(path) = resolve_logo_path() else {
            return Self {
                logo_protocol: None,
                logo_notice: Some("Logo not found in assets/".to_string()),
            };
        };

        match ImageReader::open(&path) {
            Ok(reader) => match reader.decode() {
                Ok(dynamic_image) => Self {
                    logo_protocol: Some(picker.new_resize_protocol(dynamic_image)),
                    logo_notice: None,
                },
                Err(error) => Self {
                    logo_protocol: None,
                    logo_notice: Some(format!("Unable to decode logo: {error}")),
                },
            },
            Err(error) => Self {
                logo_protocol: None,
                logo_notice: Some(format!("Unable to open logo: {error}")),
            },
        }
    }
}

fn resolve_logo_path() -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(cwd) = env::current_dir() {
        candidates.push(cwd.join("assets").join("medousa-transparent.png"));
        candidates.push(cwd.join("assets").join("medousa-cream.png"));
        candidates.push(cwd.join("assets").join("medousa-blk.png"));
    }

    if let Ok(exe) = env::current_exe()
        && let Some(bin_dir) = exe.parent()
    {
        candidates.push(bin_dir.join("assets").join("medousa-transparent.png"));
        candidates.push(bin_dir.join("assets").join("medousa-cream.png"));
        candidates.push(bin_dir.join("assets").join("medousa-blk.png"));

        if let Some(parent) = bin_dir.parent() {
            candidates.push(parent.join("assets").join("medousa-transparent.png"));
            candidates.push(parent.join("assets").join("medousa-cream.png"));
            candidates.push(parent.join("assets").join("medousa-blk.png"));
        }
    }

    candidates.into_iter().find(|candidate| candidate.is_file())
}

pub(crate) fn run(bootstrap: WizardBootstrap) -> Result<Option<WizardOutput>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut state = WizardState::new(bootstrap);
    let mut media = WizardMedia::load();

    let run_result = run_loop(&mut terminal, &mut state, &mut media);

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    run_result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut WizardState,
    media: &mut WizardMedia,
) -> Result<Option<WizardOutput>> {
    loop {
        terminal.draw(|frame| {
            ui::render(
                frame,
                state,
                media.logo_protocol.as_mut(),
                media.logo_notice.as_deref(),
            )
        })?;

        if !event::poll(Duration::from_millis(200))? {
            continue;
        }

        match event::read()? {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                match state.handle_key(key_event) {
                    WizardTransition::Continue => {}
                    WizardTransition::Cancelled => return Ok(None),
                    WizardTransition::Finished(output) => return Ok(Some(output)),
                }
            }
            Event::Resize(_, _) => {
                // redraw happens on next loop iteration
            }
            _ => {}
        }
    }
}
