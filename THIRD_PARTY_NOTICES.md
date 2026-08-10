# Third-party notices

Medousa includes third-party icon libraries and redistributed icon artwork. This
file records the sources and licenses for those materials. It is separate from
the root Medousa license: third-party material remains under its own license.

The Medousa source code and first-party Medousa brand artwork are not part of
the third-party materials listed below.

## Material Icon Theme

The 615 SVGs in
[`apps/medousa-home/static/file-icons/`](apps/medousa-home/static/file-icons/)
and the generated association map at
[`apps/medousa-home/src/lib/code/materialIconTheme.json`](apps/medousa-home/src/lib/code/materialIconTheme.json)
are derived from `material-icon-theme@5.37.0`. The SVGs are copied unchanged by
[`apps/medousa-home/scripts/sync-file-icons.mjs`](apps/medousa-home/scripts/sync-file-icons.mjs).

- Upstream project: [Material Icon Theme](https://github.com/material-extensions/vscode-material-icon-theme)
- Package license: MIT
- Package copyright: © 2025 Material Extensions
- Upstream-documented icon sources: [Material Design Icons](https://pictogrammers.com/docs/general/license/) and [Material Symbols](https://github.com/google/material-design-icons)

The Material Icon Theme package does not identify the source set for each
individual SVG. The upstream licenses and source links above are therefore
preserved together rather than assigning an unsupported per-file attribution.
Material Design Icons and Material Symbols are Apache-2.0 materials according
to their respective upstream projects; see their linked terms for details,
including any source-specific brand or logo restrictions.

### Material Icon Theme MIT license

Copyright (c) 2025 Material Extensions

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## Lucide

Medousa uses `@lucide/svelte` for interface icons in both Home and Installer.
The versions currently locked in this repository are `1.17.0` for Home and
`0.487.0` for Installer.

- Upstream project: [Lucide](https://github.com/lucide-icons/lucide)
- License: ISC
- Lucide license: [upstream license and derived-icon list](https://github.com/lucide-icons/lucide/blob/main/LICENSE)

Lucide's license identifies a set of icons derived from [Feather](https://github.com/feathericons/feather)
and includes the applicable MIT notice for those icons. The upstream Lucide
license is the authoritative source for the complete derived-icon list and
release-specific copyright wording.

### Lucide ISC license

Copyright (c) Lucide Icons and Contributors

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY
SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS.

## Maintenance

When either icon dependency is upgraded, update the versions and source trail
above, rerun the file-icon sync script when applicable, and review the
dependency's license file for any changed attribution requirements.
