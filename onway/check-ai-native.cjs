// Test only against an isolated card-host and an explicitly supplied _debug data root.
const fs = require('fs');
const path = require('path');
const {spawnSync} = require('child_process');
const base = `http://127.0.0.1:${Number(process.env.ONWAY_PORT || 8143)}`;
const root = path.resolve(process.env.ONWAY_TEST_DATA || path.join(__dirname, '_debug/ai-test'));
if (!root.includes(`${path.sep}_debug${path.sep}`)) throw Error('Use isolated _debug test data only');
const data = path.join(root, 'onway');
async function api(route) { const r = await fetch(base + route); if (!r.ok) throw Error(route); return r.json(); }
async function snap() { return (await api('/snap')).s; }
async function tap(text) {
  const w = (await snap()).find(w => w.ty === 'Button' && w.t === text && w.r[2] > 0);
  if (!w) throw Error('Missing button: ' + text);
  return api(`/click?x=${w.r[0]+w.r[2]/2}&y=${w.r[1]+w.r[3]/2}&wait=1`);
}
async function until(check) {
  for (let i = 0; i < 40; i++) { if (await check()) return; await new Promise(r => setTimeout(r, 200)); }
  throw Error('State did not arrive');
}
async function has(text) { return (await snap()).some(w => w.t === text); }
async function scroll(dy) { return api(`/m?k=scroll&x=206&y=360&dy=${dy}&precise=1&wait=1`); }
async function openPage(n) {
  const nav = (await snap()).find(w => w.ty === 'Button' && w.t?.includes('返回导航'));
  if (!nav) throw Error('Enable dev-navigation.txt in isolated test data');
  await api(`/click?x=${nav.r[0]+nav.r[2]/2}&y=${nav.r[1]+nav.r[3]/2}&wait=1`);
  for(let i=0;i<12;i++) {
    const row = (await snap()).find(w => w.ty==='Button' && w.t?.startsWith(String(n).padStart(2,'0')+'  ') && w.r[3]>15);
    if(row) {await tap(row.t); return;}
    await scroll(160);
  }
  throw Error('Missing page '+n);
}
async function scrollTo(text) {
  for(let i=0;i<12;i++) {if(await has(text)) return; await scroll(150);}
  throw Error('Missing setting '+text);
}
async function capture(name) {
  const r = await fetch(base + '/g?raw=1');
  fs.writeFileSync(path.join(root, name), Buffer.from(await r.arrayBuffer()));
}
(async () => {
  await openPage(16);
  await scrollTo('智能通勤 · 已关闭');
  await tap('智能通勤 · 已关闭');
  await capture('ai-settings.png');
  await openPage(12);
  await until(() => {
    const c = JSON.parse(fs.readFileSync(path.join(data, 'ai-context.json'), 'utf8'));
    return c.enabled && c.stage === 'idle';
  });
  // Exercise the real backend worker with a labelled fixture, never call a paid model.
  fs.rmSync(path.join(data, 'ai-ledger.json'), {force: true});
  const script = 'import sys; from server import Engine; card={"action":"push_card","title":"测试：通勤参考","body":"测试数据：历史记录47分钟，请预留出行时间。","reason":"本地联调卡片，非 MiniMax 真实回复"}; Engine(sys.argv[1], provider=lambda context: card).watch_once()';
  const run = spawnSync('python', ['-c', script, data], {cwd: path.join(__dirname, 'backend'), encoding: 'utf8'});
  if (run.status !== 0) throw Error(run.stderr);
  await until(() => has('测试：通勤参考'));
  if(!(await has('当前没有通勤记录中'))) throw Error('AI replaced the commute card');
  await capture('ai-card.png');
  await tap('收起建议');
  await until(() => has('当前没有通勤记录中'));
  await new Promise(r => setTimeout(r, 3300));
  if (await has('测试：通勤参考')) throw Error('Card was pushed twice');
  await openPage(16);
  await scrollTo('智能通勤 · 已开启');
  await tap('智能通勤 · 已开启');
  if (JSON.parse(fs.readFileSync(path.join(data, 'ai-context.json'), 'utf8')).enabled) throw Error('Opt-out was not saved');
  console.log('Native AI consent, private context, backend worker, card display, dismissal, dedup and opt-out verified (fixture only).');
})().catch(e => { console.error(e); process.exitCode = 1; });
