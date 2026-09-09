import os, json, sys
W = sys.argv[1]; S = sys.argv[2]; D = os.path.join(W,'designs')
FONTS = "https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;500;600&family=JetBrains+Mono:wght@400;500;700&family=Manrope:wght@500;600;700;800&family=DM+Mono:wght@400;500&display=swap"
blends = [
 ("g","DarkGlass","Dark Glass","D's layout and components on A's dark palette.",
  "Takes from A: the dark ground, cyan/violet/amber data colors, monospace figures, the recording status, and per-core bars and stacked breakdowns inside every card. Takes from D: the device sidebar, the plain-language headline, ring gauges, rounded frosted cards and the plain log feed.",
  "The friendliest of the three at a glance and the closest to D's flow, with A's density folded into each card. The frosted cards need a real GPU-composited window to look as intended."),
 ("h","LightInstrument","Light Instrument","A's structure and density on D's light palette.",
  "Takes from A: the icon rail, the fixed nine-panel grid where every metric is always visible, the device switcher and the dense log with level toggles. Takes from D: the light ground with a faint tint, rounded white cards, soft blue/violet/mint/peach accents and a one-line status sentence with pills in the top bar.",
  "The most information per pixel of the three and the easiest to read in a bright room, but the least 'calm': nothing is summarized, everything competes equally."),
 ("k","TwoToneWorkbench","Two-tone Workbench","A's dark sidebar beside D's light content, with a summary tier above a detail tier.",
  "Takes from A: the dark device rail with recording controls, the compact detail grid with core bars and dense tables, and the log with levels. Takes from D: the headline sentence, a single summary strip of small rings, white cards on a tinted ground. Adds one UX idea from neither: a Focus chip that pins one app so processes and the log highlight it, with per-app export.",
  "The most opinionated hierarchy (summary, then details, then evidence), so it scales best as features grow; but two palettes on one screen must be tuned carefully or the seam shows."),
]
boards=[]; notes=[]
for i,(k,stem,name,axis,why,trade) in enumerate(blends):
    css=open(os.path.join(D,f"{k}.css")).read(); body=open(os.path.join(D,f"{k}.body.html")).read()
    dc=f'''<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <script src="./support.js"></script>
</head>
<body>
<x-dc>
<helmet>
  <link rel="stylesheet" href="{FONTS}">
  <style>
    body {{ margin: 0; }}
    a {{ color: inherit; text-decoration: none; }} a:hover {{ color: inherit; }}
{css}  </style>
</helmet>
{body}</x-dc>
</body>
</html>
'''
    open(os.path.join(W,f"{stem}.dc.html"),"w").write(dc)
    x=i*1560
    boards.append({"file":f"{stem}.dc.html","x":x,"y":0,"w":1440,"h":900,"title":f"AD-{i+1} · {name}","page":"page-2"})
    notes.append({"id":f"note-{k}","x":x,"y":-150,"w":700,"text":f"AD-{i+1} · {name}\n{axis}\n\nTradeoff: {trade}","page":"page-2"})
cj=json.load(open(os.path.join(W,"canvas.json")))
cj["pages"]=[{"id":"page-1","name":"Six directions"},{"id":"page-2","name":"A + D blends"}]
cj["artboards"]=[b for b in cj["artboards"] if b.get("page")!="page-2"]+boards
cj["annotations"]=[n for n in cj["annotations"] if n.get("page")!="page-2"]+notes
cj["launch"]={"view":"canvas","page":"page-2"}
json.dump(cj,open(os.path.join(W,"canvas.json"),"w"),indent=2)
secs=[]
for i,(k,stem,name,axis,why,trade) in enumerate(blends):
    css=open(os.path.join(D,f"{k}.css")).read(); body=open(os.path.join(D,f"{k}.body.html")).read()
    secs.append(f'''
<section class="dir" id="dir-{k}">
  <div class="intro">
    <div class="letter">AD-{i+1}</div>
    <div>
      <h2>{name}</h2>
      <p class="axis">{axis}</p>
      <p>{why}</p>
      <p><strong>Tradeoff.</strong> {trade}</p>
    </div>
    <button class="sz" type="button" data-target="fr-{k}">Actual size</button>
  </div>
  <style>{css}</style>
  <div class="frame" id="fr-{k}"><div class="scaler">{body}</div></div>
</section>''')
    open(os.path.join(S,f"p-{k}.html"),"w").write(f'<!doctype html><html><head><meta charset="utf-8"><link rel="stylesheet" href="{FONTS}"><style>body{{margin:0}}</style><style>{css}</style></head><body>{body}</body></html>')
page = f'''<title>DeviceLog Blends</title>
<link rel="stylesheet" href="{FONTS}">
<style>
:root{{--bg:#f4f3f0;--ink:#1a1a1a;--mut:#6a6862;--line:#dedbd4;--panel:#ffffff;--acc:#b8452c;--chip:#ecebe6}}
@media (prefers-color-scheme: dark){{:root:not([data-theme="light"]){{--bg:#151514;--ink:#ecebe6;--mut:#9a978f;--line:#2c2b28;--panel:#1d1d1b;--acc:#e8865f;--chip:#262624}}}}
:root[data-theme="dark"]{{--bg:#151514;--ink:#ecebe6;--mut:#9a978f;--line:#2c2b28;--panel:#1d1d1b;--acc:#e8865f;--chip:#262624}}
*{{box-sizing:border-box}}
body{{background:var(--bg);color:var(--ink);font-family:"IBM Plex Sans",system-ui,sans-serif;font-size:15px;line-height:1.55}}
.hero{{max-width:1160px;margin:0 auto;padding:56px 32px 24px}}
.hero h1{{font-size:38px;font-weight:600;letter-spacing:-.02em;margin:0 0 10px;text-wrap:balance}}
.hero p{{max-width:68ch;margin:0 0 10px;color:var(--mut);text-wrap:pretty}}
nav{{position:sticky;top:0;z-index:10;background:color-mix(in srgb,var(--bg) 88%,transparent);backdrop-filter:blur(12px);border-bottom:1px solid var(--line)}}
nav ul{{list-style:none;margin:0 auto;padding:0 32px;max-width:1160px;display:flex;gap:2px;overflow-x:auto}}
nav a{{display:block;padding:12px 14px;color:var(--mut);text-decoration:none;font-weight:500;font-size:14px;white-space:nowrap;border-bottom:2px solid transparent}}
nav a:hover,nav a:focus-visible{{color:var(--ink);outline:none;border-bottom-color:var(--acc)}}
nav a b{{color:var(--acc);font-family:"JetBrains Mono",monospace;margin-right:6px;font-weight:600}}
.dir{{max-width:1160px;margin:0 auto;padding:44px 32px 20px;border-top:1px solid var(--line)}}
.intro{{display:grid;grid-template-columns:84px 1fr auto;gap:20px;align-items:start;margin-bottom:22px}}
.letter{{font-family:"JetBrains Mono",monospace;font-size:28px;font-weight:600;color:var(--acc);line-height:1.2}}
.intro h2{{margin:0 0 4px;font-size:26px;font-weight:600;letter-spacing:-.01em}}
.intro p{{margin:0 0 6px;max-width:74ch;text-wrap:pretty}}
.intro .axis{{color:var(--mut);font-size:16px;margin-bottom:12px}}
.sz{{font:inherit;font-size:13px;padding:8px 12px;border:1px solid var(--line);background:var(--panel);color:var(--ink);border-radius:6px;cursor:pointer}}
.sz:focus-visible{{outline:2px solid var(--acc);outline-offset:2px}}
.frame{{width:100%;overflow:hidden;border:1px solid var(--line);border-radius:10px;background:var(--panel);box-shadow:0 20px 50px rgba(0,0,0,.12)}}
.frame.actual{{overflow-x:auto}}
.scaler{{width:1440px;height:900px;transform-origin:0 0}}
.frame.actual .scaler{{transform:none !important}}
footer{{max-width:1160px;margin:0 auto;padding:40px 32px 80px;color:var(--mut);font-size:14px}}
@media (prefers-reduced-motion:no-preference){{.scaler{{transition:transform .2s ease}}}}
</style>
<div class="hero">
  <h1>Three ways to blend Mission Control and Glass Observatory</h1>
  <p>Same app, same sample session as the six directions. Each blend mixes A and D along a different seam: one keeps D's shapes and takes A's colors, one keeps A's structure and takes D's colors, and one splits the screen so each parent owns a half. All three keep the multi-device sidebar or switcher, an iPhone, iPad and Android phone connected at once, and the full feature list.</p>
</div>
<nav><ul>
<li><a href="#dir-g"><b>AD-1</b>Dark Glass</a></li><li><a href="#dir-h"><b>AD-2</b>Light Instrument</a></li><li><a href="#dir-k"><b>AD-3</b>Two-tone Workbench</a></li>
</ul></nav>
{''.join(secs)}
<footer>Each blend is a single 1440×900 desktop screen. Frames are scaled to fit this page; use “Actual size” to see one at true pixel size.</footer>
<script>
(function(){{
  function fit(){{
    document.querySelectorAll('.frame').forEach(function(f){{
      var s=f.querySelector('.scaler');
      if(f.classList.contains('actual')){{f.style.height='';return;}}
      var k=Math.min(1,f.clientWidth/1440);
      s.style.transform='scale('+k+')';
      f.style.height=Math.round(900*k)+'px';
    }});
  }}
  window.addEventListener('resize',fit);
  document.querySelectorAll('.sz').forEach(function(b){{
    b.addEventListener('click',function(){{
      var f=document.getElementById(b.dataset.target);
      var on=f.classList.toggle('actual');
      b.textContent=on?'Fit to page':'Actual size';
      if(on){{f.style.height='';}}
      fit();
    }});
  }});
  fit(); setTimeout(fit,300);
}})();
</script>
'''
open(os.path.join(S,"devicelog-blends.html"),"w").write(page)
print("blends page bytes", len(page))
