import json
import os
from pathlib import Path
import tempfile
import threading
import unittest
from unittest.mock import patch
from urllib import request, error
from http.server import ThreadingHTTPServer

from server import Engine, ServiceError, handler, minimax, normalize, parse_card


def context(now=10000):
    return {"schema": 1, "epoch": now, "enabled": True, "demo": True, "stage": "idle",
            "config": {"homeName": "家", "workName": "公司", "selectedPlan": 0, "threshold": 10},
            "locationTrusted": False, "history": [{"durMin": 47, "plan": 0}]}


CARD = {"action": "push_card", "title": "通勤参考", "body": "历史记录为47分钟，请预留时间。", "reason": "基于已保存历史"}


class BackendTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.now = 10000
        self.calls = []

        def provider(data):
            self.calls.append(data)
            return dict(CARD)

        self.engine = Engine(self.temp.name, provider, lambda: self.now)

    def tearDown(self):
        self.temp.cleanup()

    def test_worker_reads_context_and_publishes_card(self):
        path = Path(self.temp.name)
        (path / "ai-context.json").write_text(json.dumps(context()), encoding="utf-8")
        self.engine.watch_once()
        result = json.loads((path / "ai-result.json").read_text(encoding="utf-8"))
        self.assertEqual(result["action"], "push_card")
        self.assertEqual(result["stage"], "idle")
        self.assertTrue(result["demo"])
        self.assertEqual(self.calls[0]["history"][0]["durMin"], 47)

    def test_opt_out_and_stale_context_never_call_model(self):
        data = context()
        data["enabled"] = False
        self.assertEqual(self.engine.analyze(data)["status"], "disabled")
        self.now += 100
        with self.assertRaises(ServiceError):
            self.engine.analyze(context())
        self.assertEqual(self.calls, [])

    def test_dedup_cooldown_and_restart_ledger(self):
        self.engine.analyze(context())
        self.now += 61
        self.engine.analyze(context(self.now))
        self.assertEqual(len(self.calls), 1)
        data = context(self.now)
        data["stage"] = "walking"
        self.assertEqual(self.engine.analyze(data)["action"], "none")
        self.assertEqual(len(self.calls), 2)
        restarted = Engine(self.temp.name, self.engine.provider, lambda: self.now)
        restarted.analyze(data)
        self.assertEqual(len(self.calls), 2)

    def test_retry_after_provider_failure(self):
        self.engine.provider = lambda data: (_ for _ in ()).throw(ServiceError("上游失败", 502))
        with self.assertRaises(ServiceError):
            self.engine.analyze(context())
        self.engine.provider = lambda data: dict(CARD)
        self.now += 61
        self.assertEqual(self.engine.analyze(context(self.now))["action"], "push_card")

    def test_invalid_model_action_and_thinking(self):
        self.assertEqual(parse_card("<think>private</think>\n```json\n" + json.dumps(CARD) + "\n```"), CARD)
        with self.assertRaises(ServiceError):
            parse_card(json.dumps(dict(CARD, action="change_route")))
        with self.assertRaises(ServiceError):
            parse_card(json.dumps(dict(CARD, body="x" * 161)))

    def test_personal_coordinates_and_unknown_fields_are_removed(self):
        data = context()
        data["config"]["home"] = {"lat": 39, "lon": 116}
        data["api_key"] = "secret"
        output = normalize(data, self.now)
        self.assertNotIn("home", output["config"])
        self.assertNotIn("api_key", output)
        self.assertEqual(output["routeSource"], "prototype_only_no_live_feed")

    def test_minimax_transport_official_endpoint_and_response(self):
        class Response:
            def __enter__(self): return self
            def __exit__(self, *args): pass
            def read(self, size):
                return json.dumps({"choices": [{"message": {"content": json.dumps(CARD)}}]}).encode()

        with patch.dict(os.environ, {"MINIMAX_API_KEY": "test-only", "MINIMAX_BASE_URL": "https://api.minimax.cn/v1"}), patch("server.request.urlopen", return_value=Response()) as transport:
            self.assertEqual(minimax(context()), CARD)
            req = transport.call_args.args[0]
            self.assertEqual(req.full_url, "https://api.minimax.cn/v1/chat/completions")
            self.assertEqual(req.headers["Authorization"], "Bearer test-only")
            self.assertFalse(json.loads(req.data)["stream"])

    def test_http_auth_validation_and_card(self):
        server = ThreadingHTTPServer(("127.0.0.1", 0), handler(self.engine, "test-token"))
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        base = f"http://127.0.0.1:{server.server_port}"
        try:
            with self.assertRaises(error.HTTPError) as denied:
                request.urlopen(base + "/v1/cards/latest")
            self.assertEqual(denied.exception.code, 401)
            denied.exception.close()
            req = request.Request(base + "/v1/ai/analyze", json.dumps(context()).encode(),
                                  {"Authorization": "Bearer test-token", "Content-Type": "application/json"})
            with request.urlopen(req) as response:
                self.assertEqual(json.load(response)["action"], "push_card")
            req = request.Request(base + "/v1/ai/analyze", b"not-json", {"Authorization": "Bearer test-token"})
            with self.assertRaises(error.HTTPError) as invalid:
                request.urlopen(req)
            self.assertEqual(invalid.exception.code, 400)
            invalid.exception.close()
        finally:
            server.shutdown()
            server.server_close()
            thread.join()


if __name__ == "__main__":
    unittest.main()
