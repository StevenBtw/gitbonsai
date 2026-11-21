"""Turns `gitbonsai timelapse` frames into one H.264 video per tree and theme.

Usage: mkvideo.py <frames-dir> <site-dir>. Each tree's frames sit on one
shared canvas, so the scale stays put and the tree visibly grows. Every video
has the same size and the same scale, set by the widest tree, so the trees
compare: each stands centred on the bottom edge and the player keeps one shape.
Writes
<site-dir>/tl/<tree>-<theme>.mp4 and <site-dir>/frames.json (the ISO week and
description of every frame, for the player). Needs cairosvg, pillow, ffmpeg.

Frames are rasterized on every CPU core and the videos encoded side by side.
MKVIDEO_NVENC=1 encodes on an NVIDIA GPU (h264_nvenc) instead of libx264.
"""
import glob, io, json, math, os, re, subprocess, sys
from concurrent.futures import ProcessPoolExecutor, ThreadPoolExecutor

import cairosvg
from PIL import Image

W, FPS = 1120, 10
# The page background of each theme (tokens.css --background), baked into the frames.
BG = {'dark': (0x14, 0x13, 0x10), 'light': (0xf5, 0xf2, 0xea)}
TREES = ['showcase', 'stevenbtw', 'tj', 'torvalds', 'steipete']


def caption(desc):
    text = desc.removeprefix('Japanese maple bonsai ')
    return text[:1].upper() + text[1:]


def frame(job):
    """Rasterizes one frame onto its tree's shared canvas."""
    svg, width, size, at, bg, out = job
    tile = Image.open(io.BytesIO(cairosvg.svg2png(url=svg, output_width=width))).convert('RGBA')
    canvas = Image.new('RGBA', size, bg + (255,))
    canvas.alpha_composite(tile, at)
    canvas.convert('RGB').save(out)


def encode(frames, out):
    if os.environ.get('MKVIDEO_NVENC') == '1':
        codec = ['-c:v', 'h264_nvenc', '-preset', 'p6', '-rc', 'vbr', '-cq', '20', '-b:v', '0']
    else:
        codec = ['-c:v', 'libx264', '-preset', 'slow', '-crf', '20']
    subprocess.run(['ffmpeg', '-y', '-loglevel', 'error', '-framerate', str(FPS), '-i', f'{frames}/%04d.png',
                    *codec, '-pix_fmt', 'yuv420p', '-g', '10', '-color_range', 'tv', '-movflags', '+faststart',
                    out], check=True)


def main():
    src, out = (os.path.abspath(a) for a in sys.argv[1:3])
    os.makedirs(f'{out}/tl', exist_ok=True)
    manifest, jobs, videos, trees = {}, [], [], []
    for tree in TREES:
        meta = []
        for f in sorted(glob.glob(f'{src}/{tree}/*-dark.svg')):
            s = open(f, encoding='utf-8').read(5000)
            meta.append({'n': os.path.basename(f)[:-len('-dark.svg')],
                         'vb': [float(x) for x in re.search(r'viewBox="([^"]*)"', s).group(1).split()],
                         'wk': re.search(r'observed (\d{4}-W\d+)', s).group(1),
                         'd': caption(re.search(r'<desc id="d">([^<]*)', s).group(1))})
        if not meta:
            sys.exit(f'no frames for {tree} in {src}')
        ux = min(m['vb'][0] for m in meta); uy = min(m['vb'][1] for m in meta)
        ux2 = max(m['vb'][0] + m['vb'][2] for m in meta); uy2 = max(m['vb'][1] + m['vb'][3] for m in meta)
        trees.append((tree, meta, ux, uy, ux2 - ux, uy2 - uy))
    # One scale for every tree, so their sizes compare: the widest tree fills the width.
    k = W / max(t[4] for t in trees)
    H = math.ceil(max(t[5] for t in trees) * k / 2) * 2  # even, and never below the tallest tree
    for tree, meta, ux, uy, uw, uh in trees:
        left, drop = round((W - uw * k) / 2), round(H - uh * k)  # centred, on the bottom edge
        for theme in BG:
            tmp = f'{src}/_png/{tree}-{theme}'; os.makedirs(tmp, exist_ok=True)
            for i, m in enumerate(meta):
                jobs.append((f"{src}/{tree}/{m['n']}-{theme}.svg", max(1, round(m['vb'][2] * k)), (W, H),
                             (left + round((m['vb'][0] - ux) * k), drop + round((m['vb'][1] - uy) * k)),
                             BG[theme], f'{tmp}/{i:04d}.png'))
            videos.append((tmp, f'{out}/tl/{tree}-{theme}.mp4'))
        manifest[tree] = {'w': W, 'h': H, 'fps': FPS, 'frames': [{'wk': m['wk'], 'd': m['d']} for m in meta]}
        print(tree, len(meta), 'frames', W, 'x', H, f'(tree {round(uw * k)} x {round(uh * k)})', flush=True)
    with ProcessPoolExecutor() as pool:
        list(pool.map(frame, jobs, chunksize=8))
    # ffmpeg threads itself: a few encodes at once are enough.
    with ThreadPoolExecutor(4) as pool:
        list(pool.map(lambda v: encode(*v), videos))
    with open(f'{out}/frames.json', 'w', encoding='utf-8') as f:
        json.dump(manifest, f, separators=(',', ':'))


if __name__ == '__main__':
    main()
