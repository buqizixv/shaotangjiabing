import os
import json
import unittest
import tempfile
from pathlib import Path
from unittest.mock import patch
from .services import Amap,MiniMax,ServiceError
from .test_v060 import FakeVault

class GatewayClientTests(unittest.TestCase):
    def test_packaged_access_works_without_user_setup_and_is_bound_to_url(self):
        from .gateway_client import GatewayClient
        with tempfile.TemporaryDirectory() as directory:
            config=Path(directory)/'gateway-client.json'
            config.write_text(json.dumps({'url':'https://demo.example'}))
            (config.parent/'gateway-access.json').write_text(json.dumps({'url':'https://demo.example','token':'packaged-demo-token-0123456789'}))
            calls=[]
            def transport(url,payload,headers,**options):
                calls.append(headers);return {'ok':True,'data':{}}
            with patch.dict(os.environ,{'ONWAY_GATEWAY_URL':'https://demo.example'}):
                client=GatewayClient(FakeVault({}),transport,config)
                self.assertTrue(client.configured);client.call('amap',{})
                self.assertEqual(calls[-1]['Authorization'],'Bearer packaged-demo-token-0123456789')
                client=GatewayClient(FakeVault({'gateway':'personal-token'}),transport,config)
                client.call('amap',{})
                self.assertEqual(calls[-1]['Authorization'],'Bearer personal-token')
            with patch.dict(os.environ,{'ONWAY_GATEWAY_URL':'https://other.example'}):
                self.assertFalse(GatewayClient(FakeVault({}),transport,config).configured)
    def test_summary_prompt_requires_summary_even_without_candidates(self):
        calls=[]
        def transport(url,payload,headers):
            calls.append(payload)
            result={'task':'summary','action':'summary','summary':'连接测试成功','reason':'测试','uncertainty':'暂无行程'}
            return {'choices':[{'message':{'content':json.dumps(result)}}]}
        ai=MiniMax(FakeVault({'minimax':'test-key'}),transport,gateway=False)
        ai.analyze('summary',{'trip':{'status':'connection_test'}})
        self.assertIn('返回 JSON 的 task 和 action 都必须为 "summary"',calls[0]['messages'][0]['content'])

    def test_map_and_weather_send_only_gateway_token(self):
        calls=[]
        def transport(url,payload,headers,**options):
            calls.append((url,payload,headers));return {'ok':True,'data':{'status':'1'}}
        with patch.dict(os.environ,{'ONWAY_GATEWAY_URL':'https://120.53.104.174'}):
            amap=Amap(FakeVault({'amap':'UPSTREAM_SECRET','gateway':'client-access-token'}),transport)
            self.assertTrue(amap.configured)
            amap.query('v3/weather/weatherInfo',city='110111',extensions='base')
        self.assertNotIn('UPSTREAM_SECRET',str(calls))
        self.assertEqual(calls[0][0],'https://120.53.104.174/v1/amap')
        self.assertEqual(calls[0][2]['Authorization'],'Bearer client-access-token')
    def test_ai_gateway_validates_result(self):
        def transport(url,payload,headers,**options):return {'ok':True,'data':{'task':'memory','action':'memory','summary':'记忆','reason':'依据','uncertainty':'待验证'}}
        with patch.dict(os.environ,{'ONWAY_GATEWAY_URL':'https://gateway.example'}):
            ai=MiniMax(FakeVault({'gateway':'client-access-token'}),transport)
            self.assertEqual(ai.analyze('memory',{})['summary'],'记忆')
    def test_http_rejected_and_missing_token_no_fallback(self):
        with patch.dict(os.environ,{'ONWAY_GATEWAY_URL':'http://120.53.104.174'}):
            with self.assertRaises(ValueError):Amap(FakeVault({'amap':'secret'}))
        with patch.dict(os.environ,{'ONWAY_GATEWAY_URL':'https://gateway.example'}):
            amap=Amap(FakeVault({'amap':'secret'}),lambda *a: self.fail('must not call upstream'))
            self.assertFalse(amap.configured)
            with self.assertRaises(ServiceError):amap.query('v3/place/text',keywords='北京站')
