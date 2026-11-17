### Changes

- `install.sh` installs outsource's model proxy (`libexec/tehas-proxy`), its unit `tehas-outsource-proxy.service` and the contractor program (`libexec/tehas-contractor`) only while `TEHAS_OUTSOURCE` is on. With it off nothing of the feature is installed. `scripts/pack-release.sh` takes the two programs with `--proxy` and `--contractor`. Outsource is still being built, so a release does neither (#2278).
- The manual's outsource page says how to switch the feature on, lists all its settings, says where a cycle runs, and has a Known limits section: trusted hosts only (#2278).
