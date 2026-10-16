"""Take the pictures in the docs from the viewer.

Run from the repository root:

    uv run --with playwright --with pillow python docs/images/shots.py [NAME ...]

It bakes the test maps each shot needs with `viewer_export`, serves
`viewer/web`, opens each shot's scene in Chrome at the camera its `view`
gives, and saves `docs/images/NAME.png`. With names, it takes only those
shots. Playwright needs Chrome: `uv run --with playwright playwright install
chrome` installs it.

A shot's `view` is the viewer's own `view=` address parameter. To find one,
open the scene in the viewer, frame it, and press `v`.
"""

import subprocess
import sys
import time
from pathlib import Path

from PIL import Image
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "docs" / "images"
PORT = 8766

# name: (map, view, options). Options: `size` of the page in pixels,
# `toggle` these toolbar checkboxes by id, even while the toolbar is hidden,
# `click` each of these selectors in turn, `hover` a point to show its
# readout at, `ui` to keep the sidebar and toolbar, and `clip` the shot to
# `(x, y, width, height)`.
SHOTS = {
    "lane-layers": ("tests/data/lane_layers.xodr", "25,-26,22,50,-2,0", {}),
    "object-poses": ("tests/data/object_poses.xodr", "-6,-3.3,4.5,8.8,4.8,2.8", {}),
    "skeleton": ("tests/data/skeleton.xodr", "19,-10,5,25,-2,3", {}),
    "curve-local": ("tests/data/curve_local.xodr", "28,-21,17,28,0,0", {}),
    "inner-outlines": ("tests/data/inner_outlines.xodr", "21,-15,9,21,-4,0", {"size": (1000, 450)}),
    "outline-markings": ("tests/data/outline_markings.xodr", "16,-11,10,16,-0.5,0",
                         {"size": (1000, 450)}),
    "repeats": ("tests/data/repeats.xodr", "35,-28,40,40,22,0", {}),
    "object-surface": ("tests/data/object_surface.xodr", "21,-6,3,20,-1.75,0",
                       {"toggle": ["objects"]}),
}

# The viewer's chrome, hidden unless a shot keeps it.
HIDE = "#sidebar, #tools, #hint { display: none !important; } #app { grid-template-columns: 1fr !important; }"


def bake(maps):
    cargo = ["cargo", "run", "--quiet", "--release", "-p", "xodr-viewer", "--"]
    subprocess.run(cargo + sorted(maps), cwd=ROOT, check=True, stdout=subprocess.DEVNULL)


def shoot(browser, name, scene, view, options):
    width, height = options.get("size", (1000, 600))
    page = browser.new_page(viewport={"width": width, "height": height})
    if not options.get("ui"):
        page.add_init_script(
            "document.addEventListener('DOMContentLoaded', () => document.head.append("
            f"Object.assign(document.createElement('style'), {{ textContent: '{HIDE}' }})))"
        )
    page.goto(f"http://localhost:{PORT}/?scene={scene}&view={view}")
    page.wait_for_function("document.querySelector('canvas') !== null", timeout=60000)
    time.sleep(2.0)
    for box in options.get("toggle", []):
        page.evaluate(f"document.getElementById('{box}').click()")
        time.sleep(0.5)
    for selector in options.get("click", []):
        page.click(selector)
        time.sleep(0.5)
    if "hover" in options:
        page.mouse.move(*options["hover"])
        time.sleep(0.5)
    path = OUT / f"{name}.png"
    clip = options.get("clip")
    clip = clip and dict(zip(("x", "y", "width", "height"), clip))
    page.screenshot(path=str(path), clip=clip)
    page.close()
    Image.open(path).convert("RGB").quantize(colors=128, dither=Image.Dither.NONE).save(
        path, optimize=True
    )
    print(f"{path.relative_to(ROOT)}: {path.stat().st_size // 1024} KiB")


def main():
    wanted = sys.argv[1:] or list(SHOTS)
    shots = {name: SHOTS[name] for name in wanted}
    bake({m for m, _, _ in shots.values()})
    server = subprocess.Popen(
        [sys.executable, "-m", "http.server", str(PORT), "--bind", "127.0.0.1"],
        cwd=ROOT / "viewer" / "web",
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    time.sleep(1.0)
    try:
        with sync_playwright() as p:
            browser = p.chromium.launch(
                channel="chrome",
                args=["--use-gl=angle", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"],
            )
            for name, (xodr, view, options) in shots.items():
                shoot(browser, name, Path(xodr).stem + ".json", view, options)
            browser.close()
    finally:
        server.terminate()


if __name__ == "__main__":
    main()
