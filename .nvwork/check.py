import json,sys
d=json.load(sys.stdin)
print('status   :',d['status'],'| error:',d.get('error'))
s=d.get('server') or {}
print('server   :',s.get('name'),'| provider:',s.get('provider'))
print('download : %.2f Mbps (peak %.1f)'%(d['download_mbps'],d['download_peak_mbps']))
u=d.get('upload_mbps'); up=d.get('upload_peak_mbps')
print('upload   :', ('%.2f Mbps (peak %.1f)'%(u,up)) if u is not None else 'None')
print('ping     : %.1f ms  jitter %.2f'%(d['idle_ping_ms'],d['jitter_ms']))
print('loss/http:',d['packet_loss_percent'],'/',d['http_failures'])
q=d.get('quality') or {}
print('score    :', q.get('score'), q.get('rating'))
for n in (q.get('notes') or [])[:2]: print('  note:',n)
print('samples  :', len(d.get('down_samples') or []),'/',len(d.get('up_samples') or []))
