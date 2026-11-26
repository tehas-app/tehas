### Changes

- The state store migrates from schema 23 only, which v1.3.0 and later
  wrote. The steps for older schemas are gone, and a store older than that
  is refused with a line that says to install a v1.4 release first before
  anything is copied.

### Adopter action

A host on v1.3.0 or later (store schema 23) updates directly. An older one
installs any v1.4 release first, and this one after: the store refuses an
older database and installs nothing.
