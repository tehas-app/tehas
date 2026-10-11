# preview/

What `tehasbot preview` builds for a pull request of this repository: see
`Dockerfile`. The files in `live/` are sample data in the shape a host writes
under `~/tehas/live`. When that shape changes, change them with it.

To look at it by hand:

    git archive HEAD | docker build -f preview/Dockerfile -t tehas-preview:local -
    docker run --rm -p 127.0.0.1:8080:8080 tehas-preview:local
