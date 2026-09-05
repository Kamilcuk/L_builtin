#!/usr/bin/env -S uv run --with markdown-it-py --script
# /// script
# requires-python = ">=3.10"
# dependencies = ["markdown-it-py"]
# ///
"""Render the L_builtin subcommand reference as markdown to stdout.

Used by ``make readme`` (to write ``doc/reference.md``) and ``make test-readme``
(to diff the freshly generated output against the committed ``doc/reference.md``
and fail on drift).

Usage::

    ./scripts/gen_reference.py ./L_builtin.so
    ./scripts/gen_reference.py --help

Bash is auto-discovered from ``build/bash/*/bash`` (highest version wins).

The per-command reference text is fetched by running
``L_builtin <subcommand> -h`` inside a fresh bash with the .so enabled. The
TOC inside the output is built by parsing the rendered body with
``markdown-it-py`` so anchors always match the rendered headings.

Rules:

  * Every section title carries the ``L_builtin`` prefix, e.g.
    ``### `L_builtin lseek```.
  * ``core`` is a thin wrapper around Rust/uutils coreutils: its ``-h`` listing
    is emitted *and* a placeholder section is created for each core utility
    subcommand (with a one-line test description, not the utility's own ``-h``).
  * ``ext`` is a wrapper around Bash's example loadables: its ``-h`` listing
    is emitted *and* the ``-h`` output of every child builtin (asort, basename,
    ...) is included as a sub-section.
  * Compound commands (shm, mutex, semaphore, barrier, fcntl, epoll, eventfd,
    timerfd) recurse one level: each child action gets a sub-section.
  * ``unittest`` (a dev-only internal) is skipped.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

from markdown_it import MarkdownIt


# Commands whose -h lists child actions that we recurse into automatically.
COMPOUND = {
    "shm", "mutex", "semaphore", "barrier",
    "fcntl", "epoll", "eventfd", "timerfd",
}
# Commands never emitted at all.
SKIP = {"unittest"}


def run_builtin(bash: str, so: str, args: str) -> str:
    """Run ``L_builtin <args>`` inside a fresh bash with the .so enabled."""
    prog = (
        f"enable -f {so} L_builtin\n"
        f"L_builtin {args} 2>&1\n"
        f"exit $?\n"
    )
    cp = subprocess.run(
        [bash, "-c", prog],
        capture_output=True,
        text=True,
        check=False,
    )
    return cp.stdout


def _extract_listing(text: str, header: str) -> list[str]:
    """Return subcommand names from a two-column ``Available subcommands:`` /
    ``Subcommands:`` listing found in *text*.

    Each entry is an indented line ``  name   description``; the block ends at
    the first blank line (after entries have started) or at the first
    non-indented content line.
    """
    idx = text.find(header)
    if idx == -1:
        return []
    region = text[idx + len(header):]
    names: list[str] = []
    entry_re = re.compile(r"^(?: {4}|  )(\S+)\s+(\S.*)$")
    for line in region.splitlines():
        if line.strip() == "":
            if names:
                break
            continue
        if not line.startswith("  "):
            if names:
                break
            continue
        m = entry_re.match(line)
        if m:
            names.append(m.group(1))
    return names


def _help(b, so, cmd: str, *sub: str) -> str:
    """Fetch help text for ``L_builtin <cmd> <sub...> -h`` (fallback --help)."""
    target = " ".join([cmd, *sub])
    out = run_builtin(b, so, f"{target} -h")
    if not out.strip():
        out = run_builtin(b, so, f"{target} --help")
    return out


def _github_anchor(text: str) -> str:
    """GitHub-compatible heading anchor (matches github-slugger): lowercases,
    keeps word characters (incl. underscore), hyphen and space, drops the
    rest, then spaces -> hyphens.  Backticks are already stripped by caller."""
    t = text.lower().strip()
    t = re.sub(r"<[^>]+>", "", t)
    t = re.sub(r"[^\w\- ]", "", t)
    t = re.sub(r"\s+", "-", t)
    t = re.sub(r"-+", "-", t)
    return t.strip("-")


def build_parts(bash: str, so: str) -> list[tuple[int, str, str]]:
    """Return a list of ``(level, title, help_text)`` tuples, in display order."""
    names = _extract_listing(run_builtin(bash, so, "-h"), "Available subcommands:")
    parts: list[tuple[int, str, str]] = []

    for name in names:
        if name in SKIP:
            continue
        help_text = _help(bash, so, name)
        if not help_text.strip():
            continue

        if name == "core":
            parts.append((3, f"L_builtin {name}", help_text))
            for child in _extract_listing(help_text, "Available subcommands:"):
                child_help = f"runs coreutils `{child}` command\n"
                parts.append((4, f"L_builtin {name} {child}", child_help))
            continue

        if name == "ext":
            parts.append((3, f"L_builtin {name}", help_text))
            for child in _extract_listing(help_text, "Available subcommands:"):
                child_help = _help(bash, so, name, child)
                if child_help.strip():
                    parts.append((4, f"L_builtin {name} {child}", child_help))
            continue

        if name in COMPOUND:
            parts.append((3, f"L_builtin {name}", help_text))
            for child in _extract_listing(help_text, "Subcommands:"):
                child_help = _help(bash, so, name, child)
                if child_help.strip():
                    parts.append((4, f"L_builtin {name} {child}", child_help))
            continue

        parts.append((3, f"L_builtin {name}", help_text))

    return parts


def render_body(parts: list[tuple[int, str, str]]) -> str:
    lines: list[str] = []
    for level, title, text in parts:
        lines.append(f"{'#' * level} `{title}`")
        lines.append("")
        lines.append("```")
        lines.append(text.rstrip())
        lines.append("```")
        lines.append("")
    return "\n".join(lines)


def build_toc(markdown_body: str) -> str:
    """Build the per-file TOC from the rendered body using ``markdown-it-py``
    so anchors match the rendered headings."""
    md = MarkdownIt("commonmark")
    toks = md.parse(markdown_body)
    headings: list[tuple[int, str]] = []
    for i, t in enumerate(toks):
        if t.type != "heading_open":
            continue
        inline = toks[i + 1]
        text = inline.content.strip()
        disp = re.sub(r"^`+|`+$", "", text).strip("`")
        lvl = int(t.tag[1:])
        headings.append((lvl, disp))

    if not headings:
        return ""
    base = min(lvl for lvl, _ in headings)
    bullets: list[str] = []
    for lvl, disp in headings:
        indent = "  " * (lvl - base)
        bullets.append(f"{indent}- [{disp}](#{_github_anchor(disp)})")
    return "\n".join(bullets)


def render_reference(parts: list[tuple[int, str, str]]) -> str:
    """Render the full reference: top-level heading, auto-TOC, then the body
    of every section. Returned string ends with a trailing newline."""
    body = render_body(parts)
    toc = build_toc(body)
    return (
        "# L_builtin — Subcommand Reference\n"
        "\n"
        f"{toc}\n"
        "\n"
        f"{body}"
        "\n"
        "---\n"
        "\n"
        "Auto-generated by `scripts/gen_reference.py`. Do not hand-edit; run\n"
        "`make readme` after changes to the subcommand set or help text.\n"
    )


def find_bash() -> Path:
    """Pick a bash from ``build/bash/*/bash``: highest version wins."""
    candidates = sorted(
        Path("build/bash").glob("*/bash"),
        key=lambda p: tuple(int(x) for x in p.parent.name.split(".")),
        reverse=True,
    )
    for c in candidates:
        if c.is_file():
            return c.resolve()
    print("error: no bash found under build/bash/*/bash (run `make build` first)",
          file=sys.stderr)
    raise SystemExit(2)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    ap.add_argument("so", nargs="?", default="./L_builtin.so",
                    help="Path to the L_builtin.so to introspect "
                         "(default: ./L_builtin.so).")
    args = ap.parse_args(argv)

    so = Path(args.so).resolve()
    if not so.exists():
        print(f"error: L_builtin.so not found: {so} (run `make build` first)",
              file=sys.stderr)
        return 2

    bash = find_bash()

    parts = build_parts(str(bash), str(so))
    sys.stdout.write(render_reference(parts))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())