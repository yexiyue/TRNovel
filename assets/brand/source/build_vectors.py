"""Build the TRNovel terminal identity from geometry and outlined type.

Requires fonttools. Run from any directory: python assets/brand/source/build_vectors.py
"""

from pathlib import Path

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.ttLib import TTFont


ROOT = Path(__file__).resolve().parents[3]
BRAND = ROOT / "assets/brand"
INK = "#17212B"
PAPER = "#F5EBDD"
ORANGE = "#E78A4E"


def svg(title, viewbox, content):
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{viewbox}" role="img">\n'
        f"  <title>{title}</title>\n{content}\n</svg>\n"
    )


def terminal(color):
    # The frame, opposing chevrons and underscore are the mascot's screen face.
    return f'''<path fill="{ORANGE}" d="M26 6h18v28l-9-6-9 6Z"/>
<path fill="{color}" fill-rule="evenodd" d="M28 26h72a16 16 0 0 1 16 16v58a16 16 0 0 1-16 16H28a16 16 0 0 1-16-16V42a16 16 0 0 1 16-16Zm3 10a9 9 0 0 0-9 9v52a9 9 0 0 0 9 9h66a9 9 0 0 0 9-9V45a9 9 0 0 0-9-9Z"/>
<g fill="none" stroke="{ORANGE}" stroke-width="7" stroke-linecap="round" stroke-linejoin="round">
  <path d="m41 56 12 12-12 12m46-24L75 68l12 12M57 90h14"/>
</g>
<rect x="93" y="90" width="6" height="6" rx="1" fill="{ORANGE}"/>'''


def outlined_text(font, text, x, baseline, size, color, tracking=0):
    glyphs = font.getGlyphSet()
    cmap = font.getBestCmap()
    scale = size / font["head"].unitsPerEm
    paths = []
    for character in text:
        name = cmap[ord(character)]
        pen = SVGPathPen(glyphs)
        glyphs[name].draw(pen)
        if pen.getCommands():
            paths.append(
                f'<path d="{pen.getCommands()}" transform="translate({x:.3f} {baseline}) '
                f'scale({scale:.6f} {-scale:.6f})" fill="{color}"/>'
            )
        x += font["hmtx"][name][0] * scale + tracking
    return "\n".join(paths)


def main():
    font = TTFont(BRAND / "fonts/IBMPlexMono-Medium.ttf")
    for theme, color in [("light", INK), ("dark", PAPER)]:
        mark = svg("TRNovel — terminal companion mark", "0 0 128 128", terminal(color))
        (BRAND / f"mark-on-{theme}.svg").write_text(mark)
        wordmark = svg(
            "TRNovel — Terminal Novel Reader",
            "0 0 440 128",
            terminal(color)
            + outlined_text(font, "TRNovel", 140, 78, 58, color)
            + outlined_text(font, "TERMINAL NOVEL READER", 142, 105, 12, color, 1.1)
            + f'<rect x="398" y="63" width="12" height="15" fill="{ORANGE}"/>',
        )
        (BRAND / f"wordmark-on-{theme}.svg").write_text(wordmark)

    app_icon = svg(
        "TRNovel app icon",
        "0 0 160 160",
        f'<rect width="160" height="160" rx="32" fill="{INK}"/>'
        + f'<g transform="translate(16 14)">{terminal(PAPER)}</g>',
    )
    (BRAND / "app-icon.svg").write_text(app_icon)
    # Favicon omits the fine details that disappear at 16 px.
    favicon = svg(
        "TRNovel",
        "0 0 64 64",
        f'<rect width="64" height="64" rx="12" fill="{INK}"/>'
        + f'<rect x="7" y="10" width="50" height="47" rx="9" fill="{PAPER}"/>'
        + f'<rect x="12" y="15" width="40" height="37" rx="5" fill="{INK}"/>'
        + f'<path d="m19 25 6 6-6 6m26-12-6 6 6 6M28 44h8" fill="none" stroke="{ORANGE}" stroke-width="3.5" stroke-linecap="round" stroke-linejoin="round"/>'
        + f'<path d="M13 3h9v10l-4.5-3L13 13Z" fill="{ORANGE}"/>',
    )
    (ROOT / "docs/public/favicon.svg").write_text(favicon)
    font.close()


if __name__ == "__main__":
    main()
