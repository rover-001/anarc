import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { execSync } from 'node:child_process';

const PORT = 8877;
const BASE_DIR = path.dirname(new URL(import.meta.url).pathname);
const RESULTS_DIR = path.join(BASE_DIR, 'results');
fs.mkdirSync(RESULTS_DIR, { recursive: true });

const peakRssByBrowser = new Map();

function sampleBrowserRam(browser) {
  if (!browser) return { currentRssMb: 0, peakRssMb: 0, processCount: 0 };
  let currentKb = 0;
  let count = 0;
  try {
    const out = execSync('ps -eo pid,ppid,rss,args --no-headers', { encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 });
    const procs = [];
    for (const line of out.split('\n')) {
      const m = line.trim().match(/^(\d+)\s+(\d+)\s+(\d+)\s+(.*)$/);
      if (!m) continue;
      procs.push({
        pid: Number(m[1]),
        ppid: Number(m[2]),
        rss: Number(m[3]),
        args: m[4],
      });
    }

    const roots = new Set();
    if (browser === 'anarc') {
      for (const p of procs) {
        if (
          p.args.includes('/anarc/bin/anarc') &&
          !p.args.includes('anarc-ctl') &&
          !p.args.includes('bench-runner')
        ) {
          roots.add(p.pid);
        }
      }
    } else {
      const pattern = `/tmp/bench-${browser}`;
      for (const p of procs) {
        if (p.args.includes(pattern)) {
          roots.add(p.pid);
        }
      }
    }

    // Walk process tree (parent -> children -> grandchildren)
    const tree = new Set(roots);
    let added = true;
    while (added) {
      added = false;
      for (const p of procs) {
        if (!tree.has(p.pid) && tree.has(p.ppid)) {
          tree.add(p.pid);
          added = true;
        }
      }
    }

    for (const p of procs) {
      if (tree.has(p.pid)) {
        currentKb += p.rss;
        count++;
      }
    }
  } catch {
    // ignore transient ps errors
  }

  const currentRssMb = Math.round((currentKb / 1024) * 10) / 10;
  const prevPeak = peakRssByBrowser.get(browser) || 0;
  const peakRssMb = Math.max(prevPeak, currentRssMb);
  peakRssByBrowser.set(browser, peakRssMb);
  return { currentRssMb, peakRssMb, processCount: count };
}

const server = http.createServer((req, res) => {
  const url = new URL(req.url, `http://127.0.0.1:${PORT}`);
  res.setHeader('Access-Control-Allow-Origin', '*');
  res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
  res.setHeader('Access-Control-Allow-Headers', 'Content-Type');

  if (req.method === 'OPTIONS') {
    res.writeHead(204);
    res.end();
    return;
  }

  if (url.pathname === '/' || url.pathname === '/index.html') {
    const html = fs.readFileSync(path.join(BASE_DIR, 'index.html'));
    res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-store' });
    res.end(html);
    return;
  }

  if (url.pathname === '/bg.html') {
    const html = fs.readFileSync(path.join(BASE_DIR, 'bg.html'));
    res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-store' });
    res.end(html);
    return;
  }

  if (url.pathname === '/api/ram') {
    const browser = url.searchParams.get('browser') || '';
    const data = sampleBrowserRam(browser);
    res.writeHead(200, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' });
    res.end(JSON.stringify(data));
    return;
  }

  if (url.pathname === '/api/reset') {
    const browser = url.searchParams.get('browser') || '';
    if (browser) peakRssByBrowser.set(browser, 0);
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ ok: true }));
    return;
  }

  if (url.pathname === '/api/report' && req.method === 'POST') {
    let body = '';
    req.on('data', (chunk) => { body += chunk; });
    req.on('end', () => {
      try {
        const data = JSON.parse(body);
        const browser = (data.browser || 'unknown').replace(/[^a-zA-Z0-9_-]/g, '');
        const finalRam = sampleBrowserRam(browser);
        data.peakRssMb = Math.max(data.peakRssMb || 0, finalRam.peakRssMb);
        data.processCount = finalRam.processCount;
        fs.writeFileSync(path.join(RESULTS_DIR, `${browser}.json`), JSON.stringify(data, null, 2));
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: true, peakRssMb: data.peakRssMb }));
      } catch (err) {
        res.writeHead(400, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ ok: false, error: String(err) }));
      }
    });
    return;
  }

  res.writeHead(404);
  res.end('Not found');
});

server.listen(PORT, '127.0.0.1', () => {
  console.log(`Benchmark server listening on http://127.0.0.1:${PORT}`);
});
