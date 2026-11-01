---
id: a4f08c3e91b2
tags: [docs, guide]
updated: 2026-10-31
---

Pages are plain Markdown files in the `Docs` folder at the top of the project, so they can be edited here or in any text editor, and shared with git like any other file.

## Text

Write **bold**, *italics*, ~~struck out~~, ==highlighted== and `inline code`. A blank line starts a new paragraph.

## Links

- To another page: [[The Day and Night Cycle]], or with your own words: [[Scripts|the scripts, one by one]]
- To a page that does not exist yet: [[Level Streaming]]. Click it to write it.
- To a heading on this page: [jump to Checklists](#checklists)
- To a file in the project: [the Day level](Levels/Day.level)
- To a type in the code: [[code:Hello.Spinner]], or [[code:Voxear.Prefab|a prefab]]
- To the web: https://github.com/CookieJarTools

## Checklists

- [x] Write the page
- [ ] Add a picture
- [ ] Link it from the code

## Callouts

> [!NOTE]
> Something worth knowing.

> [!TIP]
> A better way to do it.

> [!WARNING]
> Something that will bite.

> A plain quote, for anything else.

## Tables

| Writes | Shows | Goes on |
| --- | :---: | --- |
| `[[Page]]` | a link to a page | any page |
| `- [ ]` | a box to tick | lists |

## Code

```csharp
public sealed class Enemy : Behaviour
{
    [SerializeField] private int damage = 10;
}
```

## Pictures

Put a picture in the page with `![what it shows](images/Day.png)`. Its path is from the page's folder, as for pictures kept in an `images` folder beside the page, or from the project's folder. It shows at its own size, or narrower to fit the page. PNG and JPEG pictures show; [[The Day and Night Cycle]] has one. Name a prefab, model or material instead, as `![](Prefabs/Lamp.prefab)`, and the page shows its preview, as the Project panel does. A material shows its fields under it too, to change right there.

## Guides

A link to an action or a check on a line of its own is a step the reader can run or watch tick: see [[Step-by-Step Guides]].

## From the code

Put `[Documentation("How Hello Works/The Day and Night Cycle")]` on a behaviour, or on one of its fields, and the Inspector shows a book button there that opens the page. `[Excerpt("How Hello Works/The Day and Night Cycle#who-drives-it")]` on a field shows that part of the page above it instead, and follows the page as it changes. Select the Game Manager to see both. A page lists the code that points at it under **In the code**. Right-click a page in the tree to copy its `[Documentation]` attribute, ready to paste.

The **Code Reference**'s pages are made from the code, so what you write on one with **Edit** is its note: it shows under Notes, and it is kept in `Docs/.code-notes`.

## Getting around

- **Home**, the top page, lists the pages updated lately and how healthy the docs are. **Ctrl+K** opens any page by its title.
- **Find a page** at the top of the tree searches every page's words.
- **Back** and **forward** go through the pages you have read.
- **Filter by tag** over the tree shows only the pages with every tag picked; a tag on a page picks it too.
- **On this page** lists a long page's headings beside it, when there is room, and takes you to one.
- **Code Reference**, last in the tree, has a page for every public type in the scripts and in Voxear.Core, made from their `///` comments each time the scripts build. Change the comments to change its pages, or write a note on one with **Edit**.
- **Health** lists what needs looking after: pages out of date with the files they link to, links that go nowhere, and pictures that are missing or that no page shows.
- **Edit** shows the page's title, tags and Markdown, with the page as it will read beside it. **Save** or Ctrl+S writes it to its file, and so do **Done** and opening another page.
