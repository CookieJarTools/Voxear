---
order: 5
---

# Docs Health

Pages go out of date as a game changes. **Health**, at the top of VoxDocs, lists what needs looking after. The top page says how many things it found, and links to it.

It finds:

- **Pages out of date.** A page that links to a file in your project, such as a script or a level, is out of date when that file has changed since the page did. If the project uses git, the days come from git, so changes made in any editor count.
- **Links that go nowhere**, including links to pages not written yet.
- **Missing pictures**: pictures a page shows that are not there.
- **Pictures no page shows**: files in your pages' `images` folders that nothing uses any more. Pictures elsewhere in the project are left alone, since your game probably uses them.
- **Notes for code that is gone**: notes on Code Reference pages whose type was renamed or removed.

It also lists two things worth knowing that are not mistakes, so they are not counted:

- **Code with no page**: your types that no `[Documentation]` points at.
- **Comments still to write**: how many parts of each of your types have no `///` comment.

Each entry links to the page or file it is about.

Health looks only at your own pages. The manual comes with the engine, so it is not yours to look after.
