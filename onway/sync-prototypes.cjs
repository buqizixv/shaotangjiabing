// Convert the supplied HTML layouts into editable native widgets, never a bitmap page.
const fs = require('fs');
const path = require('path');
const { chromium } = require('playwright');
const root = __dirname;
const designs = path.join(root, 'prototypes');
const bundle = path.join(root, 'bundle');
const debug = path.join(root, '_debug/prototype-native');
fs.mkdirSync(debug, { recursive: true });
const names = ['welcome','locations','plans','permissions','ready','walking','waiting','riding','transfer','lastwalk','arrived','idle','gpslost','canceled','better','settings','history'];
const files = fs.readdirSync(designs).filter(f => /^\d\d-.*\.html$/.test(f)).sort();
const q = JSON.stringify;
const num = n => Math.round(n * 1000) / 1000;
function color(s, alpha = 1) {
  const m = s.match(/[\d.]+/g);
  if (!m) return '#x00000000';
  return '#x' + [Number(m[0]),Number(m[1]),Number(m[2]),255 * (m[3] === undefined ? 1 : Number(m[3])) * alpha].map(v => Math.round(v).toString(16).padStart(2,'0')).join('');
}
const pos = r => `width: ${num(r.w)} height: ${num(r.h)} margin: Inset{left: ${num(r.x)} top: ${num(r.y)}}`;
const fonts = ['Regular','Bold'].map(n => `@font-face{font-family:Onway;src:url(data:font/ttf;base64,${fs.readFileSync(path.join(bundle,`assets/NotoSansSC-Onway-${n}.ttf`)).toString('base64')});font-weight:${n==='Bold'?700:400};} `).join('');
let svgIndex = 0;
function surface(n) {
  if (n.gradient) {
    const {w,h}=n.r, dx=Math.sin(160*Math.PI/180), dy=-Math.cos(160*Math.PI/180), length=w*dx+h*dy;
    const file=`prototype-${String(++svgIndex).padStart(3,'0')}.svg`;
    const shape=n.class==='card-top'?`<path d="M28 0H${w-28}Q${w} 0 ${w} 28V${h}H0V28Q0 0 28 0Z" fill="url(#gradient)"/>`:`<rect width="${w}" height="${h}" rx="28" fill="url(#gradient)"/>`;
    fs.writeFileSync(path.join(bundle,'assets',file),`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${w} ${h}"><defs><linearGradient id="gradient" gradientUnits="userSpaceOnUse" x1="${w/2-dx*length/2}" y1="${h/2-dy*length/2}" x2="${w/2+dx*length/2}" y2="${h/2+dy*length/2}"><stop stop-color="#0055B3"/><stop offset="1" stop-color="#2997FF"/></linearGradient></defs>${shape}</svg>`);
    return `Svg{${pos(n.r)} animating: false draw_svg.svg: http_resource("{{assets}}/assets/${file}") draw_svg.preserve_viewbox: true draw_svg.preserve_aspect: false}`;
  }
  const square = n.radii.every(v=>v===0) && n.class !== 'card-top';
  let radius = n.radii.map(v=>v===0?0.5:v/2);
  if (n.class === 'card-top') radius=[14,14,0.5,0.5];
  let extra = '';
  let bg = color(n.bg,n.opacity);
  if (n.gradient) {
    bg='#x0055b3ff';
    extra=' draw_bg.color_2: #x2997ffff';
  }
  let out = square ? `SolidView{${pos(n.r)} draw_bg.color: ${bg}}` : `RoundedAllView{${pos(n.r)} flow: Overlay padding: 0 draw_bg.color: ${bg} draw_bg.border_radius: vec4(${radius.map(num).join(', ')}) draw_bg.color_dither: 0.0${extra}}`;
  if(n.class==='plan' || n.class==='plan selected'){
    return `RoundedAllView{${pos(n.r)} draw_bg.border_radius: vec4(8, 8, 8, 8) draw_bg.border_size: 1.5 draw_bg.color: if cfg.selectedPlan == ${n.planIndex} {#x2997ff0f} else {#xffffff0a} draw_bg.border_color: if cfg.selectedPlan == ${n.planIndex} {#x2997ff80} else {#xffffff0f}}`;
  }
  if(n.class==='plan-check')return `RoundedView{${pos(n.r)} draw_bg.border_radius: 5.0 draw_bg.border_size: 1.5 draw_bg.color: if cfg.selectedPlan == ${n.planIndex} {#x2997ff} else {#x00000000} draw_bg.border_color: if cfg.selectedPlan == ${n.planIndex} {#x2997ff} else {#xffffff26}}`;
  const b=n.borders;
  if (!square && b.every(x=>x.width===b[0].width && x.color===b[0].color) && b[0].width>0) {
    out=out.slice(0,-1)+` draw_bg.border_size: ${b[0].width} draw_bg.border_color: ${color(b[0].color,n.opacity)}}`;
  } else {
    b.forEach((x,i)=>{if(x.width>0){const r={...n.r}; if(i===0)r.h=x.width; if(i===1){r.x+=r.w-x.width;r.w=x.width;} if(i===2){r.y+=r.h-x.width;r.h=x.width;} if(i===3)r.w=x.width;out+=`\nSolidView{${pos(r)} draw_bg.color: ${color(x.color,n.opacity)}}`;}});
  }
  return out;
}
function label(n) {
  let text=q(n.text);
  if(n.role==='homeValue')text='cfg.homeName';
  if(n.role==='homeValue'&&n.size===15)text='if cfg.homeName == "望京西园四区" {"朝阳区望京西园四区"} else {cfg.homeName}';
  if(n.role==='workValue')text='cfg.workName';
  if(n.role==='threshold')text='"延误 ≥" + cfg.threshold + " 分钟"';
  if(n.role==='data-source')text='"原型示例 · 非实时数据"';
  let c=color(n.color,n.opacity);
  if(n.role==='plan-badge'||n.role==='plan-time')c=`if cfg.selectedPlan == ${n.planIndex} {#x2997ff} else {${n.role==='plan-time'?'#xffffff':'#xffffff66'}}`;
  if(n.role==='workPlaceholder')text='if cfg.workName == "中关村软件园" {"搜索或使用当前位置"} else {cfg.workName}';
  if(n.text==='地图预览'){text='if setupNote == "当前显示原型示例地点" {"地图预览"} else {setupNote}';n={...n,r:{...n.r,x:28,w:304}};}
  return `View{${pos({...n.r,w:n.r.w+5})} flow: Overlay padding: 0 clip_x: false clip_y: false Label{width: Fill height: Fill padding: 0 clip_x: false clip_y: false align: Align{x: ${n.text==='地图预览'?0.5:0} y: 0.5} text: ${text} draw_text.color: ${c} draw_text.text_style: ${n.weight>=600?'OnwayBold':'OnwayRegular'}{font_size: ${num(n.size*.75)} letter_spacing: ${num(n.spacing)}}}}`;
}
function paint(nodes) {
 return nodes.map(n=>{
   if(n.type==='surface')return surface(n);
   if(n.type==='text')return label(n);
   if(n.type==='svg'){
     const file=`prototype-${String(++svgIndex).padStart(3,'0')}.svg`;
     fs.writeFileSync(path.join(bundle,'assets',file),n.svg);
     const svg=`Svg{${pos(n.r)} animating: false draw_svg.svg: http_resource("{{assets}}/assets/${file}") draw_svg.preserve_viewbox: true draw_svg.preserve_aspect: false}`;
     return n.role==='plan-check'?`if cfg.selectedPlan == ${n.planIndex} {${svg}}`:svg;
   }
   if(n.type==='toggle'){
     const key=n.index===0?'backgroundLocation':'anomalyNotice';
     return `RoundedView{${pos(n.r)} draw_bg.border_radius: ${num(n.r.h/4)} draw_bg.color: if cfg.${key} {#x30d158} else {#x444446}}\nRoundedView{width: ${n.r.h-4} height: ${n.r.h-4} margin: Inset{left: if cfg.${key} {${num(n.r.x+n.r.w-n.r.h+2)}} else {${num(n.r.x+2)}} top: ${num(n.r.y+2)}} draw_bg.border_radius: ${num((n.r.h-4)/4)} draw_bg.color: #xffffff}`;
   }
   return '';
 }).join('\n');
}
function action(h,page) {
  const text=h.text.trim();
  if(h.cls==='plan')return `selectPlan(${h.index})`;
  if(h.cls.includes('toggle'))return `togglePreference(${h.index})`;
  if(h.cls.includes('use-current'))return 'markWork()';
  if(h.tag==='INPUT')return `editLocation(${h.index===0?'true':'false'})`;
  if(h.cls==='row'){
    if(text.startsWith('家'))return 'editLocation(true)';
    if(text.startsWith('目的地'))return 'editLocation(false)';
    if(text.startsWith('去程')||text.startsWith('返程')||text.startsWith('重新规划'))return 'setScreen("plans")';
    if(text.startsWith('后台定位'))return 'togglePreference(0)';
    if(text.startsWith('异常通知'))return 'togglePreference(1)';
    if(text.startsWith('通知阈值'))return 'setScreen("threshold")';
    if(text.startsWith('清除'))return 'setScreen("clearconfirm")';
  }
  if(text.includes('补记'))return 'setScreen("manualtrip")';
  if(text.includes('放弃'))return 'setScreen("ready")';
  if(page==='permissions' && text.includes('完成设置'))return 'finishSetup()';
  if(page==='better' && text.includes('采用'))return '{cfg.selectedPlan = 2; save(); setScreen("ready")}';
  if(h.cls==='close-btn' && (page==='settings'||page==='history'))return 'closeOverlay()';
  const target=(h.href||h.onclick||'').match(/(\d\d-[\w-]+\.html)/);
  if(target){const index=files.indexOf(target[1]);if(index>=0){const s=names[index]; if(s==='settings')return 'openSettings()';if(s==='history')return 'openHistory(screen)';return `setScreen(${q(s)})`;}}
  if(h.cls==='correction')return 'openCorrection()';
  if(text==='已经下车')return 'setScreen("lastwalk")';
  if(h.cls==='user-btn')return 'openSettings()';
  return null;
}
function hits(hs,page,part){return hs.map((h,i)=>{const a=action(h,page);return a?`hit_${page}_${part}_${i} := OnwayHit{${pos(h.r)} text: "" on_click: || ${a}}`:'';}).join('\n');}
(async()=>{
 const browser=await chromium.launch({headless:true});
 const page=await browser.newPage({viewport:{width:412,height:892}});
 const layouts=[];
 for(let pi=0;pi<files.length;pi++){
  await page.goto('file:///'+path.join(designs,files[pi]).replaceAll('\\','/'));
  await page.addStyleTag({content:fonts+'body,input,button{font-family:Onway,sans-serif!important;line-height:1.2}.card-body{min-height:0!important}.card-body>*{flex-shrink:0!important}.card-footer,.card-foot,.card-top{flex-shrink:0!important}.map-preview{min-height:40px!important;flex-shrink:1!important}.card-bottom{min-height:0!important;overflow-y:auto!important}.card-bottom>*{flex-shrink:0!important}'});
  await page.evaluate(()=>document.fonts.ready);
  const layout=await page.evaluate(()=>{
    const card=document.querySelector('.card'),cr=card.getBoundingClientRect();
    const rect=(r,origin)=>({x:r.x-origin.x,y:r.y-origin.y,w:r.width,h:r.height});
    const number=v=>parseFloat(v)||0;
    const opacity=e=>{let a=1;for(let p=e;p && p!==card.parentElement;p=p.parentElement)a*=number(getComputedStyle(p).opacity);return a;};
    const pack=(e,origin)=>{
      const s=getComputedStyle(e); return {type:'surface',r:rect(e.getBoundingClientRect(),origin),bg:s.backgroundColor,gradient:s.backgroundImage.includes('gradient'),radii:[s.borderTopLeftRadius,s.borderTopRightRadius,s.borderBottomRightRadius,s.borderBottomLeftRadius].map(number),borders:['Top','Right','Bottom','Left'].map(k=>({width:number(s['border'+k+'Width']),color:s['border'+k+'Color']})),class:e.className,opacity:opacity(e)};
    };
    const parts=[];
    const planIndex=e=>{const p=e.closest('.plan');return p?[...card.querySelectorAll('.plan')].indexOf(p):-1;};
    for(const part of card.children){
      const pr=part.getBoundingClientRect(),scroll=['auto','scroll'].includes(getComputedStyle(part).overflowY)||part.scrollHeight>pr.height+3;
      const nodes=[],hit=[];let ti=0;
      function visit(e){
        const s=getComputedStyle(e);if(s.display==='none'||s.visibility==='hidden')return;
        if(e.matches('.toggle')){
          nodes.push({type:'toggle',r:rect(e.getBoundingClientRect(),pr),index:ti});
          hit.push({r:rect(e.getBoundingClientRect(),pr),cls:e.className,index:ti++,text:'',tag:e.tagName});return;
        }
        if(e.tagName.toLowerCase()==='svg'){
          const clone=e.cloneNode(true);clone.setAttribute('xmlns','http://www.w3.org/2000/svg');clone.setAttribute('opacity',opacity(e));
          for(const p of [clone,...clone.querySelectorAll('*')])for(const k of ['fill','stroke']){const v=p.getAttribute(k);if(v==='currentColor')p.setAttribute(k,s.color);}
          clone.removeAttribute('style');nodes.push({type:'svg',r:rect(e.getBoundingClientRect(),pr),svg:clone.outerHTML,role:e.closest('.plan-check')?'plan-check':'',planIndex:planIndex(e)});return;
        }
        if(s.backgroundColor!=='rgba(0, 0, 0, 0)'||s.backgroundImage!=='none'||['Top','Right','Bottom','Left'].some(k=>number(s['border'+k+'Width'])>0))nodes.push({...pack(e,pr),planIndex:planIndex(e)});
        if(e.matches('.plan-check')&&!e.querySelector('svg')){const r=e.getBoundingClientRect();nodes.push({type:'svg',r:{x:r.x-pr.x+(r.width-12)/2,y:r.y-pr.y+(r.height-12)/2,w:12,h:12},svg:'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12"><path d="M2 6l2.5 2.5L10 3" stroke="white" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round"/></svg>',role:'plan-check',planIndex:planIndex(e)});}
        if(e.matches('.timeline')){const r=e.getBoundingClientRect();nodes.push({type:'surface',r:{x:r.x-pr.x+7,y:r.y-pr.y+8,w:2,h:r.height-16},bg:'rgba(255,255,255,0.08)',gradient:false,radii:[0,0,0,0],borders:Array(4).fill({width:0,color:'transparent'}),opacity:1,class:''});}
        if(e.matches('.t-current .t-dot')){const r=e.getBoundingClientRect();nodes.splice(nodes.length-1,0,{type:'surface',r:{x:r.x-pr.x-4,y:r.y-pr.y-4,w:r.width+8,h:r.height+8},bg:'rgba(41,151,255,0.2)',radii:[20,20,20,20],borders:Array(4).fill({width:0,color:'transparent'}),opacity:1,class:''});}
        if(e.matches('input')){
          const r=e.getBoundingClientRect(),home=e.classList.contains('location-filled');
          nodes.push({type:'text',r:{x:r.x-pr.x+16,y:r.y-pr.y+14,w:r.width-32,h:22},text:home?e.value:e.placeholder,size:15,weight:400,spacing:0,color:home?'rgb(255,255,255)':'rgba(255,255,255,0.2)',opacity:1,role:home?'homeValue':'workPlaceholder'});
        }
        for(const child of e.childNodes){
          if(child.nodeType===Node.ELEMENT_NODE)visit(child);
          else if(child.nodeType===Node.TEXT_NODE && child.textContent.trim()){
            let runs=[];
            for(let i=0;i<child.textContent.length;i++){
              const range=document.createRange();range.setStart(child,i);range.setEnd(child,i+1);
              const r=range.getBoundingClientRect();if(r.width<.01||r.height<.01)continue;
              let run=runs.find(x=>Math.abs(x.y-r.y)<1);if(!run){run={x:r.x,y:r.y,right:r.right,h:r.height,text:''};runs.push(run);}
              run.right=Math.max(run.right,r.right);run.text+=child.textContent[i];
            }
            for(const run of runs){const text=run.text.replace(/\s+/g,' ');if(!text.trim())continue;let role=e.matches('.row-value')&&text.includes('延误')?'threshold':''; if(e.matches('.plan-badge,.plan-time'))role=e.className; if(e.matches('.row-value')&&text.includes('望京'))role='homeValue';if(e.matches('.row-value')&&text.includes('软件园'))role='workValue';nodes.push({type:'text',r:{x:run.x-pr.x,y:run.y-pr.y,w:run.right-run.x,h:run.h},text,size:number(s.fontSize),weight:number(s.fontWeight),spacing:number(s.letterSpacing),color:s.color,opacity:opacity(e),role,planIndex:planIndex(e)});}
          }
        }
        if(e.matches('.t-passed')){const r=e.getBoundingClientRect(),range=document.createRange();range.selectNodeContents(e);const rr=range.getBoundingClientRect();nodes.push({type:'text',r:{x:rr.right-pr.x+6,y:r.y-pr.y+9,w:12,h:16},text:'✓',size:11,weight:400,spacing:0,color:'rgba(41,151,255,0.6)',opacity:1});}
        const clickable=e.matches('[onclick],a,button,.plan,.toggle,.row,.use-current,input,.correction,.better-dismiss');
        if(clickable)hit.push({r:rect(e.getBoundingClientRect(),pr),cls:typeof e.className==='string'?e.className:'',text:e.innerText||'',tag:e.tagName,href:e.getAttribute('href'),onclick:e.getAttribute('onclick'),index:e.matches('.plan')?[...e.parentNode.querySelectorAll('.plan')].indexOf(e):e.matches('input')?[...card.querySelectorAll('input')].indexOf(e):0});
      }
      visit(part);parts.push({r:rect(pr,cr),scroll,height:Math.max(part.scrollHeight,pr.height),nodes,hit});
    }
    const cs=getComputedStyle(card);return{background:cs.backgroundColor,gradient:cs.backgroundImage!=='none',parts};
  });
  await page.screenshot({path:path.join(debug,files[pi].replace('.html','-reference.png'))});
  layouts.push(layout);
 }
 await browser.close();
 const controllerFile=path.join(root,'controller.splash');
 let controller=fs.readFileSync(controllerFile,'utf8');
 controller=controller.replace(/fn load\(\)\{[\s\S]*?\n\}\n\nfn save/,`fn load(){
    if fs.exists("cfg.json") {
        let stored=fs.read("cfg.json")
        if stored.search(${q('"schema"')})>=0 {let c=stored.parse_json(); if c.schema==3 {cfg=c}}
        else {if fs.exists("legacy-cfg-v0.3.json")==false {fs.write("legacy-cfg-v0.3.json",stored)}}
    }
    if cfg == nil { seed() }
    if fs.exists("hist.json") {let h=fs.read("hist.json").parse_json();if h!=nil {hist=h}}
    if cfg.onboarded { screen = "ready" }
    save()
}

fn save`);
 controller=controller.replace('if cfg.onboarded { screen = "ready" }','screen = "welcome"');
 controller=controller.replace('let hist = []','let hist = []\nlet historyEdited = false');
 controller=controller.replace('if cfg == nil { seed() }','if cfg == nil { seed() }; if fs.exists("history-edited.txt") {historyEdited=fs.read("history-edited.txt")=="true"}');
 controller=controller.replace('backgroundLocation: false','backgroundLocation: true').replace('anomalyNotice: false','anomalyNotice: true');
 controller=controller.replace('start_timeout(0.05, || { load(); refresh() })', 'start_timeout(0.05, || {load();ui.board.render()})');
 controller=controller.replace('start_interval(3.0, || refresh())', 'start_interval(3.0, || {if screen == "locationstatus" || screen == "locationeditor" {readLocation();ui.location_status.set_text(locationNote)} elif screen == "locations" {readLocation()}})');
 controller+=`\nlet editorHome = true\nlet editorBack = "locations"\nlet editorText = ""\nlet manualMinutes = "47"\nlet notice = ""\nfn editLocation(home){editorHome=home; editorBack=screen; if home {editorText=cfg.homeName} else {editorText=cfg.workName}; setScreen("locationeditor")}\nfn saveLocation(){if editorText.trim().len()>0 {if editorHome {cfg.homeName=editorText} else {cfg.workName=editorText}; save(); setScreen(editorBack)}}\nfn selectPlan(i){cfg.selectedPlan=i;save();ui.board.render()}\nfn togglePreference(i){if i==0 {cfg.backgroundLocation = cfg.backgroundLocation == false} else {cfg.anomalyNotice = cfg.anomalyNotice == false};save();ui.board.render()}\nfn setThreshold(n){cfg.threshold=n;save();setScreen("settings")}\nfn openCorrection(){previousScreen=screen;setScreen("correction")}\nfn clearHistory(){hist=[];cfg.historyEdited=true;save();setScreen("history")}\nfn manualTrip(){let d=manualMinutes.trim().to_f64(); if d>=1 && d<=1440 {hist.push({epoch:time_now(),direction:"to_work",plan:cfg.selectedPlan,durMin:round(d)});cfg.historyEdited=true;save();setScreen("history")} else {notice="请输入 1～1440 分钟";ui.board.render()}}\nfn historyAverage(){let total=0;for trip in hist {total=total+trip.durMin};if hist.len()>0 {return round(total/hist.len())};return 0}\n`;
 controller=controller.replaceAll('cfg.historyEdited=true','historyEdited=true;fs.write("history-edited.txt","true")');
 const styles=`\nlet OnwayRegular = TextStyle{font_family: FontFamily{latin := FontMember{res: http_resource("{{assets}}/assets/NotoSansSC-Onway-Regular.ttf") asc: -0.2 desc: 0.08 weight: 400}} line_spacing: 1.0}\nlet OnwayBold = TextStyle{font_family: FontFamily{latin := FontMember{res: http_resource("{{assets}}/assets/NotoSansSC-Onway-Bold.ttf") asc: -0.2 desc: 0.08 weight: 700}} line_spacing: 1.0}\nlet Hit = ButtonFlat{padding: 0 draw_bg +: {color: #x00000000 color_hover: #x00000000 color_down: #x00000008 border_size: 0.0} draw_text.color: #x00000000}\nlet MenuButton = ButtonFlat{width: Fill height: 40 draw_bg +: {color: #x303033 color_hover: #x38383a color_down: #x0066cc border_radius: 7.0 border_size: 0.0} draw_text +: {color: #xffffffff color_hover: #xffffffff color_down: #xffffffff text_style: OnwayRegular{font_size: 10.5}}}\nlet MenuTitle = Label{draw_text.color: #xffffff draw_text.text_style: OnwayBold{font_size: 18}}\n`;
 let body=`SolidView{width: Fill height: Fill flow: Down align: Align{x: 0.5 y: 0.5} padding: 20 draw_bg.color: #xf5f5f7\nboard := View{width: 360 height: Fit flow: Down spacing: 16 on_render: || {\n View{width: Fill height: 18 flow: Right align: Align{x: 0.5 y: 0.5}\n ButtonFlat{text: "‹  返回导航" width: Fit height: 18 padding: 0 draw_bg +: {color: #x00000000 color_hover: #x00000000 color_down: #x00000000 border_size: 0.0} draw_text +: {color: #x86868b color_hover: #x0066cc text_style: OnwayRegular{font_size: 9.75}} on_click: || setScreen("navigation")}\n}\n`;
 layouts.forEach((layout,pi)=>{
  body+=`${pi===0?'if':'elif'} screen == ${q(names[pi])} {\nRoundedShadowView{width: 360 height: 509 flow: Overlay padding: 0 clip_x: false clip_y: false draw_bg.color: ${layout.gradient?'#x0055b3':color(layout.background)} draw_bg.border_radius: 14.0 draw_bg.shadow_radius: 60.0 draw_bg.shadow_offset: vec2(0, 25) draw_bg.shadow_color: #x0000001f\n`;
  if(pi===16)body+=`if cfg.historyEdited == true {View{width: Fill height: Fill flow: Down padding: 24 spacing: 18 View{width: Fill height: 32 flow: Right MenuTitle{text: "通勤历史"} View{width: Fill} MenuButton{width: 32 height: 32 text: "×" on_click: || closeOverlay()}} Label{text: "平均耗时 " + historyAverage() + " 分钟 · 共 " + hist.len() + " 次记录" draw_text.color: #xffffff draw_text.text_style: OnwayRegular{font_size: 10.5}} ScrollYView{width: Fill height: Fill flow: Down spacing: 12 if hist.len()==0 {Label{text: "暂无通勤记录" draw_text.color: #xffffff66 draw_text.text_style: OnwayRegular{font_size: 12}}} for trip in hist {RoundedView{width: Fill height: 72 flow: Down padding: 14 spacing: 6 draw_bg.color: #xffffff08 draw_bg.border_radius: 7.0 Label{text: "去程 · " + trip.durMin + " 分钟" draw_text.color: #xffffff draw_text.text_style: OnwayBold{font_size: 11.25}} Label{text: "手动补记 · 方案 " + (trip.plan+1) draw_text.color: #xffffff66 draw_text.text_style: OnwayRegular{font_size: 9}}}} Label{text: "仅统计你的已保存记录" draw_text.color: #xffffff40 draw_text.text_style: OnwayRegular{font_size: 8.25}}}} else {\n`;
  if(layout.gradient){body+=surface({r:{x:0,y:0,w:360,h:509},bg:'rgb(0,85,179)',gradient:true,radii:[28,28,28,28],borders:Array(4).fill({width:0,color:'transparent'}),opacity:1,class:''})+'\n';}
  for(const p of layout.parts){
   const wrap=p.scroll?'ScrollYView':'View';
   body+=`${wrap}{${pos(p.r)} flow: ${p.scroll?'Down':'Overlay'} padding: 0 clip_x: true clip_y: true\n`;
   if(p.scroll)body+=`View{width: ${num(p.r.w)} height: ${num(p.height)} flow: Overlay padding: 0\n`;
   for(const node of p.nodes)if(node.type==='text'&&/^(路线数据|到站数据|步行估算|换乘数据|距离基于|进度基于)/.test(node.text))node.role='data-source';
   body+=paint(p.nodes)+'\n'+hits(p.hit,names[pi],layout.parts.indexOf(p))+'\n';
   if(p.scroll)body+='}\n';body+='}\n';
  }
  if(pi===16)body+='}\n';
  body+='}\n}\n';
 });
 body+=`else {RoundedView{width: 360 height: 509 flow: Down padding: 28 spacing: 10 draw_bg.color: #x272729 draw_bg.border_radius: 14.0\n`;
 body+=`if screen == "navigation" {MenuTitle{text: "在途 · 页面导航"} Label{text: "原型预览 · 路线与班次为示例" draw_text.color: #x99999b draw_text.text_style: OnwayRegular{font_size: 9.75}} ScrollYView{width: Fill height: Fill flow: Down spacing: 8\n`;
 const titles=['欢迎','标记地点','选择方案','完善设置','准备出门','前往上车站','候车中','乘车中','换乘中','步行前往目的地','已到达','当前无行程','定位丢失','行程已取消','更快方案','设置','通勤历史'];
 titles.forEach((t,i)=>body+=`MenuButton{text: ${q(String(i+1).padStart(2,'0')+'  '+t)} on_click: || setScreen(${q(names[i])})}\n`);
 body+=`MenuButton{text: "检查真实定位" on_click: || {readLocation();setScreen("locationstatus")}}}}\n`;
 body+=`elif screen == "locationstatus" {MenuTitle{text: "真实定位"} location_status := Label{width: Fill height: 52 text: locationNote draw_text.color: #xcccccc draw_text.text_style: OnwayRegular{font_size: 10.5}} Label{width: Fill height: 64 text: "定位来自系统 · 不使用模拟位置\\n路线与班次仍为示例数据" draw_text.color: #x99999b draw_text.text_style: OnwayRegular{font_size: 9.75}} MenuButton{text: "重新检查" on_click: || {readLocation();ui.location_status.set_text(locationNote)}} MenuButton{text: "标记家" on_click: || editLocation(true)} MenuButton{text: "标记目的地" on_click: || editLocation(false)} MenuButton{text: "返回导航" on_click: || setScreen("navigation")}}\n`;
 body+=`elif screen == "locationeditor" {MenuTitle{text: if editorHome {"标记家"} else {"标记目的地"}} TextInput{width: Fill height: 50 text: editorText draw_text.text_style: OnwayRegular{font_size: 11.25} on_change: |t| {editorText=t}} location_status := Label{width: Fill height: 52 text: locationNote draw_text.color: #xcccccc draw_text.text_style: OnwayRegular{font_size: 10.5}} MenuButton{text: "使用当前位置" on_click: || {if editorHome {markHome();if hasFix {editorText=cfg.homeName}} else {markWork();if hasFix {editorText=cfg.workName}};ui.board.render()}} MenuButton{text: "保存地点" on_click: || saveLocation()} MenuButton{text: "返回" on_click: || setScreen(editorBack)}}\n`;
 body+=`elif screen == "threshold" {MenuTitle{text: "通知阈值"} MenuButton{text: "延误 ≥5 分钟" on_click: || setThreshold(5)} MenuButton{text: "延误 ≥10 分钟" on_click: || setThreshold(10)} MenuButton{text: "延误 ≥15 分钟" on_click: || setThreshold(15)} MenuButton{text: "返回设置" on_click: || setScreen("settings")}}\n`;
 body+=`elif screen == "clearconfirm" {MenuTitle{text: "清除历史数据？"} Label{text: "清除后无法恢复" draw_text.color: #x99999b} MenuButton{text: "确认清除" on_click: || clearHistory()} MenuButton{text: "返回设置" on_click: || setScreen("settings")}}\n`;
 body+=`elif screen == "manualtrip" {MenuTitle{text: "补记一次通勤"} Label{text: "去程 · 当前保存方案 · 耗时（分钟）" draw_text.color: #x99999b draw_text.text_style: OnwayRegular{font_size: 9.75}} TextInput{width: Fill height: 50 text: manualMinutes draw_text.text_style: OnwayRegular{font_size: 11.25} on_change: |t| {manualMinutes=t}} Label{text: notice draw_text.color: #xff9f0a draw_text.text_style: OnwayRegular{font_size: 9.75}} MenuButton{text: "确认补记" on_click: || manualTrip()} MenuButton{text: "返回" on_click: || setScreen("idle")}}\n`;
 body+=`else {MenuTitle{text: "纠正当前状态"} MenuButton{text: "前往上车站" on_click: || setScreen("walking")} MenuButton{text: "候车中" on_click: || setScreen("waiting")} MenuButton{text: "乘车中" on_click: || setScreen("riding")} MenuButton{text: "定位暂不可用" on_click: || setScreen("gpslost")} MenuButton{text: "步行前往目的地" on_click: || setScreen("lastwalk")} MenuButton{text: "已到达" on_click: || setScreen("arrived")} MenuButton{text: "返回起点，取消行程" on_click: || setScreen("canceled")}}\n`;
 body+='}}\n}}}\n';
 fs.writeFileSync(path.join(debug,'layouts.json'),JSON.stringify(layouts,null,2));
 const finalStyles = styles.replace('let Hit =', 'let OnwayHit =');
 body = body.replace('board := View{width: 360 height: Fit', 'board := View{clip_x: false clip_y: false width: 360 height: Fit');
 body=body.replace('OnwayRegular{font_size: 9}}}} Label{text: "仅统计', 'OnwayRegular{font_size: 9}}}}} Label{text: "仅统计');
 body=body.replace('cfg.historyEdited == true','historyEdited');
 body=body.replace('draw_bg.shadow_color: #x0000001f','draw_bg.shadow_color: #x0050b433');
 body=body.replaceAll('ScrollYView{','OnwayScroll{');
 const scrollStyle='\nlet OnwayScroll = ScrollYView{scroll_bars.scroll_bar_y.draw_bg.color: #x00000000 scroll_bars.scroll_bar_y.draw_bg.color_hover: #x00000000 scroll_bars.scroll_bar_y.draw_bg.color_drag: #x00000000 scroll_bars.scroll_bar_y.draw_bg.border_color: #x00000000 scroll_bars.scroll_bar_y.draw_bg.border_color_hover: #x00000000 scroll_bars.scroll_bar_y.draw_bg.border_color_drag: #x00000000}\n';
 const source = controller + finalStyles + scrollStyle + body;
 let depth=0;
 for(const line of source.split('\n')) {const unquoted=line.replace(/"(?:\\.|[^"\\])*"/g,'').replace(/\/\/.*$/,'');depth+=(unquoted.match(/\{/g)||[]).length-(unquoted.match(/\}/g)||[]).length;if(depth<0)throw Error('Unbalanced native widget source');}
 if(depth!==0)throw Error('Unbalanced native widget source');
 fs.writeFileSync(path.join(bundle,'main.splash'),source);
 console.log(`Converted ${layouts.length} prototype pages into native widgets with ${svgIndex} original SVG icons.`);
})().catch(e=>{console.error(e);process.exit(1)});
