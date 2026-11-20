### Changes

- A stream of the front-end kit takes `broken: "end"`: a stream that breaks after it opened waits as after a clean end, and only failed connects make the wait grow. A page that read its own stream this way, such as a ticket page, keeps its old waits. A stream without it behaves as before.
