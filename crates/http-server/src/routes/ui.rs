use axum::response::Html;

pub async fn models_page() -> Html<&'static str> {
    Html(
        r#"<!doctype html>
<html>
<head>
  <meta charset=\"utf-8\" />
  <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\" />
  <title>MAI Model Manager</title>
  <style>
    body { font-family: -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif; margin: 24px; color: #202327; }
    .grid { display: grid; gap: 12px; max-width: 980px; }
    label { font-size: 13px; color: #445; display: block; margin-bottom: 4px; }
    input, select, button, textarea { font-size: 14px; padding: 8px; border-radius: 8px; border: 1px solid #c9d0da; }
    textarea { min-height: 160px; width: 100%; box-sizing: border-box; }
    .row { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
    .ok { color: #146c2e; }
    .err { color: #9d1a1a; }
    .muted { color: #66727f; font-size: 12px; }
    .card { border: 1px solid #d5dbe3; border-radius: 12px; padding: 12px; background: #fff; }
    .status-pill { display: inline-block; padding: 3px 8px; border-radius: 999px; font-size: 12px; text-transform: uppercase; letter-spacing: 0.03em; }
    .status-queued { background: #e9eef5; color: #2b3b4f; }
    .status-running { background: #dff0ff; color: #0d3e7c; }
    .status-succeeded { background: #e4f6e7; color: #16602b; }
    .status-failed { background: #fde8e8; color: #8f1f1f; }
    .downloads { border-collapse: collapse; width: 100%; font-size: 13px; }
    .downloads th, .downloads td { border-bottom: 1px solid #edf0f3; padding: 8px; text-align: left; vertical-align: middle; }
    .progress { width: 180px; height: 9px; background: #edf1f5; border-radius: 999px; overflow: hidden; }
    .progress > span { display: block; height: 100%; background: linear-gradient(90deg, #4a92ff, #2f6fe0); }
    .telemetry { display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr)); gap: 8px; }
    .telemetry .metric { border: 1px solid #e2e7ee; border-radius: 10px; padding: 8px; background: #f9fbfd; }
    .telemetry .label { font-size: 11px; color: #637082; text-transform: uppercase; letter-spacing: 0.04em; }
    .telemetry .value { font-size: 18px; font-weight: 600; }
  </style>
</head>
<body>
  <h1>MAI Model Manager</h1>
  <p>Background downloads with progress telemetry and retry/resume recovery.</p>

  <div class=\"grid\">
    <div class=\"card\">
      <div class=\"row\">
        <div>
          <label>Model ID</label>
          <input id=\"id\" placeholder=\"phi-3-mini-q4\" />
        </div>
        <div>
          <label>Name</label>
          <input id=\"name\" placeholder=\"Phi 3 Mini Q4\" />
        </div>
      </div>

      <div class=\"row\" style=\"margin-top:10px;\">
        <div>
          <label>Quant</label>
          <select id=\"quant\">
            <option>Q4KM</option>
            <option>Q3KS</option>
            <option>Q5KM</option>
            <option>Q6K</option>
          </select>
        </div>
        <div>
          <label>Destination Path</label>
          <input id=\"dest\" placeholder=\"/tmp/model.gguf\" />
        </div>
      </div>

      <div style=\"margin-top:10px;\">
        <label>Source URL (HTTP/S)</label>
        <input id=\"url\" placeholder=\"https://example.com/model.gguf\" />
      </div>

      <div style=\"margin-top:10px;\">
        <label>Source Path (local fallback)</label>
        <input id=\"path\" placeholder=\"/tmp/source-model.gguf\" />
      </div>

      <div style=\"margin-top:12px;\">
        <button onclick=\"startDownload()\">Start Download</button>
        <span id=\"status\" class=\"muted\" style=\"margin-left:10px;\"></span>
      </div>
    </div>

    <div class=\"card\">
      <h3 style=\"margin-top:0;\">Download Telemetry</h3>
      <div id=\"telemetry\" class=\"telemetry\"></div>
      <div id=\"telemetry-note\" class=\"muted\" style=\"margin-top:8px;\"></div>
    </div>

    <div class=\"card\">
      <h3 style=\"margin-top:0;\">Download Jobs</h3>
      <div style=\"margin-bottom:8px;\">
        <button onclick=\"refreshDownloads()\">Refresh Jobs</button>
      </div>
      <table class=\"downloads\">
        <thead>
          <tr>
            <th>Job</th>
            <th>Model</th>
            <th>Status</th>
            <th>Progress</th>
            <th>Transferred</th>
            <th>Retries</th>
            <th>Action</th>
          </tr>
        </thead>
        <tbody id=\"downloads\"></tbody>
      </table>
    </div>

    <div class=\"card\">
      <h3 style=\"margin-top:0;\">Catalog</h3>
      <button onclick=\"refreshCatalog()\">Refresh Catalog</button>
      <textarea id=\"catalog\" readonly></textarea>
    </div>

    <div class=\"card\">
      <h3 style=\"margin-top:0;\">Registered Models</h3>
      <button onclick=\"refreshModels()\">Refresh Models</button>
      <textarea id=\"models\" readonly></textarea>
    </div>
  </div>

  <script>
    function formatBytes(bytes) {
      if (bytes == null || Number.isNaN(bytes)) return '-';
      const gb = 1024 * 1024 * 1024;
      const mb = 1024 * 1024;
      if (bytes >= gb) return (bytes / gb).toFixed(2) + ' GB';
      if (bytes >= mb) return (bytes / mb).toFixed(1) + ' MB';
      return bytes + ' B';
    }

    function metricFromProm(text, name) {
      const re = new RegExp('^' + name + '\\\\s+([0-9.]+)$', 'm');
      const match = text.match(re);
      return match ? Number(match[1]) : null;
    }

    function badge(status) {
      return `<span class="status-pill status-${status}">${status}</span>`;
    }

    async function refreshCatalog() {
      const res = await fetch('/v1/models/catalog');
      const data = await res.json();
      document.getElementById('catalog').value = JSON.stringify(data, null, 2);
    }

    async function refreshModels() {
      const res = await fetch('/v1/models');
      const data = await res.json();
      document.getElementById('models').value = JSON.stringify(data, null, 2);
    }

    function renderDownloads(jobs) {
      const tbody = document.getElementById('downloads');
      if (!jobs.length) {
        tbody.innerHTML = '<tr><td colspan="7" class="muted">No downloads yet.</td></tr>';
        return;
      }

      tbody.innerHTML = jobs.map(job => {
        const pct = job.progress_pct == null ? 0 : Math.max(0, Math.min(100, job.progress_pct));
        const transferred = (job.resumed_from_bytes || 0) + (job.downloaded_bytes || 0);
        const total = job.total_bytes || null;
        const transferText = total ? `${formatBytes(transferred)} / ${formatBytes(total)}` : formatBytes(transferred);
        const progressText = job.status === 'succeeded' ? '100%' : `${pct.toFixed(1)}%`;
        const action = job.status === 'failed'
          ? `<button onclick="retryDownload('${job.job_id}')">Retry</button>`
          : '<span class="muted">-</span>';
        const errorLine = job.error ? `<div class="err muted">${job.error}</div>` : '';
        return `
          <tr>
            <td><code>${job.job_id}</code></td>
            <td><strong>${job.model_id}</strong><div class="muted">${job.destination_path}</div>${errorLine}</td>
            <td>${badge(job.status)}</td>
            <td>
              <div class="progress"><span style="width:${pct}%;"></span></div>
              <div class="muted">${progressText}</div>
            </td>
            <td>${transferText}</td>
            <td>${job.retries || 0}</td>
            <td>${action}</td>
          </tr>
        `;
      }).join('');
    }

    async function refreshDownloads() {
      const res = await fetch('/v1/models/downloads');
      const data = await res.json();
      renderDownloads(data.data || []);
      await refreshModels();
    }

    async function refreshTelemetry() {
      const container = document.getElementById('telemetry');
      const note = document.getElementById('telemetry-note');

      try {
        const res = await fetch('/metrics');
        if (!res.ok) {
          note.textContent = 'Metrics endpoint unavailable (auth may be required).';
          return;
        }
        const text = await res.text();
        const metrics = {
          started: metricFromProm(text, 'mai_downloads_started_total'),
          completed: metricFromProm(text, 'mai_downloads_completed_total'),
          failed: metricFromProm(text, 'mai_downloads_failed_total'),
          active: metricFromProm(text, 'mai_downloads_active'),
          bytes: metricFromProm(text, 'mai_download_bytes_total'),
        };

        container.innerHTML = `
          <div class="metric"><div class="label">Started</div><div class="value">${metrics.started ?? '-'}</div></div>
          <div class="metric"><div class="label">Completed</div><div class="value">${metrics.completed ?? '-'}</div></div>
          <div class="metric"><div class="label">Failed</div><div class="value">${metrics.failed ?? '-'}</div></div>
          <div class="metric"><div class="label">Active</div><div class="value">${metrics.active ?? '-'}</div></div>
          <div class="metric"><div class="label">Downloaded</div><div class="value">${formatBytes(metrics.bytes ?? 0)}</div></div>
        `;
        note.textContent = '';
      } catch (_err) {
        note.textContent = 'Failed to load telemetry.';
      }
    }

    async function startDownload() {
      const payload = {
        id: document.getElementById('id').value,
        name: document.getElementById('name').value,
        quant: document.getElementById('quant').value,
        destination_path: document.getElementById('dest').value,
        source_url: document.getElementById('url').value || null,
        source_path: document.getElementById('path').value || null,
      };

      const status = document.getElementById('status');
      status.className = 'muted';
      status.textContent = 'Scheduling download...';

      const res = await fetch('/v1/models/download', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });
      const body = await res.json();

      if (res.ok) {
        status.className = 'ok';
        status.textContent = `Scheduled ${body.job_id}. Live progress below.`;
        await refreshDownloads();
      } else {
        status.className = 'err';
        status.textContent = body.error || 'Failed to schedule download';
      }
    }

    async function retryDownload(jobId) {
      const status = document.getElementById('status');
      status.className = 'muted';
      status.textContent = `Retrying ${jobId}...`;

      const res = await fetch(`/v1/models/downloads/${encodeURIComponent(jobId)}/retry`, {
        method: 'POST',
      });
      const body = await res.json();
      if (res.ok) {
        status.className = 'ok';
        status.textContent = `Retry scheduled as ${body.job.job_id}`;
        await refreshDownloads();
      } else {
        status.className = 'err';
        status.textContent = body.error || 'Retry failed';
      }
    }

    async function tick() {
      await Promise.all([refreshDownloads(), refreshTelemetry()]);
    }

    refreshCatalog();
    refreshModels();
    tick();
    setInterval(tick, 1000);
  </script>
</body>
</html>"#,
    )
}
