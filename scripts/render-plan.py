#!/usr/bin/env python3
"""Render docs/PLAN.md phases into a progress page. Usage: render-plan.py <out.html>"""
import re, sys, html, datetime, pathlib, os
root = pathlib.Path(__file__).resolve().parents[1]
plan = (root / "docs" / "PLAN.md").read_text()
status = re.search(r"Status line \(keep current\): \*\*(.+?)\*\*", plan).group(1)
phases = []
for m in re.finditer(r"^### (Phase \d+ — .+?)\n(.*?)(?=^### |^---|\Z)", plan, re.M | re.S):
    title, body = m.group(1), m.group(2)
    steps, acc = [], ""
    cur = None
    for line in body.splitlines():
        s = re.match(r"^(\d+\.\d+) (.*)$", line)
        if s:
            cur = {"id": s.group(1), "text": s.group(2)}; steps.append(cur)
        elif line.startswith("    ") and cur:
            cur["text"] += " " + line.strip()
        elif line.startswith("Acceptance:"):
            cur = None; acc = line[len("Acceptance:"):].strip()
        elif acc and line.strip() and not line.startswith(("###", "-")) and cur is None:
            acc += " " + line.strip()
    now_ids = set(filter(None, os.environ.get("PLAN_NOW", "").split(",")))
    for st in steps:
        t = st["text"]
        d = re.search(r"· done (\d{4}-\d{2}-\d{2})", t)
        st["state"] = "done" if d else ("now" if ("· in progress" in t or st["id"] in now_ids) else "todo")
        st["date"] = d.group(1) if d else ""
        t = re.sub(r"\s*· (done \d{4}-\d{2}-\d{2}|in progress)", "", t)
        parts = [p.strip() for p in t.split(" · ")]
        st["what"] = parts[0]; st["owner"] = parts[1] if len(parts) > 1 else ""; st["verify"] = " · ".join(parts[2:])
    phases.append({"title": title, "steps": steps, "acc": acc})
def esc(x): return html.escape(x)
cards = []
for p in phases:
    n = len(p["steps"]); done = sum(s["state"] == "done" for s in p["steps"])
    pct = round(100 * done / n) if n else 0
    state = "done" if done == n and n else ("now" if any(s["state"] != "todo" for s in p["steps"]) else "todo")
    rows = "".join(
        f'<li class="step {s["state"]}"><span class="id">{esc(s["id"])}</span><span class="what">{esc(s["what"])}'
        + (f' <em>{esc(s["owner"])}</em>' if s["owner"] else "")
        + (f'<small>{esc(s["verify"])}</small>' if s["verify"] else "")
        + '</span><span class="mark">' + ("✓ " + s["date"] if s["state"] == "done" else ("in progress" if s["state"] == "now" else "")) + "</span></li>"
        for s in p["steps"])
    cards.append(f'''<section class="phase {state}">
<header><h2>{esc(p["title"])}</h2><div class="bar"><i style="width:{pct}%"></i></div><span class="pct">{done}/{n}</span></header>
<ol>{rows}</ol>
<p class="acc"><b>Acceptance</b> {esc(p["acc"])}</p>
</section>''')
total = sum(len(p["steps"]) for p in phases); tdone = sum(s["state"] == "done" for p in phases for s in p["steps"])
page = f'''<title>StatasMon Build Plan</title>
<style>
:root{{--bg:#f3f4f7;--ink:#171b26;--mut:#5f6a80;--line:#dfe3ea;--card:#ffffff;--acc:#178fa6;--ok:#2ea982;--now:#d9822b;--mono:"JetBrains Mono",ui-monospace,Menlo,Consolas,monospace}}
@media (prefers-color-scheme: dark){{:root:not([data-theme="light"]){{--bg:#0f1219;--ink:#e6e9f2;--mut:#8b93a7;--line:#252b39;--card:#161a24;--acc:#5ad0e6;--ok:#6fd39a;--now:#f0b64a}}}}
:root[data-theme="dark"]{{--bg:#0f1219;--ink:#e6e9f2;--mut:#8b93a7;--line:#252b39;--card:#161a24;--acc:#5ad0e6;--ok:#6fd39a;--now:#f0b64a}}
*{{box-sizing:border-box}}body{{background:var(--bg);color:var(--ink);font-family:system-ui,-apple-system,"Segoe UI",sans-serif;font-size:14.5px;line-height:1.5}}
.wrap{{max-width:980px;margin:0 auto;padding:40px 24px 80px}}
h1{{font-size:28px;margin:0 0 4px;letter-spacing:-.02em}}
.status{{color:var(--mut);font-family:var(--mono);font-size:13px}}
.total{{margin:18px 0 28px;display:flex;align-items:center;gap:14px}}
.total .bar{{flex:1}}
.bar{{height:8px;background:var(--line);border-radius:4px;overflow:hidden}}.bar i{{display:block;height:100%;background:var(--ok)}}
.phase{{background:var(--card);border:1px solid var(--line);border-radius:12px;padding:18px 20px;margin-bottom:14px}}
.phase.now{{border-color:var(--now)}}.phase.done{{opacity:.85}}
.phase header{{display:flex;align-items:center;gap:14px;margin-bottom:10px}}
.phase h2{{font-size:17px;margin:0;flex:1}}.phase header .bar{{width:160px}}.pct{{font-family:var(--mono);font-size:12px;color:var(--mut);width:36px;text-align:right}}
ol{{list-style:none;margin:0;padding:0}}
.step{{display:grid;grid-template-columns:44px 1fr 120px;gap:10px;padding:7px 0;border-top:1px solid var(--line);align-items:start}}
.step .id{{font-family:var(--mono);font-size:12.5px;color:var(--mut);padding-top:2px}}
.step .what em{{font-style:normal;color:var(--acc);font-size:12px;margin-left:6px}}
.step .what small{{display:block;color:var(--mut);font-size:12px}}
.step .mark{{font-family:var(--mono);font-size:12px;text-align:right}}
.step.done .mark{{color:var(--ok)}}.step.now .mark{{color:var(--now)}}
.step.done .what{{text-decoration:line-through;text-decoration-color:var(--line)}}
.acc{{margin:10px 0 0;color:var(--mut);font-size:13px}}.acc b{{color:var(--ink);font-weight:600}}
.legend{{display:flex;gap:16px;color:var(--mut);font-size:12.5px;margin-bottom:16px}}.legend i{{display:inline-block;width:10px;height:10px;border-radius:50%;margin-right:6px;vertical-align:-1px}}
</style>
<div class="wrap">
<h1>StatasMon build plan</h1>
<div class="status">{esc(status)} · rendered {datetime.date.today().isoformat()}</div>
<div class="total"><div class="bar"><i style="width:{round(100*tdone/total) if total else 0}%"></i></div><span class="pct" style="width:auto">{tdone}/{total} steps</span></div>
<div class="legend"><span><i style="background:var(--ok)"></i>done</span><span><i style="background:var(--now)"></i>in progress</span><span><i style="background:var(--line)"></i>not started</span></div>
{''.join(cards)}
<p class="acc">Full text: docs/PLAN.md in the repository. This page is regenerated from it at every milestone.</p>
</div>'''
pathlib.Path(sys.argv[1]).write_text(page)
print(f"{tdone}/{total} steps done; {len(phases)} phases")
