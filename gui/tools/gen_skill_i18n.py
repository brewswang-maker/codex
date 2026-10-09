#!/usr/bin/env python3
"""Regenerate crates/codex-gui-core/src/plugin_i18n_skills.tsv.

Scans every marketplace source for bundled SKILL.md files, derives each
skill's `plugin:skill` wire name (the same form the app-server sends to
the GUI), and looks up the Chinese one-liner in
tools/skill_i18n_source.tsv (tab-separated: english-prefix<TAB>chinese).

After a marketplace refresh, run this to rebuild the TSV; new skills
whose descriptions are not in the source table are reported so they can
be appended there (add the translation, rerun).

Usage: python3 gui/tools/gen_skill_i18n.py
"""
import glob
import json
import os
import re
import sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))  # gui/
DEST = os.path.join(
    REPO, "crates", "codex-gui-core", "src", "plugin_i18n_skills.tsv"
)
SOURCE = os.path.join(REPO, "tools", "skill_i18n_source.tsv")


def frontmatter(path):
    try:
        head = open(path, encoding="utf-8").read(4000)
    except OSError:
        return None
    m = re.search(r"^---\s*\n(.*?)\n---", head, re.S)
    if not m:
        return None
    block = m.group(1)
    name = re.search(r"^name:\s*[\"']?(.+?)[\"']?\s*$", block, re.M)
    desc = re.search(
        r"^description:\s*(.+?)(?=\n[a-zA-Z_-]+:|\Z)", block, re.M | re.S
    )
    if not name:
        return None
    text = re.sub(r"\s+", " ", desc.group(1)).strip().strip("\"'") if desc else ""
    # Strip YAML block-scalar indicators (`|`, `>`, with -+ modifiers).
    text = re.sub(r"^[|>][-+]?\s*", "", text)
    return name.group(1).strip(), text


def skill_files():
    """Yield (plugin, name, description) for every marketplace skill."""
    home = os.path.expanduser("~")
    for f in glob.glob(f"{home}/.codex/.tmp/plugins/plugins/*/skills/*/SKILL.md"):
        plugin = f.split("/skills/")[0].rsplit("/", 1)[1]
        yield plugin, f
    for mk, wire in [("ponytail", "ponytail"), ("open-code-review", "open-code-review-codex")]:
        for f in glob.glob(f"{home}/.codex/.tmp/marketplaces/{mk}/**/SKILL.md", recursive=True):
            yield wire, f
    for market in ("openai-skills", "awesome-codex-skills", "skillhub-demo"):
        for pattern in ("plugins/*/skills/*/SKILL.md", "*/SKILL.md", "skills/*/SKILL.md"):
            for f in glob.glob(f"{home}/marketplaces/{market}/{pattern}"):
                plugin = f.split("/marketplaces/" + market + "/")[1].split("/")[0]
                if plugin == "skills":
                    plugin = f.split("/skills/")[1].split("/")[0]
                yield plugin, f
    # Standalone user- and repo-level skills: the wire name has no plugin
    # prefix, so yield plugin None and key the row by the bare name.
    for base in (f"{home}/.codex/skills", f"{REPO}/../.codex/skills"):
        for f in glob.glob(f"{base}/*/SKILL.md"):
            yield None, f


def has_cjk(text):
    """True when the leading excerpt reads as Chinese: descriptions that
    merely embed a few CJK trigger words stay on the translation path."""
    head = text[:24]
    cjk = sum(
        1
        for ch in head
        if "\u3000" <= ch <= "\u303f" or "\u4e00" <= ch <= "\u9fff" or "\uff00" <= ch <= "\uffef"
    )
    return cjk * 3 >= len(head)


def condense(text):
    """Mirror the Rust picker fallback: first CJK sentence, cap at 80 chars."""
    for i, ch in enumerate(text):
        if ch in "。！？" and i >= 10:
            text = text[: i + 1]
            break
    if len(text) > 80:
        return text[:80] + "…"
    return text


def main():
    translations = {}
    for line in open(SOURCE, encoding="utf-8"):
        line = line.rstrip("\n")
        if "\t" in line:
            en, zh = line.split("\t", 1)
            translations[en] = zh.strip()

    rows, misses = [], []
    seen = set()
    for plugin, path in skill_files():
        parsed = frontmatter(path)
        if not parsed:
            continue
        name, desc = parsed
        wire = f"{plugin}:{name}" if plugin else name
        if wire in seen:
            continue
        seen.add(wire)
        # Chinese descriptions are already the target copy; pass them through.
        zh = condense(desc) if has_cjk(desc) else translations.get(desc[:400])
        if zh:
            rows.append(f"{wire}\t{wire}\t{zh}")
        elif desc:
            misses.append(wire)

    with open(DEST, "w", encoding="utf-8") as out:
        out.write("\n".join(rows) + "\n")
    print(f"wrote {len(rows)} rows to {os.path.relpath(DEST, REPO)}")
    if misses:
        print(f"{len(misses)} skills lack translations (append to {os.path.relpath(SOURCE, REPO)}):")
        for wire in sorted(misses)[:40]:
            print("  ", wire)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
