---
id: a4f08c3e91b2
tags: [cookbook, guide]
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

## Getting around

- **Find a page** at the top of the tree searches every page's words.
- **Back** and **forward** go through the pages you have read.
- **Edit** shows the page's Markdown; **Save** writes it to its file.
