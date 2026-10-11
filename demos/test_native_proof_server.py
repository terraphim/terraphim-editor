"""Real loopback proof transport tests; no browser or mock application."""
import importlib.util
import json
from pathlib import Path
import tempfile
import threading
import unittest
import urllib.request
import urllib.error
spec=importlib.util.spec_from_file_location('native_proof',Path(__file__).with_name('native-proof-server.py'))
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)

class NativeProofTests(unittest.TestCase):
    def test_real_transport_preserves_assets_and_records_exact_snapshot(self):
        with tempfile.TemporaryDirectory() as directory:
            output=Path(directory)/'proof.jsonl'
            server=module.serve({'index.html':b'<title>Markdown Editor</title></body>','x.js':b'unchanged'},output)
            worker=threading.Thread(target=server.serve_forever,daemon=True);worker.start()
            base=f'http://127.0.0.1:{server.server_port}'
            try:
                self.assertEqual(urllib.request.urlopen(base+'/x.js').read(),b'unchanged')
                observed=urllib.request.urlopen(base+'/').read().decode()
                self.assertIn('Terraphim Native Proof',observed)
                self.assertIn("'pointerdown','pointermove','pointerup','pointercancel'",observed)
                self.assertIn('buttons:e.buttons,pointerType:e.pointerType',observed)
                self.assertNotIn('dispatchEvent',observed)
                value={'body':'Public text.','source':'Public text.','annotations':{},'events':[{'type':'drop','trusted':True}],'receiptID':1,'totalEvents':1,'traceTruncated':False,'observedAt':'2026-10-11T15:00:00Z'}
                data=json.dumps(value).encode()
                for origin,status in [(base,204),('https://elsewhere.invalid',403)]:
                    request=urllib.request.Request(base+'/receipt',data=data,headers={'Origin':origin})
                    try: actual=urllib.request.urlopen(request).status
                    except urllib.error.HTTPError as error: actual=error.code
                    self.assertEqual(actual,status)
                rows=output.read_text().splitlines();self.assertEqual(len(rows),1)
                saved=json.loads(rows[0]);self.assertEqual(saved['body'],value['body']);self.assertEqual(saved['events'],value['events'])
                self.assertEqual(len(saved['sourceSha256']),64)
                request=urllib.request.Request(base+'/receipt',data=b'{}',headers={'Origin':base})
                with self.assertRaises(urllib.error.HTTPError) as error: urllib.request.urlopen(request)
                self.assertEqual(error.exception.code,400)
            finally:
                server.shutdown();server.server_close();worker.join()

if __name__=='__main__':unittest.main()
