---
order: 19
---

Items are what a player carries: tools, blocks to put down, food, a bucket. A recipe makes one from others. Both are data assets you make and edit in the Inspector, and the engine applies their rules with a block's: how long a block takes to break with a tool, what it drops, what an item places and what a grid of items makes.

1. [[Items]]: what an item is, and the item a block is carried as
2. [[Tool Kinds and Stations]]: the kinds of tool a game has, and the places things are made at
3. [[Recipes]]: a grid of items and what they make
4. [[Blocks and Items from Scripts]]: digging, placing, drops, buckets and matching recipes in C#

What holds items is yours to write, or comes from the **Voxear Inventory** package: an Inventory, a hotbar, a screen with a crafting grid, and a Miner that digs and builds with what is in the hand. A game that wants none of that carries none of it.
