<h1>Tehas dashboard</h1>
<tehas-factory-state data-tehas-island="contents" src="/live/status.json" running="/live/running.json">
<div class="dash-top">
  <span id="state" class="pill idle">loading</span>
  <span id="allhands" class="pill pause" title="tehas:all-hands is on the control issue" hidden>all hands</span>
  <span id="readonly" class="pill pause" title="TEHAS_READ_ONLY=1: this factory writes nothing to GitHub" hidden>read-only</span>
  <span id="fresh" class="muted"></span>
  <a id="all" class="muted" href="#">all issues on GitHub</a>
</div>
</tehas-factory-state>
<tehas-factory-wikilink data-tehas-island="contents" src="/live/wiki.json"><a id="wiki" class="muted" hidden>wiki</a></tehas-factory-wikilink>

<tehas-factory-console data-tehas-island="contents" src="/live/running.json" status="/live/status.json">
<section class="now" id="now" hidden></section>
</tehas-factory-console>

<div class="cols">
  <tehas-factory-issues data-tehas-island="contents" src="/live/status.json" list="queued"><section><h2>In line <span class="n" id="n-queued"></span></h2><ul class="issues" id="queued"></ul></section></tehas-factory-issues>
  <tehas-factory-issues data-tehas-island="contents" src="/live/status.json" list="awaiting"><section><h2>Waiting for your review <span class="n" id="n-awaiting"></span></h2><ul class="issues" id="awaiting"></ul></section></tehas-factory-issues>
  <tehas-factory-issues data-tehas-island="contents" src="/live/status.json" list="stopped"><section><h2>Stopped, needs attention <span class="n" id="n-stopped"></span></h2><ul class="issues" id="stopped"></ul></section></tehas-factory-issues>
  <tehas-factory-issues data-tehas-island="contents" src="/live/status.json" list="completed"><section><h2>Completed <span class="n" id="n-completed"></span></h2><ul class="issues" id="completed"></ul></section></tehas-factory-issues>
</div>

<tehas-factory-readonly data-tehas-island="contents" src="/live/status.json">
<section id="ro" hidden></section>
</tehas-factory-readonly>

<tehas-factory-days data-tehas-island="contents" src="/live/analytics.json">
<section id="analytics" hidden></section>
</tehas-factory-days>

<tehas-factory-previews data-tehas-island="contents" src="/live/previews.json">
<section id="previews" hidden></section>
</tehas-factory-previews>

<tehas-factory-host data-tehas-island="contents" src="/live/top.json">
<section id="host" hidden></section>
</tehas-factory-host>

<link rel="stylesheet" href="/fun/kit/kit.css">
<link rel="stylesheet" href="/fun/factory/factory.css">
<script src="/fun/host-bridge.js"></script>
<script type="module" src="/fun/factory/tehas-factory-state.js"></script>
<script type="module" src="/fun/factory/tehas-factory-console.js"></script>
<script type="module" src="/fun/factory/tehas-factory-issues.js"></script>
<script type="module" src="/fun/factory/tehas-factory-readonly.js"></script>
<script type="module" src="/fun/factory/tehas-factory-days.js"></script>
<script type="module" src="/fun/factory/tehas-factory-previews.js"></script>
<script type="module" src="/fun/factory/tehas-factory-host.js"></script>
<script type="module" src="/fun/factory/tehas-factory-wikilink.js"></script>
