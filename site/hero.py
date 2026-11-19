"""Writes the hero gallery's data from the plates build.sh rendered.

Usage: hero.py <plates.tsv> <site-dir>. plates.tsv lists `id<TAB>name` in
gallery order; <site-dir>/hero/<id>-<theme>.svg already exist. Writes
<site-dir>/plates.json (each plate's label, alt text and viewBox, read from the
SVG itself so nothing is typed in twice) and a small PNG thumbnail per plate
and theme, <site-dir>/hero/<id>-thumb-<theme>.png. Needs cairosvg.
"""
import json, re, sys

import cairosvg

THUMB_W = 192  # twice the widest the thumbnail is drawn, for sharp screens


def read(svg):
    """The style, ISO week, description and viewBox an SVG records."""
    text = open(svg, encoding='utf-8').read(8000)
    meta = re.search(r'<metadata>([^<]*)</metadata>', text).group(1)
    # "gitbonsai 0.4.0; observed 2026-W44; moyogi; dark"
    parts = [p.strip() for p in meta.split(';')]
    week = parts[1].removeprefix('observed ')
    desc = re.search(r'<desc id="d">([^<]*)</desc>', text).group(1)
    box = [float(v) for v in re.search(r'viewBox="([^"]+)"', text).group(1).split()]
    return parts[2], week, desc, box


def main(tsv, site):
    plates = []
    for line in open(tsv, encoding='utf-8'):
        pid, name = line.rstrip('\n').split('\t')
        style, week, desc, box = read(f'{site}/hero/{pid}-dark.svg')
        plates.append({'id': pid, 'name': name, 'style': style, 'week': week, 'alt': desc, 'box': box})
        for theme in ('light', 'dark'):
            cairosvg.svg2png(url=f'{site}/hero/{pid}-{theme}.svg', write_to=f'{site}/hero/{pid}-thumb-{theme}.png',
                             output_width=THUMB_W)
    assert any(p['id'] == 'showcase' for p in plates), 'the hero opens on the Showcase plate'
    with open(f'{site}/plates.json', 'w', encoding='utf-8', newline='\n') as f:
        json.dump(plates, f, separators=(',', ':'))
    print(f'{len(plates)} plates', file=sys.stderr)


if __name__ == '__main__':
    main(*sys.argv[1:3])
