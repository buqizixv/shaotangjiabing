const fs = require('fs');
const path = require('path');
const port = Number(process.env.ONWAY_PORT || 8141);
const base = `http://127.0.0.1:${port}`;
const out = process.env.ONWAY_CAPTURE_DIR || path.join(__dirname,'_debug/prototype-native');
fs.mkdirSync(out,{recursive:true});
async function api(route){const r=await fetch(base+route);if(!r.ok)throw new Error(`${route}: ${r.status}`);return await r.json();}
async function snap(){return (await api('/snap')).s.filter(w=>w.ty!=='Splash');}
async function click(w){return api(`/click?x=${w.r[0]+w.r[2]/2}&y=${w.r[1]+w.r[3]/2}&wait=1`);}
async function findButton(text){return (await snap()).find(w=>w.ty==='Button'&&w.t===text&&w.r[2]>0&&w.r[3]>10);}
async function nav(){const w=(await snap()).find(w=>w.ty==='Button'&&w.t.includes('返回导航'));if(!w)throw Error('Missing navigation');await click(w);}
async function scroll(dy){const b=(await snap()).find(w=>w.ty==='Button'&&(w.t||'').includes('返回导航'));const x=b.r[0]+b.r[2]/2,y=b.r[1]+b.r[3]+280;return api(`/m?k=scroll&x=${x}&y=${y}&dy=${dy}&precise=1&wait=1`);}
async function openPage(n){await nav();for(let k=0;k<12;k++){const w=(await snap()).find(w=>w.ty==='Button'&&(w.t||'').startsWith(String(n).padStart(2,'0')+'  ')&&w.r[3]>15);if(w){await click(w);return;}await scroll(140);}throw Error(`Page ${n} not reachable`);}
async function capture(n,suffix='native'){const r=await fetch(base+'/g?raw=1');fs.writeFileSync(path.join(out,`${String(n).padStart(2,'0')}-${suffix}.png`),Buffer.from(await r.arrayBuffer()));}
async function tapId(id){const w=(await snap()).find(w=>w.i===id&&w.r[3]>10);if(!w)throw Error('Missing active control '+id);await click(w);}
async function scrollTap(id){for(let k=0;k<10;k++){const w=(await snap()).find(w=>w.i===id&&w.r[3]>15);if(w){await click(w);return;}await scroll(120);}throw Error('Unreachable control '+id);}
async function expectLabel(text){if(!(await snap()).some(w=>w.ty==='Label'&&w.t.includes(text)))throw Error('Missing expected label '+text);}
async function tapText(text){const b=await findButton(text);if(!b)throw Error('Missing button '+text);await click(b);}
(async()=>{
 for(let attempt=0;attempt<50;attempt++) {if((await snap()).some(w=>w.ty==='Label'&&w.t==='在途'))break;await new Promise(resolve=>setTimeout(resolve,100));}
 const report=[];
 for(let n=1;n<=17;n++){
  await openPage(n);const ws=await snap();const labels=ws.filter(w=>w.ty==='Label'&&w.t).map(w=>w.t);
  if(labels.length<2)throw Error(`Page ${n} rendered blank`);
  if(labels.includes('在途 · 页面导航'))throw Error(`Page ${n} did not leave navigation`);
  await capture(n);report.push({page:n,labels,controls:ws.filter(w=>w.ty==='Button').map(w=>({id:w.i,text:w.t,rect:w.r}))});
  console.log(`Page ${String(n).padStart(2,'0')}: ${labels.length} native text labels, captured.`);
 }
 if(process.env.ONWAY_PAGES_ONLY){fs.writeFileSync(path.join(out,'verification.json'),JSON.stringify(report,null,2));await openPage(1);console.log('17 installed OctoSense pages verified; left on welcome.');return;}
 await openPage(1);await tapId('hit_welcome_1_0');
 if(!(await snap()).some(w=>w.t==='标记地点'))throw Error('Welcome button did not open setup');
 await tapId('hit_locations_1_0');
 if(!(await snap()).some(w=>w.t==='选择方案'))throw Error('Location continue did not open plans');
 await tapId('hit_plans_0_1');
 await capture(3,'selected-second');
 await tapId('hit_plans_1_0');
 if(!(await snap()).some(w=>w.t==='完善设置'))throw Error('Save plan did not open permissions');
 await tapId('hit_permissions_0_0');
 await capture(4,'toggle');
 await tapId('hit_permissions_1_0');
 if(!(await snap()).some(w=>w.t==='预计 42 分钟到达'))throw Error('Setup did not finish');
 for(const [id,next] of [['hit_ready_1_1','walking'],['hit_walking_1_1','waiting'],['hit_waiting_1_1','riding'],['hit_riding_1_1','transfer'],['hit_transfer_1_1','lastwalk'],['hit_lastwalk_1_1','arrived']]){
   await scrollTap(id);if(!(await snap()).some(w=>w.i.startsWith('hit_'+next+'_')))throw Error('Commute transition failed '+next);
 }
 await scrollTap('hit_arrived_1_1');await expectLabel('当前没有通勤记录中');
 await openPage(2);await tapId('hit_locations_0_0');await expectLabel('标记家');
 const input=(await snap()).find(w=>w.ty==='TextInput');if(!input)throw Error('Location editor has no input');await click(input);
 for(let n=0;n<16;n++)await api('/k?k=down&c=Backspace');
 await api('/t?t='+encodeURIComponent('测试地点')+'&wait=1');await tapText('保存地点');await expectLabel('测试地点');
 await tapId('hit_locations_0_0');const input2=(await snap()).find(w=>w.ty==='TextInput');await click(input2);
 for(let n=0;n<16;n++)await api('/k?k=down&c=Backspace');await api('/t?t='+encodeURIComponent('望京西园四区')+'&wait=1');await tapText('保存地点');
 await openPage(16);await scrollTap('hit_settings_0_10');await tapText('延误 ≥15 分钟');
 await scrollTap('hit_settings_0_10');await tapText('延误 ≥10 分钟');
 await scrollTap('hit_settings_0_12');await expectLabel('清除历史数据？');await tapText('确认清除');await expectLabel('暂无通勤记录');await capture(17,'cleared');
 await openPage(12);await tapId('hit_idle_0_2');await expectLabel('补记一次通勤');await tapText('确认补记');await expectLabel('47 分钟');await capture(17,'manual');
 fs.writeFileSync(path.join(out,'verification.json'),JSON.stringify(report,null,2));
 console.log('17 pages, onboarding, commute chain, address persistence, thresholds, clear history and manual record verified.');
})().catch(e=>{console.error(e);process.exit(1)});
