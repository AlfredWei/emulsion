"""Writes out/labeler.html: a page for marking each sampled face's two eyes (o open / c closed / h half / x cannot tell),
keyboard first, autosaving in the browser, exporting labels.csv in the format blink_score.py reads.
Needs out/faces.json, out/assets/ (embedded into the page) (blink_measure.py with ONLY=only.json) and only.json. It shows no model output.
Usage: python make_labeler.py [only.json]; then open out/labeler.html (one self-contained file; nothing is uploaded anywhere)."""
import base64, json, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
OUT_DIR = os.environ.get("OUT_DIR") or os.path.join(HERE, "out")
BATCH = os.environ.get("BATCH", "1")   # names the browser storage and the exported file, so two batches never mix
only = json.load(open(sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "only.json")))
by_id = {r["id"]: r for r in json.load(open(os.path.join(OUT_DIR, "faces.json")))}
def uri(name, mime):   # the pictures are embedded, so the page works wherever it is opened or copied to
    return f"data:{mime};base64," + base64.b64encode(open(os.path.join(OUT_DIR, "assets", name), "rb").read()).decode()
faces = [dict(id=i, photo=by_id[i]["photo"], w=round(by_id[i]["face_w_px"]), f=uri(f"{i:03d}_face.jpg", "image/jpeg"),
              l=uri(f"{i:03d}_image_left.png", "image/png"), r=uri(f"{i:03d}_image_right.png", "image/png"))
         for i in only if i in by_id and os.path.exists(os.path.join(OUT_DIR, "assets", f"{i:03d}_face.jpg"))]
html = r"""<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Blink labels</title>
<style>
:root{--bg:#161412;--panel:#201d1a;--line:#3a352f;--fg:#ece6dc;--dim:#9a9286;--o:#4aa86b;--c:#d6553f;--h:#d9a441;--x:#7a86a6;--acc:#f2a53a}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--fg);font:15px/1.4 system-ui,-apple-system,sans-serif}
header{display:flex;gap:16px;align-items:center;padding:10px 16px;border-bottom:1px solid var(--line);flex-wrap:wrap}
header h1{font-size:16px;margin:0;font-weight:600}.bar{flex:1;min-width:160px;height:8px;background:var(--line);border-radius:4px;overflow:hidden}
.bar i{display:block;height:100%;width:0;background:var(--acc);transition:width .15s}
button{font:inherit;color:var(--fg);background:var(--panel);border:1px solid var(--line);border-radius:6px;padding:6px 12px;cursor:pointer}button:hover{border-color:var(--acc)}
main{display:flex;gap:20px;justify-content:center;align-items:flex-start;padding:18px 16px 8px;flex-wrap:wrap}
.face{background:var(--panel);border:1px solid var(--line);border-radius:10px;padding:10px;text-align:center}
.face img{width:340px;max-width:100%;height:auto;border-radius:6px;display:block}
.meta{color:var(--dim);font-size:13px;margin-top:6px}
.eyes{display:flex;flex-direction:row;gap:14px;flex-wrap:wrap}
.eye{background:var(--panel);border:2px solid var(--line);border-radius:10px;padding:10px;width:290px;opacity:.55}.eye.on{border-color:var(--acc);opacity:1}
.eye h2{margin:0 0 6px;font-size:13px;font-weight:600;color:var(--dim);text-transform:uppercase;letter-spacing:.06em}
.eye img{width:100%;height:auto;aspect-ratio:1;border-radius:6px;display:block;background:#000}
.btns{display:grid;grid-template-columns:repeat(4,1fr);gap:6px;margin-top:8px}
.btns button{padding:8px 0;font-weight:600}.btns button[data-v=o]{color:var(--o)}.btns button[data-v=c]{color:var(--c)}.btns button[data-v=h]{color:var(--h)}.btns button[data-v=x]{color:var(--x)}
.btns button.sel{color:#111;border-color:transparent}.btns button.sel[data-v=o]{background:var(--o)}.btns button.sel[data-v=c]{background:var(--c)}.btns button.sel[data-v=h]{background:var(--h)}.btns button.sel[data-v=x]{background:var(--x)}
.help{max-width:900px;margin:6px auto 0;padding:0 16px;color:var(--dim);font-size:13px;text-align:center}.help b{color:var(--fg)}
#grid{display:flex;flex-wrap:wrap;gap:4px;max-width:1000px;margin:14px auto 40px;padding:0 16px}
#grid i{width:22px;height:22px;border-radius:4px;border:1px solid var(--line);font:11px/20px system-ui;text-align:center;color:var(--dim);cursor:pointer;font-style:normal}
#grid i.part{background:#4a3f2a}#grid i.done{background:#2f4a38;color:var(--fg)}#grid i.cur{outline:2px solid var(--acc)}
</style></head><body>
<header><h1>Blink labels</h1><span id="count"></span><div class="bar"><i id="fill"></i></div>
<button id="exp">Export labels.csv</button><button id="imp">Import labels.csv</button><button id="rst">Start over</button><input id="file" type="file" accept=".csv" hidden></header>
<main>
<div class="face"><img id="face" alt="face"><div class="meta" id="meta"></div></div>
<div class="eyes">
<div class="eye" id="eL"><h2>Image left eye</h2><img id="iL" alt="image left eye"><div class="btns" data-eye="L"></div></div>
<div class="eye" id="eR"><h2>Image right eye</h2><img id="iR" alt="image right eye"><div class="btns" data-eye="R"></div></div>
</div></main>
<p class="help">Keys: <b>o</b> open, <b>c</b> closed, <b>h</b> half closed, <b>x</b> cannot tell (blur, glasses glare, sunglasses, looking away). Each key marks the highlighted eye and moves on; after the second eye it goes to the next face.
<b>Shift+letter</b> (O C H X) marks both eyes at once. <b>Backspace</b> steps back, <b>Tab</b> switches eye, <b>&larr; &rarr;</b> move between faces. "Image left / right" is the left / right side of the picture, not the person's. Your marks save in this browser as you go.</p>
<div id="grid"></div>
<script>
const FACES=__FACES__;
const BATCH=__BATCH__;const KEY="blink-labels-v1-"+BATCH;let L={},cur=0,eye="L";
try{L=JSON.parse(localStorage.getItem(KEY)||"{}")}catch(e){}
const $=s=>document.querySelector(s);
function save(){try{localStorage.setItem(KEY,JSON.stringify(L))}catch(e){}}
function get(i,s){return (L[FACES[i].id]||{})[s]||""}
function set(i,s,v){const id=FACES[i].id;L[id]=L[id]||{};if(v)L[id][s]=v;else delete L[id][s];save()}
function done(i){return get(i,"L")&&get(i,"R")}
function firstOpen(){const k=FACES.findIndex((_,i)=>!done(i));return k<0?0:k}
for(const b of document.querySelectorAll(".btns"))for(const v of "ochx"){const x=document.createElement("button");x.dataset.v=v;x.textContent=v.toUpperCase();x.onclick=()=>{eye=b.dataset.eye;mark(v)};b.appendChild(x)}
function render(){
  const f=FACES[cur];$("#face").src=f.f;$("#iL").src=f.l;$("#iR").src=f.r;
  $("#meta").textContent="Face "+(cur+1)+" of "+FACES.length+"  ·  "+f.w+" px wide  ·  "+f.photo;
  for(const s of ["L","R"]){$("#e"+s).classList.toggle("on",eye===s);for(const b of $("#e"+s+" .btns").children)b.classList.toggle("sel",b.dataset.v===get(cur,s))}
  const n=FACES.filter((_,i)=>done(i)).length;$("#count").textContent=n+" / "+FACES.length+" faces";$("#fill").style.width=(100*n/FACES.length)+"%";
  [...$("#grid").children].forEach((c,i)=>{c.className=(done(i)?"done":(get(i,"L")||get(i,"R"))?"part":"")+(i===cur?" cur":"")})
}
function mark(v,both){
  if(both){set(cur,"L",v);set(cur,"R",v);next();return}
  set(cur,eye,v);const other=eye==="L"?"R":"L";
  if(!get(cur,other)){eye=other;render()}else next()
}
function next(){if(cur<FACES.length-1){cur++;eye=get(cur,"L")?(get(cur,"R")?"L":"R"):"L"}render()}
function prev(){if(eye==="R"&&cur>=0&&!(get(cur,"R"))&&get(cur,"L")){set(cur,"L","");eye="L";render();return}
  if(eye==="R"){set(cur,"R","");eye="L";render();return}
  if(cur>0){cur--;eye="R"}render()}
addEventListener("keydown",e=>{
  if(e.metaKey||e.ctrlKey||e.altKey)return;const k=e.key;
  if("ochx".includes(k)){mark(k);e.preventDefault()}
  else if("OCHX".includes(k)&&k.length===1){mark(k.toLowerCase(),true);e.preventDefault()}
  else if(k==="Backspace"){prev();e.preventDefault()}
  else if(k==="Tab"){eye=eye==="L"?"R":"L";render();e.preventDefault()}
  else if(k==="ArrowRight"||k===" "){if(cur<FACES.length-1){cur++;eye="L";render()}e.preventDefault()}
  else if(k==="ArrowLeft"){if(cur>0){cur--;eye="L";render()}e.preventDefault()}
});
for(let i=0;i<FACES.length;i++){const c=document.createElement("i");c.textContent=i+1;c.onclick=()=>{cur=i;eye="L";render()};$("#grid").appendChild(c)}
function toCsv(){const rows=["id,photo,face_w_px,image_left,image_right"];
  for(const f of FACES){const l=L[f.id]||{};rows.push([f.id,'"'+f.photo.replace(/"/g,'""')+'"',f.w,l.L||"",l.R||""].join(","))}
  return rows.join("\n")+"\n"}
$("#exp").onclick=()=>{
  const n=FACES.filter((_,i)=>done(i)).length;
  if(n<FACES.length&&!confirm(n+" of "+FACES.length+" faces are fully marked. Export anyway? (unmarked eyes stay empty)"))return;
  const a=document.createElement("a");a.href=URL.createObjectURL(new Blob([toCsv()],{type:"text/csv"}));a.download=BATCH==="1"?"labels.csv":"labels_"+BATCH+".csv";a.click()};
$("#imp").onclick=()=>$("#file").click();
$("#file").onchange=async e=>{const t=await e.target.files[0].text();const lines=t.trim().split(/\r?\n/).slice(1);
  for(const ln of lines){const m=ln.match(/^(\d+),(?:"(?:[^"]|"")*"|[^,]*),[^,]*,([ochx]?),([ochx]?)\s*$/);if(m){L[m[1]]={...(m[2]?{L:m[2]}:{}),...(m[3]?{R:m[3]}:{})}}}
  save();cur=firstOpen();eye="L";render()};
$("#rst").onclick=()=>{if(confirm("Clear all marks in this browser?")){L={};save();cur=0;eye="L";render()}};
cur=firstOpen();render();
</script></body></html>"""
html = html.replace("__FACES__", json.dumps(faces)).replace("__BATCH__", json.dumps(BATCH))
open(os.path.join(OUT_DIR, "labeler.html"), "w").write(html)
print(len(faces), "faces ->", os.path.join(OUT_DIR, "labeler.html"))
