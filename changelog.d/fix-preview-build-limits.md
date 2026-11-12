### Changes

- A preview's build runs with no network and in `tehas.slice`, under the slice's memory limit. A `FROM` image is still pulled; a `RUN` step reaches nothing.

### Adopter action

A `preview/Dockerfile` whose `RUN` steps fetch from the network (a package install, a download) now fails to build. Put what it needs in the commit or in the `FROM` image. To try a recipe by hand, build it with `docker build --network none`.
