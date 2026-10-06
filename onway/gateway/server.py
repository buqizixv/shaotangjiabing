"""Onway's bounded API proxy. Bind to loopback behind an HTTPS reverse proxy."""
import argparse
from contextlib import closing
import hashlib
import hmac
import json
import os
from pathlib import Path
import re
import sqlite3
import sys
import threading
import time
from datetime import datetime, timezone, timedelta
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from backend.v060.services import Amap, MiniMax, ServiceError

PATHS = {
    'v3/place/text': {'keywords','city','citylimit','offset','extensions'},
    'v3/geocode/regeo': {'location','extensions'},
    'v3/assistant/coordinate/convert': {'locations','coordsys'},
    'v3/direction/walking': {'origin','destination'},
    'v3/direction/transit/integrated': {'origin','destination','city','cityd','extensions','strategy'},
    'v3/weather/weatherInfo': {'city','extensions'},
}
TASKS = {'intent','recommend','assist','summary','memory'}
MAX_BODY = 96 * 1024


class EnvVault:
    def get(self, service):
        return os.environ.get({'amap':'AMAP_API_KEY','minimax':'MINIMAX_API_KEY'}[service], '')


class Rejected(Exception):
    def __init__(self, status, message):
        self.status, self.message = status, message


class Quota:
    def __init__(self, path, limits=None, clock=time.time):
        self.path, self.clock = str(path), clock
        self.limits = limits or {
            'ai': (int(os.getenv('AI_DAILY_LIMIT','100')), int(os.getenv('AI_GLOBAL_DAILY_LIMIT','1000')), int(os.getenv('AI_MINUTE_LIMIT','10'))),
            'amap': (int(os.getenv('MAP_DAILY_LIMIT','2000')), int(os.getenv('MAP_GLOBAL_DAILY_LIMIT','10000')), int(os.getenv('MAP_MINUTE_LIMIT','120'))),
        }
        Path(path).parent.mkdir(parents=True, exist_ok=True)
        with closing(sqlite3.connect(self.path)) as db, db:
            db.execute('CREATE TABLE IF NOT EXISTS usage (owner TEXT, service TEXT, period TEXT, count INTEGER, PRIMARY KEY(owner,service,period))')

    def consume(self, owner, service):
        now = self.clock()
        day = datetime.fromtimestamp(now,timezone(timedelta(hours=8))).strftime('%Y-%m-%d')
        individual, total, minute = self.limits[service]
        buckets = [(owner,'day:'+day,individual), ('global','day:'+day,total), (owner,'minute:'+str(int(now//60)),minute)]
        # Zero disables that quota; retain bounded requests and upstream concurrency.
        buckets = [bucket for bucket in buckets if bucket[2] > 0]
        if not buckets:
            return
        with closing(sqlite3.connect(self.path, timeout=5)) as db, db:
            db.execute('BEGIN IMMEDIATE')
            for key, period, limit in buckets:
                row = db.execute('SELECT count FROM usage WHERE owner=? AND service=? AND period=?',(key,service,period)).fetchone()
                if row and row[0] >= limit:
                    raise Rejected(429,'网关调用额度已达上限，请稍后重试')
            for key, period, limit in buckets:
                db.execute('INSERT INTO usage VALUES (?,?,?,1) ON CONFLICT(owner,service,period) DO UPDATE SET count=count+1',(key,service,period))
            # Keep only today's counters; no locations, prompts or key contents are persisted.
            db.execute("DELETE FROM usage WHERE period LIKE 'day:%' AND period != ?",('day:'+day,))
            db.execute("DELETE FROM usage WHERE period LIKE 'minute:%' AND period != ?",('minute:'+str(int(now//60)),))


class Gateway:
    def __init__(self, tokens, quota, amap=None, minimax=None):
        if not tokens or any(not re.fullmatch(r'[a-f0-9]{64}', token) for token in tokens):
            raise ValueError('ONWAY_TOKEN_HASHES 必须配置访问凭证的 SHA256，不能为空')
        self.tokens, self.quota = tokens, quota
        self.amap = amap or Amap(EnvVault(), gateway=False)
        self.minimax = minimax or MiniMax(EnvVault(), gateway=False)
        self.slots = threading.BoundedSemaphore(8)

    def authenticate(self, header):
        if not header.startswith('Bearer ') or not 24 <= len(header[7:]) <= 256:
            raise Rejected(401,'访问凭证无效')
        digest = hashlib.sha256(header[7:].encode()).hexdigest()
        if not any(hmac.compare_digest(digest, token) for token in self.tokens):
            raise Rejected(401,'访问凭证无效')
        return digest

    def execute(self, path, payload, owner):
        if not isinstance(payload, dict):
            raise Rejected(400,'请求格式不合法')
        if path == '/v1/amap':
            endpoint, params = payload.get('path'), payload.get('params')
            if not isinstance(endpoint,str) or endpoint not in PATHS or not isinstance(params,dict) or not params or set(params)-PATHS[endpoint]:
                raise Rejected(400,'地图接口或参数不允许')
            if any(type(v) not in (str,int,float,bool) or len(str(v))>2048 for v in params.values()):
                raise Rejected(400,'地图参数不合法')
            self.quota.consume(owner,'amap')
            return self.amap.query(endpoint,**params)
        if path == '/v1/ai':
            task, context = payload.get('task'), payload.get('context')
            if not isinstance(task,str) or task not in TASKS or not isinstance(context,dict) or 'task' in context or 'messages' in context or 'model' in context:
                raise Rejected(400,'AI 任务或上下文不允许')
            for field,maximum in [('history',30),('routes',12),('candidates',8)]:
                if field == 'history' and task == 'recommend' and isinstance(context.get(field),dict):
                    if len(context[field]) > maximum:
                        raise Rejected(400,'AI 上下文过大')
                    continue
                if field in context and (not isinstance(context[field],list) or len(context[field])>maximum):
                    raise Rejected(400,'AI 上下文过大')
            self.quota.consume(owner,'ai')
            # The server supplies the fixed application prompt, model and output-token cap.
            return self.minimax.analyze(task,context)
        raise Rejected(404,'接口不存在')


def create_server(gateway, port=8080):
    class Handler(BaseHTTPRequestHandler):
        def setup(self):
            super().setup()
            self.connection.settimeout(30)

        def log_message(self, *args):
            pass

        def reply(self, code, value):
            body = json.dumps(value,ensure_ascii=False).encode()
            self.send_response(code)
            self.send_header('Content-Type','application/json; charset=utf-8')
            self.send_header('Cache-Control','no-store')
            self.send_header('X-Content-Type-Options','nosniff')
            self.send_header('Content-Length',str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self):
            self.reply(200,{'ok':True,'service':'onway-gateway'}) if self.path == '/health' else self.reply(404,{'ok':False,'message':'接口不存在'})

        def do_POST(self):
            acquired = False
            try:
                owner = gateway.authenticate(self.headers.get('Authorization',''))
                if self.path not in {'/v1/amap','/v1/ai'}:
                    raise Rejected(404,'接口不存在')
                if self.headers.get('Transfer-Encoding') or self.headers.get_content_type() != 'application/json':
                    raise Rejected(400,'只接受 JSON 请求')
                length = int(self.headers.get('Content-Length','0'))
                if not 0 < length <= MAX_BODY:
                    raise Rejected(413,'请求内容过大或为空')
                raw = self.rfile.read(length)
                if len(raw) != length:
                    raise Rejected(400,'请求内容不完整')
                payload = json.loads(raw)
                acquired = gateway.slots.acquire(blocking=False)
                if not acquired:
                    raise Rejected(503,'网关繁忙，请稍后重试')
                self.reply(200,{'ok':True,'data':gateway.execute(self.path,payload,owner)})
            except Rejected as exc:
                self.reply(exc.status,{'ok':False,'message':exc.message})
            except (ValueError,UnicodeError,RecursionError):
                self.reply(400,{'ok':False,'message':'请求格式不合法'})
            except ServiceError:
                self.reply(502,{'ok':False,'message':'上游接口暂时不可用，请检查服务器配置或稍后重试'})
            except Exception:
                self.reply(503,{'ok':False,'message':'网关暂时不可用'})
            finally:
                if acquired:gateway.slots.release()

    return ThreadingHTTPServer(('127.0.0.1',port),Handler)


if __name__ == '__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--port',type=int,default=8080)
    args=parser.parse_args()
    tokens=[v.strip() for v in os.getenv('ONWAY_TOKEN_HASHES','').split(',') if v.strip()]
    gateway=Gateway(tokens,Quota(os.getenv('ONWAY_QUOTA_DB','/var/lib/onway-gateway/usage.sqlite3')))
    create_server(gateway,args.port).serve_forever()
