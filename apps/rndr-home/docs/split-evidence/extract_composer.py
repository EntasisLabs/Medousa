"""Finish the leaf composer boundary; preserves spawn order and entity identity."""
from pathlib import Path
import hashlib
p = Path('apps/rndr-home/src')
s = (p/'shell.rs').read_text()
c = (p/'composer.rs').read_text()
assert hashlib.sha256(s.encode()).hexdigest() == 'a083c5bfb2d93ae9bd22f7239db5cdc931b02117379d7f93c85a3c970937c391'
assert hashlib.sha256(c.encode()).hexdigest() == '3ffbb715cdfa98b6edf79e62e9736cfee069572c071d2af60266f4edbacf5499'
a = s.index('    let stickies =')
b = s.index('    let bottom_space =')
body = s[a:b]
fields = ['composer', 'model', 'send', 'cancel', 'runtime', 'mode', 'behavior']
handles = 'pub(super) struct Handles {\n' + ''.join('    pub(super) '+f+': Entity,\n' for f in fields) + '}\n'
setup = '\npub(super) fn setup(world: &mut World, center: Entity) -> Handles {\n    let theme = presentation::theme(world);\n' + body + '    Handles { '+', '.join(fields)+' }\n}\n'
c = c.replace('use rndr_core::*;', 'use crate::presentation::{self, button};\nuse crate::shell::{muted, row};\nuse rndr_app::child;\nuse rndr_core::*;\nuse rndr_ui::Typography;')
(p/'composer.rs').write_text(c + '\n' + handles + setup)
s = s[:a] + '    let crate::composer::Handles { '+', '.join(fields)+' } = crate::composer::setup(world, center);\n' + s[b:]
s = s.replace('    let theme = presentation::theme(world);\n', '')
(p/'shell.rs').write_text(s)
