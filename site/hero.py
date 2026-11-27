"""Writes the hero gallery's data from the plates build.sh rendered.

Usage: hero.py <plates.tsv> <site-dir>. plates.tsv lists `id<TAB>name` in
gallery order; <site-dir>/hero/<id>-<theme>.svg and <id>-legend-<theme>.svg
already exist. Writes <site-dir>/plates.json (each plate's label, alt text and
viewBox with and without its legend, read from the SVGs themselves so nothing is
typed in twice) and a small PNG thumbnail per plate and theme,
<site-dir>/hero/<id>-thumb-<theme>.png. It also trims hero/waiting-<theme>.svg
to the seedling it draws. Needs cairosvg and pillow.
"""
import io, json, re, sys

import cairosvg
from PIL import Image

THUMB_W = 192  # twice the widest the thumbnail is drawn, for sharp screens


def crop(svg, margin=0.1):
    """Trims an SVG's viewBox to what it draws, so the small waiting seedling fills its picture."""
    text = open(svg, encoding='utf-8').read()
    x, y, w, h = [float(v) for v in re.search(r'viewBox="([^"]+)"', text).group(1).split()]
    k = 4  # pixels per unit for the measuring render
    left, top, right, bottom = Image.open(io.BytesIO(cairosvg.svg2png(
        bytestring=text.encode(), output_width=round(w * k)))).getchannel('A').getbbox()
    pad = margin * max(right - left, bottom - top) / k
    box = [x + left / k - pad, y + top / k - pad, (right - left) / k + 2 * pad, (bottom - top) / k + 2 * pad]
    head = re.match(r'<svg[^>]*>', text).group(0)
    new = re.sub(r'viewBox="[^"]+"', 'viewBox="%s"' % ' '.join(f'{v:g}' for v in box), head)
    new = re.sub(r'height="[^"]+"', f'height="{round(400 * box[3] / box[2])}"', new)
    with open(svg, 'w', encoding='utf-8', newline='\n') as f:
        f.write(new + text[len(head):])


def read(svg):
    """The style, description and viewBox an SVG records."""
    text = open(svg, encoding='utf-8').read(8000)
    meta = re.search(r'<metadata>([^<]*)</metadata>', text).group(1)
    # "gitbonsai 0.4.0; observed 2026-W44; moyogi; dark"
    style = [p.strip() for p in meta.split(';')][2]
    desc = re.search(r'<desc id="bonsai-desc">([^<]*)</desc>', text).group(1)
    box = [float(v) for v in re.search(r'viewBox="([^"]+)"', text).group(1).split()]
    return style, desc, box


def main(tsv, site):
    plates = []
    for line in open(tsv, encoding='utf-8'):
        pid, name = line.rstrip('\n').split('\t')
        style, desc, box = read(f'{site}/hero/{pid}-dark.svg')
        lbox = read(f'{site}/hero/{pid}-legend-dark.svg')[2]
        plates.append({'id': pid, 'name': name, 'style': style, 'alt': desc, 'box': box, 'lbox': lbox})
        for theme in ('light', 'dark'):
            cairosvg.svg2png(url=f'{site}/hero/{pid}-{theme}.svg', write_to=f'{site}/hero/{pid}-thumb-{theme}.png',
                             output_width=THUMB_W)
    assert any(p['id'] == 'showcase' for p in plates), 'the hero opens on the Showcase plate'
    for theme in ('light', 'dark'):
        crop(f'{site}/hero/waiting-{theme}.svg')
    with open(f'{site}/plates.json', 'w', encoding='utf-8', newline='\n') as f:
        json.dump(plates, f, separators=(',', ':'))
    print(f'{len(plates)} plates', file=sys.stderr)


if __name__ == '__main__':
    main(*sys.argv[1:3])
