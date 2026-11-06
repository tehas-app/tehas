<h1>Tehas dashboard</h1>
<div class="dash-top">
  <span id="state" class="pill idle">loading</span>
  <span id="allhands" class="pill pause" title="tehas:all-hands is on the control issue" hidden>all hands</span>
  <span id="readonly" class="pill pause" title="TEHAS_READ_ONLY=1: this factory writes nothing to GitHub" hidden>read-only</span>
  <span id="fresh" class="muted"></span>
  <a id="all" class="muted" href="#">all issues on GitHub</a>
</div>

<section class="now" id="now" hidden>
  <div class="console-tabs" id="tabs" role="tablist" aria-label="Cycles" hidden></div>
  <h2 id="now-title"></h2>
  <div class="step" id="now-step"></div>
  <pre id="console" role="tabpanel" aria-label="Console of the selected cycle" tabindex="0"></pre>
  <div class="console-bar">
    <label><input type="checkbox" id="follow" checked> follow the output</label>
    <span class="muted" id="console-note"></span>
  </div>
</section>

<div class="cols">
  <section><h2>In line <span class="n" id="n-queued"></span></h2><ul class="issues" id="queued"></ul></section>
  <section><h2>Waiting for your review <span class="n" id="n-awaiting"></span></h2><ul class="issues" id="awaiting"></ul></section>
  <section><h2>Stopped, needs attention <span class="n" id="n-stopped"></span></h2><ul class="issues" id="stopped"></ul></section>
  <section><h2>Completed <span class="n" id="n-completed"></span></h2><ul class="issues" id="completed"></ul></section>
</div>

<section id="ro" hidden>
  <h2>Read-only results <span class="n" id="n-ro"></span></h2>
  <p class="muted">This factory writes nothing to GitHub. Each cycle's branch stays in its workspace on the host, with the pull request it would have opened.</p>
  <ul class="issues" id="ro-list"></ul>
</section>

<section id="analytics" hidden>
  <h2>Last 10 days</h2>
  <p class="muted" id="analytics-note"></p>
  <div class="an-figures" id="an-figures"></div>
  <h3>Per day</h3>
  <div class="an-chart" id="an-chart"></div>
  <p class="muted an-legend"><span class="an-key shipped"></span> shipped <span class="an-key failed"></span> failed runs <span class="an-key minutes"></span> cycle-minutes</p>
  <div class="cols">
    <section><h3>Top stop reasons</h3><p class="muted" id="an-reasons-note"></p><ul class="issues" id="an-reasons"></ul></section>
    <section><h3>Station yield</h3><p class="muted" id="an-steps-note"></p><ul class="issues" id="an-steps"></ul></section>
  </div>
  <h3>Versions</h3>
  <p class="muted" id="an-versions-note"></p>
  <div class="tablewrap"><table>
    <thead><tr><th>tehas</th><th>pi-rukas</th><th>runs</th><th>to a PR</th><th>median time</th><th>median tokens</th></tr></thead>
    <tbody id="an-versions"></tbody>
  </table></div>
</section>

<section id="previews" hidden>
  <h2>Previews <span class="n" id="n-previews"></span></h2>
  <p class="muted" id="previews-note"></p>
  <ul class="issues" id="preview-list"></ul>
</section>

<section id="host" hidden>
  <h2>Host <span class="n" id="n-host"></span></h2>
  <p class="muted" id="host-note"></p>
  <ul class="issues" id="host-list"></ul>
  <h3>Units <span class="n" id="n-units"></span></h3>
  <ul class="issues" id="unit-list"></ul>
</section>

<script src="/fun/console-tabs.js"></script>
<script type="module" src="/fun/dashboard-view.js"></script>
