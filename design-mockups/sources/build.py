import os, json, sys
W = sys.argv[1]; S = sys.argv[2]
D = os.path.join(W, 'designs')
FONTS = "https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;500;600&family=IBM+Plex+Mono:wght@400;600&family=JetBrains+Mono:wght@400;500;700&family=Newsreader:ital,wght@0,400;1,400&family=Archivo:wght@400;500;600;800;900&family=Space+Mono:wght@400;700&family=Manrope:wght@500;600;700;800&family=DM+Mono:wght@400;500&family=Sora:wght@400;600;700&family=Source+Code+Pro:wght@400;600&display=swap"
designs = [
 ("a","Main","Mission Control","Dark instrument panel: every metric on one screen, no scrolling.",
  "For a monitoring tool the strongest UX argument is glanceability. Nine panels, one grid, fixed positions, so after a day of use you know where to look without reading. Cyan carries live data; amber and red are reserved for state.",
  "Density has a cost: on a small laptop screen the panels get cramped, and the per-app log view needs a second screen or a mode switch."),
 ("b","FieldNotebook","Field Notebook","Light, editorial, report-first: the session reads like a lab journal.",
  "Numbers get sentences. A thermal change is a margin note tied to a time, not a colored pill. This is the direction if the main output of the app is something you hand to someone else: the screen is already the export.",
  "Slower for live watching. Big serif figures and prose captions trade real-time scanning for understanding after the fact."),
 ("c","SwissGrid","Swiss Grid","Black, white and one red. Hairline modular grid, oversized numerals.",
  "Typography does all the work: one hierarchy, no chrome, no color except the alarm. It is the easiest of the six to read at arm's length and prints beautifully. The connected-devices strip shows all three devices side by side.",
  "Austerity can feel cold, and with only one accent the design has nowhere to go when two things are wrong at once."),
 ("d","GlassObservatory","Glass Observatory","Soft, friendly, card-based: a consumer-grade feel for a developer tool.",
  "Leads with a plain-language headline that says what is wrong, then rings and cards you already know how to read. Devices live in the sidebar as first-class objects, so switching between an iPhone, an iPad and an Android phone is one click.",
  "Rings and rounded cards hide precision. Power users will want the raw table view within a week, so this direction needs a dense mode behind it."),
 ("e","TerminalNative","Terminal Native","Pure monospace, tmux-style panes, keyboard-first.",
  "Zero ambiguity about what this app is for. Numbered panes, a filter command line, and one-key export make it the fastest direction to operate once learned, and it is trivially theme-able.",
  "Highest learning curve and the least visual hierarchy: everything is the same weight, so the app relies on color to say what matters."),
 ("f","TimelineStudio","Timeline Studio","Time is the organizing axis: stacked tracks, a playhead, a log that scrubs with it.",
  "Most device problems are correlations: memory warning after video playback, thermal state after a sync. Putting CPU, memory, thermal, network and events on one time axis, with the log following the playhead, makes those relationships visible instead of inferred.",
  "Less good for the pure 'right now' glance, and it is the most complex to build: every metric needs history and the log needs time-indexed navigation."),
]
# 1. .dc.html working files + canvas.json
boards=[]; notes=[]
for i,(k,stem,name,axis,why,trade) in enumerate(designs):
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
    col=i%3; row=i//3
    x=col*1560; y=row*1120
    boards.append({"file":f"{stem}.dc.html","x":x,"y":y,"w":1440,"h":900,"title":f"{chr(65+i)} · {name}"})
    notes.append({"id":f"note-{k}","x":x,"y":y-150,"w":700,"text":f"{chr(65+i)} · {name}\n{axis}\n\nTradeoff: {trade}"})
json.dump({"artboards":boards,"annotations":notes,"launch":{"view":"canvas"}},open(os.path.join(W,"canvas.json"),"w"),indent=2)

# 2. gallery page
secs=[]
for i,(k,stem,name,axis,why,trade) in enumerate(designs):
    css=open(os.path.join(D,f"{k}.css")).read(); body=open(os.path.join(D,f"{k}.body.html")).read()
    secs.append(f'''
<section class="dir" id="dir-{k}">
  <div class="intro">
    <div class="letter">{chr(65+i)}</div>
    <div>
      <h2>{name}</h2>
      <p class="axis">{axis}</p>
      <p><strong>Why it works.</strong> {why}</p>
      <p><strong>Tradeoff.</strong> {trade}</p>
    </div>
    <button class="sz" type="button" data-target="fr-{k}">Actual size</button>
  </div>
  <style>{css}</style>
  <div class="frame" id="fr-{k}"><div class="scaler">{body}</div></div>
</section>''')
page = f'''<title>DeviceLog Directions</title>
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
.hero .covers{{display:flex;flex-wrap:wrap;gap:6px;margin-top:18px}}
.hero .covers span{{font-size:12.5px;padding:4px 10px;border-radius:999px;background:var(--chip);color:var(--ink)}}
nav{{position:sticky;top:0;z-index:10;background:color-mix(in srgb,var(--bg) 88%,transparent);backdrop-filter:blur(12px);border-bottom:1px solid var(--line)}}
nav ul{{list-style:none;margin:0 auto;padding:0 32px;max-width:1160px;display:flex;gap:2px;overflow-x:auto}}
nav a{{display:block;padding:12px 14px;color:var(--mut);text-decoration:none;font-weight:500;font-size:14px;white-space:nowrap;border-bottom:2px solid transparent}}
nav a:hover,nav a:focus-visible{{color:var(--ink);outline:none;border-bottom-color:var(--acc)}}
nav a b{{color:var(--acc);font-family:"IBM Plex Mono",monospace;margin-right:6px;font-weight:600}}
.dir{{max-width:1160px;margin:0 auto;padding:44px 32px 20px;border-top:1px solid var(--line)}}
.intro{{display:grid;grid-template-columns:64px 1fr auto;gap:20px;align-items:start;margin-bottom:22px}}
.letter{{font-family:"IBM Plex Mono",monospace;font-size:40px;font-weight:600;color:var(--acc);line-height:1}}
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
  <h1>Six directions for the device monitor</h1>
  <p>Same app, same sample session, six different answers to how it should look and feel. Every screen shows an iPhone under test with an iPad and an Android phone connected alongside, and every screen covers the full feature list below. Pick the one you want to live in and I will build it out.</p>
  <p>All figures are sample data from an invented session; the app is a debug build called FieldKit standing in for whatever you will be testing.</p>
  <div class="covers"><span>Multiple devices at once</span><span>iPhone · iPad · Android</span><span>CPU per core</span><span>Memory breakdown &amp; pressure</span><span>Thermal state &amp; sensors</span><span>Battery voltage, current, health, cycles</span><span>Storage</span><span>Network speed &amp; packets</span><span>Processes</span><span>Log panel with levels</span><span>Per-app filtering</span><span>Export</span><span>Device info</span><span>Sessions / history</span></div>
</div>
<nav><ul>
<li><a href="#dir-a"><b>A</b>Mission Control</a></li><li><a href="#dir-b"><b>B</b>Field Notebook</a></li><li><a href="#dir-c"><b>C</b>Swiss Grid</a></li><li><a href="#dir-d"><b>D</b>Glass Observatory</a></li><li><a href="#dir-e"><b>E</b>Terminal Native</a></li><li><a href="#dir-f"><b>F</b>Timeline Studio</a></li>
</ul></nav>
{''.join(secs)}
<footer>Each direction is a single 1440×900 desktop screen. Frames are scaled to fit this page; use “Actual size” to see one at true pixel size. Directions can be mixed: for example Timeline Studio's tracks inside Mission Control's chrome, or Swiss Grid's typography on Glass Observatory's layout.</footer>
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
open(os.path.join(S,"devicelog-directions.html"),"w").write(page)
print("gallery bytes", len(page))
