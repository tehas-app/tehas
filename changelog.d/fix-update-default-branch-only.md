### Changes

- `tehas update` on a host that updates from its checkout
  (`TEHAS_UPDATE_FROM=git`) updates only the default branch of origin. A
  checkout on another branch, or one whose `origin/HEAD` is not set, now
  changes nothing and exits 1. To install another branch, run `./install.sh`
  in the checkout by hand.
- The command printed for root when `/usr/local/bin/tehas` is not current now
  reads `sudo <root-owned copy>/bootstrap.sh dev <user>`. The bootstrap page
  says to run `bootstrap.sh` as root only from a copy root owns, never from
  the tehas user's checkout or `~/tehas/current`.

### Adopter action

On a host with `TEHAS_UPDATE_FROM=git`, check that the platform checkout is on
origin's default branch and that `origin/HEAD` is set:
`git -C ~/tehas/platform remote set-head origin --auto` sets it. Before root
next runs `bootstrap.sh`, make root's own copy as the bootstrap page says
(`sudo git clone <the platform repository> /opt/tehas`) and run it from there.
