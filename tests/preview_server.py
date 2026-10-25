from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
class Handler(SimpleHTTPRequestHandler):
    def __init__(self,*args,**kwargs):
        super().__init__(*args,directory=str(ROOT/'web'),**kwargs)
    def do_GET(self):
        if self.path=='/':
            data=(ROOT/'web/index.html').read_text().replace('<!--SSR_HEAD-->','<title>Rust UI Platform mock preview</title><meta name="robots" content="noindex">').replace('<!--SSR_SCHEMA-->','').replace('<script type="module" src="/app.js"></script>','<script src="/mock-engine.js"></script><script type="module" src="/app.js"></script>')
            self.send_response(200);self.send_header('Content-Type','text/html; charset=utf-8');self.end_headers();self.wfile.write(data.encode())
        else:super().do_GET()
if __name__=='__main__':
    print('Preview only; this page does not execute Rust.')
    ThreadingHTTPServer(('127.0.0.1',8788),Handler).serve_forever()
