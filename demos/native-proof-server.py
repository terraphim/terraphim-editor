"""Loopback-only native Safari proof harness; production assets stay unchanged.

The injected observer captures real events and snapshots only. It never dispatches
editing/drag events. The public fixture is setup input, never a successful output.
"""
import argparse
import hashlib
import json
import mimetypes
import pathlib
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlsplit
from browser_demo_common import verified_assets

SCRIPT = r'''<script>
(() => {
 let events=[],totalEvents=0,receiptID=0;
 for(const type of ['pointerdown','pointermove','pointerup','pointercancel','mousedown','mousemove','mouseup','dragstart','dragover','drop','dragend']) document.addEventListener(type,e=>{
   totalEvents++;if(events.length>=1200)return;
   const t=e.target.nodeType===3?e.target.parentElement:e.target;
   events.push({type,trusted:e.isTrusted,x:e.clientX,y:e.clientY,
     target:t.closest('.te-overflow')?'overflow':t.closest('[contenteditable]')?'surface':'other',
     buttons:e.buttons,pointerType:e.pointerType||null,payload:type==='drop'?e.dataTransfer?.getData('text/plain'):null});
 },{capture:true});
 const ready=setInterval(()=>{
   const ed=window.terraphimEditor;if(!ed?.surface||!window.wasmBindings)return;
   clearInterval(ready);
   const button=document.createElement('button');button.textContent='Record actual output';
   button.style='position:fixed;bottom:45px;left:8px;z-index:100000';
   button.onclick=async()=>{
     button.disabled=true;
     const count=events.length,windowTotal=totalEvents;
     try {
       const snapshot={body:ed.surface.getText(),source:ed.persistence.serialise(),
         annotations:ed.annotations(),events:events.slice(),
         receiptID:++receiptID,totalEvents:windowTotal,traceTruncated:windowTotal>count,observedAt:new Date().toISOString()};
       const response=await fetch('/receipt',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(snapshot)});
       if(response.ok){events.splice(0,count);totalEvents-=windowTotal;}
       button.textContent=response.ok?'Output recorded':'Recording failed';
     } catch(error) { button.textContent='Recording failed'; }
     finally { button.disabled=false; }
     setTimeout(()=>button.textContent='Record actual output',1200);
   };document.body.append(button);
 },100);
})();</script>'''


def serve(assets, output):
    """Serve frozen manifest bytes and append observed receipts to a fresh file."""
    lock = threading.Lock()
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass
        def do_GET(self):
            name = urlsplit(self.path).path.lstrip('/') or 'index.html'
            if name not in assets:
                self.send_error(404); return
            data = assets[name]
            if name == 'index.html':
                data = data.replace(b'<title>Markdown Editor</title>',b'<title>Terraphim Native Proof</title>')
                data = data.replace(b'</body>',SCRIPT.encode()+b'</body>')
            self.send_response(200)
            self.send_header('Content-Type',mimetypes.guess_type(name)[0] or 'application/octet-stream')
            self.send_header('Content-Length',str(len(data)))
            self.send_header('Cache-Control','no-store')
            self.end_headers(); self.wfile.write(data)
        def do_POST(self):
            if self.path != '/receipt' or self.headers.get('Origin') != f'http://127.0.0.1:{self.server.server_port}':
                self.send_error(403); return
            try:
                size = int(self.headers.get('Content-Length','0'))
                if not 0 < size <= 2*1024*1024: raise ValueError('size')
                value = json.loads(self.rfile.read(size))
                if set(value) != {'body','source','annotations','events','receiptID','totalEvents','traceTruncated','observedAt'}: raise ValueError('schema')
                if not isinstance(value['body'],str) or not isinstance(value['source'],str): raise ValueError('text')
                if not isinstance(value['events'],list) or len(value['events'])>1200: raise ValueError('events')
                if type(value['receiptID']) is not int or value['receiptID']<1: raise ValueError('receipt')
                if type(value['totalEvents']) is not int or value['totalEvents']<len(value['events']): raise ValueError('total')
                if value['traceTruncated'] is not (value['totalEvents']>len(value['events'])): raise ValueError('truncation')
                value['sourceSha256'] = hashlib.sha256(value['source'].encode()).hexdigest()
                with lock:
                    with output.open('a') as stream: stream.write(json.dumps(value)+'\n')
            except (ValueError,TypeError):
                self.send_error(400); return
            self.send_response(204); self.end_headers()
    return ThreadingHTTPServer(('127.0.0.1',0),Handler)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--delivery',type=pathlib.Path,required=True)
    parser.add_argument('--manifest',type=pathlib.Path,required=True)
    parser.add_argument('--output',type=pathlib.Path,required=True)
    args=parser.parse_args()
    assets=verified_assets(args.delivery,json.loads(args.manifest.read_text()))
    with args.output.open('x'): pass
    server=serve(assets,args.output)
    print(f'http://127.0.0.1:{server.server_port}/',flush=True)
    server.serve_forever()

if __name__=='__main__': main()
