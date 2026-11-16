"""Generated-loopback smart Git fixture. Credentials stay in a protected temporary file."""
import base64,hmac,json,os,secrets,subprocess,sys,threading
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from urllib.parse import urlsplit

class GitHttpServer(BaseHTTPRequestHandler):
    token=secrets.token_urlsafe(32)
    lock=threading.Lock()
    counts={'authorized':0,'rejected':0,'trap':0}
    def log_message(self,*args):pass
    def response(self,status,body=b'',headers=None):
        self.send_response(status)
        for key,value in (headers or {}).items():self.send_header(key,value)
        self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
    def handle_git(self):
        with self.lock:
            if self.server.trap:self.counts['trap']+=1
            else:
                expected='Basic '+base64.b64encode(('oauth2:'+self.token).encode()).decode()
                valid=hmac.compare_digest(self.headers.get('Authorization',''),expected)
                self.counts['authorized' if valid else 'rejected']+=1
            temporary=self.server.stats.with_suffix('.tmp');temporary.write_text(json.dumps(self.counts));temporary.replace(self.server.stats)
        if self.server.trap:self.response(404);return
        if not valid:self.response(401,headers={'WWW-Authenticate':'Basic realm="fixture"'});return
        parsed=urlsplit(self.path)
        if parsed.path.startswith('/team/redirect.git/'):
            self.response(302,headers={'Location':self.server.trap_origin+'/must-not-follow'});return
        if not parsed.path.startswith('/team/source.git/'):
            self.response(404);return
        length=int(self.headers.get('Content-Length','0'))
        if length>1_048_576 or self.headers.get('Transfer-Encoding'):
            self.response(413);return
        body=self.rfile.read(length)
        environment={'LC_ALL':'C','GIT_CONFIG_NOSYSTEM':'1','GIT_CONFIG_GLOBAL':'/dev/null','GIT_CONFIG_SYSTEM':'/dev/null','GIT_PROJECT_ROOT':str(self.server.root),'GIT_HTTP_EXPORT_ALL':'1','REQUEST_METHOD':self.command,'PATH_INFO':parsed.path,'QUERY_STRING':parsed.query,'CONTENT_TYPE':self.headers.get('Content-Type',''),'CONTENT_LENGTH':str(length),'REMOTE_USER':'fixture','HTTP_GIT_PROTOCOL':self.headers.get('Git-Protocol','')}
        result=subprocess.run(['/usr/bin/git','http-backend'],env=environment,input=body,capture_output=True,timeout=20)
        if result.returncode:self.response(500);return
        raw=result.stdout
        separator=b'\r\n\r\n' if b'\r\n\r\n' in raw else b'\n\n'
        head,content=raw.split(separator,1);headers={};status=200
        for line in head.decode('ascii').splitlines():
            key,value=line.split(':',1)
            if key.lower()=='status':status=int(value.strip().split()[0])
            elif key.lower() not in ['content-length','transfer-encoding']:headers[key]=value.strip()
        self.response(status,content,headers)
    do_GET=handle_git
    do_POST=handle_git
    @classmethod
    def main(cls):
        root=Path(sys.argv[1]);settings=Path(sys.argv[2]);stats=Path(sys.argv[3])
        trap=ThreadingHTTPServer(('127.0.0.1',0),cls);trap.trap=True;trap.stats=stats
        server=ThreadingHTTPServer(('127.0.0.1',0),cls);server.trap=False;server.stats=stats;server.root=root;server.trap_origin=f'http://127.0.0.1:{trap.server_port}'
        stats.write_text(json.dumps(cls.counts));settings.write_text(json.dumps({'origin':f'http://127.0.0.1:{server.server_port}','token':cls.token}));settings.chmod(0o600)
        for value in [trap,server]:threading.Thread(target=value.serve_forever,daemon=True).start()
        print('ready',flush=True)
        sys.stdin.read()
        server.shutdown();trap.shutdown()

if __name__=='__main__':GitHttpServer.main()
