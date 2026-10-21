/* Existing feedback responsibility behind injected platform ports; no eager IO. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoFeedbackRemote = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';
  function create(ports) {
    var FEEDBACK_WORKER_URL = 'https://nemo-feedback.mysteropodes-auth.workers.dev';
    async function workerPost(path, payload) {
      var resp = await ports.workerFetch()(FEEDBACK_WORKER_URL + path, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });
      var data = null;
      try { data = await resp.json(); } catch (e) {}
      if (!resp.ok) throw new Error((data && data.error) || ('feedback worker error ' + resp.status));
      return data;
    }

    function fbTagLabelPlain(tag) {
      return { bug: 'bug', perf: 'perf', idee: 'idée', polish: 'polish' }[tag] || tag;
    }
    // Commits each screenshot into strokemotion-feedback's attachments/
    // folder via the Worker's /attachment endpoint (GitHub Contents API
    // under the hood) and returns an array of raw.githubusercontent.com
    // URLs that render inline in the issue body via normal Markdown image
    // syntax — GFM does NOT render data: URIs, so each file has to actually
    // land in the repo, not just be inlined as base64 in the issue text.
    //
    // feedback #208: was one screenshot per entry (uploadScreenshotIfAny,
    // singular) — now uploads however many were attached, sequentially (not
    // Promise.all: the Worker writes via the GitHub Contents API, which
    // 409s on two concurrent commits to the same repo — a real risk once
    // this is more than one file per submission). filename includes an
    // index (entry.id + '-' + i) so multiple images from the SAME entry
    // never collide on the single-screenshot naming the old code used
    // unconditionally. Compat: an older stored entry may only have the
    // singular screenshotDataUrl field.
    async function uploadScreenshotsIfAny(entry) {
      var dataUrls = entry.screenshotDataUrls && entry.screenshotDataUrls.length ? entry.screenshotDataUrls
        : (entry.screenshotDataUrl ? [entry.screenshotDataUrl] : []);
      var urls = [];
      for (var i = 0; i < dataUrls.length; i++) {
        var m = /^data:image\/(\w+);base64,(.+)$/.exec(dataUrls[i]);
        if (!m) continue;
        var ext = m[1] === 'jpeg' ? 'jpg' : m[1];
        var filename = entry.id + (dataUrls.length > 1 ? '-' + (i + 1) : '') + '.' + ext;
        var data = await workerPost('/attachment', { filename: filename, contentBase64: m[2] });
        if (data && data.url) urls.push(data.url);
      }
      return urls;
    }
    async function publishToGitHubIssue(entry) {
      var title = (entry.blocking ? '[BLOQUANT] ' : '') + (entry.note || '(sans titre)').slice(0, 80);
      var labels = ['pending'].concat((entry.tags || []).map(fbTagLabelPlain));
      if (entry.blocking) labels.push('blocking');
      var shotUrls = [];
      try { shotUrls = await uploadScreenshotsIfAny(entry); } catch (e) { ports.warn('[feedback] screenshot upload failed', e); }
      // One heading, one image per URL — GFM renders each ![]() on its own
      // line as a separate inline image, so N screenshots just means N lines
      // here instead of the single line the old code always produced.
      var shotsBlock = shotUrls.length
        ? ('**Capture' + (shotUrls.length > 1 ? 's d\'écran' : ' d\'écran') + '**\n'
          + shotUrls.map(function (u, i) { return '![capture ' + (i + 1) + '](' + u + ')'; }).join('\n') + '\n')
        : '';
      var body = [
        '**Note**', entry.note || '(vide)', '',
        shotsBlock,
        '**Contexte**',
        '- Frame: ' + entry.frame,
        '- Auteur: ' + (entry.author ? entry.author.name + ' (' + entry.author.role + ')' : 'inconnu'),
        (entry.author && entry.author.email) ? ('- Email: ' + entry.author.email) : '',
        '- Projet: ' + entry.projectKey,
        '- Enregistré précisément: ' + (entry.recorded ? 'oui' : 'non (dernières actions)'),
        '',
        // Feedback #115 ("des infos non intrusive pour le debugguage et des
        // rapports de connexion simple") — only values already sitting in
        // the page (navigator/screen/ports.title(), the same source
        // updater-bridge.js's showVersion() keeps in sync — CLAUDE.md §7's
        // single-source-of-truth version display, so no new version-tracking
        // needed here). No IP, no geolocation, no analytics SDK — this is
        // the same privacy profile the click/action trail below already has.
        '**Infos techniques**',
        '- Plateforme: ' + (ports.available() ? 'App bureau' : 'Navigateur web'),
        '- Version: ' + (ports.title().match(/v[\d.]+/) || ['inconnue'])[0],
        '- Navigateur: ' + ports.userAgent(),
        '- Fenêtre: ' + ports.width() + '×' + ports.height() + ' (écran ' + ports.screenWidth() + '×' + ports.screenHeight() + ')',
        '- Connexion: ' + (ports.onLine() ? 'en ligne' : 'hors ligne'),
        '',
        '**Trail d\'actions**',
        '```json', JSON.stringify(entry.actionTrail || [], null, 2), '```',
        '',
        '**Trail de clics**',
        '```json', JSON.stringify(entry.clickTrail || [], null, 2), '```',
        '',
        '<!-- sm-feedback-id: ' + entry.id + ' -->',
      ].join('\n');
      await workerPost('/issue', { title: title, body: body, labels: labels });
    }

    // ---- Dev-side triage over the GitHub feedback repo (Cyril only — a
    // beta tester's shipped copy of this app has these functions too, since
    // it's all one codebase, but they're useless to them without CYRIL'S OWN
    // GitHub token, which is never embedded/shipped — see setGithubTriageToken
    // below). Reading issues needs no auth at all (public repo); only
    // labeling/closing/commenting needs Cyril's token, entered once in
    // Réglages → Feedback and kept in localStorage on his own machine only. ----
    var GH_REPO = 'mysteropodes/nemo';
    var GH_TOKEN_KEY = 'nemo-github-triage-token';
    function githubTriageToken() { try { return ports.storage().getItem(GH_TOKEN_KEY) || ''; } catch (e) { return ''; } }
    function setGithubTriageToken(token) { try { ports.storage().setItem(GH_TOKEN_KEY, token || ''); } catch (e) {} }

    // Uses the Search API rather than /issues, because GH_REPO is now the code
    // repo: /issues returns pull requests too, and with 700+ PRs the newest 100
    // items would be almost entirely PRs, so the real reports would fall off the
    // end and simply not show up. `is:issue` excludes them server-side, so all
    // 100 slots hold actual feedback. The pull_request filter below is kept as a
    // belt-and-braces guard.
    async function fetchGithubIssues() {
      var q = encodeURIComponent('repo:' + GH_REPO + ' is:issue');
      var res = await ports.fetch('https://api.github.com/search/issues?q=' + q + '&per_page=100&sort=created&order=desc', {
        headers: { 'Accept': 'application/vnd.github+json' },
      });
      if (!res.ok) throw new Error('GitHub list failed: ' + res.status);
      var payload = await res.json();
      var issues = payload.items || [];
      return issues.filter(function (i) { return !i.pull_request; }).map(function (i) {
        var m = /sm-feedback-id:\s*(\S+)\s*-->/.exec(i.body || '');
        return {
          number: i.number, id: m ? m[1] : ('gh_' + i.number), nodeId: i.node_id,
          title: i.title, body: i.body, url: i.html_url,
          labels: (i.labels || []).map(function (l) { return l.name; }),
          state: i.state, createdAt: i.created_at,
        };
      });
    }
    function ghAuthHeaders() {
      var token = githubTriageToken();
      if (!token) throw new Error('Aucun token GitHub configuré (Réglages → Feedback)');
      return { 'Authorization': 'Bearer ' + token, 'Accept': 'application/vnd.github+json', 'Content-Type': 'application/json' };
    }
    async function patchGithubIssue(number, payload) {
      var res = await ports.fetch('https://api.github.com/repos/' + GH_REPO + '/issues/' + number, {
        method: 'PATCH', headers: ghAuthHeaders(), body: JSON.stringify(payload),
      });
      if (!res.ok) throw new Error('GitHub update failed: ' + res.status + ' ' + await res.text());
      return res.json();
    }
    async function approveGithubIssue(number, currentLabels) {
      var labels = (currentLabels || []).filter(function (l) { return l !== 'pending'; });
      return patchGithubIssue(number, { labels: labels });
    }
    async function resolveGithubIssue(number, currentLabels, resolutionText) {
      if (resolutionText) {
        await ports.fetch('https://api.github.com/repos/' + GH_REPO + '/issues/' + number + '/comments', {
          method: 'POST', headers: ghAuthHeaders(), body: JSON.stringify({ body: resolutionText }),
        });
      }
      var labels = (currentLabels || []).filter(function (l) { return l !== 'pending'; });
      if (labels.indexOf('resolved') < 0) labels.push('resolved');
      return patchGithubIssue(number, { labels: labels, state: 'closed' });
    }
    async function editGithubIssueBody(number, newBody) {
      return patchGithubIssue(number, { body: newBody });
    }
    // Actual deletion (not just close) — REST has no DELETE for issues, this
    // needs the GraphQL deleteIssue mutation + the issue's node_id (captured
    // by fetchGithubIssues above). Requires the token's user to have admin
    // rights on the repo (verified: works for the repo owner even with a
    // fine-grained PAT scoped to just "Issues" — the permission check is on
    // the acting user's role, not the token's own granularity).
    async function deleteGithubIssue(nodeId) {
      var res = await ports.fetch('https://api.github.com/graphql', {
        method: 'POST', headers: ghAuthHeaders(),
        body: JSON.stringify({ query: 'mutation($id:ID!){deleteIssue(input:{issueId:$id}){clientMutationId}}', variables: { id: nodeId } }),
      });
      var json = await res.json();
      if (!res.ok || json.errors) throw new Error('GitHub delete failed: ' + (json.errors ? json.errors.map(function (e) { return e.message; }).join(', ') : res.status));
      return true;
    }

    return Object.freeze({
      publishToGitHubIssue: publishToGitHubIssue,
      githubTriageToken: githubTriageToken,
      setGithubTriageToken: setGithubTriageToken,
      fetchGithubIssues: fetchGithubIssues,
      approveGithubIssue: approveGithubIssue,
      resolveGithubIssue: resolveGithubIssue,
      editGithubIssueBody: editGithubIssueBody,
      deleteGithubIssue: deleteGithubIssue
    });
  }
  return Object.freeze({ create: create });
}));
