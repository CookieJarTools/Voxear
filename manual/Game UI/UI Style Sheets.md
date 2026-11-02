---
order: 3
---

A `.vss` style sheet says how UI looks and where things go. It is written like CSS on the web, with a smaller set of rules:

```css
.menu {
    width: 320px;
    padding: 24px;
    gap: 12px;
    background-color: #1e1f24;
    border: 1px solid #3a3b42;
    border-radius: 8px;
}

.menu Button:hover {
    background-color: var(--vx-accent);
}
```

## Picking the elements to style

| Write | What it picks |
| --- | --- |
| `Button` | Every element of that kind |
| `.menu` | Every element with that class |
| `#resume` | The element with that name |
| `.menu Button` | Buttons anywhere inside a `.menu` |
| `.menu > Button` | Buttons directly inside a `.menu` |
| `*` | Every element |
| `:root` | The element the sheet is linked to |

Add a state to style an element only while it is in it: `:hover` (the pointer is over it), `:active` (being pressed), `:focus`, `:focus-visible` (focused, and reached with the keyboard, or a text field: the state for a focus ring), `:checked` (a toggle that is on), `:selected` and `:disabled`.

When rules disagree, the more specific one wins: a name beats a class, which beats a kind. Text settings, such as `color` and `font-size`, pass down from an element to what is inside it.

## Laying out UI

Elements inside a box are laid out in a line, top to bottom unless you say otherwise:

| Setting | What it does |
| --- | --- |
| `flex-direction: row` | Lays the elements inside left to right. `column` is the default |
| `gap: 8px` | Space between them |
| `padding: 16px`, `margin: 8px` | Space inside the box's edge, and outside it |
| `width`, `height`, `min-width`, `max-width`… | Size, in pixels or as a percentage of the box it is in |
| `flex-grow: 1` | Takes the space left over |
| `align-items`, `justify-content` | Lines elements up across and along the line: `center`, `flex-start`, `flex-end`, `stretch`, `space-between`… |
| `position: absolute` with `left`, `top`, `right`, `bottom` | Places an element exactly, within the box it is in, out of the line |
| `display: none` | Hides it, taking no space |
| `display: grid` | Lays elements out in a grid, with `grid-template-columns` and the rest of CSS's grid settings |

To centre a menu on screen, centre what the document holds:

```css
:root {
    align-items: center;
    justify-content: center;
}
```

## How UI elements look

| Setting | What it does |
| --- | --- |
| `background-color` | The fill |
| `border: 1px solid #3a3b42` | An edge: its width first, then `solid` or `dashed`, then its colour |
| `border-radius: 8px` | Rounded corners |
| `box-shadow: 0 4px 12px #00000080` | A shadow |
| `opacity: 0.5` | See-through |
| `color`, `font-size: 16px`, `font-weight: bold`, `text-align: center` | Text |
| `font-family: mono` | `sans` (the default) or `mono` |
| `backdrop-filter: blur(8px)` | Blurs the game behind the element |
| `pointer-events: none` | Lets clicks pass through to the game |

Colours are written `#rgb`, `#rrggbb`, `#rrggbbaa` (with see-through), `rgb(…)`, `rgba(…)` or `oklch(…)`. The only named colours are `white`, `black` and `transparent`.

Sizes are in `px` or `%`. There is no `em`, `rem`, `vw` or `vh`: use percentages and flexible layouts for screens of different sizes. UI is laid out in pixels that follow the screen's scale setting, so it looks the same size on a high-resolution screen.

## Transitions and animations

```css
Button {
    transition: background-color 120ms ease-out;
}

.warning {
    animation: pulse 1s ease-in-out infinite alternate;
}

@keyframes pulse {
    from { opacity: 1; }
    to { opacity: 0.4; }
}
```

Transitions and animations change colours and opacity smoothly.

## Style sheet variables and the theme

`--name: value` sets a variable, and `var(--name)` uses it:

```css
:root {
    --gold: #e8b64c;
}

.coins {
    color: var(--gold);
}
```

Your UI starts from Voxear's own dark theme, which styles every kind of element and sets variables such as `--vx-accent`, `--vx-panel` and `--vx-text`. Set those in your sheet's `:root` to recolour every control at once:

```css
:root {
    --vx-accent: #e8b64c;
}
```

A dropdown's open list is drawn apart from your document, so it keeps the default theme's look whatever your sheet says.

## Mistakes in a style sheet

A setting Voxear does not know, or a value it cannot read, is skipped, and the Console names the file and line. The rest of the sheet still applies. Some habits from the web that do not work here: `background` (write `background-color`), a border written with its colour first, named colours other than the three above, `em` sizes, and `@media`.
