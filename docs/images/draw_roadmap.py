"""Draws the README's roadmap picture as two SVGs, for GitHub's light and dark themes.

Run it from the repository's root after a milestone changes, with the list below updated to match
docs/ROADMAP.md:

    python3 docs/images/draw_roadmap.py docs/images
"""

import sys
from html import escape

OUT = sys.argv[1]

MILESTONES = [
    ("M0", "Foundation", "Archetype ECS, Unity's lifecycle, C# on CoreCLR, the Vulkan device", "done"),
    ("M1", "Rendering core", "Render graph, bindless, GPU culling and LODs, shadows, clustered lights", "done"),
    ("M2", "Content, scenes and levels", "Assets, prefabs, levels, post-processing, content builds, async loading", "done"),
    ("M3", "Editor and Voxear UI", "The Unity-style editor, its UI toolkit, play mode, undo, VoxDocs", "now"),
    ("", "Packages", "Share code, tools and content from GitHub, as Unity's packages", "next"),
    ("M4", "Input, localization, audio, physics", "Input actions, string tables, the mixer, Jolt and Box2D", "now"),
    ("M5", "Lighting", "Voxel GI, volumetric fog, soft shadows, probes, upscalers", "planned"),
    ("M6", "Animation", "GPU skinning, state machines, blend trees, motion matching, IK", "planned"),
    ("", "Gameplay kits", "Guns with procedural recoil, controllers, building destruction", "planned"),
    ("M7", "Materials and particles", "Shader Graph, material variants, and GPU particles", "planned"),
    ("M8", "Terrain", "CDLOD terrain, layers, stamps, erosion, scatter, generators", "planned"),
    ("M9", "Navigation, cameras, Timeline", "Pathfinding and avoidance, camera rigs, Timeline", "planned"),
    ("M10", "2D and networking", "Sprites and tilemaps, replication and rollback", "planned"),
    ("M11", "Modding and platform services", "Content and script mods, Steam, the Workshop", "planned"),
    ("M12", "Ship it", "Builds end to end, crash reports, store uploads", "planned"),
    ("M13", "Virtualised geometry", "Nanite-style clusters, streamed and rasterised on the GPU", "planned"),
]

THEMES = {
    "light": {
        "bg": "#ffffff", "border": "#d1d9e0", "text": "#1f2328", "muted": "#59636e", "line": "#d1d9e0",
        "done": "#1a7f37", "now": "#5b6cf0", "next": "#9a6700", "planned": "#8c959f", "row": "#f6f8fa",
    },
    "dark": {
        "bg": "#0d1117", "border": "#30363d", "text": "#e6edf3", "muted": "#9198a1", "line": "#30363d",
        "done": "#3fb950", "now": "#8a97ff", "next": "#d29922", "planned": "#656c76", "row": "#151b23",
    },
}

LABELS = {"done": "Done", "now": "In progress", "next": "Next", "planned": "Planned"}

WIDTH, TOP, ROW, LEFT = 880, 92, 40, 40
FONT = "-apple-system, BlinkMacSystemFont, 'Segoe UI', 'Noto Sans', Helvetica, Arial, sans-serif"
MONO = "ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, monospace"


def marker(x, y, state, c):
    if state == "done":
        return (f'<circle cx="{x}" cy="{y}" r="8" fill="{c["done"]}"/>'
                f'<path d="M{x-3.5} {y} l2.5 2.5 l4.5-5" fill="none" stroke="{c["bg"]}" stroke-width="2" '
                f'stroke-linecap="round" stroke-linejoin="round"/>')
    if state == "now":
        return (f'<circle cx="{x}" cy="{y}" r="7.5" fill="{c["bg"]}" stroke="{c["now"]}" stroke-width="2"/>'
                f'<path d="M{x} {y-5.5} a5.5 5.5 0 0 1 0 11 z" fill="{c["now"]}"/>')
    color = c["next"] if state == "next" else c["planned"]
    return f'<circle cx="{x}" cy="{y}" r="7" fill="{c["bg"]}" stroke="{color}" stroke-width="2"/>'


def draw(theme):
    c = THEMES[theme]
    height = TOP + ROW * len(MILESTONES) + 28
    out = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{height}" viewBox="0 0 {WIDTH} {height}" '
        f'role="img" aria-label="Voxear roadmap">',
        f'<rect x="0.5" y="0.5" width="{WIDTH-1}" height="{height-1}" rx="12" fill="{c["bg"]}" stroke="{c["border"]}"/>',
        f'<text x="{LEFT-8}" y="44" font-family="{FONT}" font-size="20" font-weight="600" fill="{c["text"]}">Roadmap</text>',
    ]
    # The legend, right-aligned.
    x = WIDTH - 36
    for state in ["planned", "next", "now", "done"]:
        label = LABELS[state]
        width = 7.4 * len(label) + 40
        x -= width
        out.append(marker(x + 8, 39, state, c))
        out.append(f'<text x="{x + 22}" y="44" font-family="{FONT}" font-size="13" fill="{c["muted"]}">{label}</text>')
    first, last = TOP + ROW // 2, TOP + ROW * (len(MILESTONES) - 1) + ROW // 2
    out.append(f'<line x1="{LEFT+16}" y1="{first}" x2="{LEFT+16}" y2="{last}" stroke="{c["line"]}" stroke-width="2"/>')
    for i, (code, title, detail, state) in enumerate(MILESTONES):
        y = TOP + ROW * i + ROW // 2
        if state in ("now", "next"):
            out.append(f'<rect x="{LEFT-8}" y="{y-16}" width="{WIDTH-2*LEFT+16}" height="32" rx="7" fill="{c["row"]}"/>')
        out.append(marker(LEFT + 16, y, state, c))
        out.append(f'<text x="{LEFT+40}" y="{y+5}" font-family="{MONO}" font-size="12" fill="{c["muted"]}">{escape(code)}</text>')
        weight = "600" if state in ("now", "next") else "500"
        out.append(f'<text x="{LEFT+80}" y="{y+5}" font-family="{FONT}" font-size="14" font-weight="{weight}" '
                   f'fill="{c["text"]}">{escape(title)}</text>')
        out.append(f'<text x="{LEFT+340}" y="{y+5}" font-family="{FONT}" font-size="13" fill="{c["muted"]}">{escape(detail)}</text>')
    out.append("</svg>")
    return "\n".join(out) + "\n"


for theme in THEMES:
    with open(f"{OUT}/roadmap-{theme}.svg", "w") as file:
        file.write(draw(theme))
