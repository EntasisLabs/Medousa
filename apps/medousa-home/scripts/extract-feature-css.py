#!/usr/bin/env python3
"""Extract feature CSS from app.postcss by selector family (H09 Train 5.1).

Dry-run (default): print exclusive family line counts.
Write: --write emits feature sheets and rewrites app.postcss.
"""
from __future__ import annotations

import argparse
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "src" / "app.postcss"
STYLES = ROOT / "src" / "lib" / "styles"

# Exclusive first-match families. More specific vault slices come first.
FAMILY_PATTERNS: list[tuple[str, re.Pattern[str]]] = [
    (
        "vault-find",
        re.compile(r"\.vault-find|::highlight\(medousa-vault-find|@keyframes\s+vault-find"),
    ),
    (
        "vault-live-properties",
        re.compile(
            r"\.vault-live-properties|@keyframes\s+vault-live-(?:properties|tag)"
        ),
    ),
    ("vault-live", re.compile(r"\.vault-live|@keyframes\s+vault-live")),
    ("vault-kanban", re.compile(r"\.vault-kanban|\.liquid-mini-kanban|@keyframes\s+vault-kanban")),
    (
        "vault-chart",
        re.compile(r"\.vault-chart|\.liquid-chart|@keyframes\s+vault-chart"),
    ),
    (
        "vault-liquid",
        re.compile(r"\.(?:vault-(?:callout|liquid|quiet)|ledger-|medousa-view)"),
    ),
    (
        "vault-editor",
        re.compile(
            r"\.(?:vault-(?:editor|slash|selection|format|markdown|paper|cm|codemirror|source|preview|note-status)|"
            r"cm-|CodeMirror|ProseMirror|tiptap)|"
            r"@keyframes\s+vault-(?:chip|format|sticky)"
        ),
    ),
    (
        "vault-workshop",
        re.compile(r"\.vault-(?:note-workshop|workshop|dock|note-chat|chat-context)"),
    ),
    ("vault-browse", re.compile(r"\.vault-|@keyframes\s+vault-")),
    ("composer", re.compile(r"\.composer-|@keyframes\s+composer-")),
    (
        "chat",
        re.compile(r"\.(?:chat-|session-|liquid-chat|duet-stage)"),
    ),
    (
        "settings",
        re.compile(
            r"\.(?:settings-|models?-|provider-|manifest-|daemon-|theme-option)"
        ),
    ),
    (
        "scripts",
        re.compile(
            r"\.(?:grapheme-|scripts?-|automations-|flow-)|"
            r"@keyframes\s+scripts-"
        ),
    ),
    ("context", re.compile(r"\.context-")),
    ("mobile", re.compile(r"\.mobile-|@keyframes\s+mobile-")),
    ("markdown", re.compile(r"\.markdown-|@keyframes\s+markdown-")),
    ("lme", re.compile(r"\.lme-|@keyframes\s+lme-")),
    ("profiles", re.compile(r"\.profiles-")),
    ("cron", re.compile(r"\.cron-|\.friendly-schedule")),
    ("spotlight", re.compile(r"\.command-spotlight")),
    ("artifact", re.compile(r"\.artifact-")),
    ("work", re.compile(r"\.work-|@keyframes\s+work-")),
    ("messaging", re.compile(r"\.messaging-")),
    ("shell-tabs", re.compile(r"\.shell-tab|@keyframes\s+shell-tab")),
    ("status", re.compile(r"\.status-desktop")),
    ("workshop-extra", re.compile(r"\.workshop-|@keyframes\s+workshop-")),
    ("calendar", re.compile(r"\.calendar-|@keyframes\s+calendar-")),
    ("identity", re.compile(r"\.identity-|@keyframes\s+identity-")),
    ("resume", re.compile(r"\.resume-")),
    ("syn", re.compile(r"\.syn-")),
]

# Sheets we actually emit. Tiny leftover families stay in app.postcss unless
# needed to get under the 2000-line cap.
EMIT_FAMILIES = [
    "vault-find",
    "vault-live-properties",
    "vault-live",
    "vault-kanban",
    "vault-chart",
    "vault-liquid",
    "vault-editor",
    "vault-workshop",
    "vault-browse",
    "composer",
    "chat",
    "settings",
    "scripts",
    "context",
    "mobile",
    "markdown",
    "lme",
    "profiles",
    "cron",
    "spotlight",
    "artifact",
    "work",
    "messaging",
    "shell-tabs",
    "status",
    "workshop-extra",
]

SHEET_META: dict[str, tuple[str, str]] = {
    # filename stem, header comment
    "vault-find": (
        "vault-find.postcss",
        "Vault find-in-note bar and match marks.",
    ),
    "vault-live-properties": (
        "vault-live-properties.postcss",
        "Vault Live properties panel and tags.",
    ),
    "vault-live": (
        "vault-live.postcss",
        "Vault Live editor prose, organisms, embed, card, and table chrome.",
    ),
    "vault-kanban": (
        "vault-kanban.postcss",
        "Vault kanban board and peek chrome.",
    ),
    "vault-chart": (
        "vault-chart.postcss",
        "Vault chart builder and liquid chart shell.",
    ),
    "vault-liquid": (
        "vault-liquid.postcss",
        "Vault liquid/callout builders, ledger tables, medousa views.",
    ),
    "vault-editor": (
        "vault-editor.postcss",
        "Vault editor chrome, CodeMirror/source, slash menu, format bubble.",
    ),
    "vault-workshop": (
        "vault-workshop.postcss",
        "Vault note workshop dock, chat, and branch chrome.",
    ),
    "vault-browse": (
        "vault-browse.postcss",
        "Vault library browse, tree, and sidebar chrome.",
    ),
    "composer": (
        "composer.postcss",
        "Chat composer, voice, attachments, model picker.",
    ),
    "chat": (
        "chat.postcss",
        "Chat transcript and session sidebar.",
    ),
    "settings": (
        "settings.postcss",
        "Settings shell, native rows, model catalog.",
    ),
    "scripts": (
        "scripts.postcss",
        "Grapheme script editor, scripts workbench, flow composer.",
    ),
    "context": (
        "context.postcss",
        "Context map, witness, and layer chrome.",
    ),
    "mobile": (
        "mobile-shell.postcss",
        "Leftover mobile shell chrome (home widgets live in mobile-home-convergence).",
    ),
    "markdown": (
        "markdown-content.postcss",
        "Shared .markdown-content rendering (chat, vault, liquid).",
    ),
    "lme": (
        "lme.postcss",
        "Library/modules explorer chrome.",
    ),
    "profiles": (
        "profiles.postcss",
        "Profiles field, shelf, and focus cards.",
    ),
    "cron": (
        "cron.postcss",
        "Cron / recurring automations panel.",
    ),
    "spotlight": (
        "command-spotlight.postcss",
        "Command spotlight overlay.",
    ),
    "artifact": (
        "artifact.postcss",
        "Artifact panel chrome.",
    ),
    "work": (
        "work.postcss",
        "Work / asks / motion chrome.",
    ),
    "messaging": (
        "messaging.postcss",
        "Messaging channels settings chrome.",
    ),
    "shell-tabs": (
        "shell-tabs.postcss",
        "Shell tab notch / drawer chrome.",
    ),
    "status": (
        "status-desktop.postcss",
        "Desktop status-strip marks.",
    ),
    "workshop-extra": (
        "workshop-surfaces.postcss",
        "Workshop surface extras not in workshop-shell.postcss.",
    ),
}


def split_statements(body: str) -> list[str]:
    statements: list[str] = []
    buf: list[str] = []
    i = 0
    n = len(body)
    depth = 0
    in_s = in_d = in_comment = False
    while i < n:
        c = body[i]
        nxt = body[i + 1] if i + 1 < n else ""
        if in_comment:
            buf.append(c)
            if c == "*" and nxt == "/":
                buf.append(nxt)
                i += 2
                in_comment = False
                if depth == 0:
                    statements.append("".join(buf))
                    buf = []
                continue
            i += 1
            continue
        if not in_s and not in_d and c == "/" and nxt == "*":
            buf.append(c)
            buf.append(nxt)
            i += 2
            in_comment = True
            continue
        if in_s:
            buf.append(c)
            if c == "\\" and i + 1 < n:
                buf.append(body[i + 1])
                i += 2
                continue
            if c == "'":
                in_s = False
            i += 1
            continue
        if in_d:
            buf.append(c)
            if c == "\\" and i + 1 < n:
                buf.append(body[i + 1])
                i += 2
                continue
            if c == '"':
                in_d = False
            i += 1
            continue
        if c == "'":
            in_s = True
            buf.append(c)
            i += 1
            continue
        if c == '"':
            in_d = True
            buf.append(c)
            i += 1
            continue
        if c == "{":
            depth += 1
            buf.append(c)
            i += 1
            continue
        if c == "}":
            depth -= 1
            buf.append(c)
            i += 1
            if depth == 0:
                statements.append("".join(buf))
                buf = []
            continue
        if c == ";" and depth == 0:
            buf.append(c)
            statements.append("".join(buf))
            buf = []
            i += 1
            continue
        buf.append(c)
        i += 1
    if buf and "".join(buf).strip():
        statements.append("".join(buf))
    return statements


def unwrap_layer(text: str, name: str) -> tuple[str, str, str]:
    marker = f"@layer {name} {{"
    start = text.find(marker)
    if start < 0:
        raise SystemExit(f"missing {marker}")
    i = start + len(marker)
    depth = 1
    in_s = in_d = in_comment = False
    while i < len(text):
        c = text[i]
        nxt = text[i + 1] if i + 1 < len(text) else ""
        if in_comment:
            if c == "*" and nxt == "/":
                in_comment = False
                i += 2
                continue
            i += 1
            continue
        if not in_s and not in_d and c == "/" and nxt == "*":
            in_comment = True
            i += 2
            continue
        if in_s:
            if c == "\\":
                i += 2
                continue
            if c == "'":
                in_s = False
            i += 1
            continue
        if in_d:
            if c == "\\":
                i += 2
                continue
            if c == '"':
                in_d = False
            i += 1
            continue
        if c == "'":
            in_s = True
            i += 1
            continue
        if c == '"':
            in_d = True
            i += 1
            continue
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return text[:start], text[start + len(marker) : i], text[i + 1 :]
        i += 1
    raise SystemExit("unclosed layer")


def rule_prelude(stmt: str) -> str:
    s = stmt.strip()
    if s.startswith("/*"):
        return ""
    brace = s.find("{")
    if brace < 0:
        return s
    return s[:brace].strip()


def is_comment(stmt: str) -> bool:
    return stmt.strip().startswith("/*")


def is_wrap_atrule(stmt: str) -> bool:
    p = rule_prelude(stmt)
    return p.startswith("@media") or p.startswith("@supports")


def is_keyframes(stmt: str) -> bool:
    return rule_prelude(stmt).startswith("@keyframes")


def inner_body(stmt: str) -> str:
    s = stmt.rstrip()
    # keep original indent by using the stripped version only for braces
    stripped = s.strip()
    brace = stripped.find("{")
    if brace < 0:
        return ""
    if not stripped.endswith("}"):
        return stripped[brace + 1 :]
    return stripped[brace + 1 : -1]


def classify(hay: str) -> str:
    for name, pat in FAMILY_PATTERNS:
        if pat.search(hay):
            return name
    return "core"


def classify_statement(stmt: str) -> str:
    if is_comment(stmt):
        return "comment"
    prelude = rule_prelude(stmt)
    if is_wrap_atrule(stmt):
        return classify(prelude + " " + inner_body(stmt))
    return classify(prelude)


def line_count(text: str) -> int:
    if not text:
        return 0
    return text.count("\n") + (0 if text.endswith("\n") else 1)


def normalize_rule(stmt: str) -> str:
    """Trim trailing whitespace; keep internal structure."""
    return stmt.rstrip() + "\n"


def indent_block(text: str, spaces: int = 2) -> str:
    pad = " " * spaces
    lines = text.split("\n")
    out = []
    for line in lines:
        if line.strip() == "":
            out.append("")
        else:
            out.append(pad + line)
    return "\n".join(out)


def wrap_feature_sheet(comment: str, rules: str) -> str:
    body = rules.rstrip() + "\n"
    return (
        f"/**\n * {comment}\n"
        " * Loaded by the destination feature entry; not imported from app.postcss.\n"
        " */\n\n"
        "@layer features {\n"
        f"{indent_block(body, 2)}"
        "}\n"
    )


GLOBAL_REDUCED_MOTION = """
  /* Global reduced-motion policy — feature sheets may add their own overrides. */
  @media (prefers-reduced-motion: reduce) {
    *,
    *::before,
    *::after {
      animation-duration: 0.01ms !important;
      animation-iteration-count: 1 !important;
      transition-duration: 0.01ms !important;
      scroll-behavior: auto !important;
    }
  }
"""


def attach_comments(stmts: list[str]) -> list[tuple[list[str], str]]:
    """Group leading comments with the following rule."""
    grouped: list[tuple[list[str], str]] = []
    pending: list[str] = []
    for stmt in stmts:
        if is_comment(stmt):
            pending.append(stmt)
            continue
        grouped.append((pending, stmt))
        pending = []
    if pending:
        grouped.append((pending, ""))
    return grouped


def split_wrap_atrule(stmt: str) -> dict[str, str]:
    """Split a @media/@supports block by inner rule family."""
    prelude = rule_prelude(stmt)
    inner = inner_body(stmt)
    inner_stmts = split_statements(inner)
    buckets: dict[str, list[str]] = {}
    pending_comments: list[str] = []
    for inn in inner_stmts:
        if is_comment(inn):
            pending_comments.append(inn)
            continue
        fam = classify_statement(inn)
        buckets.setdefault(fam, [])
        buckets[fam].extend(pending_comments)
        buckets[fam].append(inn)
        pending_comments = []
    if pending_comments:
        buckets.setdefault("core", []).extend(pending_comments)

    result: dict[str, str] = {}
    for fam, parts in buckets.items():
        inner_txt = "".join(parts)
        # reconstruct at-rule with original prelude
        inner_txt = inner_txt if inner_txt.startswith("\n") else "\n" + inner_txt
        result[fam] = f"\n  {prelude} {{{inner_txt}  }}\n"
    return result


def extract(write: bool) -> None:
    text = APP.read_text()
    pre, body, post = unwrap_layer(text, "components")
    stmts = split_statements(body)

    buckets: dict[str, list[str]] = {name: [] for name, _ in FAMILY_PATTERNS}
    buckets["core"] = []

    pending_comments: list[str] = []
    for stmt in stmts:
        if is_comment(stmt):
            pending_comments.append(stmt)
            continue
        if is_wrap_atrule(stmt):
            split = split_wrap_atrule(stmt)
            # attach pending comments to the first family that isn't empty;
            # prefer core if present so chrome comments stay put, else first emitted.
            if pending_comments:
                target = "core" if "core" in split else next(iter(split))
                split[target] = "".join(pending_comments) + split[target]
                pending_comments = []
            for fam, chunk in split.items():
                buckets.setdefault(fam, []).append(chunk)
            continue
        fam = classify_statement(stmt)
        buckets.setdefault(fam, [])
        if pending_comments:
            buckets[fam].extend(pending_comments)
            pending_comments = []
        buckets[fam].append(stmt)
    if pending_comments:
        buckets["core"].extend(pending_comments)

    print("=== exclusive family line counts (raw rules, before sheet wrap) ===")
    totals: list[tuple[str, int, int]] = []
    for name in list(dict.fromkeys([n for n, _ in FAMILY_PATTERNS] + ["core"])):
        raw = "".join(buckets.get(name, []))
        if not raw.strip():
            continue
        totals.append((name, line_count(raw), len(buckets[name])))
    for name, lines, n in sorted(totals, key=lambda t: -t[1]):
        emit = "EMIT" if name in EMIT_FAMILIES else "keep"
        print(f"  {name:18} lines={lines:5} stmts={n:4}  {emit}")

    core_raw = "".join(buckets["core"])
    print(f"\ncore leftover raw lines: {line_count(core_raw)}")
    print(f"preamble lines: {line_count(pre)}")

    # reconstruct leftover components layer
    leftover_components = core_raw
    # families not in EMIT stay in app.postcss
    for name, _ in FAMILY_PATTERNS:
        if name not in EMIT_FAMILIES:
            leftover_components += "".join(buckets.get(name, []))

    # Inject global reduced-motion into @layer base if missing from leftover.
    if "prefers-reduced-motion" not in pre and "prefers-reduced-motion" not in leftover_components:
        # Insert before the closing of @layer base in `pre`.
        base_close = pre.rfind("}")
        if base_close > 0:
            pre = pre[:base_close] + GLOBAL_REDUCED_MOTION + pre[base_close:]

    leftover_body = leftover_components.rstrip() + "\n"
    new_app = pre + "@layer components {\n" + leftover_body + "}\n"
    extra = post.lstrip("\n")
    if extra.strip():
        new_app += extra
    if not new_app.endswith("\n"):
        new_app += "\n"

    print(f"\nprojected app.postcss lines: {line_count(new_app)}")

    sheet_lines: dict[str, int] = {}
    for fam in EMIT_FAMILIES:
        raw = "".join(buckets.get(fam, []))
        if not raw.strip():
            print(f"  skip empty {fam}")
            continue
        # Strip one level of indent from @layer components body (rules are typically 2-space indented)
        stripped = "\n".join(
            line[2:] if line.startswith("  ") else line for line in raw.split("\n")
        )
        filename, comment = SHEET_META[fam]
        sheet = wrap_feature_sheet(comment, stripped)
        sheet_lines[filename] = line_count(sheet)
        flag = "OVER 2000" if sheet_lines[filename] > 2000 else ("review" if sheet_lines[filename] > 1000 else "ok")
        print(f"  {filename:32} {sheet_lines[filename]:5}  {flag}")
        if write:
            (STYLES / filename).write_text(sheet)

    if write:
        APP.write_text(new_app)
        print(f"\nwrote {APP} ({line_count(new_app)} lines)")

    # leftover class prefixes in core
    leftover_classes = re.findall(r"\.([A-Za-z_][\w-]*)", leftover_components)
    from collections import Counter

    first = Counter(c.split("-")[0] for c in leftover_classes)
    print("\n=== leftover first-segment classes in app.postcss components layer ===")
    for k, v in first.most_common(40):
        print(f"  .{k}-*  {v}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    extract(write=args.write)


if __name__ == "__main__":
    main()
