---
order: 2
---

## Links to pages

Put a page's title in double square brackets to link to it:

```markdown
See [[Combat]] for how damage works.
See [[Combat|the combat rules]] to word the link yourself.
```

If two pages share a title, use the page's path instead, as `[[Enemies/Combat]]`.

You can also link to:

| To | Write |
| --- | --- |
| A heading on the same page | `[the rules](#rules)` |
| A heading on another page | `[the rules](Combat.md#rules)` |
| A file in your project | `[the first level](Levels/First.level)` |
| A page of this manual | `[[manual:Prefabs]]` |
| A type in your code | `[[code:MyGame.Enemy]]` |
| A website | `https://example.com`, or `[words](https://example.com)` |

A heading's link is its text in lower case, with dashes for spaces and other marks left out: `## Hit Points!` is `#hit-points`.

Clicking a link to a file in your project shows that file in the Project panel.

While editing, **Link…** lists your pages and the manual's, and puts in a link to the one you pick.

## Pictures

```markdown
![The first level at night](images/Night.png)
```

The words in square brackets say what the picture shows. PNG, JPEG and GIF pictures work, and GIFs play. A picture is shown at its own size, or smaller to fit the page.

The path starts from the page's folder. Pictures you add while editing go into an `images` folder beside the page, so that is where most of them live. A path that is not found there is tried from the project's folder.

Ways to add a picture while editing:

- **Drop a picture file** on the editor. It is copied into the page's `images` folder and put in where the cursor is.
- **Image…** lists the pictures in your project, and the prefabs, models and materials, to pick from.
- **Capture Scene** takes a picture of the Scene view.
- **Record Scene** and **Record Game** record three seconds of the Scene or Game view as a GIF.

Captures and recordings leave out the editor's own lines and handles, so they show only the game.

## Live previews

Name a prefab, model or material in place of a picture and the page shows its preview, as the Project panel draws it:

```markdown
![The lamp](Prefabs/Lamp.prefab)
```

The preview stays up to date as the asset changes. A material also shows its fields under the preview, as the Inspector does, so you can change its colours right there on the page.

Next: [[Step-by-Step Guides]].
