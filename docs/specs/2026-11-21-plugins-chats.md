# Spec: plugins, skills for chats

**Status**: Draft 2026-11-21. Part of the plugins spec, `2026-11-21-plugins.md`, which holds the terms, the settings and the manifest. Skills and tools for Obeya's and industry's chats. Nothing is built.

## Skills for chats

The factory's chats are not tehas's: they are Obeya's chat and
tehas-industry's ticket wizard and assistant, each a model turn with tools
of its host's own, where a person presses any button. A plugin may add a
**skill** to them: instructions for a kind of question, and, if it wants,
**tools** the chat's model may call. tehas holds and checks them; the chat's
host chooses to use them, through the same factory API it already reads.

### How a chat gets them

- `tehas plugin skills --json` prints the granted skills: plugin, id, its
  Markdown text, the tools it uses with their `about` and parameter schema,
  and the plugin's pin. Nothing that is not granted is listed.
- `tehas plugin call <plugin> <tool> --json` takes the arguments as one JSON
  object on standard input and prints the answer. Industry runs it on the
  factory as it runs the other commands, and offers it to Obeya behind its
  own routes. A chat host never reaches a plugin any other way.
- `tehas api` lists `plugin-skills` once it ships.

### What a skill may be

- Markdown, at most 8 KiB, no HTML, no images. Links are kept as text.
- The chat puts it in a block of its own, named as the plugin's skill, below
  the host's own rules. A skill cannot change those rules or the host's
  tools, and the host says so in the block. What the skill says is a
  suggestion to the model, never a permission.
- The chat shows the person when a turn used a plugin's skill or tool, by
  the plugin's label. Obeya lets a person turn a plugin's skills off for
  themselves; that is Obeya's to build, and a person can only narrow what
  the factory granted.

### What a tool call is

- One short run of the plugin's image, in the base container with
  `--network none`, the tool's `timeout` of at most 60 seconds, and
  `/run/context.json` holding the tool, its arguments, the factory and the
  chat host's name. No person's login and no chat text beyond the
  arguments.
- `/state` is read only in a call, and there is no `/out` view: a call can
  keep nothing and publish nothing, so what a person said cannot reach a
  view or a later run. With `product` granted, `/product` is the
  default branch's checkout of the last run.
- The answer is `/out/_tehas/answer.json`, JSON of at most 32 KiB, checked
  against the tool's declared answer shape if it has one. tehas passes it
  on marked `"untrusted": true`, as Obeya marks its own reads.
- A tool reads and answers. It cannot press, file, label or comment, and it
  offers no button in v1. A chat host that turns an answer into an offer
  does it with its own rules, and the person presses it.
- At most one call of a plugin at a time per chat host, and at most the
  instance's `TEHAS_PLUGIN_CALLS_MINUTE` calls a minute in all. A call over
  either is answered as busy.
- With `model` granted, a call has the proxy and the run's budget like any
  run, and a key made for the call.

### Granting

A skill and each of its tools are granted one by one:
`skill:map tool:find-part`. A skill whose tool is not granted is listed
without that tool. `tehas plugin review` shows a skill's whole text, so a
person reads what will go into the chats before granting it.
