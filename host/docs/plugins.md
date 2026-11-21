---
title: Plugins
description: "Programs from pinned git repositories that add views and checks to a factory: what each capability lets one do."
---

# Plugins

Plugins are a feature still being built. A release refuses
`TEHAS_PLUGIN=on` with exit 78, and nothing of a plugin is fetched, built or
run on a host yet. [Features](/features) says how far each part is built.

A plugin is a program from a git repository, pinned by commit. It gets only
what its grant line in the instance settings names, for that commit:
`TEHAS_PLUGIN_GRANTS_<NAME>`, the commit, then the capabilities. A new commit
grants nothing until its line is written again.

## Capabilities

`tehas plugin review <name>` prints, for each capability a plugin asks for,
the line of this table. Read it before you write the grant.

| Capability | What it lets the plugin do |
| --- | --- |
| `view` | Its /out is served behind the sign-in, and a panel in the dock: everyone the sign-in lets in sees it |
| `view:<id>` | One of its views, the part of its /out at that view's path, is served behind the sign-in and opens from the plugins menu: everyone the sign-in lets in sees it |
| `menu` | Its granted views are listed in the plugins menu of both front pages, by its name and their labels, in place of a panel in the dock: names and labels are public, as live/ is |
| `product` | Reads all of the product's code, read only, in every run |
| `model:<tokens>` | Chat completions through a socket, up to the tokens of the grant a run |
| `model:images` | Images in its model requests, each carried in the request and never fetched: what it sends of the product's pages goes to the model |
| `action:<id>` | A button in the view: a signed-in person asks for a refresh |
| `hook:after:work` | A run on each pull request a cycle leaves; its summary is on the pull request for every reader of the repository |
| `skill:<id>` | Its text from the pinned commit goes into the chats' instructions |
| `needs:<name>` | Runs only while the named plugin is ready, and its chat gets the skills that plugin grants to a plugin's chat; it grants the other plugin nothing |
| `tool:<id>` | The chats it names may call it: each call is a run with the caller's arguments and the plugin's /state read only, and its answer goes to the model |

A grant of anything not in this table is refused when the settings load.

## The plugins menu

A plugin granted `menu` takes no panel in the dock. `tehas update` lists it
in `live/plugins.json` instead: its name, its label, and each of its views it
is granted, with the view's label, its address and whether it opens in the
dock or a new tab. Both front pages read the file and show each plugin's
label in the menu, after the person's panels, with its views as links under
it. `live/` is public, so a plugin's name and labels are, as a panel's are;
what a view shows is behind the sign-in. A plugin without `menu` keeps the
one panel of its `view`, as before.
