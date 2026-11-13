# Обрабатывает скриншоты и собирает «КАК СДЕЛАТЬ ТЕМУ.html» + guide_files.rs.
import html, json, os, shutil
from PIL import Image, ImageDraw, ImageFont

SP = os.path.join(__import__('tempfile').gettempdir(), 'vgd-theme-guide')
REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..'))
ASSETS = os.path.join(REPO, 'assets', 'theme-guide')
IMGDIR = os.path.join(ASSETS, 'инструкция')
RAW = os.path.join(SP, 'raw')
__file_b = __file__
exec(open(os.path.join(os.path.dirname(__file__), 'guide_a.py'), encoding='utf-8').read().split('def bg_image')[0])
__file__ = __file_b  # RAINBOW и др.

shutil.rmtree(ASSETS, ignore_errors=True)
os.makedirs(IMGDIR)
FONT = ImageFont.truetype(r'C:\Windows\Fonts\segoeuib.ttf', 15)


def save(im, name):
    p = os.path.join(IMGDIR, name + '.png')
    im = im.quantize(colors=256, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.FLOYDSTEINBERG)
    im.save(p, optimize=True)
    return 'инструкция/' + name + '.png'


def small(sid, w=640):
    im = Image.open(os.path.join(RAW, sid + '.png')).convert('RGB')
    im = im.resize((w, round(im.height * w / im.width)), Image.LANCZOS)
    return save(im, sid)


# ---------- карта цветов: номера на «радужных» скриншотах ----------
# (номер, поле, точка на скриншоте, сдвиг значка)
MARKS = {
    'rainbow-look': [
        (1, 'bg', (520, 120), (0, 0)), (2, 'sidebar', (38, 420), (0, 0)), (3, 'sidebar_line', (75, 560), (26, 0)),
        (4, 'surface', (440, 432), (0, 0)), (5, 'surface_hi', (38, 200), (34, -18)), (6, 'pill', (449, 500), (0, -26)),
        (7, 'border', (346, 494), (0, -24)), (8, 'divider', (400, 397), (0, -22)), (9, 'field', (612, 591), (0, -30)),
        (10, 'text', (260, 28), (36, 0)), (11, 'muted', (343, 72), (34, 0)), (12, 'faint', (323, 535), (36, 4)),
        (13, 'accent', (240, 500), (0, -26)), (14, 'on_accent', (201, 511), (-50, -26)), (15, 'toggle_off', (571, 510), (8, -28)),
        (16, 'knob_off', (553, 510), (-10, 28)), (17, 'knob_on', (526, 510), (-14, 28)), (18, 'ok', (609, 510), (0, -26)),
        (19, 'err', (659, 510), (0, 26)), (20, 'warn', (716, 510), (0, -26)),
    ],
    'rainbow-download': [
        (21, 'outline', (494, 357), (0, 22)), (7, 'border', (820, 86), (22, 0)), (4, 'surface', (720, 84), (0, 0)),
        (13, 'accent', (808, 150), (0, 26)), (5, 'surface_hi', (38, 85), (34, -18)),
    ],
}


def annotate(sid):
    im = Image.open(os.path.join(RAW, sid + '.png')).convert('RGB')
    d = ImageDraw.Draw(im)
    for n, _, (x, y), (dx, dy) in MARKS[sid]:
        bx, by = x + dx, y + dy
        if dx or dy:
            d.line([(x, y), (bx, by)], fill=(255, 255, 255), width=3)
            d.line([(x, y), (bx, by)], fill=(0, 0, 0), width=1)
            d.ellipse([x - 3, y - 3, x + 3, y + 3], fill=(255, 255, 255), outline=(0, 0, 0))
        r = 12
        d.ellipse([bx - r, by - r, bx + r, by + r], fill=(255, 255, 255), outline=(0, 0, 0), width=2)
        d.text((bx, by), str(n), font=FONT, fill=(0, 0, 0), anchor='mm')
    return save(im, sid)


COLORS = [  # поле, где видно, номер на картинке
    ('bg', 'фон окна', 1), ('sidebar', 'панель навигации', 2), ('sidebar_line', 'край панели навигации (линия)', 3),
    ('surface', 'карточки, поле для ссылок, обычные кнопки', 4), ('surface_hi', 'наведение и выбранный пункт навигации', 5),
    ('pill', 'маленькие кнопки («маленькая», «сохранить»); у кнопок «tonal» — обычные кнопки и индикатор навигации', 6),
    ('border', 'рамки кнопок, выпадающих списков, поля для ссылок', 7), ('divider', 'линии между строками в карточках', 8),
    ('outline', 'рамка пустой очереди на странице «скачать»', 21), ('window', 'всплывающие окна (выпадающий список, подтверждения)', None),
    ('field', 'поля ввода (название темы, ключи)', 9), ('selection', 'выделение текста мышью', None),
    ('text', 'основной текст', 10), ('text_hover', 'текст при наведении', None), ('muted', 'подписи разделов, второстепенный текст', 11),
    ('faint', 'подсказки, бледный текст, номер версии', 12), ('accent', 'главная кнопка, выбранный вариант, включённый тумблер, прогресс', 13),
    ('accent_hover', 'главная кнопка при наведении', None), ('on_accent', 'текст и значок на акценте', 14),
    ('toggle_off', 'выключенный тумблер', 15), ('knob_off', 'кружок выключенного тумблера', 16), ('knob_on', 'кружок включённого тумблера', 17),
    ('track', 'незаполненная часть полосы прогресса и ползунка', None), ('ok', '«готово», «подключено»', 18),
    ('ok_bg', 'фон зелёных плашек (подсказка про расширение)', None), ('err', 'ошибки', 19), ('warn', '«внимание», ожидание', 20),
    ('warn_bg', 'фон жёлтых плашек (предупреждение про spotify)', None),
]

# ---------- параметры раскладки ----------
P = []  # (заголовок, json, значения, описание, [(подпись, картинка)])


def param(title, js, values, desc, pics):
    P.append((title, js, values, desc, pics))


param('Скругление углов', '"radius": 1.0', '0 … 3 (дробные можно: 1.5)',
      'Во сколько раз скруглять углы карточек, кнопок и окон. 1 — как в тёмной, 0 — квадратные, 2.5 — почти круглые. '
      'Внутренние отступы растут вместе со скруглением, чтобы текст не прилипал к углам. Меняется и в программе ползунком.',
      [('до (1.0)', 'base-look'), ('0', 'radius-0'), ('2.5', 'radius-25')])
param('Шрифт', '"font": "mono"', '"mono" — моноширинный (Cascadia Mono / Consolas), "sans" — Segoe UI, "roboto" — Roboto (если не установлен — Segoe UI)',
      'Шрифт всего окна. Меняется и в программе переключателем.', [('до (mono)', 'base-look'), ('sans', 'font-sans'), ('roboto', 'font-roboto')])
param('Значки', '"icons": "line"', '"line" — тонкие значки Windows, "symbols" — обычные знаки ↓ ★ ⚙ ☰',
      'Набор значков в навигации и на кнопках.', [('до (line)', 'base-download'), ('symbols', 'icons-symbols')])
param('Навигация: где', '"layout": { "nav": "left" }', '"left" — слева, "top" — полосой сверху, "bottom" — полосой снизу',
      'Где стоят «скачать / подписки / настройки / журнал».', [('до (left)', 'base-download'), ('top', 'nav-top'), ('bottom', 'nav-bottom')])
param('Навигация: подписи', '"layout": { "nav_labels": true }', 'true / false',
      'false — только значки (название видно при наведении).', [('до (true)', 'base-download'), ('false', 'nav-labels-off')])
param('Навигация: ширина', '"layout": { "nav_width": 76 }', '60 … 260 (только для "left")',
      'Ширина левой панели. От 140 — значок и подпись в одну строку, как в боковых меню.', [('до (76)', 'base-download'), ('200 + "header": "name"', 'nav-wide')])
param('Шапка', '"layout": { "header": "logo" }', '"logo" — значок программы, "name" — значок и название, "none" — ничего',
      'Что стоит в начале навигации. Название видно в широкой левой панели и в полосе сверху/снизу.', [('до (logo)', 'base-download'), ('none', 'header-none'), ('name (широкая панель)', 'nav-wide')])
param('Выбранный пункт навигации', '"layout": { "nav_indicator": "block" }', '"block" — подсвечен весь пункт, "pill" — «таблетка» вокруг значка (как Navigation Rail в Material 3)',
      'Как выделяется открытая страница. Цвет «таблетки» — pill.', [('до (block)', 'base-download'), ('pill', 'nav-indicator-pill')])
param('Ширина колонки', '"layout": { "column": 640 }', '420 … 1200',
      'Наибольшая ширина колонки с содержимым по центру окна. Заметно, только если окно шире.', [])
param('Карточки', '"layout": { "cards": "fill" }', '"fill" — залиты цветом surface, "outline" — полупрозрачные с рамкой border, "none" — без фона',
      'Вид карточек настроек и поля для ссылок.', [('до (fill)', 'base-look'), ('outline', 'cards-outline'), ('none', 'cards-none')])
param('Тень и свечение', '"layout": { "shadow": "none" }', '"none", "shadow" — мягкая тень, "glow" — свечение цвета accent',
      'Под карточками. Тень лучше видна на светлых темах, свечение — на тёмных.', [('светлая без тени', 'shadow-light-base'), ('shadow', 'shadow-shadow'), ('glow (+ outline)', 'shadow-glow')])
param('Плотность', '"layout": { "density": 1.0 }', '0.7 … 1.6',
      'Высота строк и кнопок: 0.8 — компактно, 1.3 — просторно.', [('до (1.0)', 'base-look'), ('0.8', 'density-08'), ('1.4', 'density-14')])
param('Отступы', '"layout": { "padding": 1.0 }', '0.5 … 2.5', 'Внутренние поля карточек и кнопок.', [('до (1.0)', 'base-look'), ('1.8', 'padding-18')])
param('Кнопки', '"layout": { "buttons": "pill" }',
      '"pill" — скруглённые, "rect" — прямоугольные, "outline" — контур, "flat" — без рамок, "tonal" — как в Material 3 (обычные — цветом pill, маленькие — контуром)',
      'Вид всех кнопок и переключателя вариантов.',
      [('до (pill)', 'base-look'), ('rect', 'buttons-rect'), ('outline', 'buttons-outline'), ('flat', 'buttons-flat'), ('tonal', 'buttons-tonal')])
param('Скругление кнопок', '"layout": { "button_radius": -1 }', '-1 — по radius, 0 … 100 — в точках (100 — «таблетка»)',
      'Отдельно от radius: можно оставить карточки почти квадратными, а кнопки — круглыми.', [('до (-1)', 'base-look'), ('100', 'button-radius-100')])
param('Скругление поля для ссылок', '"layout": { "field_radius": -1 }', '-1 — по radius, 0 … 100 — в точках', 'Только поле «вставь ссылку».',
      [('до (-1)', 'base-download'), ('0', 'field-radius-0')])
param('Тумблеры', '"layout": { "switch": "classic" }', '"classic" — как в тёмной, "m3" — крупнее, с рамкой в выключенном виде (Material 3)', '',
      [('до (classic)', 'base-look'), ('m3', 'switch-m3')])
param('Фон-градиент', '"layout": { "background": ["#0d1b4d", "#2b0d4d", "#000000"] }', '[] — сплошной цвет bg; 2 или 3 цвета — градиент сверху вниз',
      'Рисуется под содержимым (панель навигации остаётся цветом sidebar).', [('до', 'base-download'), ('3 цвета', 'bg-gradient')])
param('Картинка фона', '"layout": { "background_image": "фон.png", "image_opacity": 0.35 }',
      'имя PNG-файла в папке тем; непрозрачность 0 … 1',
      'Картинка растягивается на всё окно с сохранением пропорций (лишнее обрезается) и рисуется поверх градиента. Только PNG. '
      'Для пробы скопируйте <code>инструкция\\фон-пример.png</code> в папку тем и укажите <code>"background_image": "фон-пример.png"</code>.',
      [('до', 'base-download'), ('картинка, 0.35', 'bg-image')])
param('Размер шрифта', '"layout": { "font_scale": 1.0 }', '0.8 … 1.4', 'Весь текст и значки. Для масштаба всего окна есть «масштаб» в настройках → интерфейс.',
      [('до (1.0)', 'base-look'), ('1.25', 'font-scale-125')])
param('Жирные заголовки', '"layout": { "bold_headings": false }', 'true / false', 'Заголовки страниц и разделов настроек.', [('до', 'base-look'), ('true', 'bold')])
param('Подписи разделов', '"layout": { "caps": "lower" }', '"lower" — как есть, "upper" — ПРОПИСНЫМИ', '', [('до', 'base-look'), ('upper', 'caps-upper')])

# ---------- сборка ----------
used = set()


def img(sid, cap, annotate_it=False):
    if sid not in used:
        used.add(sid)
        IMG[sid] = annotate(sid) if annotate_it else small(sid)
    return f'<figure><img src="{IMG[sid]}" alt="{html.escape(cap)}" loading="lazy"><figcaption>{html.escape(cap)}</figcaption></figure>'


IMG = {}
Image.open(os.path.join(SP, 'фон-пример.png')).convert('RGB').resize((960, 720), Image.LANCZOS).save(os.path.join(IMGDIR, 'фон-пример.png'), optimize=True)
example = open(os.path.join(REPO, 'src', 'theme_example.json'), encoding='utf-8').read()

h = []
w = h.append
rainbow_look = img('rainbow-look', 'каждому цвету дан свой оттенок — номера ниже в таблице', True)
rainbow_dl = img('rainbow-download', 'страница «скачать» в той же «радужной» теме', True)
rows = ''.join(
    f'<tr><td>{n if n else "—"}</td><td><span class="sw" style="background:{RAINBOW.get(k, "transparent")}"></span></td>'
    f'<td><code>{k}</code></td><td>{html.escape(d)}</td></tr>' for k, d, n in COLORS)

params_html = ''
for title, js, values, desc, pics in P:
    figs = ''.join(img(s, c) for c, s in pics)
    params_html += (f'<section class="param"><h3>{html.escape(title)}</h3><pre><code>{html.escape(js)}</code></pre>'
                    f'<p><b>значения:</b> {html.escape(values)}</p>' + (f'<p>{desc}</p>' if desc else '') +
                    (f'<div class="figs">{figs}</div>' if figs else '') + '</section>')

A2 = '"layout": {\n    "nav": "top", "header": "name",\n    "background": ["#0b0820", "#1a0b2e", "#061428"]\n  }'
A3 = ('"layout": {\n    "nav": "top", "header": "name",\n    "background": ["#0b0820", "#1a0b2e", "#061428"],\n'
      '    "cards": "outline", "shadow": "glow", "buttons": "outline",\n    "caps": "upper", "bold_headings": true\n  }')
B2 = '"radius": 2.0,\n  "layout": { "nav": "bottom", "nav_labels": false, "header": "none" }'
B3 = ('"radius": 2.0, "bg": "#fdf3f6",\n  "layout": {\n    "nav": "bottom", "nav_labels": false, "header": "none",\n'
      '    "shadow": "shadow", "buttons": "flat", "density": 1.15,\n    "background": ["#fff0f5", "#f0f4ff"]\n  }')

TEMPLATE = open(os.path.join(os.path.dirname(__file__), 'guide_template.html'), encoding='utf-8').read()
out = (TEMPLATE
       .replace('{{RAINBOW_LOOK}}', rainbow_look).replace('{{RAINBOW_DL}}', rainbow_dl).replace('{{COLOR_ROWS}}', rows)
       .replace('{{PARAMS}}', params_html).replace('{{EXAMPLE}}', html.escape(example))
       .replace('{{A1}}', img('a1', 'шаг 1: только цвета')).replace('{{A2}}', img('a2', 'шаг 2: навигация сверху и градиент'))
       .replace('{{A3}}', img('a3', 'шаг 3: рамки со свечением, контурные кнопки')).replace('{{A3L}}', img('a3-look', 'итог: настройки'))
       .replace('{{A2CODE}}', html.escape(A2)).replace('{{A3CODE}}', html.escape(A3))
       .replace('{{B1}}', img('b1', 'шаг 1: светлые цвета, segoe ui, розовый акцент')).replace('{{B2}}', img('b2', 'шаг 2: навигация снизу, круглые углы'))
       .replace('{{B3}}', img('b3', 'шаг 3: тени, плоские кнопки, градиент')).replace('{{B2CODE}}', html.escape(B2)).replace('{{B3CODE}}', html.escape(B3))
       .replace('{{C1}}', img('c-dark', '"dark": true')).replace('{{C2}}', img('c-light', '"dark": false'))
       .replace('{{ERR}}', img('error', 'файл с опечаткой: тема не появилась в списке, под ним — ошибка')))
assert '{{' not in out, out[out.index('{{'):out.index('{{') + 40]
open(os.path.join(ASSETS, 'КАК СДЕЛАТЬ ТЕМУ.html'), 'w', encoding='utf-8', newline='\n').write(out)

# список файлов для программы
files = ['    (HTML, include_bytes!("../../../assets/theme-guide/КАК СДЕЛАТЬ ТЕМУ.html")),']
for f in sorted(os.listdir(IMGDIR)):
    files.append(f'    ("инструкция/{f}", include_bytes!("../../../assets/theme-guide/инструкция/{f}")),')
open(os.path.join(REPO, 'desktop', 'src', 'theme', 'guide_files.rs'), 'w', encoding='utf-8', newline='\n').write(
    '// Собрано скриптом вместе со скриншотами: файлы инструкции по темам (путь в папке тем, содержимое).\n'
    'pub const FILES: &[(&str, &[u8])] = &[\n' + '\n'.join(files) + '\n];\n')
total = sum(os.path.getsize(os.path.join(IMGDIR, f)) for f in os.listdir(IMGDIR))
print(len(os.listdir(IMGDIR)), 'картинок,', total // 1024, 'КБ')
