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

Put a picture in the page with `![what it shows](images/Day.png)`. Its path is from the page's folder, as for pictures kept in an `images` folder beside the page, or from the project's folder. It shows at its own size, or narrower to fit the page. PNG and JPEG pictures show; [[The Day and Night Cycle]] has one.

## Getting around

- **Find a page** at the top of the tree searches every page's words.
- **Back** and **forward** go through the pages you have read.
- **Filter by tag** over the tree shows only the pages with every tag picked; a tag on a page picks it too.
- **On this page** lists a long page's headings beside it, when there is room, and takes you to one.
- **Health** lists what needs looking after: pages out of date with the files they link to, links that go nowhere, and pictures that are missing or that no page shows.
- **Edit** shows the page's title, tags and Markdown, with the page as it will read beside it. **Save** or Ctrl+S writes it to its file, and so do **Done** and opening another page.
