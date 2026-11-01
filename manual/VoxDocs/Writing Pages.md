---
order: 1
---

A page is a Markdown file in your project's `Docs` folder. Its file name is its title, and folders in `Docs` show as folders in the tree.

## Writing your first page

A new project has no pages yet, so its top page says so and links to a page called Welcome that does not exist yet. Links to missing pages are drawn faded. Click one and VoxDocs makes the page and opens it for writing.

That is also how you add pages later: link to the page you want from one you already have, then click the link.

## Editing a page

Click **Edit** at the top of a page. You get its title, its tags and its Markdown, with the page as it will look beside them, updated a moment after you stop typing.

- **Save**, or Ctrl+S, writes the page to its file. **Done** saves too, and so does opening another page, so you never lose what you wrote.
- Changing the title renames the file. Links that used the old title then go nowhere, and **Health** lists them so you can fix them.
- **Tags** sort pages into groups. Type a tag and press Enter or a comma. VoxDocs suggests tags your other pages already use.

## Markdown you can write

```markdown
# A heading
## A smaller heading

Plain text. A blank line starts a new paragraph.
**Bold**, *italics*, ~~struck out~~, ==highlighted== and `code`.

- A list
- [ ] A box to tick
- [x] A ticked box

> [!TIP]
> A callout: NOTE, TIP, IMPORTANT, WARNING or CAUTION.

| Name | What it does |
| --- | --- |
| Spinner | Turns the Obj it is on |
```

Ticking a box on a page saves the page, so a checklist remembers what is done.

A callout is a box that stands out from the page, for a note or a warning. Each kind, NOTE, TIP, IMPORTANT, WARNING or CAUTION, has its own colour and icon.

A block of code between three backticks keeps its spacing and gets a **Copy** button. Write the language after the backticks, as `csharp`, and its name shows over the block.

## Finding your way around VoxDocs

- **Find a page**, over the tree, finds pages with every word you type, in their titles, tags or text.
- The **tags** over the tree show only the pages that have every tag you pick.
- **Back** and **Forward** go through the pages you have read.
- **On this page**, beside a long page, lists its headings. Click one to go there.
- **Ctrl+K** opens the command bar, which finds any page by its title, the manual's included.
- The top page, **Home**, lists the pages changed most lately.

Next: [[Links and Pictures]].
