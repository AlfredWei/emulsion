import sys, json
sys.argv_ = sys.argv
exec(open("bench_ort.py").read().split("res={}")[0])
name, prov = sys.argv[1], sys.argv[2]
for n,p,f in cases:
    if n==name:
        print(n, prov, json.dumps(run(p,f,CPU if prov=="cpu" else ML, n=3)), flush=True)
