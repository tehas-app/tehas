### Changes

- The business page leaves core. `/business` is no longer served, and `fun/business.html`, `business-view.js`, `business.css` and `mode.js` are gone. It returns in 2.1 as the plugin `business`.
- The front page `/` is the factory floor in both designs, as `/floor` is. `TEHAS_DESIGN=beesknees26`, the default, draws it in the beesknees26 theme; `TEHAS_DESIGN` keeps both of its values.
- The 17 rules other pages take from `business.css` by selector are in `/fun/pages.css`, with the same text. No page of tehas links it.

### Adopter action

None to keep tehas running: an install removes the old files. `/` now shows the factory floor where it showed the business page, and a link to `/business` answers 404: point it at `/`. A build that takes rules from `business.css` takes them from `pages.css`.
