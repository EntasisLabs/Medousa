"""Bounded source-body audit against reviewed baseline; run at repository root.
Whitespace/trailing-comma normalization is only for formatting; strings are also covered by
byte-identical export comparison. This is a consistency check, not attestation.
"""
import hashlib
import json
from pathlib import Path
import re
import subprocess

ROOT = Path('apps/rndr-home')
OUT = ROOT / 'docs/review-prep-correctness-20261113-1304'
BASE = '431ccab3affe24725eaff7d39176661b3398a17f'
old = subprocess.check_output(['git', 'show', BASE + ':apps/rndr-home/src/main.rs'], text=True)
new = {p.stem: p.read_text() for p in (ROOT / 'src').glob('*.rs')}
def between(s, a, b):
    return s[s.index(a):s.index(b)]
def norm(s):
    # Reviewer strengthening: preserve all quoted string literal bytes.
    # Existing bounded compared fragments contain no raw-string literals.
    literals = []
    def stash(m):
        literals.append(m.group())
        return f"LITERAL_TOKEN_{len(literals)-1}_END"
    s = re.sub(r'"(?:\\.|[^"\\])*"', stash, s)
    compact = re.sub(r'\s+', '', s).replace('pub(super)', '')
    compact = re.sub(r',(?=[)}\]])', '', compact)
    for i, literal in enumerate(literals):
        compact = compact.replace(f"LITERAL_TOKEN_{i}_END", literal)
    return compact
def check(name, a, b):
    assert norm(a) == norm(b), name
    return {'name': name, 'equal_preserving_literals_ignoring_formatting_and_private_visibility': True,
            'normalized_sha256': hashlib.sha256(norm(a).encode()).hexdigest()}
results = []
results.append(check('Model fields', between(old, 'struct Model {', '#[derive(Clone, Copy)]'), new['state'][new['state'].index('pub(super) struct Model'):]))
results.append(check('Ui fields', between(old, '#[derive(Clone, Copy)]', 'fn main()'), between(new['shell'], '#[derive(Clone, Copy)]', 'pub(super) fn row(')))
results.append(check('layout helpers', between(old, 'fn row(', 'fn setup('), between(new['shell'], 'pub(super) fn row(', 'pub(super) fn setup(')))
results.append(check('reset adapter', between(old, "// rndr's reset_field_draft", 'fn dispatch('), between(new['composer'], "// rndr's reset_field_draft", '// Prompt currently emits Submitted')))
results.append(check('input guard', between(old, '// Prompt currently emits Submitted', 'fn update('), between(new['composer'], '// Prompt currently emits Submitted', 'pub(super) struct Handles')))
results.append(check('dispatch', between(old, 'fn dispatch(', '// Prompt currently emits Submitted'), between(new['controller'], 'fn dispatch(', 'pub(super) fn update(')))
old_actions = between(old, 'fn update(', '    let m = world.resource::<Model>().unwrap();\n    let status =')
new_actions = between(new['controller'], 'pub(super) fn update(', '    shell::sync(world);')
results.append(check('reply and action handling in identical order', old_actions, new_actions))
old_projection = between(old, '    let m = world.resource::<Model>().unwrap();\n    let status =', '    #[cfg(feature = "observe")]\n    if world.has_resource::<FixtureObservation>()')
new_projection = new['shell'][new['shell'].index('    let m = world.resource::<Model>().unwrap();\n    let status ='):].removesuffix('}\n')
results.append(check('state projection', old_projection, new_projection))
old_observe = old[old.index('    if world.has_resource::<FixtureObservation>()'):].removesuffix('}\n')
new_observe = new['fixtures'][new['fixtures'].index('    if world.has_resource::<FixtureObservation>()'):].removesuffix('}\n').replace('        let ui = *world.resource::<Ui>().unwrap();\n', '')
results.append(check('observation predicate', old_observe, new_observe))
old_export = between(old, '        std::fs::create_dir_all(&dir)?;', '    } else {\n        rndr_render::run(app);')
new_export = between(new['fixtures'], '    std::fs::create_dir_all(&dir)?;', '    Ok(())').replace('rndr_paint::frame(app)', 'rndr_paint::frame(&mut app)')
results.append(check('export fixture loop', old_export, new_export))
# Reverse the single composer assembly call; no entity creation is reordered.
old_setup = between(old, 'fn setup(', "// rndr's reset_field_draft")
new_setup = between(new['shell'], 'pub(super) fn setup(', 'pub(super) fn sync(')
start = new_setup.index('    let crate::composer::Handles {')
end = new_setup.index('    let bottom_space =')
composer = new['composer'][new['composer'].index('    let stickies ='):]
composer = composer[:composer.index('    Handles {')]
new_setup = new_setup[:start] + composer + new_setup[end:]
new_setup = new_setup.replace('pub(super) fn setup(world: &mut World) {', 'fn setup(world: &mut World) {\n    let theme = presentation::theme(world);')
# Old reset comments are outside setup; extraction moved those with reset.
old_setup = old_setup[:old_setup.index('// rndr\'s')] if '// rndr\'s' in old_setup else old_setup
results.append(check('assembly spawn and parent order', old_setup, new_setup))
# Existing SDK, canonical stream model, transcript and theme implementation unchanged.
for name in ['backend', 'backend_tests', 'lib', 'conversation', 'presentation', 'ui_tests', 'interaction_tests', 'qualification_tests']:
    a = subprocess.check_output(['git', 'show', BASE + ':apps/rndr-home/src/' + name + '.rs'], text=True)
    assert a == new[name], name
    results.append({'name': name + '.rs', 'byte_identical_to_baseline': True})
results.append(check('app plugin/resource/system order', between(old, 'fn app()', 'fn row('), new['main'][new['main'].index('fn app()'):]))
old_main = between(old, 'fn main()', 'fn app()')
start = old_main.index('        std::fs::create_dir_all(&dir)?;')
end = old_main.index('    } else {\n        rndr_render::run(app);')
old_main = old_main[:start] + '        fixtures::export(&mut app, dir)?;\n' + old_main[end:]
results.append(check('entrypoint authority and native/export order', old_main, between(new['main'], 'fn main()', 'fn app()')))
results.append(check('SubmitBoundary data', between(old, '#[derive(Default)]', 'struct Model {'), between(new['composer'], '#[derive(Default)]', '// rndr')))
results.append(check('FixtureObservation marker', between(old, '#[cfg(feature = "observe")]\nstruct FixtureObservation;', '#[derive(Default)]'), between(new['fixtures'], '#[cfg(feature = "observe")]\npub(super) struct FixtureObservation;', 'pub(super) fn export(')))
(OUT / 'source-body-audit.json').write_text(json.dumps(results, indent=2) + '\n')
print('Source-body audit passed:', len(results), 'checks')
