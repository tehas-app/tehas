### Changes

- The events have their contract: `api/events.txt` names them, `api/event.schema.json` is the envelope, and each event has a schema with two samples in `api/`. The start gate's ask and a gate's answer have schemas too, for 2.1. No factory raises an event yet.
- The shared kit gains `events.js`: `raise(envelope)` dispatches a factory's event on a page as `tehas:<event>`, such as `tehas:cycle.ended`. A page that shows many factories may raise the events it builds from its own stream.
