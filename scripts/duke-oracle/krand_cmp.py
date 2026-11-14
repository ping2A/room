import sys,subprocess,functools,collections
choc,room=sys.argv[1],sys.argv[2]
@functools.lru_cache(None)
def fn(a): return subprocess.run(['addr2line','-f','-e',__import__('os').environ.get('CHOC','build/chocolate-duke3d'),a],capture_output=True,text=True).stdout.split('\n')[0]
C=collections.defaultdict(list); R=collections.defaultdict(list)
for l in open(choc):
  if l.startswith('#'): continue
  t,a,s,i,*rest=l.split(); C[int(t)+1].append((int(s),a,i+' '+' '.join(rest)))
for l in open(room):
  if l.startswith('#'): continue
  p=l.rstrip('\n').split(' ',2); R[int(p[0])].append((int(p[1]),p[2]))
C[0]=C[-0]; 
for t in sorted(R):
  if t<=0: continue
  cs=[x[0] for x in C[t]]; rs=[x[0] for x in R[t]]
  if cs!=rs:
    print('tic',t,'choc',len(cs),'room',len(rs))
    n=max(len(cs),len(rs))
    k=next((j for j in range(n) if j>=len(cs) or j>=len(rs) or cs[j]!=rs[j]),n)
    for j in range(max(0,k-int(__import__("os").environ.get("KB","5"))),min(n,k+14)):
      c=C[t][j] if j<len(cs) else None; r=R[t][j] if j<len(rs) else None
      mark='  ' if c and r and c[0]==r[0] else '>>'
      print(mark, (f"{c[0]} {fn(c[1])} i={c[2]}" if c else '-').ljust(80), '|', (f"{r[0]} {r[1][-100:]}" if r else '-'))
    break
else: print('room log matches choc for tics', max(R))
