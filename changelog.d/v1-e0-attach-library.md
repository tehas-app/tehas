### Changes

- A cycle's `manifest.json` starts with `"version": 1`, the format it is written in. A field is only added within a version.
- The attachment checker, the dispatcher and the Protolab read attachment links by one rule and fetch them by one fetch. Nothing else changes in what they take.
