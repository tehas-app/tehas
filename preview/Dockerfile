# The preview of this repository as a product: the manual, the dashboard and
# the front page as this commit has them, served the way a host's docs server
# serves them. `tehasbot preview` builds it for every open pull request.
#
# There is no factory behind it. The files in preview/live stand where a host
# has its queue, its running cycle and its health, so the pages have something
# to show. Nothing is fetched and nothing runs but the server.
# caddy 2.8.4, named by digest so a moved tag cannot change what runs.
# host/versions.env records the version and this digest together.
FROM caddy@sha256:226d1f059b75399fe19182893c7184591c07b97afc8dfcf44eeb80c9a77a530f
COPY host/edge/docs.Caddyfile /etc/caddy/Caddyfile
COPY host/docs /srv/docs
COPY host/fun /srv/fun
COPY preview/live /srv/live
# A preview is never for search engines, and counts no visitors.
ENV TEHAS_INDEXING=0
