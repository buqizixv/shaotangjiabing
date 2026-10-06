import tempfile
import unittest
from pathlib import Path
from .engine import Engine, atomic_json


class InitialHistoryTests(unittest.TestCase):
    def test_first_install_seeds_once_and_clear_stays_empty(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            engine = Engine(root, seed_history=True)
            self.assertEqual(len(engine.hist), 10)
            self.assertEqual(len({h['id'] for h in engine.hist}), 10)
            self.assertEqual(sum(h['direction'] is not None for h in engine.hist), 6)
            self.assertTrue(all(h['source'] == 'imported' and not h['completeSegments'] for h in engine.hist))
            self.assertTrue(all(60 <= abs(h['duration'] - h['initialEstimate']) <= 120 for h in engine.hist))
            self.assertIn('个性化偏好', engine.state['aiMemory']['text'])
            self.assertEqual(len(Engine(root, seed_history=True).hist), 10)
            engine.command({'id':'clear-test', 'action':'clear_history', 'payload':{}})
            self.assertEqual(Engine(root, seed_history=True).hist, [])

    def test_existing_history_is_never_replaced(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            engine = Engine(root)
            engine.hist = []
            engine.save()
            self.assertEqual(Engine(root, seed_history=True).hist, [])

    def test_delete_one_record_invalidates_memory_and_persists(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            engine = Engine(root, seed_history=True)
            target = engine.hist[0]['id']
            remaining = [h for h in engine.hist if h['id'] != target]
            engine.command({'id':'delete-one', 'action':'delete_history', 'payload':{'id':target}})
            self.assertEqual(engine.hist, remaining)
            self.assertEqual(engine.state['aiMemory']['historyKey'], '')
            self.assertEqual(Engine(root, seed_history=True).hist, remaining)
            with self.assertRaises(ValueError):
                engine.command({'id':'missing', 'action':'delete_history', 'payload':{'id':'unknown'}})
            for i,h in enumerate(list(engine.hist)):
                engine.command({'id':f'delete-{i}', 'action':'delete_history', 'payload':{'id':h['id']}})
            self.assertEqual(Engine(root, seed_history=True).hist, [])
            self.assertIn('暂无已完成行程', engine.state['aiMemory']['text'])

    def test_legacy_install_is_not_seeded(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            atomic_json(root / 'cfg.json', {})
            self.assertEqual(Engine(root, seed_history=True).hist, [])
