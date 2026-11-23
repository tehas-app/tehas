### Changes

- A cycle's `gh issue edit` through the relay is made only if the issue's title and body did not change while the host read it. When another account changed them, the cycle's `gh` fails with one line asking it to read the issue again.
