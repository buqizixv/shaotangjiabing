import tempfile
import unittest
from datetime import date, datetime
from unittest.mock import patch
from .engine import Engine, TZ, memory_sections, atomic_json
from .services import Amap, ServiceError
from .worker import Worker
from .test_v060 import fixture, FakeVault
from .workcalendar import is_workday


class CommuteTests(unittest.TestCase):
    def test_preview_uses_saved_home_and_work_without_saved_commute(self):
        with tempfile.TemporaryDirectory() as root:
            w = Worker(root)
            try:
                origin, dest, route = fixture()
                origin['type'], dest['type'] = 'home', 'work'
                e = w.engine
                e.cfg.update(aiEnabled=True, tracking=False)
                e.cfg['places'] = [origin, dest]
                self.assertEqual(e.cfg['commute']['originId'], '')
                def prepare(candidate, generation):
                    return {'candidate':{**candidate, 'suggestedAt':e.clock(), 'routeId':route['id']}, 'generation':generation,
                            'result':{'action':'intent', 'reason':'准备去上班吗？', 'uncertainty':''}, 'routes':[route]}
                with patch.object(w, 'prepare_intent', side_effect=prepare) as api:
                    w.process_command({'id':'home-preview', 'action':'preview_intent', 'payload':{}})
                    w.jobs['intent-prepare'][0].result()
                    w.tick()
                self.assertEqual(api.call_args.args[0]['origin']['id'], origin['id'])
                self.assertEqual(api.call_args.args[0]['destination']['id'], dest['id'])
                self.assertTrue(e.view()['cardVisible'])
                self.assertTrue(e.state['intent']['preview'])
                self.assertEqual(e.state['previewStatus'], '')
                self.assertEqual(e.hist, [])
                self.assertEqual(e.cfg['commute']['originId'], '')
            finally:
                w.pool.shutdown()

    def test_preview_errors_are_visible_on_home(self):
        with tempfile.TemporaryDirectory() as root:
            w = Worker(root)
            try:
                e = w.engine
                e.cfg.update(aiEnabled=True, tracking=False)
                atomic_json(e.root/'commands/cmd-test.json', {'id':'missing-place', 'action':'preview_intent','payload':{}})
                w.tick()
                self.assertIn('设置家和公司', e.view()['previewStatus'])
                with patch.object(w,'prepare_intent',side_effect=ServiceError('路线查询失败')):
                    origin, dest, _ = fixture()
                    origin['type'], dest['type'] = 'home', 'work'
                    e.cfg['places'] = [origin, dest]
                    w.process_command({'id':'failed-preview', 'action':'preview_intent', 'payload':{}})
                    try:
                        w.jobs['intent-prepare'][0].result()
                    except ServiceError:
                        pass
                    w.tick()
                self.assertIn('推荐生成失败', e.view()['previewStatus'])
                self.assertFalse(e.view()['cardVisible'])
            finally:
                w.pool.shutdown()

    def test_preview_still_displays_real_routes_if_ai_format_fails(self):
        with tempfile.TemporaryDirectory() as root:
            w = Worker(root)
            try:
                origin, dest, route = fixture()
                candidate = {'id':'preview', 'preview':True, 'direction':'outbound', 'origin':origin,
                             'destination':dest,'plannedAt':w.engine.clock(), 'expires':w.engine.clock()+600}
                with patch.object(w.amap,'routes',return_value={'routes':[route]}), patch.object(w.amap,'weather',return_value={'status':'unavailable'}), patch.object(w.minimax,'analyze',side_effect=ServiceError('模型未返回有效结构化结果')):
                    result = w.prepare_intent(candidate, 0)
                self.assertEqual(result['result']['action'], 'intent')
                self.assertIn('AI 建议暂时不可用', result['result']['reason'])
                self.assertFalse(result['routes'][0]['recommended'])
                self.assertEqual(result['candidate']['routeId'], route['id'])
            finally:
                w.pool.shutdown()

    def test_official_holiday_and_makeup_workday(self):
        self.assertFalse(is_workday(date(2026, 10, 6)))
        self.assertTrue(is_workday(date(2026, 10, 10)))
        self.assertFalse(is_workday(date(2026, 10, 11)))
        self.assertIsNone(is_workday(date(2027, 1, 4)))

    def test_custom_weekdays_override_holiday(self):
        with tempfile.TemporaryDirectory() as root:
            now = datetime(2026, 10, 6, 7, 55, tzinfo=TZ).timestamp()
            e = Engine(root, clock=lambda: now)
            origin, dest, route = fixture()
            e.cfg['places'] = [origin, dest]
            e.cfg['aiEnabled'] = True
            e.cfg['commute'].update(originId=origin['id'], destinationId=dest['id'])
            e.state['location'] = {**origin, 'timestamp':now, 'accuracy':5}
            self.assertIsNone(e.intent_candidate())
            e.cfg['commute']['dateMode'] = 'custom'
            self.assertIsNotNone(e.intent_candidate())

    def test_old_memory_is_reordered_without_discarding_sections(self):
        result = memory_sections('历史分析：旧行程\n\n个性化偏好分析：偏好公交', {'mode':'all', 'route':'fast'})
        self.assertEqual(result, {'personal':'偏好公交', 'historical':'旧行程'})

    def test_live_route_weather_and_deadline_are_used(self):
        with tempfile.TemporaryDirectory() as root:
            w = Worker(root)
            try:
                origin, dest, route = fixture()
                now = datetime(2026, 10, 10, 8, 30, tzinfo=TZ).timestamp()
                w.engine.clock = lambda: now
                decision = {'action':'intent', 'routeId':route['id'], 'reason':'准备一下吧', 'uncertainty':''}
                candidate = {'id':'2026-10-10:outbound', 'origin':origin, 'destination':dest,
                             'plannedAt':now, 'direction':'outbound', 'requiredArrival':'09:00', 'expires':now+3600}
                weather = {'status':'available', 'condition':'小雨', 'temperature':'15', 'reportedAt':now}
                with patch.object(w.amap, 'routes', return_value={'routes':[route]}), patch.object(w.amap, 'weather', return_value=weather), patch.object(w.minimax, 'analyze', return_value=decision) as ai:
                    result = w.prepare_intent(candidate, 0)
                self.assertEqual(result['candidate']['suggestedAt'], datetime(2026,10,10,8,35,tzinfo=TZ).timestamp())
                context = ai.call_args.args[1]
                self.assertEqual(context['weather'], weather)
                self.assertIn('preferences', context)
                self.assertIn('memory', context)
                self.assertEqual(result['candidate']['timingNote'], '按路线预估，预留 10 分钟余量。')
            finally:
                w.pool.shutdown()

    def test_preview_and_confirmation_never_start_a_trip(self):
        with tempfile.TemporaryDirectory() as root:
            e = Engine(root)
            origin, dest, route = fixture()
            e.state.update(origin=origin, destination=dest, routes=[route], selected=None)
            e.state['intent'] = {'id':'preview', 'preview':True, 'expires':e.clock()+600}
            e.intent_feedback('intent_decline', {})
            self.assertIsNone(e.state['trip'])
            self.assertEqual(e.hist, [])
            e.state['intent'] = {'id':'actual', 'expires':e.clock()+600, 'origin':origin, 'destination':dest,
                                 'plannedAt':e.clock(), 'direction':'outbound'}
            e.intent_feedback('intent_confirm', {})
            self.assertIsNone(e.state['trip'])
            self.assertEqual(e.state['navigation']['page'], 'routes')
            self.assertFalse(e.view()['cardVisible'])

    def test_weather_mismatch_expired_and_negative_temperature(self):
        amap = Amap(FakeVault())
        origin = {**fixture()[0], 'adcode':'110111'}
        live = {'adcode':'110111', 'city':'房山区', 'weather':'晴', 'temperature':'-5', 'reporttime':'2026-10-06 17:00:00'}
        now = datetime(2026,10,6,17,1,tzinfo=TZ).timestamp()
        with patch.object(amap,'query',return_value={'lives':[live]}), patch('backend.v060.services.time.time',return_value=now):
            self.assertEqual(amap.weather(origin)['temperature'], '-5')
        for bad in ({**live,'adcode':'110101'}, {**live,'reporttime':'2026-10-05 17:00:00'}, {**live,'reporttime':'2026-10-07 17:00:00'}):
            with patch.object(amap,'query',return_value={'lives':[bad]}), patch('backend.v060.services.time.time',return_value=now):
                with self.assertRaises(ServiceError):
                    amap.weather(origin)


if __name__ == '__main__':
    unittest.main()
