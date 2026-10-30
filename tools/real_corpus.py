#!/usr/bin/env python3
"""Настоящие тексты из общественного достояния для замеров вёрстки (tests/corpus/real/).

Русская проза — с Викитеки, английская — с Project Gutenberg. Текст складывается в FB2
без исправлений: кавычки и тире расставит типограф. Запуск: python3 tools/real_corpus.py
"""
import html
import json
import re
import urllib.parse
import urllib.request
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "tests" / "corpus" / "real"
UA = {"User-Agent": "BookWorm-corpus/0.1 (personal e-reader tests)"}


def get(url):
    return urllib.request.urlopen(urllib.request.Request(url, headers=UA), timeout=60).read()


def wikisource(page):
    q = urllib.parse.urlencode({"action": "parse", "page": page, "prop": "text", "format": "json", "formatversion": "2"})
    h = json.loads(get("https://ru.wikisource.org/w/api.php?" + q))["parse"]["text"]
    ps = [html.unescape(re.sub(r"<[^>]+>", "", p)).strip() for p in re.findall(r"<p>(.*?)</p>", h, re.S)]
    # Без служебных абзацев Викитеки (лицензия, сведения об издании).
    junk = ("авторского права", "общественном достоянии", "Public Domain", "Источник")
    return [re.sub(r"\[\d+\]", "", p) for p in ps if len(p) > 40 and not any(j in p for j in junk)]


def gutenberg(num, start, end):
    t = get(f"https://www.gutenberg.org/cache/epub/{num}/pg{num}.txt").decode("utf-8-sig")
    t = t[t.index(start):t.index(end)]
    ps = [" ".join(p.split()) for p in re.split(r"\n\s*\n", t)]
    return [p for p in ps if len(p) > 40]


def fb2(title, author, lang, paragraphs):
    esc = lambda s: s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
    body = "\n".join(f"<p>{esc(p)}</p>" for p in paragraphs)
    first, last = author.split(" ", 1)
    return (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0">\n'
        f"<description><title-info><author><first-name>{first}</first-name><last-name>{last}</last-name></author>"
        f"<book-title>{esc(title)}</book-title><lang>{lang}</lang></title-info></description>\n"
        f"<body><section><title><p>{esc(title)}</p></title>\n{body}\n</section></body></FictionBook>\n"
    )


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    books = [
        ("pushkin_pikovaya_dama.fb2", "Пиковая дама", "Александр Пушкин", "ru", wikisource("Пиковая дама (Пушкин)")),
        ("gogol_shinel.fb2", "Шинель", "Николай Гоголь", "ru", wikisource("Шинель (Гоголь)/ПСС 1938 (СО)")),
        (
            "austen_pride_ch1-10.fb2", "Pride and Prejudice", "Jane Austen", "en",
            gutenberg(1342, "Chapter I.]", "CHAPTER XI.")[:400],
        ),
    ]
    for name, title, author, lang, ps in books:
        (OUT / name).write_text(fb2(title, author, lang, ps), encoding="utf-8")
        print(f"{name}: абзацев {len(ps)}, знаков {sum(map(len, ps))}")


if __name__ == "__main__":
    main()
