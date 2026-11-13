"""One-shot extraction from reviewed main.rs; no framework or protocol edits.
Run from repository root only after capturing before evidence.
"""
from pathlib import Path
import hashlib
p = Path('apps/rndr-home/src')
s = (p / 'main.rs').read_text()
assert hashlib.sha256(s.encode()).hexdigest() == 'a5c1c766437d6962f768b3f183617da4b111b97b0bfe52db8f313b5a3dbd20d6'
def section(start, end):
    return s[s.index(start):s.index(end)]
def write(name, text):
    path = p / name
    assert not path.exists(), path
    path.write_text(text)
# State remains application-private; no new public library API.
model = section('struct Model {', '#[derive(Clone, Copy)]\nstruct Ui')
model = model.replace('struct Model {', 'pub(super) struct Model {')
model = '\n'.join('    pub(super) ' + line[4:] if line.startswith('    ') and ':' in line and not line.lstrip().startswith('//') else line for line in model.split('\n'))
write('state.rs', '//! Canonical app session/projection and pending-action state.\nuse crate::backend::Request;\nuse medousa_rndr_home::Chat;\nuse tokio::sync::mpsc;\n\n' + model)
ui = section('#[derive(Clone, Copy)]\nstruct Ui', 'fn main()')
ui = ui.replace('struct Ui {', 'pub(super) struct Ui {')
ui = '\n'.join('    pub(super) ' + line[4:] if line.startswith('    ') else line for line in ui.split('\n'))
helpers_setup = section('fn row(', '// rndr\'s reset_field_draft')
helpers_setup = helpers_setup.replace('fn row(', 'pub(super) fn row(').replace('fn muted(', 'pub(super) fn muted(').replace('fn setup(', 'pub(super) fn setup(')
projection = section('    let m = world.resource::<Model>().unwrap();\n    let status =', '    #[cfg(feature = "observe")]\n    if world.has_resource::<FixtureObservation>()')
write('shell.rs', '//! Shell assembly and read-only state-to-view synchronization.\nuse crate::{conversation, menus, presentation, state::Model};\nuse crate::presentation::{button, label};\nuse rndr_app::{child, column};\nuse rndr_core::*;\nuse rndr_ui::{Token, Typography, UiStyle};\n\n' + ui + helpers_setup + '\npub(super) fn sync(world: &mut World) {\n    let ui = *world.resource::<Ui>().unwrap();\n' + projection + '}\n')
reset = section('// rndr\'s reset_field_draft', 'fn dispatch(').replace('fn reset_prompt(', 'pub(super) fn reset_prompt(')
guard = section('// Prompt currently emits Submitted', 'fn update(').replace('fn guard_submit_boundary(', 'pub(super) fn guard_submit_boundary(')
write('composer.rs', '//! Prompt input boundary and synchronous draft reset adapter.\nuse rndr_core::*;\n\n#[derive(Default)]\npub(super) struct SubmitBoundary {\n    pub(super) safe: bool,\n}\n\n' + reset + guard)
dispatch = section('fn dispatch(', '// Prompt currently emits Submitted')
update = section('fn update(', '    let m = world.resource::<Model>().unwrap();\n    let status =').replace('fn update(', 'pub(super) fn update(')
write('controller.rs', '//! Sole runtime writer of session identity, generations and pending recovery.\nuse crate::{conversation, menus, presentation, shell, state::Model};\nuse crate::backend::{Command, Reply, Request, Update};\nuse crate::composer::{SubmitBoundary, reset_prompt};\nuse crate::shell::Ui;\nuse medousa_rndr_home::{Chat, turn_request};\nuse rndr_core::*;\nuse rndr_ui::{Appearance, Contrast};\n\n' + dispatch + update + '    shell::sync(world);\n    #[cfg(feature = "observe")]\n    crate::fixtures::sync_observation(world);\n}\n')
export = section('        std::fs::create_dir_all(&dir)?;', '    } else {\n        rndr_render::run(app);')
export = export.replace('app.world', 'app.world').replace('rndr_paint::frame(&mut app)', 'rndr_paint::frame(app)')
observe = s[s.index('    #[cfg(feature = "observe")]\n    if world.has_resource::<FixtureObservation>()'):]
# Remove final update brace and cfg (function itself is feature-gated).
observe = observe.removeprefix('    #[cfg(feature = "observe")]\n').removesuffix('}\n')
write('fixtures.rs', '//! Synthetic exports and deny-by-default offline fixture observation only.\nuse crate::{menus, state::Model, shell::Ui};\nuse crate::composer::reset_prompt;\nuse rndr_core::*;\nuse std::{path::PathBuf, sync::Arc};\n\n#[cfg(feature = "observe")]\npub(super) struct FixtureObservation;\n\npub(super) fn export(app: &mut App, dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {\n' + export + '    Ok(())\n}\n\n#[cfg(feature = "observe")]\npub(super) fn sync_observation(world: &mut World) {\n' + observe + '}\n')
main = s[:s.index('#[cfg(feature = "observe")]\nstruct FixtureObservation;')] + s[s.index('fn main()'):s.index('fn row(')]
main = main.replace('use backend::{Command, Reply, Request, Update};', 'use backend::{Command, Request};\nuse state::Model;\nuse shell::{Ui, setup};\nuse composer::{SubmitBoundary, guard_submit_boundary, reset_prompt};\nuse controller::update;\n#[cfg(feature = "observe")]\nuse fixtures::FixtureObservation;\n#[cfg(test)]\nuse backend::{Reply, Update};\n#[cfg(test)]\nuse shell::{row, muted};')
main = main.replace('use medousa_rndr_home::{Chat, turn_request};', 'use medousa_rndr_home::Chat;').replace('use presentation::{button, label};\n', '#[cfg(test)]\nuse presentation::{button, label};\n').replace('use rndr_app::{child, column};\n', '#[cfg(test)]\nuse rndr_app::{child, column};\n').replace('use rndr_ui::{Appearance, Contrast, Token, Typography, UiStyle, UiTheme};', 'use rndr_ui::{Appearance, Contrast, UiTheme};\n#[cfg(test)]\nuse rndr_ui::{Token, Typography};').replace('use std::{path::PathBuf, sync::Arc};', 'use std::path::PathBuf;')
main = main.replace(s[s.index('        std::fs::create_dir_all(&dir)?;'):s.index('    } else {\n        rndr_render::run(app);')], '        fixtures::export(&mut app, dir)?;\n')
main = 'mod composer;\nmod controller;\nmod fixtures;\nmod shell;\nmod state;\n' + main
# Cross-module access is explicit; root reexports retained only for existing tests.
main = main.replace('use shell::{Ui, setup};', 'use shell::setup;\n#[cfg(test)]\nuse shell::Ui;').replace('use composer::{SubmitBoundary, guard_submit_boundary, reset_prompt};', 'use composer::{SubmitBoundary, guard_submit_boundary};\n#[cfg(test)]\nuse composer::reset_prompt;')
(p / 'main.rs').write_text(main)
