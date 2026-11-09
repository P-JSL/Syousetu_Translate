#!/usr/bin/env python3
"""Check a saved bilingual novel against locally cached Kakuyomu chapter HTML.

This performs no network requests and writes only counts/hashes, not novel text.
It verifies structural completeness, not semantic translation quality.
"""
import argparse
import hashlib
from html.parser import HTMLParser
import json
from pathlib import Path
import re


class Node:
    def __init__(self, tag="", attrs=()):
        self.tag, self.attrs, self.children = tag, dict(attrs), []


class Document(HTMLParser):
    def __init__(self, html):
        super().__init__(convert_charrefs=True)
        self.root = Node()
        self.stack = [self.root]
        self.feed(html)

    def handle_starttag(self, tag, attrs):
        node = Node(tag, attrs)
        self.stack[-1].children.append(node)
        if tag not in {"br", "hr", "img", "meta", "link", "input", "source", "wbr", "area", "base", "col", "embed", "param", "track"}:
            self.stack.append(node)

    def handle_startendtag(self, tag, attrs):
        self.handle_starttag(tag, attrs)
        self.handle_endtag(tag)

    def handle_endtag(self, tag):
        for i in range(len(self.stack) - 1, 0, -1):
            if self.stack[i].tag == tag:
                del self.stack[i:]
                break

    def handle_data(self, data):
        self.stack[-1].children.append(data)


def elements(node):
    if isinstance(node, Node):
        yield node
        for child in node.children:
            yield from elements(child)


def render(node, blocks=True):
    if isinstance(node, str):
        return node
    if node.tag in {"script", "style", "noscript", "rt", "rp"}:
        return ""
    if node.tag == "br":
        return "\n"
    result = "".join(render(c, blocks) for c in node.children)
    return result + ("\n" if blocks and node.tag in {"p", "div", "section", "li", "h2", "h3"} else "")


def episode(html):
    nodes = list(elements(Document(html).root))
    body = next((n for n in nodes if "widget-episodeBody" in n.attrs.get("class", "").split()), None)
    assert body is not None, "Missing cached episode body"
    title = next((n for n in nodes if "widget-episodeTitle" in n.attrs.get("class", "").split()), None)
    if title is None:
        title = next((n for n in nodes if n.tag == "h1"), None)
    assert title is not None, "Missing cached episode title"
    lines = [line.rstrip() for line in render(body).replace("\r\n", "\n").split("\n")]
    while lines and not lines[0].strip():
        lines.pop(0)
    while lines and not lines[-1].strip():
        lines.pop()
    assert lines, "Empty cached episode"
    return " ".join(render(title, False).split()), lines


def validate(source_dir, output, chapters):
    data = output.read_bytes()
    text = data.decode("utf-8")
    markers = list(re.finditer(r"^\[(\d+)/(\d+)\] (.*)$", text, re.M))
    assert [int(m[1]) for m in markers] == list(range(1, chapters + 1)), "Missing, duplicate, or reordered chapters"
    assert all(int(m[2]) == chapters for m in markers), "Incorrect total chapter count"
    report = []
    for i, marker in enumerate(markers, 1):
        source = source_dir / f"{i}.html"
        raw = source.read_bytes()
        title, original = episode(raw.decode("utf-8"))
        assert marker[3].endswith(" / " + title), f"Chapter {i}: title/order mismatch"
        end = markers[i].start() if i < len(markers) else len(text)
        section = text[marker.end():end]
        assert section.startswith("\n\n"), f"Chapter {i}: missing header separator"
        saved = section[2:].split("\n")
        assert len(saved) >= len(original) * 2, f"Chapter {i}: missing bilingual lines"
        nonempty = hangul = 0
        for j, line in enumerate(original):
            assert saved[j * 2] == line, f"Chapter {i}, source line {j+1}: missing, modified, or reordered original"
            translation = saved[j * 2 + 1]
            if line.strip():
                assert translation.strip(), f"Chapter {i}, source line {j+1}: empty translation"
                nonempty += 1
                hangul += any("가" <= ch <= "힣" for ch in translation)
        assert not any(s.strip() for s in saved[len(original) * 2:]), f"Chapter {i}: unexpected extra body lines"
        report.append({"chapter": i, "source_lines": len(original), "nonempty_source_lines": nonempty,
                       "korean_lines": hangul, "source_sha256": hashlib.sha256(raw).hexdigest()})
    return {"chapters": chapters, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest(),
            "source_lines": sum(r["source_lines"] for r in report), "complete": True,
            "semantic_accuracy_certified": False, "chapter_checks": report}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--chapters", type=int, default=35)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    assert args.chapters > 0
    report = validate(args.source_dir, args.output, args.chapters)
    if args.report:
        args.report.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({k: v for k, v in report.items() if k != "chapter_checks"}))


if __name__ == "__main__":
    main()
