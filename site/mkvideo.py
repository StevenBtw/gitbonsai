"""Turns `gitbonsai timelapse` frames into one H.264 video per tree and theme.

Usage: mkvideo.py <frames-dir> <site-dir>. Each tree's frames sit on one
shared canvas, so the scale stays put and the tree visibly grows. Writes
<site-dir>/tl/<tree>-<theme>.mp4 and <site-dir>/frames.json (the ISO week and
description of every frame, for the player). Needs cairosvg, pillow, ffmpeg.
"""
import glob, io, json, os, re, subprocess, sys

import cairosvg
from PIL import Image

SRC, OUT = (os.path.abspath(a) for a in sys.argv[1:3])
W, FPS = 1120, 10
# The page background of each theme (tokens.css --background), baked into the frames.
BG = {'dark': (0x14, 0x13, 0x10), 'light': (0xf5, 0xf2, 0xea)}
TREES = ['showcase', 'newcomer', 'stevenbtw', 'jacobwilliams', 'tj', 'torvalds', 'steipete']


def caption(desc):
    text = desc.removeprefix('Japanese maple bonsai ')
    return text[:1].upper() + text[1:]


os.makedirs(f'{OUT}/tl', exist_ok=True)
manifest = {}
for tree in TREES:
    meta = []
    for f in sorted(glob.glob(f'{SRC}/{tree}/*-dark.svg')):
        s = open(f, encoding='utf-8').read(5000)
        meta.append({'n': os.path.basename(f)[:-len('-dark.svg')],
                     'vb': [float(x) for x in re.search(r'viewBox="([^"]*)"', s).group(1).split()],
                     'wk': re.search(r'observed (\d{4}-W\d+)', s).group(1),
                     'd': caption(re.search(r'<desc id="d">([^<]*)', s).group(1))})
    if not meta:
        sys.exit(f'no frames for {tree} in {SRC}')
    ux = min(m['vb'][0] for m in meta); uy = min(m['vb'][1] for m in meta)
    ux2 = max(m['vb'][0] + m['vb'][2] for m in meta); uy2 = max(m['vb'][1] + m['vb'][3] for m in meta)
    k = W / (ux2 - ux); H = int((uy2 - uy) * k + 1) // 2 * 2
    for theme in BG:
        tmp = f'{SRC}/_png/{tree}-{theme}'; os.makedirs(tmp, exist_ok=True)
        for i, m in enumerate(meta):
            png = cairosvg.svg2png(url=f"{SRC}/{tree}/{m['n']}-{theme}.svg", output_width=max(1, round(m['vb'][2] * k)))
            tile = Image.open(io.BytesIO(png)).convert('RGBA')
            canvas = Image.new('RGBA', (W, H), BG[theme] + (255,))
            canvas.alpha_composite(tile, (round((m['vb'][0] - ux) * k), round((m['vb'][1] - uy) * k)))
            canvas.convert('RGB').save(f'{tmp}/{i:04d}.png')
        subprocess.run(['ffmpeg', '-y', '-loglevel', 'error', '-framerate', str(FPS), '-i', f'{tmp}/%04d.png',
                        '-c:v', 'libx264', '-preset', 'slow', '-crf', '20', '-pix_fmt', 'yuv420p', '-g', '10',
                        '-color_range', 'tv', '-movflags', '+faststart', f'{OUT}/tl/{tree}-{theme}.mp4'], check=True)
    manifest[tree] = {'w': W, 'h': H, 'fps': FPS, 'frames': [{'wk': m['wk'], 'd': m['d']} for m in meta]}
    print(tree, len(meta), 'frames', W, 'x', H, flush=True)
with open(f'{OUT}/frames.json', 'w', encoding='utf-8') as f:
    json.dump(manifest, f, separators=(',', ':'))
