#!/usr/bin/env python3
"""Генерирует синтетический тестовый корпус FB2/EPUB в tests/corpus/synthetic/.

Каждый файл нацелен на свой класс трудностей (см. tests/corpus/README.md).
Входные тексты намеренно «грязные» — такими их делают реальные конвертеры.
Запуск: python3 tools/gen_corpus.py  (нужен ImageMagick для картинок)
"""
import base64
import random
import shutil
import subprocess
import zipfile
from pathlib import Path
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests" / "corpus" / "synthetic"
IMG = OUT / "_images"

# ---------------------------------------------------------------- тексты

S_RU = [
    "Утро выдалось серым и тихим, как будто город ещё не решил, стоит ли просыпаться.",
    "Он долго стоял у окна, разглядывая мокрые крыши и редких прохожих под зонтами.",
    "Письмо пришло неожиданно: конверт был помят, а марка наклеена вверх ногами.",
    "В библиотеке пахло старой бумагой, клеем и немного пылью, которую никто не решался тревожить.",
    "Она читала медленно, возвращаясь к каждой странной фразе, словно к знакомой тропинке в лесу.",
    "Поезд опоздал на сорок минут, и на перроне успела собраться небольшая толпа недовольных.",
    "Никто не знал, откуда в деревне появился этот человек и почему он так хорошо говорит по-французски.",
    "Сад за домом давно зарос, но яблони всё ещё плодоносили, упрямо и щедро.",
    "Ветер переменился к вечеру, и над рекой поплыл тяжёлый запах скошенной травы.",
    "Разговор не клеился: каждый думал о своём и боялся сказать лишнее.",
    "Старик достал из кармана часы, щёлкнул крышкой и, не взглянув на циферблат, убрал их обратно.",
    "Лампа на столе мигнула, погасла и через мгновение загорелась снова, уже тусклее.",
    "Дорога шла вдоль оврага, то поднимаясь на холмы, то ныряя в сырые берёзовые рощи.",
    "Он понимал, что опоздал, но всё равно шёл быстрее, почти бежал, задыхаясь от холодного воздуха.",
    "На подоконнике лежала раскрытая книга, страницы которой шевелил сквозняк.",
    "Слова приходили сами, и нужно было только успевать их записывать.",
    "Внизу, во дворе, дети строили крепость из мокрого снега и спорили о том, кто будет королём.",
    "Её голос звучал спокойно, но пальцы, сжимавшие край стола, выдавали волнение.",
    "Да.",
    "Так прошла неделя.",
    "Город, в котором прошло его детство, за двадцать лет изменился до неузнаваемости: исчезли "
    "деревянные дома с резными наличниками, на месте рынка выросли стеклянные башни, а тихая улица, "
    "где когда-то цвела сирень, превратилась в шумный проспект с бесконечным потоком машин.",
    "Экспериментально-исследовательская лаборатория высокочастотной электрофизиологии располагалась "
    "в полуподвальном помещении бывшего сельскохозяйственного техникума.",
]

S_EN = [
    "The morning was grey and quiet, as if the town had not yet decided whether to wake up.",
    "He stood by the window for a long time, watching the wet roofs and the few passers-by under their umbrellas.",
    "The letter arrived unexpectedly; the envelope was creased and the stamp had been stuck on upside down.",
    "The library smelled of old paper, glue, and a little dust that nobody dared to disturb.",
    "She read slowly, returning to every strange phrase as if to a familiar path in the woods.",
    "Nobody knew where the stranger had come from, or why he spoke such excellent French.",
    "The garden behind the house had long been overgrown, yet the apple trees still bore fruit, stubbornly and generously.",
    "Yes.",
    "So a week went by.",
    "The office was affluent in a quiet way: the fluffy carpets, the official flags, the fjord painted "
    "above the fireplace, and the efficient staff who shuffled files without a word.",
    "Incomprehensibilities and characteristically uncharacteristic counterrevolutionaries were, "
    "notwithstanding their institutionalization, extraordinarily misunderstood.",
]


def para(rng, pool, lo, hi):
    return " ".join(rng.choice(pool) for _ in range(rng.randint(lo, hi)))


def p(text):
    """Абзац FB2. Текст с префиксом RAW: вставляется как есть (с разметкой)."""
    if text.startswith("RAW:"):
        return f"<p>{text[4:]}</p>"
    return f"<p>{escape(text)}</p>"


# ---------------------------------------------------------------- картинки

def magick(*args):
    subprocess.run(["magick", *args], check=True, capture_output=True)


def make_images():
    IMG.mkdir(parents=True, exist_ok=True)
    magick("-size", "600x900", "gradient:#2b4a6f-#d9c7a3",
           "-fill", "white", "-pointsize", "64", "-gravity", "center",
           "-annotate", "+0-120", "BookWorm", "-pointsize", "36", "-annotate", "+0+0", "Test Corpus",
           "-quality", "85", str(IMG / "cover.jpg"))
    magick("-size", "1600x500", "gradient:#8fb3d9-#f2e6c9", "-fill", "#3a3a3a",
           "-draw", "circle 300,250 300,100", "-draw", "rectangle 900,120 1400,380",
           "-quality", "85", str(IMG / "wide.jpg"))
    magick("-size", "500x1600", "gradient:#f2e6c9-#6b8e5a", "-fill", "#3a3a3a",
           "-draw", "polygon 250,200 450,1400 50,1400", str(IMG / "tall.png"))
    # Чёрные линии на прозрачном фоне: исчезают в ночной теме, если не обработать.
    magick("-size", "800x500", "xc:none", "-stroke", "black", "-strokewidth", "6", "-fill", "none",
           "-draw", "circle 200,250 200,80", "-draw", "line 400,80 750,420",
           "-draw", "rectangle 420,300 760,460", str(IMG / "lineart_alpha.png"))
    magick("-size", "48x48", "xc:#c0392b", "-fill", "white", "-draw", "circle 24,24 24,10",
           str(IMG / "inline.png"))
    magick("-size", "1200x1700", "xc:white", "-fill", "black", "-pointsize", "22",
           "-annotate", "+60+80", "Scanned page (grayscale, almost full screen)",
           "-colorspace", "Gray", str(IMG / "fullpage_gray.png"))


def img_bytes(name):
    return (IMG / name).read_bytes()


def b64(name):
    return base64.encodebytes(img_bytes(name)).decode("ascii")


# ---------------------------------------------------------------- FB2

FB2_NS = ('xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" '
          'xmlns:l="http://www.w3.org/1999/xlink"')


def fb2(title, body, *, notes="", binaries="", lang="ru", annotation="",
        sequence=None, cover=None, encoding="utf-8", author=("Тест", "Корпусов")):
    seq = f'<sequence name="{escape(sequence[0])}" number="{sequence[1]}"/>' if sequence else ""
    cov = f'<coverpage><image l:href="#{cover}"/></coverpage>' if cover else ""
    ann = f"<annotation>{p(annotation)}</annotation>" if annotation else ""
    return f"""<?xml version="1.0" encoding="{encoding}"?>
<FictionBook {FB2_NS}>
<description>
 <title-info>
  <genre>prose_contemporary</genre>
  <author><first-name>{author[0]}</first-name><last-name>{author[1]}</last-name></author>
  <book-title>{escape(title)}</book-title>
  {ann}{cov}
  <lang>{lang}</lang>
  {seq}
 </title-info>
 <document-info><author><nickname>gen_corpus</nickname></author><date>2026</date><id>{escape(title)}</id><version>1.0</version></document-info>
</description>
<body>
{body}
</body>
{notes}
{binaries}
</FictionBook>
"""


def binary(id_, name, ctype):
    return f'<binary id="{id_}" content-type="{ctype}">{b64(name)}</binary>'


def section(title, content, epigraph=""):
    t = "".join(p(x) for x in ([title] if isinstance(title, str) else title))
    return f"<section><title>{t}</title>{epigraph}{content}</section>"


# 01 — типограф
TYPO_CASES = [
    ("Многоточия", [
        "Он помолчал... Потом сказал: «Ну что ж...»",
        "...А потом наступило утро.",
        "Что?.. Неужели!.. Да ну?!. Не может быть...",
        "Тишина . . . и снова тишина.",
        "Он ждал…… и ждал....",
    ]),
    ("Кавычки и апострофы", [
        'Он сказал: "Это слово "свобода" ничего не значит".',
        '"Привет!" - крикнул он.',
        'Она ответила: "Нет", и вышла.',
        'Название "The Lord of the Rings" он произносил по-английски.',
        "Уже правильные: «Он сказал: „Это так“».",
        'Смешанные: «Он сказал: "Это так"».',
        "Ёлочки внутри лапок: „Он сказал: «Это так»“ — так быть не должно.",
        "Апострофы: д'Артаньян, О'Генри, Жанна д’Арк, rock'n'roll, Mc'Donalds.",
    ]),
    ("Тире и дефисы", [
        "- Ты куда? - спросила она. - Уже поздно.",
        "-- Никуда, -- ответил он.",
        "– Правда? – Правда.",
        "Москва - столица России. Жизнь -- это то, что происходит, пока ты строишь планы.",
        "Кто-нибудь, из-за угла, по-русски, северо-восточный, сказал-то, ну-ка, Петров-Водкин.",
        "Годы 1812-1814, страницы 25-30, рейс Москва-Петербург, счёт 3:2, 5-6 человек.",
        "Тире в конце строки — не должно оказаться в начале следующей, а дефис-минус -5 — это число.",
    ]),
    ("Числа, знаки и сокращения", [
        "В 1812 г. армия прошла 2500 км за 3 мес. и потеряла 10 000 чел.",
        "Дом № 5, квартира №12, § 3, §4, стр. 15, с. 25, рис. 3, т. 2.",
        "Т.е. всё, т. е. всё, и т.д., и т. д., и т.п., и пр., и др., см. выше.",
        "XIX в., XX век, Пётр I, 10 %, 15%, −5 °C, +3°C, 36,6°.",
        "2×2=4, x², H₂O, ½ стакана, 3/4 ложки, 1/2, 10^6, 5' 10\".",
        "Телефон: +7 (495) 123-45-67, ГОСТ 7.0.11-2011.",
        "Цена: 1 000 000 руб., или 1000000 руб., или $15.99, или 12,5 тыс. €.",
    ]),
    ("Инициалы и имена", [
        "А.С. Пушкин, А. С. Пушкин, Пушкин А.С., Л.Н.Толстой, Ф. М. Достоевский.",
        "Письмо адресовано г-ну Иванову И. И. и г-же Петровой М. А., копия — проф. Сидорову.",
    ]),
    ("Короткие слова на концах строк", [
        "Он шёл в лес и в поле, а к ночи с горы у реки о чём-то думал, но не мог и не хотел "
        "ни с кем говорить: в доме у сестры за столом по вечерам из окна от ветра до утра "
        "был слышен шум, и в нём, как в море, тонули и голоса, и шаги, и мысли, а он всё шёл "
        "и шёл, и в каждой встречной тени ему чудился кто-то знакомый.",
    ]),
    ("Длинные слова", [
        "Результаты рентгеноэлектрокардиографического исследования показали, что "
        "человеконенавистничество и частнопредпринимательский дух его высокопревосходительства "
        "превысокомногорассмотрительствующий совет счёл сельскохозяйственно-машиностроительными.",
        "Достопримечательность, переосвидетельствоваться, водогрязеторфопарафинолечение, "
        "электрофоретически, автоматизированный, непредсказуемость, безукоризненно.",
    ]),
    ("Ё и знаки ударения", [
        "Ёлка, ещё, всё, её, ёж, Пётр Фёдорович, Семёнов, тёмно-зелёный.",
        "За́мок и замо́к, му́ка и мука́, о́рган и орга́н, вода́ — во́ды, а́тлас и атла́с.",
    ]),
    ("Пробелы и невидимые символы", [
        "Двойные  пробелы,\tтабуляция, неразрывный\u00a0пробел, тонкая\u2009шпация, "
        "пробел\u200bнулевой\u200bширины, мягкий пере\u00adнос в сло\u00adве, неразрывный\u2011дефис.",
        "   Пробелы в начале и в конце абзаца.   ",
        "Абзац\nс переводом\nстроки внутри.",
    ]),
    ("Латиница и другие языки в русском тексте", [
        "Мoсква (здесь латинская «o»), Cанкт-Петербург (латинская «C») — частая ошибка распознавания.",
        "Eh bien, mon prince. Gênes et Lucques ne sont plus que des apanages, des поместья, de la famille Buonaparte.",
        "Он любил повторять: to be or not to be, that is the question, — и смеялся.",
        "Немецкое Donaudampfschifffahrtsgesellschaft не переносится по русским правилам.",
    ]),
    ("Ссылки и адреса", [
        "Подробности: https://ru.wikipedia.org/wiki/Типографика_электронных_книг_и_правила_вёрстки "
        "и почта reader.feedback@example.com, а также файл C:\\Users\\Reader\\Documents\\book.fb2.",
    ]),
    ("Разметка внутри абзаца", [
        "RAW:Слово с <emphasis>курсивом</emphasis>, <strong>жирным</strong>, "
        "<strong><emphasis>и тем и другим</emphasis></strong>, по<emphasis>лу</emphasis>курсив "
        "внутри слова, <strikethrough>зачёркнутое</strikethrough>, <code>моноширинный_код()</code>, "
        "E = mc<sup>2</sup>, H<sub>2</sub>O, <emphasis>курсив с кавычками \"внутри\"</emphasis>.",
        "RAW:<emphasis>Целый абзац курсивом, который занимает несколько строк, чтобы было видно, "
        "как курсивное начертание выглядит при выключке по ширине и с переносами слов в длинных строках.</emphasis>",
    ]),
]


def gen_typography():
    body = [section(["Типографика", "грязный входной текст"], "")]
    for name, paras in TYPO_CASES:
        body.append(section(name, "".join(p(x) for x in paras)))
    return fb2("01 Типограф", "\n".join(body))


# 02 — структура FB2
def gen_structure():
    rng = random.Random(2)
    pushkin = """<poem><title><p>К ***</p></title>
<stanza><v>Я помню чудное мгновенье:</v><v>Передо мной явилась ты,</v>
<v>Как мимолётное виденье,</v><v>Как гений чистой красоты.</v></stanza>
<stanza><v>В томленьях грусти безнадежной,</v><v>В тревогах шумной суеты,</v>
<v>Звучал мне долго голос нежный</v><v>И снились милые черты.</v></stanza>
<text-author>А. С. Пушкин</text-author><date>1825</date></poem>"""
    lermontov = """<poem><stanza><v>Белеет парус одинокой</v><v>В тумане моря голубом!..</v>
<v>Что ищет он в стране далекой?</v><v>Что кинул он в краю родном?..</v></stanza></poem>"""
    long_verse = """<poem><title><p>Длинные строки стиха</p></title><stanza>
<v>Эта строка стиха настолько длинна, что обязательно перенесётся на следующую строку экрана, и продолжение должно получить отступ, как у Kindle,</v>
<v>а эта короткая.</v>
<v>И снова очень длинная строка, в которой много слов, чтобы проверить, как выглядит висячий отступ продолжения при узком экране телефона.</v>
</stanza><stanza><v>Вторая строфа</v><v>   с отступом из пробелов в начале строки</v><v>и обычная строка.</v></stanza></poem>"""
    epigraph1 = ('<epigraph><p>И жить торопится, и чувствовать спешит.</p>'
                 '<text-author>Князь Вяземский</text-author></epigraph>')
    epigraph2 = f'<epigraph>{lermontov}<text-author>М. Ю. Лермонтов</text-author></epigraph>'
    cite = ('<cite><p>Цитата из двух абзацев. Первый абзац достаточно длинный, чтобы занять '
            'несколько строк и показать отступы цитаты слева и справа.</p><p>Второй абзац цитаты.</p>'
            '<text-author>Неизвестный автор</text-author></cite>')
    table = ('<table><tr><th>Год</th><th>Событие</th><th>Примечание</th></tr>'
             '<tr><td>1812</td><td>Отечественная война</td><td>Длинный текст в ячейке, который должен перенестись внутри ячейки</td></tr>'
             '<tr><td>1825</td><td>Восстание декабристов</td><td>—</td></tr>'
             '<tr><td align="right">1861</td><td colspan="2">Отмена крепостного права (объединённые ячейки)</td></tr></table>')

    ch1 = section(["Глава первая", "Заголовок из двух строк"], (
        p(para(rng, S_RU, 4, 6))
        + '<p>Абзац со сноской<a l:href="#n1" type="note">[1]</a> и ещё одной сноской'
          '<a l:href="#n2" type="note">2</a>, которая очень длинная.</p>'
        + p(para(rng, S_RU, 3, 5))
        + "<empty-line/>"
        + p(para(rng, S_RU, 2, 4))
        + "<subtitle>* * *</subtitle>"
        + p(para(rng, S_RU, 3, 6))
        + cite
        + p(para(rng, S_RU, 3, 5))
    ), epigraph1)

    sub = "".join([
        section("Часть 1.1. Вложенный раздел", p(para(rng, S_RU, 3, 5)) + pushkin + p(para(rng, S_RU, 2, 4))),
        section("Часть 1.2. Раздел с подразделами",
                section("1.2.1. Третий уровень", p(para(rng, S_RU, 2, 4)))
                + section("1.2.2. Ещё третий уровень",
                          section("1.2.2.1. Четвёртый уровень", p(para(rng, S_RU, 2, 3))))),
    ])
    ch2 = f"<section><title><p>Глава вторая</p></title>{epigraph2}{sub}</section>"

    ch3 = section("Глава третья. Картинки и таблицы", (
        p("Широкая картинка ниже должна вписаться по ширине.")
        + '<image l:href="#wide.jpg" title="Широкая картинка"/>'
        + p("Высокая картинка должна вписаться по высоте страницы целиком, не разрываясь.")
        + '<image l:href="#tall.png"/>'
        + p("Чёрные линии на прозрачном фоне — проверка ночной темы:")
        + '<image l:href="#lineart_alpha.png"/>'
        + '<p>Картинка внутри строки <image l:href="#inline.png"/> посреди текста.</p>'
        + table
        + '<p>Сноска на сноску<a l:href="#n3" type="note">3</a>, сноска со стихами'
        '<a l:href="#n4" type="note">4</a> и повторная ссылка на первую<a l:href="#n1" type="note">1</a>.</p>'
        + '<p>Комментарий из отдельного тела comments<a l:href="#c1" type="note">*</a>.</p>'
        + '<image l:href="#fullpage_gray.png"/>'
        + long_verse
    ))
    ch4 = section("Глава четвёртая, пустая", "")
    ch5 = section("Глава пятая. Последняя", p(para(rng, S_RU, 3, 5)) + p("Конец."))

    body = "<title><p>Тест Корпусов</p><p>02 Структура FB2</p></title>" + ch1 + ch2 + ch3 + ch4 + ch5
    notes = f"""<body name="notes"><title><p>Примечания</p></title>
<section id="n1"><title><p>1</p></title><p>Короткая сноска.</p></section>
<section id="n2"><title><p>2</p></title>{''.join(p(para(rng, S_RU, 4, 6)) for _ in range(4))}</section>
<section id="n3"><title><p>3</p></title><p>Эта сноска ссылается на первую<a l:href="#n1" type="note">1</a>.</p></section>
<section id="n4"><title><p>4</p></title><p>Сноска со стихами:</p>{lermontov}</section>
</body>
<body name="comments"><section id="c1"><title><p>*</p></title><p>Комментарий редактора.</p></section></body>"""
    bins = "\n".join([
        binary("cover.jpg", "cover.jpg", "image/jpeg"),
        binary("wide.jpg", "wide.jpg", "image/jpeg"),
        binary("tall.png", "tall.png", "image/png"),
        binary("lineart_alpha.png", "lineart_alpha.png", "image/png"),
        binary("inline.png", "inline.png", "image/png"),
        binary("fullpage_gray.png", "fullpage_gray.png", "image/png"),
    ])
    return fb2("02 Структура FB2", body, notes=notes, binaries=bins, cover="cover.jpg",
               sequence=("Тестовый корпус", 2),
               annotation="Аннотация: проверка всех структурных элементов FB2.")


# 03 — windows-1251
def gen_cp1251():
    paras = ["- Ну что, поехали? - спросил он.", "\"Поехали\", - ответила она...",
             "Дом № 5, т.е. совсем рядом. Цена - 100 руб.", "Ёлка, ещё, всё — «ёлочки» и „лапки“."]
    body = section("Кодировка windows-1251", "".join(p(x) for x in paras + S_RU[:6]))
    return fb2("03 Кодировка cp1251", body, encoding="windows-1251").encode("cp1251")


# 05 — разбиение на страницы
def gen_pagination():
    rng = random.Random(5)
    out = []
    for i in range(1, 41):
        parts = []
        if i % 7 == 0:
            parts.append("<subtitle>Подзаголовок сразу после заголовка</subtitle>")
        for _ in range(rng.randint(1, 9)):
            kind = rng.random()
            if kind < 0.15:
                parts.append(p(rng.choice(["Да.", "Нет.", "Так прошла неделя.", "— Ну?", "Тишина."])))
            elif kind < 0.25:
                parts.append(p(para(rng, S_RU, 14, 22)))  # абзац длиннее страницы
            elif kind < 0.32:
                parts.append("<poem><stanza>" + "".join(
                    f"<v>{escape(rng.choice(S_RU)[:rng.randint(20, 70)].rstrip())}</v>"
                    for _ in range(rng.randint(4, 12))) + "</stanza></poem>")
            else:
                parts.append(p(para(rng, S_RU, 2, 5)))
        if i % 5 == 0:
            sub = section(f"{i}.1. Подраздел", p(para(rng, S_RU, 1, 3)))
            parts.append(sub)
        out.append(section(f"Раздел {i}", "".join(parts)))
    return fb2("05 Разбиение на страницы", "\n".join(out))


# 08 — большой файл
def gen_big():
    rng = random.Random(8)
    out = []
    for i in range(1, 201):
        paras = "".join(p(para(rng, S_RU, 2, 8)) for _ in range(rng.randint(25, 45)))
        out.append(section(f"Глава {i}", paras))
    return fb2("08 Большая книга", "\n".join(out))


# 09 — сломанный FB2
def gen_broken():
    return ("\ufeff\n  <?xml version=\"1.0\" encoding=\"utf-8\"?>\n"
            '<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" '
            'xmlns:xlink="http://www.w3.org/1999/xlink">\n'
            "<description><title-info><genre>prose</genre><author><last-name>Сломанный</last-name></author>"
            "<book-title>09 Сломанный FB2</book-title><lang>RU</lang>"
            '<coverpage><image xlink:href="cover.jpg"/></coverpage></title-info></description>\n'
            "<body><section><title><p>Ошибки разметки</p></title>\n"
            "<p>Неэкранированный амперсанд: Smith & Wesson, AT&T.</p>\n"
            "<p>Неизвестный тег: <span class=\"x\">текст в span</span> и <font color=\"red\">font</font>.</p>\n"
            "<p>Ссылка xlink:href вместо l:href<a xlink:href=\"#n1\" type=\"note\">1</a>.</p>\n"
            "<p>Ссылка на несуществующую сноску<a xlink:href=\"#n99\" type=\"note\">99</a>.</p>\n"
            "<p>Картинка без бинарника:</p><image xlink:href=\"#missing.jpg\"/>\n"
            "<p>Незакрытый абзац\n"
            "<p>Следующий абзац после незакрытого.</p>\n"
            "</section><section></section><section><title><p>Раздел только с заголовком</p></title></section>\n"
            "<section><p>Раздел без заголовка.</p><p>Атрибут без кавычек: <a l:href=#n1>ссылка</a>.</p></section>\n"
            "</body>\n"
            '<body name="notes"><section id="n1"><p>Сноска.</p></section><section id="n1"><p>Дубликат id.</p></section></body>\n'
            "</FictionBook>\n")


# 10 — калибровка (тот же текст в FB2 и EPUB)
CALIB_TITLE = "10 Калибровка"
CALIB_HEAD = "Глава первая"
CALIB_PARAS = [
    "Утро выдалось серым и тихим, как будто город ещё не решил, стоит ли просыпаться. Он долго "
    "стоял у окна, разглядывая мокрые крыши и редких прохожих под зонтами. Письмо пришло неожиданно: "
    "конверт был помят, а марка наклеена вверх ногами.",
    "— Ты куда собрался? — спросила она, не поднимая глаз от книги.",
    "— В город, — ответил он. — Там меня ждут.",
    "В библиотеке пахло старой бумагой, клеем и немного пылью, которую никто не решался тревожить. "
    "Она читала медленно, возвращаясь к каждой странной фразе, словно к знакомой тропинке в лесу. "
    "Поезд опоздал на сорок минут, и на перроне успела собраться небольшая толпа недовольных.",
    "Никто не знал, откуда в деревне появился этот человек и почему он так хорошо говорит "
    "по-французски. Сад за домом давно зарос, но яблони всё ещё плодоносили, упрямо и щедро. "
    "Ветер переменился к вечеру, и над рекой поплыл тяжёлый запах скошенной травы.",
    "Разговор не клеился: каждый думал о своём и боялся сказать лишнее. Старик достал из кармана "
    "часы, щёлкнул крышкой и, не взглянув на циферблат, убрал их обратно. «Ну что ж…» — сказал он "
    "наконец и вышел, не попрощавшись.",
    "Дорога шла вдоль оврага, то поднимаясь на холмы, то ныряя в сырые берёзовые рощи. Он понимал, "
    "что опоздал, но всё равно шёл быстрее, почти бежал, задыхаясь от холодного воздуха. На "
    "подоконнике лежала раскрытая книга, страницы которой шевелил сквозняк.",
    "Так прошла неделя.",
]
CALIB_EPIGRAPH = ("И жить торопится, и чувствовать спешит.", "Князь Вяземский")


def gen_calibration_fb2():
    ep = f"<epigraph><p>{CALIB_EPIGRAPH[0]}</p><text-author>{CALIB_EPIGRAPH[1]}</text-author></epigraph>"
    return fb2(CALIB_TITLE, section(CALIB_HEAD, "".join(p(x) for x in CALIB_PARAS), ep))


# ---------------------------------------------------------------- EPUB

CONTAINER = """<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"""


def write_epub(path, files):
    with zipfile.ZipFile(path, "w") as z:
        z.writestr(zipfile.ZipInfo("mimetype"), "application/epub+zip", compress_type=zipfile.ZIP_STORED)
        z.writestr("META-INF/container.xml", CONTAINER, compress_type=zipfile.ZIP_DEFLATED)
        for name, data in files.items():
            z.writestr(name, data, compress_type=zipfile.ZIP_DEFLATED)


def xhtml3(title, body, lang="en", css="style.css"):
    return f"""<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" lang="{lang}" xml:lang="{lang}">
<head><title>{title}</title><link rel="stylesheet" type="text/css" href="{css}"/></head>
<body>
{body}
</body>
</html>"""


EPUB3_CSS = """@font-face { font-family: "Fancy"; src: url(fonts/missing.otf); }
body { font-family: "Times New Roman", serif; font-size: 12pt; line-height: 1.2; color: #333; margin: 0 5%; background: #fffff0; }
p { text-indent: 1.5em; margin: 0; }
p.noindent, h1 + p, hr + p { text-indent: 0; }
h1 { font-size: 2em; text-align: center; page-break-before: always; color: navy; margin: 3em 0 1em; }
h2 { font-size: 1.4em; font-family: Arial; margin-top: 2em; }
.center { text-align: center; text-indent: 0; }
.right { text-align: right; }
.smallcaps { font-variant: small-caps; }
.dropcap { float: left; font-size: 3.2em; line-height: 0.8; margin-right: 0.05em; }
.hidden { display: none; }
.poem { margin: 1em 2em; }
.poem p.line { text-indent: -1em; margin-left: 1em; }
.poem p.indent { margin-left: 2em; }
.poem .stanza { margin-bottom: 1em; }
blockquote { margin: 1em 2em; font-style: italic; }
.attribution { text-align: right; font-style: normal; }
figure { margin: 1em 0; text-align: center; }
figcaption { font-size: 0.85em; font-style: italic; }
.huge { font-size: 48px; }
table { border-collapse: collapse; margin: 1em auto; }
td, th { border: 1px solid #999; padding: 0.2em 0.5em; }
pre { font-family: monospace; font-size: 0.9em; white-space: pre; }
"""


def gen_epub3():
    rng = random.Random(6)
    ch1 = xhtml3("Chapter One", f"""<section epub:type="chapter" id="ch1">
<h1>Chapter One<br/><span class="smallcaps">The Letter</span></h1>
<p><span class="dropcap">T</span><span class="smallcaps">he morning was grey</span> and quiet, as if the town had not yet decided whether to wake up. {escape(para(rng, S_EN, 3, 5))}</p>
<p>"Well," he said, "I don't think it's possible -- not in the '90s, anyway."</p>
<p>"Why not?" she asked. 'Because,' he would have said, "the rules were different then."</p>
<p>She'd been there in 1914-1918 . . . and again in 1939. Rock 'n' roll, the '60s, ma'am, five o'clock, the Joneses' house.</p>
<p>A footnote here<a epub:type="noteref" href="#fn1" id="r1">1</a> and an endnote there<a epub:type="noteref" href="notes.xhtml#en1" id="r2"><sup>2</sup></a>. {escape(para(rng, S_EN, 2, 4))}</p>
<aside epub:type="footnote" id="fn1"><p>This is a footnote inside the chapter file. It must not appear in the text flow; it should show as a pop-up.</p></aside>
<p>Ligatures: office, affluent, fluffy, official, fjord, flight, ffl, ffi, Th, "fi" and "fl" in the middle of words.</p>
<p>Chemistry and physics: H<sub>2</sub>O, E = mc<sup>2</sup>, 10<sup>-6</sup>, x<sub>i</sub><sup>2</sup>.</p>
<p class="hidden">This paragraph is hidden by CSS and must not be shown.</p>
<p class="huge">A paragraph with an absolute 48px font size from the publisher.</p>
<blockquote><p>A quotation set as a block, with several lines of text, to check indentation on both sides and the italic style from publisher CSS.</p><p class="attribution">— Somebody Famous</p></blockquote>
<hr/>
<p>{escape(para(rng, S_EN, 4, 7))}</p>
<p class="center">* * *</p>
<p>{escape(para(rng, S_EN, 3, 6))}</p>
<ul><li>First item</li><li>Second item with a much longer text that wraps onto the next line on a phone screen<ul><li>Nested item</li></ul></li><li>Third item</li></ul>
<ol><li>Numbered one</li><li>Numbered two</li></ol>
<table><tr><th>Year</th><th>Event</th></tr><tr><td>1914</td><td>War begins</td></tr><tr><td>1918</td><td>Armistice</td></tr></table>
<pre>fn main() {{
    println!("preformatted, must not be justified");
}}</pre>
<p>A long URL: https://www.example.com/a/very/long/path/that/cannot/be/hyphenated/normally/index.html?query=string&amp;more=params</p>
<p lang="ru" xml:lang="ru">Абзац на русском языке внутри английской книги: здесь должны работать русские переносы и русская типографика, а "кавычки" -- стать ёлочками.</p>
</section>""")

    jabber = [
        ["'Twas brillig, and the slithy toves", "Did gyre and gimble in the wabe:",
         "All mimsy were the borogoves,", "And the mome raths outgrabe."],
        ["\"Beware the Jabberwock, my son!", "The jaws that bite, the claws that catch!",
         "Beware the Jubjub bird, and shun", "The frumious Bandersnatch!\""],
    ]
    poem = "".join(
        '<div class="stanza">' + "".join(
            f'<p class="{"line indent" if i % 2 else "line"}">{escape(l)}</p>' for i, l in enumerate(st)) + "</div>"
        for st in jabber)
    ch2 = xhtml3("Chapter Two", f"""<section epub:type="chapter" id="ch2">
<h1>Chapter Two</h1>
<div class="epigraph right"><p class="noindent"><i>Shall I compare thee to a summer's day?<br/>Thou art more lovely and more temperate.</i></p><p class="right">— W. Shakespeare</p></div>
<h2 id="poem">Jabberwocky</h2>
<div class="poem">{poem}</div>
<p>{escape(para(rng, S_EN, 4, 6))}</p>
<figure><img src="images/wide.jpg" alt="Wide picture"/><figcaption>Figure 1. A wide image with a caption that must stay together with the picture.</figcaption></figure>
<p>{escape(para(rng, S_EN, 2, 3))}</p>
<div><img src="images/tall.png" alt="Tall picture"/></div>
<p>Line art on a transparent background: <img src="images/lineart_alpha.png" alt="line art" style="width:100%"/></p>
<p>An inline icon <img src="images/inline.png" alt="icon"/> in the middle of a line.</p>
<h2 id="long">Long words</h2>
<p>{escape(S_EN[-1])} {escape(para(rng, S_EN, 3, 5))}</p>
<p>Back to the second endnote<a epub:type="noteref" href="notes.xhtml#en2" id="r3">3</a>.</p>
</section>""")

    notes = xhtml3("Notes", """<section epub:type="endnotes"><h1>Notes</h1><ol>
<li id="en1" epub:type="endnote"><p>An endnote in a separate file. <a href="chapter1.xhtml#r2">↩</a></p></li>
<li id="en2" epub:type="endnote"><p>A longer endnote with two paragraphs. The first one explains something at length so that the pop-up has to scroll or be truncated with a “Go to note” button.</p><p>The second paragraph. <a href="chapter2.xhtml#r3">↩</a></p></li>
</ol></section>""")

    cover = """<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Cover</title>
<style type="text/css">body{margin:0;padding:0} svg{height:100%;width:100%}</style></head>
<body><div><svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" version="1.1" width="100%" height="100%" viewBox="0 0 600 900" preserveAspectRatio="xMidYMid meet">
<image width="600" height="900" xlink:href="images/cover.jpg"/></svg></div></body></html>"""

    nav = """<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Contents</title></head>
<body><nav epub:type="toc" id="toc"><h1>Contents</h1><ol>
<li><a href="chapter1.xhtml">Chapter One: The Letter</a></li>
<li><a href="chapter2.xhtml">Chapter Two</a><ol>
<li><a href="chapter2.xhtml#poem">Jabberwocky</a></li><li><a href="chapter2.xhtml#long">Long words</a></li></ol></li>
<li><a href="notes.xhtml">Notes</a></li></ol></nav>
<nav epub:type="landmarks" hidden=""><ol><li><a epub:type="bodymatter" href="chapter1.xhtml">Start</a></li></ol></nav>
</body></html>"""

    opf = """<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid" xml:lang="en">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
<dc:identifier id="uid">urn:uuid:bookworm-corpus-06</dc:identifier>
<dc:title>06 English EPUB3</dc:title><dc:creator>Test Corpus</dc:creator><dc:language>en</dc:language>
<meta property="dcterms:modified">2026-10-25T00:00:00Z</meta>
<meta property="belongs-to-collection" id="c1">Test Corpus</meta><meta refines="#c1" property="group-position">6</meta>
</metadata>
<manifest>
<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
<item id="css" href="style.css" media-type="text/css"/>
<item id="cover" href="cover.xhtml" media-type="application/xhtml+xml" properties="svg"/>
<item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
<item id="ch2" href="chapter2.xhtml" media-type="application/xhtml+xml"/>
<item id="notes" href="notes.xhtml" media-type="application/xhtml+xml"/>
<item id="img-cover" href="images/cover.jpg" media-type="image/jpeg" properties="cover-image"/>
<item id="img-wide" href="images/wide.jpg" media-type="image/jpeg"/>
<item id="img-tall" href="images/tall.png" media-type="image/png"/>
<item id="img-line" href="images/lineart_alpha.png" media-type="image/png"/>
<item id="img-inline" href="images/inline.png" media-type="image/png"/>
</manifest>
<spine><itemref idref="cover" linear="no"/><itemref idref="ch1"/><itemref idref="ch2"/><itemref idref="notes" linear="no"/></spine>
</package>"""

    write_epub(OUT / "06_english_epub3.epub", {
        "OEBPS/content.opf": opf, "OEBPS/nav.xhtml": nav, "OEBPS/style.css": EPUB3_CSS,
        "OEBPS/cover.xhtml": cover, "OEBPS/chapter1.xhtml": ch1, "OEBPS/chapter2.xhtml": ch2,
        "OEBPS/notes.xhtml": notes,
        **{f"OEBPS/images/{n}": img_bytes(n) for n in
           ["cover.jpg", "wide.jpg", "tall.png", "lineart_alpha.png", "inline.png"]},
    })


EPUB2_CSS = """.calibre { display: block; font-size: 1em; padding: 0; margin: 0 5pt; }
.calibre1 { display: block; text-indent: 2em; margin: 0; text-align: justify; }
.calibre2 { font-style: italic; }
.calibre3 { font-weight: bold; }
.title { display: block; font-size: 1.6em; font-weight: bold; text-align: center; margin: 2em 0 1em; }
.subtitle { display: block; font-size: 1.1em; font-weight: bold; text-align: center; margin: 1em 0; }
.epigraph { display: block; margin-left: 40%; font-size: 0.9em; font-style: italic; }
.poem { display: block; margin-left: 15%; }
.note { font-size: 0.7em; vertical-align: super; }
"""


def gen_epub2():
    rng = random.Random(7)
    doctype = ('<?xml version="1.0" encoding="utf-8"?>\n<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" '
               '"http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">\n')

    def page(body, css=True):
        link = '<link href="stylesheet.css" rel="stylesheet" type="text/css"/>' if css else ""
        return (doctype + '<html xmlns="http://www.w3.org/1999/xhtml"><head><meta http-equiv="Content-Type" '
                f'content="text/html; charset=utf-8"/><title>Книга</title>{link}</head><body class="calibre">{body}</body></html>')

    text1 = page(f"""
<p class="title" id="ch1">Глава 1</p>
<p class="epigraph">Эпиграф без тегов эпиграфа,&nbsp;только классом.<br/>— Автор</p>
<p class="calibre1">{escape(para(rng, S_RU, 3, 5))}</p>
<p class="calibre1"><span class="calibre2">Курсив задан классом CSS, а не тегом.</span> <span class="calibre3">Жирный тоже классом.</span> Сущности: &mdash; тире, &hellip; многоточие, &laquo;ёлочки&raquo;, &nbsp;неразрывный.</p>
<p class="calibre1">Сноска в стиле EPUB2<a href="notes.html#n1" id="back1"><span class="note">1</span></a>, без epub:type, просто ссылка на другой файл.</p>
<div class="calibre1">Текст прямо в div без абзаца. {escape(para(rng, S_RU, 2, 3))}</div>
<p class="poem">Белеет парус одинокой<br/>В тумане моря голубом!..<br/>Что ищет он в стране далекой?<br/>Что кинул он в краю родном?..</p>
<p class="calibre1">{escape(para(rng, S_RU, 4, 6))}</p>
<p class="title" id="ch2">Глава 2</p>
<p class="subtitle">Вторая глава в том же файле, оглавление ведёт на якорь</p>
<p class="calibre1">{escape(para(rng, S_RU, 3, 5))}</p>
<p class="calibre1">   </p>
<p class="calibre1">{escape(para(rng, S_RU, 3, 5))}</p>""")

    text2 = page(f"""
<h2 class="calibre">Глава 3 (настоящий тег h2)</h2>
<p>{escape(para(rng, S_RU, 3, 5))}</p>
Текст прямо в body без обёртки, такое встречается в книгах из конвертеров.
<p>{escape(para(rng, S_RU, 2, 4))}</p>
<p><img src="images/wide.jpg" alt=""/></p>""")

    notes = page('<p class="title">Примечания</p>'
                 '<p id="n1"><a href="text1.html#back1">1</a> Текст сноски EPUB2. '
                 'Читалка должна распознать её как сноску по короткому тексту и ссылке назад.</p>')

    ncx = """<?xml version="1.0" encoding="utf-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1" xml:lang="ru">
<head><meta name="dtb:uid" content="bookworm-corpus-07"/><meta name="dtb:depth" content="2"/></head>
<docTitle><text>07 EPUB2 Calibre</text></docTitle>
<navMap>
<navPoint id="np1" playOrder="1"><navLabel><text>Глава 1</text></navLabel><content src="text1.html#ch1"/></navPoint>
<navPoint id="np2" playOrder="2"><navLabel><text>Глава 2</text></navLabel><content src="text1.html#ch2"/>
 <navPoint id="np2a" playOrder="3"><navLabel><text>Вложенный пункт</text></navLabel><content src="text1.html#ch2"/></navPoint></navPoint>
<navPoint id="np3" playOrder="4"><navLabel><text>Глава 3</text></navLabel><content src="text2.html"/></navPoint>
</navMap></ncx>"""

    opf = """<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="uuid_id">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf" xmlns:calibre="http://calibre.kovidgoyal.net/2009/metadata">
<dc:title>07 EPUB2 Calibre</dc:title><dc:creator opf:role="aut" opf:file-as="Корпусов, Тест">Тест Корпусов</dc:creator>
<dc:language>ru</dc:language><dc:identifier id="uuid_id" opf:scheme="uuid">bookworm-corpus-07</dc:identifier>
<meta name="cover" content="cover"/><meta name="calibre:series" content="Тестовый корпус"/><meta name="calibre:series_index" content="7"/>
</metadata>
<manifest>
<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
<item id="css" href="stylesheet.css" media-type="text/css"/>
<item id="t1" href="text1.html" media-type="application/xhtml+xml"/>
<item id="t2" href="text2.html" media-type="application/xhtml+xml"/>
<item id="nt" href="notes.html" media-type="application/xhtml+xml"/>
<item id="cover" href="images/cover.jpg" media-type="image/jpeg"/>
<item id="wide" href="images/wide.jpg" media-type="image/jpeg"/>
</manifest>
<spine toc="ncx"><itemref idref="t1"/><itemref idref="t2"/><itemref idref="nt"/></spine>
<guide><reference type="cover" title="Cover" href="images/cover.jpg"/></guide>
</package>"""

    write_epub(OUT / "07_epub2_calibre.epub", {
        "OEBPS/content.opf": opf, "OEBPS/toc.ncx": ncx, "OEBPS/stylesheet.css": EPUB2_CSS,
        "OEBPS/text1.html": text1, "OEBPS/text2.html": text2, "OEBPS/notes.html": notes,
        "OEBPS/images/cover.jpg": img_bytes("cover.jpg"), "OEBPS/images/wide.jpg": img_bytes("wide.jpg"),
    })


def gen_styled_subtitles():
    body = (ROOT / "tests/fixtures/epub-styled-subtitles.xhtml").read_text()
    write_epub(OUT / "11_styled_subtitles.epub", {
        "OEBPS/content.opf": """<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">styled-subtitles</dc:identifier>
<dc:title>Подзаголовки обычного кегля</dc:title><dc:language>ru</dc:language></metadata>
<manifest><item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/></manifest>
<spine><itemref idref="chapter"/></spine></package>""",
        "OEBPS/chapter.xhtml": body,
    })


def gen_calibration_epub():
    body = (f'<section epub:type="chapter"><h1>{CALIB_HEAD}</h1>'
            f'<blockquote class="epigraph"><p>{CALIB_EPIGRAPH[0]}</p><p class="author">{CALIB_EPIGRAPH[1]}</p></blockquote>'
            + "".join(f"<p>{escape(x)}</p>" for x in CALIB_PARAS) + "</section>")
    css = ("p { text-indent: 1.5em; margin: 0; } h1 { text-align: center; } "
           ".epigraph { margin-left: 40%; font-style: italic; } .author { text-align: right; }")
    opf = """<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="uid">urn:uuid:bookworm-calibration</dc:identifier>
<dc:title>10 Калибровка</dc:title><dc:creator>Тест Корпусов</dc:creator><dc:language>ru</dc:language>
<meta property="dcterms:modified">2026-10-25T00:00:00Z</meta></metadata>
<manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
<item id="css" href="style.css" media-type="text/css"/><item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/></manifest>
<spine><itemref idref="c1"/></spine></package>"""
    nav = xhtml3("Оглавление", f'<nav epub:type="toc"><ol><li><a href="ch1.xhtml">{CALIB_HEAD}</a></li></ol></nav>', "ru")
    write_epub(OUT / "10_calibration.epub", {
        "OEBPS/content.opf": opf, "OEBPS/nav.xhtml": nav, "OEBPS/style.css": css,
        "OEBPS/ch1.xhtml": xhtml3(CALIB_TITLE, body, "ru"),
    })


# ---------------------------------------------------------------- main

def main():
    if OUT.exists():
        shutil.rmtree(OUT)
    OUT.mkdir(parents=True)
    make_images()

    def save(name, data):
        (OUT / name).write_bytes(data if isinstance(data, bytes) else data.encode("utf-8"))

    save("01_typography_ru.fb2", gen_typography())
    structure = gen_structure()
    save("02_structure_ru.fb2", structure)
    save("03_cp1251.fb2", gen_cp1251())
    with zipfile.ZipFile(OUT / "04_zipped.fb2.zip", "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("Корпусов_Структура.fb2", structure)
    save("05_pagination_ru.fb2", gen_pagination())
    gen_epub3()
    gen_epub2()
    save("08_big_ru.fb2", gen_big())
    save("09_broken.fb2", gen_broken())
    save("10_calibration.fb2", gen_calibration_fb2())
    gen_calibration_epub()
    gen_styled_subtitles()
    shutil.rmtree(IMG)

    for f in sorted(OUT.iterdir()):
        print(f"{f.stat().st_size:>10,}  {f.name}")


if __name__ == "__main__":
    main()
