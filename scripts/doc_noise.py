#!/usr/bin/env python3
"""List doc comments that probably only repeat the name of what they document.

A candidate is a `///` comment of one line on a field, variant or function whose words (without
filler words) are all found in the item's name. It is a list for a person to review, not a
fixer: some of these comments carry a unit or an invariant in few words and should stay.

Usage: scripts/doc_noise.py [path ...]    (default: src)
"""
import re
import subprocess
import sys

FILLER = {"the", "a", "an", "of", "to", "for", "is", "in", "on", "and", "or", "this", "that", "its", "it"}
ITEM = re.compile(r"^\s*(?:pub(?:\([a-z]+\))?\s+)?(?:async\s+)?(?:const\s+)?(?:fn|struct|enum|type|static)?\s*([A-Za-z_][A-Za-z0-9_]*)")


def words(text):
    return [w for w in re.findall(r"[a-z0-9]+", text.lower()) if w not in FILLER]


def name_words(name):
    parts = re.sub(r"([a-z0-9])([A-Z])", r"\1 \2", name).replace("_", " ")
    return {w for w in parts.lower().split()}


def scan(path):
    try:
        lines = open(path, encoding="utf-8").read().split("\n")
    except OSError:
        return
    for i, line in enumerate(lines):
        if not line.lstrip().startswith("///") or line.lstrip().startswith("////"):
            continue
        before = lines[i - 1].lstrip().startswith("///") if i else False
        after = lines[i + 1].lstrip().startswith("///") if i + 1 < len(lines) else False
        if before or after:
            continue  # more than one line: it says something
        j = i + 1
        while j < len(lines) and lines[j].lstrip().startswith("#["):
            j += 1
        if j >= len(lines):
            continue
        match = ITEM.match(lines[j])
        if not match:
            continue
        doc = line.lstrip()[3:].strip().rstrip(".")
        if "ProtoOA" in doc:
            continue  # names the wire message: information the name does not carry
        doc_words = words(doc)
        if not doc_words:
            continue
        item_words = name_words(match.group(1))
        singular = {w.rstrip("s") for w in item_words}
        if all(w in item_words or w.rstrip("s") in singular for w in doc_words) or len(doc_words) <= 1:
            print(f"{path}:{i + 1}: {doc}")


def main():
    roots = sys.argv[1:] or ["src"]
    files = subprocess.check_output(["git", "ls-files", *roots], text=True).split()
    for path in files:
        if path.endswith(".rs"):
            scan(path)


if __name__ == "__main__":
    main()
