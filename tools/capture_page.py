#!/usr/bin/env python3
"""Capture HTML/CSS/fonts for Solara's offline page_probe (no script execution).

Use a new output directory per capture. Full URLs key resources, so CDN hosts
and query variants cannot overwrite each other. This bounded diagnostic CSS
URL scanner is not the browser's stylesheet loader or a general site mirror.
"""
import argparse
from collections import deque
import hashlib
from html.parser import HTMLParser
import json
from pathlib import Path
import re
from urllib.parse import urljoin, urlsplit, urldefrag
from urllib.request import Request, urlopen

MAX_RESOURCE = 8 * 1024 * 1024
MAX_TOTAL = 64 * 1024 * 1024
MAX_REQUESTS = 128


class Page(HTMLParser):
    def __init__(self):
        super().__init__()
        self.base = None
        self.styles = []
        self.inline_css = []
        self.in_style = False
        self.scripts = []

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "base" and self.base is None:
            self.base = attrs.get("href")
        if tag == "link" and "stylesheet" in attrs.get("rel", "").lower().split():
            self.styles.append(attrs.get("href", ""))
        if tag == "style":
            self.in_style = True
        if tag == "script":
            self.scripts.append(attrs)

    def handle_endtag(self, tag):
        if tag == "style":
            self.in_style = False

    def handle_data(self, data):
        if self.in_style:
            self.inline_css.append(data)


def css_urls(css):
    # Also include quoted @import forms which do not contain url(...).
    css = re.sub(r"/\*.*?\*/", "", css, flags=re.S)
    for match in re.finditer(r"url\(\s*['\"]?([^\s)'\"]+)|@import\s+['\"]([^'\"]+)", css):
        yield match[1] or match[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("url")
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    args.directory.mkdir(parents=True, exist_ok=False)
    total = 0

    def fetch(url, limit):
        nonlocal total
        if urlsplit(url).scheme not in ("http", "https"):
            raise ValueError("only HTTP(S) resources are captured")
        with urlopen(Request(url, headers={"User-Agent": "Solara-layout-probe/1"}), timeout=30) as response:
            data = response.read(min(limit, MAX_TOTAL - total) + 1)
            if len(data) > limit or total + len(data) > MAX_TOTAL:
                raise ValueError("capture byte budget exceeded")
            total += len(data)
            return data, response.url, response.headers.get_content_type()

    html, page_url, _ = fetch(args.url, 16 * 1024 * 1024)
    (args.directory / "page.html").write_bytes(html)
    (args.directory / "page.url").write_text(page_url)
    page = Page()
    page.feed(html.decode("utf-8"))
    base = urljoin(page_url, page.base or "")
    pending = deque((urljoin(base, href), True) for href in page.styles)
    pending.extend((urljoin(base, href), False) for href in css_urls("\n".join(page.inline_css)))
    seen, entries, errors = set(), [], []
    while pending and len(seen) < MAX_REQUESTS:
        url, stylesheet = pending.popleft()
        url = urldefrag(url)[0]
        if url in seen or urlsplit(url).scheme not in ("http", "https"):
            continue
        seen.add(url)
        try:
            data, final_url, content_type = fetch(url, MAX_RESOURCE)
            name = hashlib.sha256(url.encode()).hexdigest()
            (args.directory / name).write_bytes(data)
            entries.append(f"{url}\t{name}\t{final_url}\n")
            if stylesheet or content_type == "text/css":
                pending.extend((urljoin(final_url, href), False) for href in css_urls(data.decode("utf-8")))
        except Exception as error:
            errors.append({"url": url, "error": str(error)})
    (args.directory / "resources.tsv").write_text("".join(entries))
    report = {"url": page_url, "bytes": total, "resources": len(entries),
              "scripts": page.scripts, "errors": errors, "request_limit_reached": bool(pending)}
    (args.directory / "capture.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
