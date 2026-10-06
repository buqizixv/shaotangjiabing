import copy
from concurrent.futures import Future
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock
from .bus import BeijingBus, BusError
from .engine import Engine, atomic_json
from .worker import Worker
from .test_v060 import fixture, FakeVault
from .services import Amap, MiniMax


class BusTests(unittest.TestCase):
    def setUp(self):
        self.now = 1791267796.0
        self.segment = {"mode":"transit", "name":"993路(窦店公交场站--西红门西站)", "from":"良乡西门", "to":"黄辛庄路口"}
        self.config = {"city_id":"027","line_id":"010-993-0","target_order":"17","station_id":"010-9507","lat":"39.729656","lng":"116.131076","direction":"西红门西站","station":"良乡西门"}

    def provider(self, data):
        bus = BeijingBus(transport=lambda url:copy.deepcopy(data), clock=lambda:self.now)
        bus.resolve = lambda segment:self.config
        return bus

    def response(self, buses, real=True):
        return {"line":{"lineId":"010-993-0"}, "targetOrder":17, "realData":real,"buses":buses}

    def test_sort_units_timezone_and_unknown_values(self):
        buses=[{"eta":{"travelTime":180,"arrivalTime":self.now*1000+180000}}, {"eta":None},
            {"eta":{"travelTime":65,"arrivalTime":self.now*1000+65000},"distanceToWaitStn":612},
            {"eta":{"travelTime":-1}}, {"eta":{"travelTime":True}}, {"eta":{"travelTime":float('nan')}}]
        data=self.provider(self.response(buses)).arrivals(self.segment)
        self.assertEqual(data['nearest']['seconds'],65)
        self.assertEqual(data['nearest']['distance'],612)
        self.assertEqual(data['nearest']['arrivalText'],'14:24')
        self.assertEqual(self.provider(self.response(buses,False)).arrivals(self.segment)['status'],'no_prediction')
        self.assertIsNone(self.provider(self.response([{'eta':{'travelTime':0},'distanceToWaitStn':-1}])).arrivals(self.segment)['nearest']['distance'])

    def test_wrong_line_or_station_is_rejected(self):
        for field,value in [('targetOrder',28),('line',{'lineId':'010-993-1'})]:
            data=self.response([]);data[field]=value
            with self.assertRaises(BusError):self.provider(data).arrivals(self.segment)

    def test_resolve_uses_direction_station_order_and_wgs_platform(self):
        stations=[{'sn':'良乡西门','order':17,'sId':'010-9507','wgsLat':39.729656,'wgsLng':116.131076}, {'sn':'黄辛庄路口','order':23}]
        def transport(url):
            if '/search?' in url:return {'lines':[{'name':'993','directions':[{'lineId':'0'},{'lineId':'1'}]}]}
            key='0' if 'line_id=0' in url else '1'
            return {'line':{'lineId':key,'endSn':'西红门西站' if key=='0' else '窦店公交场站'},'stations':stations if key=='0' else list(reversed(stations))}
        bus=BeijingBus(transport,clock=lambda:self.now)
        config=bus.resolve(self.segment)
        self.assertEqual(config['line_id'],'0')
        self.assertEqual(config['lat'],'39.729656')
        self.assertEqual(config['target_order'],'17')
        bus.transport=Mock(side_effect=AssertionError('cache missed'))
        self.assertEqual(bus.resolve(self.segment),config)

    def test_ambiguous_platform_has_no_prediction(self):
        bus=BeijingBus(lambda url:{'lines':[]},clock=lambda:self.now)
        self.assertEqual(bus.arrivals(self.segment)['status'],'no_prediction')

    def setup_worker(self, directory):
        bus=Mock();bus.arrivals.return_value={'status':'predicted','fetchedAt':self.now,'nearest':{'seconds':65,'arrivalText':'14:24','distance':612}}
        worker=Worker(directory,Amap(FakeVault()),MiniMax(FakeVault()),bus=bus)
        engine=worker.engine;engine.clock=lambda:self.now;engine.cfg['tracking']=False
        origin,dest,route=fixture('transit');route['segments'][1].update(self.segment)
        engine.state.update(origin=origin,destination=dest,routes=[route],selected=route['id'])
        engine.command({'id':'start','action':'start','payload':{}})
        return worker,bus

    def test_shared_polling_visibility_refresh_and_stale_result(self):
        with tempfile.TemporaryDirectory() as tmp:
            w,bus=self.setup_worker(tmp)
            try:
                w.poll_bus();self.assertFalse(w.jobs)
                atomic_json(Path(tmp)/'bus-view.json',{'epoch':self.now})
                w.poll_bus();w.jobs['bus-arrivals'][0].result();w.tick()
                self.assertEqual(bus.arrivals.call_count,1)
                w.poll_bus();self.assertEqual(bus.arrivals.call_count,1)
                w.engine.command({'id':'depart','action':'correct','payload':{'stage':'departed'}})
                w.engine.command({'id':'wait','action':'correct','payload':{'stage':'waiting','segment':1}})
                self.assertEqual(w.engine.view()['card']['visual']['hero'],'2')
                self.now+=91
                self.assertIn('数据已过期',w.engine.view()['card']['visual']['connectionStatus'])
                w.poll_bus();self.assertNotIn('bus-arrivals',w.jobs)
                atomic_json(Path(tmp)/'bus-view.json',{'epoch':self.now})
                w.poll_bus();self.assertIn('bus-arrivals',w.jobs)
                w.jobs['bus-arrivals'][0].result();w.tick()
                w.engine.command({'id':'last','action':'correct','payload':{'stage':'lastwalk','segment':2}})
                w.poll_bus();self.assertIsNone(w.engine.state['busArrival'])
                self.assertFalse(w.engine.view()['card']['visual']['busVisible'])
            finally:w.pool.shutdown()

    def test_old_trip_response_ignored_and_failure_backs_off(self):
        with tempfile.TemporaryDirectory() as tmp:
            w,bus=self.setup_worker(tmp)
            try:
                w.poll_bus();old=w.bus_marker
                future=Future();future.set_result({'status':'predicted','fetchedAt':self.now,'nearest':{'seconds':10,'arrivalText':'14:23','distance':10}})
                w.jobs['bus-arrivals']=(future,old)
                w.engine.state['trip']=None;w.poll_bus();w.tick()
                self.assertIsNone(w.engine.state.get('busArrival'))
                origin,dest,route=fixture('transit');route['segments'][1].update(self.segment)
                w.engine.state.update(routes=[route],selected=route['id'])
                w.engine.command({'id':'start-again','action':'start','payload':{}})
                w.poll_bus();future=Future();future.set_exception(BusError('查询失败',60))
                w.jobs['bus-arrivals']=(future,w.bus_marker);w.tick()
                self.assertEqual(w.bus_due,self.now+60)
                self.assertEqual(w.engine.state['busArrival']['status'],'error')
            finally:w.pool.shutdown()

    def test_subway_and_pure_walk_do_not_query_bus(self):
        with tempfile.TemporaryDirectory() as tmp:
            w,bus=self.setup_worker(tmp)
            try:
                w.engine.state['trip']['route']['segments'][1]['name']='地铁房山线'
                self.assertIsNone(w.engine.bus_target())
                self.assertFalse(w.engine.view()['card']['visual']['busVisible'])
            finally:w.pool.shutdown()


if __name__=='__main__':unittest.main()
