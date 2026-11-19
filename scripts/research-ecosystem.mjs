// Read-only GitHub research. Uses gh authentication without exporting credentials.
// npm run research -- [output.json]; review snapshots before changing product code.
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import fs from 'node:fs';
import path from 'node:path';
const exec = promisify(execFile);
const repositories = [
  'librespeed/speedtest', 'librespeed/speedtest-cli', 'librespeed/speedtest-go',
  'sivel/speedtest-cli', 'showwin/speedtest-go', 'm-lab/ndt-server', 'm-lab/ndt7-client-go',
  'cloudflare/speedtest', 'openspeedtest/Speed-Test', 'henrywhitaker3/Speedtest-Tracker',
  'alexjustesen/speedtest-tracker', 'AdguardTeam/AdGuardHome', 'Zoxc/crusader', 'esnet/iperf',
  'DNSCrypt/dnscrypt-proxy', 'traviscross/mtr', 'fujiapple852/trippy', 'nmap/nmap',
  'raboof/nethogs', 'rolandriegel/nload', 'imsnif/bandwhich', 'ameshkov/dnslookup',
  'tgraf/bmon', 'aristocratos/btop', 'nicolargo/glances', 'netdata/netdata',
  'prometheus/blackbox_exporter', 'prometheus/node_exporter', 'Tantalor93/dnspyre',
  'NLnetLabs/unbound', 'tauri-apps/tauri', 'tauri-apps/plugins-workspace',
];
const results = [];
let cursor = 0;
async function worker() {
  while (cursor < repositories.length) {
    const name = repositories[cursor++];
    const [owner, repo] = name.split('/');
    const query = `query { repository(owner:${JSON.stringify(owner)}, name:${JSON.stringify(repo)}) {
      nameWithOwner url description stargazerCount forkCount pushedAt isArchived
      licenseInfo { spdxId } primaryLanguage { name }
      defaultBranchRef { name target { ... on Commit { oid committedDate
        history(first:100) { nodes { committedDate messageHeadline } pageInfo { hasNextPage } }
      } } }
      releases(first:3,orderBy:{field:CREATED_AT,direction:DESC}) { nodes { tagName publishedAt url isPrerelease } }
      issues(first:3,orderBy:{field:UPDATED_AT,direction:DESC},states:OPEN) { nodes { title url updatedAt } }
      readme: object(expression:"HEAD:README.md") { ... on Blob { text } }
      readmeRst: object(expression:"HEAD:README.rst") { ... on Blob { text } }
      tree: object(expression:"HEAD:") { ... on Tree { entries { name type } } }
      source: object(expression:"HEAD:src") { ... on Tree { entries { name type } } }
    } }`;
    try {
      const { stdout } = await exec('gh', ['api', 'graphql', '-f', `query=${query}`], { maxBuffer: 3_000_000 });
      const response = JSON.parse(stdout);
      if (!response.data?.repository) throw new Error(JSON.stringify(response.errors));
      const r = response.data.repository;
      const readme = r.readme?.text || r.readmeRst?.text || '';
      delete r.readme; delete r.readmeRst;
      r.readmeInspected = !!readme;
      // Keep structure and selected feature keywords, not copies of upstream prose.
      r.readmeSections = readme.split('\n').filter(l => /^#{1,3} /.test(l)).slice(0, 16);
      r.signals = ['Android','iOS','Windows','Linux','macOS','Docker','JSON','CSV','latency','jitter','packet loss','Prometheus','license','update']
        .filter(word => readme.toLowerCase().includes(word.toLowerCase()));
      const commits = r.defaultBranchRef?.target?.history;
      r.activity = Object.fromEntries([30,90,180].map(days => [days, {
        observedCommits: commits?.nodes.filter(c => new Date(c.committedDate) >= new Date(Date.now()-days*86400000)).length || 0,
        cappedAt100: !!commits?.pageInfo.hasNextPage,
      }]));
      results.push(r);
      console.log(`Inspected ${name}`);
    } catch (error) { results.push({ nameWithOwner:name, error:String(error) }); console.error(`Unavailable ${name}`); }
  }
}
await Promise.all(Array.from({length:4}, worker));
const destination = process.argv[2] || 'docs/research/ecosystem-snapshot.json';
fs.mkdirSync(path.dirname(destination), {recursive:true});
fs.writeFileSync(destination, JSON.stringify({ observedAt: new Date().toISOString(), method:'GitHub GraphQL: README, root/src trees, last 100 commits, releases, open issues; no upstream code copied', repositories:results.sort((a,b)=>a.nameWithOwner.localeCompare(b.nameWithOwner)) }, null, 2)+'\n');
console.log(`${results.filter(r=>!r.error).length} repositories inspected; saved ${destination}`);
