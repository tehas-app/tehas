---
title: Previews and the Protolab on the internet
description: "How a tehas host publishes a preview of every pull request and the Protolab's tickets under names of their own: the domain, the certificate, the proxy and the edge router."
---

# Previews and the Protolab on the internet

A host can publish two things under names of their own: a running preview of
every open pull request, and the Protolab's page for every ticket at the
prototype stage. This page is how that was set up for the tehas-tehas
instance, written so that another instance can follow it.

| Name | What answers |
| --- | --- |
| `uat-<pr>.tehas.app` | The preview of that pull request |
| `lab-<issue>.tehas.app` | That ticket's page in the Protolab |
| `lab.tehas.app` | The Protolab's list |

How far this is proven:

- The steps for the domain, the certificate and the proxy, 1 to 6 below, were
  carried out and checked from outside on 10 October 2026, on Ubuntu 26.04 LTS
  with Caddy 2.11 as a system service.
- The router's configuration was run in Caddy 2.8.4, the version its container
  uses, with a container on an internal Docker network, and every route
  answered as this page says. That was on a development machine.
- The preview image of this repository was built and run with the options a
  preview gets, and served the front page, the dashboard and `/health`.
- `tehasbot preview` is covered by the shell tests. It has never run on a
  host, against GitHub or against Docker. Steps 7 to 10 are what the code
  does, not a record of a run.

See [Features](/features).

## What it needs

- A domain whose DNS is at Cloudflare, proxied, with a wildcard record. The
  dashboard's own name is already there in the same way.
- A proxy in front of the host that can reach the host's private address. For
  tehas-tehas that is a Caddy on another machine, the one that already sends
  `tehas.app` to port 8080.
- Docker on the host, which a tehas host has.

Nothing here needs a DNS token or a paid certificate.

## Why the names are one level deep

Cloudflare's free certificate covers the domain and one level under it:
`*.tehas.app`. A name two levels down, such as `12.uat.tehas.app`, resolves and
then fails at the TLS handshake. You can see both halves:

```
dig +short 1.uat.tehas.app          # answers: the wildcard record matches
curl -sI https://1.uat.tehas.app    # fails: no certificate for two levels
curl -sI https://uat-1.tehas.app    # gets an answer
```

Two levels need Cloudflare's paid certificates, or a DNS token on the host and
the host's address made public. `uat-12.tehas.app` needs neither.

## The proxy needs a certificate too

Cloudflare speaks HTTPS to the proxy in front of the host. A Caddy that gets a
certificate per site has none for a name it has no site for, and Cloudflare
then answers 525. That is what `lab.tehas.app` said before this was set up.

The answer is one certificate for `*.tehas.app` on the proxy. A Cloudflare
origin certificate is free, lasts for years and needs no renewal job. It is
trusted by Cloudflare only, which is enough: these names are only ever reached
through Cloudflare.

### 1. Make the certificate

In Cloudflare, in the domain: **SSL/TLS**, **Origin Server**, **Create
Certificate**.

- Leave "Generate private key and CSR with Cloudflare" chosen.
- Host names: `*.tehas.app` and `tehas.app`, which is the default.
- Validity: the default of 15 years.
- Format: PEM.

The next page shows the certificate and the private key. The key is shown
once. Keep the page open for the next step. On **SSL/TLS**, **Overview**, the
encryption mode has to be Full or Full (strict).

### 2. Put the two files on the proxy's machine

As root there:

```
mkdir -p /etc/caddy/certs
chmod 700 /etc/caddy/certs
nano /etc/caddy/certs/origin.pem     # the Origin Certificate, with its BEGIN and END lines
nano /etc/caddy/certs/origin.key     # the Private Key, the same
chown -R caddy:caddy /etc/caddy/certs
chmod 644 /etc/caddy/certs/origin.pem
chmod 600 /etc/caddy/certs/origin.key
```

### 3. Check the files before touching the Caddyfile

```
openssl x509 -in /etc/caddy/certs/origin.pem -noout -ext subjectAltName -enddate
sudo -u caddy test -r /etc/caddy/certs/origin.key && echo "caddy can read the key"
```

The first lists both host names and an end date years away. The second prints
its line.

> **The files come first.** A site that names certificate files that are not
> there stops Caddy from starting at all. If Caddy is then restarted, every
> site in that Caddyfile is down, not only the new one. This happened while
> these steps were first taken. To get back: take the new site out,
> `caddy validate --config /etc/caddy/Caddyfile`, `systemctl start caddy`.

### 4. Add the site

Under the dashboard's site in `/etc/caddy/Caddyfile`, with the host's private
address in place of `<address>`:

```
*.tehas.app {
    tls /etc/caddy/certs/origin.pem /etc/caddy/certs/origin.key
    @edge header_regexp Host ^(lab|lab-[0-9]+|uat-[0-9]+)\.tehas\.app$
    handle @edge {
        reverse_proxy <address>:8081
    }
    handle {
        respond 404
    }
}
```

Caddy takes a wildcard as a whole label only, so `lab-*` cannot be a site
address; the matcher picks the names. The sites `tehas.app` and
`www.tehas.app` are more specific and are not touched. The Host header is
passed on as it is, which is what the router picks by.

### 5. Validate, then reload

As root on the proxy's machine, as in step 2:

```
caddy validate --config /etc/caddy/Caddyfile && systemctl reload caddy
```

Reload, never restart. A reload with a configuration Caddy refuses keeps the
old one serving. Warnings about formatting or deprecated directives elsewhere
in the file do not matter; the last line has to be `Valid configuration`.

### 6. What the names answer now

| Name | Answer | It means |
| --- | --- | --- |
| `tehas.app` | 200 | The dashboard is as before |
| `www.tehas.app` | 301 | So is its redirect |
| `lab.tehas.app`, `lab-1.tehas.app`, `uat-1.tehas.app` | 502 | The certificate and the site work. Nothing listens on port 8081 of the host yet |
| `nothing.tehas.app`, `uat-x.tehas.app` | 404 | Other names are refused |
| any of them | 521 | Caddy on the proxy is not running. See the box above |
| any of them | 525 | The certificate is not in place, or the encryption mode is not Full |

```
for h in tehas.app lab.tehas.app uat-1.tehas.app nothing.tehas.app; do
  printf '%-20s ' "$h"; curl -s -o /dev/null -w '%{http_code}\n' "https://$h"
done
```

## On the host

### 7. The settings

In the instance's `instance.env`:

```
TEHAS_DOMAIN=tehas.app
TEHAS_PREVIEWS=1
```

Where the file is depends on the instance directory:

- Inside the platform checkout, as for tehas-tehas, whose settings are
  `instances/tehas-tehas/` in the repository: commit the two lines to the
  repository. They arrive on the host with `tehas update` in step 8. Do not
  edit the file on the host. `tehas update` changes nothing while the
  checkout has local changes: it stops with exit 1 and
  `has local changes; nothing was changed`.
- Anywhere else: edit the file in place on the host.

`TEHAS_PREVIEW_MAX` and `TEHAS_PREVIEW_MIN_FREE` have defaults; see
[Commands](/tools). `TEHAS_PREVIEWS=1` without a domain stops every command
that reads the settings with exit 78.

`TEHAS_DOMAIN` by itself is enough for the Protolab's names: each ticket is
then at `lab-<issue>.tehas.app`, and the Protolab's address becomes
`https://lab.tehas.app` unless `TEHAS_PROTOLAB_URL` says otherwise.

### 8. Install and start the router

```
tehas update
. ~/tehas/env.sh
docker compose -f ~/tehas/edge/router.compose.yaml up -d
curl -i -H 'Host: uat-1.tehas.app' http://tehas-tehas:8081/
```

`tehas update` pulls the platform checkout and runs `install.sh`, which
installs the router's files and the units and makes the Docker network
`tehas-previews`. What it does about the router depends on the host:

- A host that ran the Protolab's server under its old name, the compose
  project `tehas-protolab`, has that container on port 8081. `tehas update`
  takes it down and starts the router in its place.
- A host that never ran it gets nothing started. The `docker compose` line
  above starts the router. Where the router already runs, the line changes
  nothing.

The last line answers 404 with "No preview is running at this address". From
outside, `uat-1.tehas.app` now answers 404 where it answered 502.

### 9. Start the timers

```
systemctl --user enable --now tehas-protolab.timer tehas-preview.timer tehas-preview-sweep.timer
```

| Timer | Every | What it does |
| --- | --- | --- |
| `tehas-protolab.timer` | 5 minutes | Builds the Protolab's pages |
| `tehas-preview.timer` | 5 minutes | Builds a preview for a new commit, removes the previews of merged and closed pull requests |
| `tehas-preview-sweep.timer` | 3 hours | Checks the host for anything a closed pull request left, and removes it |

Each interval is counted from the end of the run before. The timers are
enabled by the operator, here. `tehas update` restarts a unit only where it is
already active, so on a host that was installed before previews existed the
two new timers are installed and do not run until this line is given.

### 10. A preview, end to end

The product needs a `preview/Dockerfile`: see [A new project](/new-project).
Push a branch to the product repository and open a pull request. At the next
run, which is at most five minutes away, the preview is built. Once the build
is done:

- the bot comments on the pull request with `https://uat-<pr>.tehas.app/`,
- the dashboard lists it under Previews,
- `tail ~/tehas/logs/preview.log` has a line `#<pr>: serving`.

Merge or close it, and at the next run the log has
`#<pr>: pull request no longer open, removed`, and the name answers 404.

When a preview is not there, the dashboard says why. The pull request's
comment says so for a failed build and for a full disk. See
[Troubleshooting](/troubleshooting).

## What a preview is given, and what it is not

A preview runs the pull request's own files on the factory's host. It is
given nothing of the host: no directory, no setting, no token, no port, a
read-only root, 256 MB and one CPU. It is on a Docker network with the router
and the other previews and nothing else, so it cannot reach the internet, the
model host or the dashboard.

That network, `tehas-previews`, has to be an internal one. `tehasbot preview`
checks it at the start of every run. If it is not internal the run stops
before it builds or removes anything, and the log says
`the network tehas-previews is not internal; nothing was built`.

Only a pull request whose branch is in the product repository gets one.
Pushing there needs write access. A pull request from a fork never gets one.

## Limits

- A preview is public. A pull request opened from a branch of the repository
  is on the internet at the next run, with nobody looking at it first.
- The build runs the pull request's `preview/Dockerfile` on the host's Docker,
  with network access. That is why forks are left out.
- `uat-<pr>.tehas.app` and `tehas.app` are one site to a browser. A preview's
  script can set a cookie for the domain. Nothing on the dashboard logs in, so
  this is accepted today.
- The origin certificate is trusted by Cloudflare only. A browser sent
  straight to the proxy's machine for these names is warned.
- Not checked: the origin certificate covers `tehas.app` too, so Caddy on the
  proxy may serve it for the dashboard's own names in place of the
  certificate it got for them. Behind Cloudflare nothing changes.
  `journalctl -u caddy | grep 'skipping automatic certificate management'` on
  the proxy shows whether it does.
- Under `TEHAS_PREVIEW_MIN_FREE` of free disk no preview is built. Docker's
  build cache is not pruned by tehas.
- A full disk does not stop a cycle. Only previews hold back.
- The commits are fetched into a bare clone, `~/tehas/preview/repo.git`. It
  takes the address of its origin from the product checkout, as written
  there. A checkout whose address carries a credential would have that
  credential written to a second file. Keep credentials in a credential
  helper, not in the address.
- The newest 100 open pull requests are looked at, and the first 100 comments
  of a pull request are read to find the bot's own. When more than 100
  comments come before it, the bot's comment is not found, and a new one is
  written at every change instead of the old one being edited.
