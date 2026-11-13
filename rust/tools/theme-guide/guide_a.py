# Готовит папки APPDATA для каждого скриншота инструкции.
import json, os, re, shutil, sys
from PIL import Image, ImageDraw, ImageFilter

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..'))
OUT = os.path.join(__import__('tempfile').gettempdir(), 'vgd-theme-guide')
APP = os.path.join(OUT, 'appdata')
src = open(os.path.join(REPO, 'src', 'theme.rs'), encoding='utf-8').read()


def colors(fn):
    b = src[src.index(f'pub fn {fn}()'):]
    b = b[:b.index('layout:')]
    return {k: '#' + v[2:] for k, v in re.findall(r'(\w+): Rgb::hex\((0x[0-9a-f]{6})\)', b)}


LIGHT = colors('light')
RAINBOW = {
    'bg': '#0d1b4d', 'sidebar': '#4d0d2b', 'sidebar_line': '#ff00ff', 'surface': '#0d4d2b', 'surface_hi': '#5a5a10',
    'pill': '#6b3a00', 'border': '#00e5ff', 'divider': '#ff8800', 'outline': '#ffff00', 'field': '#3a0d0d',
    'text': '#ffffff', 'muted': '#9ad0ff', 'faint': '#ff9ad0', 'accent': '#00ff66', 'accent_hover': '#66ff99',
    'on_accent': '#000000', 'toggle_off': '#8800ff', 'knob_off': '#ffcc00', 'knob_on': '#ff0000',
    'ok': '#66ff66', 'err': '#ff4444', 'warn': '#ffd000',
}
NEON = {'bg': '#0b0820', 'sidebar': '#0e0a26', 'sidebar_line': '#2c1f5c', 'surface': '#140f33', 'surface_hi': '#231a52',
        'pill': '#120d2c', 'border': '#4b2f8f', 'divider': '#261c55', 'outline': '#3a2a78', 'field': '#08061a',
        'text': '#eae6ff', 'muted': '#a79fd6', 'faint': '#6e66a3', 'accent': '#00e0ff', 'accent_hover': '#7af0ff',
        'on_accent': '#001418', 'toggle_off': '#2c2160', 'knob_off': '#a79fd6', 'knob_on': '#001418'}

L = lambda **k: {'layout': k}
# id: (тема, страница[, доп. файлы])
SHOTS = {
    'base-download': ({}, 'download'),
    'base-look': ({}, 'look'),
    'rainbow-download': (RAINBOW, 'download'),
    'rainbow-look': (RAINBOW, 'look'),
    'radius-0': ({'radius': 0}, 'look'),
    'radius-25': ({'radius': 2.5}, 'look'),
    'font-sans': ({'font': 'sans'}, 'look'),
    'font-roboto': ({'font': 'roboto'}, 'look'),
    'icons-symbols': ({'icons': 'symbols'}, 'download'),
    'nav-top': (L(nav='top'), 'download'),
    'nav-bottom': (L(nav='bottom'), 'download'),
    'nav-labels-off': (L(nav_labels=False), 'download'),
    'nav-wide': (L(nav_width=200, header='name'), 'download'),
    'header-none': (L(header='none'), 'download'),
    'nav-indicator-pill': (L(nav_indicator='pill'), 'download'),
    'cards-outline': (L(cards='outline'), 'look'),
    'cards-none': (L(cards='none'), 'look'),
    'shadow-glow': ({**{'accent': '#00e0ff'}, **L(shadow='glow', cards='outline')}, 'look'),
    'shadow-light-base': ({**LIGHT}, 'look'),
    'shadow-shadow': ({**LIGHT, **L(shadow='shadow')}, 'look'),
    'density-08': (L(density=0.8), 'look'),
    'density-14': (L(density=1.4), 'look'),
    'padding-18': (L(padding=1.8), 'look'),
    'buttons-rect': (L(buttons='rect'), 'look'),
    'buttons-outline': (L(buttons='outline'), 'look'),
    'buttons-flat': (L(buttons='flat'), 'look'),
    'buttons-tonal': (L(buttons='tonal'), 'look'),
    'button-radius-100': (L(button_radius=100), 'look'),
    'field-radius-0': (L(field_radius=0), 'download'),
    'switch-m3': (L(switch='m3'), 'look'),
    'bg-gradient': (L(background=['#0d1b4d', '#2b0d4d', '#000000']), 'download'),
    'bg-image': (L(background_image='фон-пример.png', image_opacity=0.35), 'download'),
    'font-scale-125': (L(font_scale=1.25), 'look'),
    'bold': (L(bold_headings=True), 'look'),
    'caps-upper': (L(caps='upper'), 'look'),
    # рецепт А: неон
    'a1': (NEON, 'download'),
    'a2': ({**NEON, **L(nav='top', header='name', background=['#0b0820', '#1a0b2e', '#061428'])}, 'download'),
    'a3': ({**NEON, **L(nav='top', header='name', background=['#0b0820', '#1a0b2e', '#061428'], cards='outline',
                       shadow='glow', buttons='outline', caps='upper', bold_headings=True)}, 'download'),
    'a3-look': ({**NEON, **L(nav='top', header='name', background=['#0b0820', '#1a0b2e', '#061428'], cards='outline',
                            shadow='glow', buttons='outline', caps='upper', bold_headings=True)}, 'look'),
    # рецепт Б: мягкая светлая
    'b1': ({**LIGHT, 'font': 'sans', 'accent': '#e0457b', 'accent_hover': '#c7366a'}, 'download'),
    'b2': ({**LIGHT, 'font': 'sans', 'accent': '#e0457b', 'accent_hover': '#c7366a', 'radius': 2.0,
            **L(nav='bottom', nav_labels=False, header='none')}, 'download'),
    'b3': ({**LIGHT, 'font': 'sans', 'accent': '#e0457b', 'accent_hover': '#c7366a', 'radius': 2.0, 'bg': '#fdf3f6',
            **L(nav='bottom', nav_labels=False, header='none', shadow='shadow', buttons='flat', density=1.15,
                background=['#fff0f5', '#f0f4ff'])}, 'look'),
    # рецепт В: Material 3 из одного цвета
    'c-dark': ({'m3': {'seed': '#386a20', 'dark': True}}, 'download'),
    'c-light': ({'m3': {'seed': '#386a20', 'dark': False}}, 'look'),
    # ошибка в файле
    'error': ({}, 'look', {'сломанная.json': '{\n  "name": "сломанная",\n  "accent": "#ff0000"\n  "bg": "#000000"\n}\n'}),
}


def bg_image(path):
    w, h = 1200, 900
    im = Image.new('RGB', (w, h), (10, 12, 30))
    d = ImageDraw.Draw(im)
    for i, (c, x, y, r) in enumerate([((255, 60, 160), 250, 200, 260), ((40, 200, 255), 900, 650, 320), ((120, 80, 255), 700, 150, 200)]):
        d.ellipse([x - r, y - r, x + r, y + r], fill=c)
    im = im.filter(ImageFilter.GaussianBlur(90))
    im.save(path)


def main():
    shutil.rmtree(APP, ignore_errors=True)
    os.makedirs(APP)
    img = os.path.join(OUT, 'фон-пример.png')
    bg_image(img)
    collapsed = ['расширение для браузера', 'приложение spotify', 'сохранение', 'видео', 'музыка', 'пресеты', 'интерфейс', 'доступ и обновления']
    for sid, spec in SHOTS.items():
        theme, page = spec[0], spec[1]
        extra = spec[2] if len(spec) > 2 else {}
        d = os.path.join(APP, sid, 'VeryGoodDownloader')
        td = os.path.join(d, 'themes')
        os.makedirs(td)
        shutil.copy(img, td)
        t = {'name': sid, **theme}
        json.dump(t, open(os.path.join(td, sid + '.json'), 'w', encoding='utf-8'), ensure_ascii=False, indent=1)
        for fn, text in extra.items():
            open(os.path.join(td, fn), 'w', encoding='utf-8').write(text)
        key = '' if sid == 'error' else 'file:' + sid
        st = {'theme': key, 'win_w': 900, 'win_h': 780, 'ui_scale': 1.0, 'tray': False, 'subs_on_start': False}
        if page == 'look':
            st['collapsed'] = collapsed
        json.dump(st, open(os.path.join(d, 'settings.json'), 'w', encoding='utf-8'), ensure_ascii=False)
        open(os.path.join(APP, sid, 'page.txt'), 'w').write('download' if page == 'download' else 'settings')
    print(len(SHOTS))


main()
