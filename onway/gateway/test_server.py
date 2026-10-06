import hashlib
import json
import tempfile
import threading
import unittest
from urllib import request,error
from pathlib import Path
from .server import Gateway,Quota,Rejected,create_server

TOKEN='test-access-token-for-unit-tests-only-012345'

class FakeMap:
    def query(self,path,**params):return {'status':'1','path':path,'params':params}

class FakeAI:
    def analyze(self,task,context):return {'task':task,'action':'memory','summary':'测试记忆','reason':'依据记录','uncertainty':'样本有限'}

class GatewayTests(unittest.TestCase):
    def setUp(self):
        self.directory=tempfile.TemporaryDirectory()
        self.path=Path(self.directory.name)/'usage.sqlite3'
        self.quota=Quota(self.path,{'ai':(2,3,10),'amap':(2,3,10)})
        self.gateway=Gateway([hashlib.sha256(TOKEN.encode()).hexdigest()],self.quota,FakeMap(),FakeAI())
        self.server=create_server(self.gateway,0)
        threading.Thread(target=self.server.serve_forever,daemon=True).start()
    def tearDown(self):
        self.server.shutdown();self.server.server_close();self.directory.cleanup()
    def post(self,path,payload,token=TOKEN):
        req=request.Request('http://127.0.0.1:'+str(self.server.server_port)+path,json.dumps(payload).encode(),{'Content-Type':'application/json','Authorization':'Bearer '+token})
        try:
            with request.urlopen(req) as response:return response.status,json.load(response)
        except error.HTTPError as response:
            with response:return response.code,json.load(response)
    def test_auth_and_fixed_paths(self):
        self.assertEqual(self.post('/v1/amap',{},'invalid')[0],401)
        self.assertEqual(self.post('/v1/amap',{'path':'https://attacker.example','params':{'key':'secret'}})[0],400)
        self.assertEqual(self.post('/v1/amap',{'path':'v3/place/text','params':{'key':'secret'}})[0],400)
        code,value=self.post('/v1/amap',{'path':'v3/place/text','params':{'keywords':'北京站'}})
        self.assertEqual(code,200);self.assertEqual(value['data']['params'],{'keywords':'北京站'})
    def test_task_context_and_size(self):
        self.assertEqual(self.post('/v1/ai',{'task':'chat','context':{}})[0],400)
        self.assertEqual(self.post('/v1/ai',{'task':'memory','context':{'model':'expensive'}})[0],400)
        self.assertEqual(self.post('/v1/ai',{'task':'memory','context':{'history':[{}]*31}})[0],400)
        self.assertEqual(self.post('/v1/ai',{'task':'memory','context':{'memory':'x'*100000}})[0],413)
        self.assertEqual(self.post('/v1/ai',{'task':'memory','context':{'history':[]}})[0],200)
        self.assertEqual(self.post('/v1/ai',{'task':'recommend','context':{'history':{},'routes':[]}})[0],200)
    def test_quota_survives_restart_and_is_atomic(self):
        owner=self.gateway.authenticate('Bearer '+TOKEN)
        self.quota.consume(owner,'ai');self.quota.consume(owner,'ai')
        with self.assertRaises(Rejected):Quota(self.path,{'ai':(2,3,10)}).consume(owner,'ai')
    def test_errors_never_echo_secret(self):
        def broken(*args,**kwargs):
            from backend.v060.services import ServiceError
            raise ServiceError('LEAKED_UPSTREAM_SECRET')
        self.gateway.amap.query=broken
        code,value=self.post('/v1/amap',{'path':'v3/place/text','params':{'keywords':'北京站'}})
        self.assertEqual(code,502);self.assertNotIn('LEAKED',json.dumps(value))
