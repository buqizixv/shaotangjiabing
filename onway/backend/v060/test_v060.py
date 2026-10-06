import copy
from concurrent.futures import Future
from datetime import datetime
import json
import tempfile
import time
import unittest
from pathlib import Path
from urllib import request, error
from unittest.mock import patch
import io
from .engine import Engine, TZ, atomic_json, distance, valid_fix, route_progress
from .services import Amap, MiniMax, ServiceError, validate_ai, normalized_route, http_json
from .worker import Worker, configure_server


def place(name, lon, lat=39.9, system="gcj02"):
    return {"id": name, "name": name, "address": name, "lon": lon, "lat": lat, "system": system, "source": "amap", "type": "custom"}


def fixture(mode="walk"):
    origin, dest = place("测试起点", 116.4), place("测试目的地", 116.41)
    segments = [{"mode": "walk", "name": "步行", "duration": 900, "distance": 850,
        "steps": [{"instruction": "沿测试道路向东步行"}], "polyline": "116.400000,39.900000;116.410000,39.900000"}]
    if mode == "transit":
        segments = [
            {"mode": "walk", "name": "步行", "duration": 100, "distance": 170, "steps": [], "polyline": "116.4,39.9;116.402,39.9"},
            {"mode": "transit", "name": "测试公交", "duration": 600, "distance": 510, "from": "A站", "to": "B站",
             "direction": "B站", "start": place("A", 116.402), "end": place("B", 116.408), "polyline": "116.402,39.9;116.408,39.9"},
            {"mode": "walk", "name": "步行", "duration": 100, "distance": 170, "steps": [], "polyline": "116.408,39.9;116.41,39.9"}]
    route = {"id": "fixture-route", "source": "test", "mode": mode, "duration": 900, "distance": 850,
             "walkingDistance": 850, "transfers": 0, "summary": "测试路线（非真实接入）", "segments": segments, "queriedAt": 1791220000}
    return origin, dest, route


class FakeVault:
    def __init__(self, values=None):
        self.values = values or {}
    def get(self, service):
        return self.values.get(service, "")
    def set(self, service, key):
        self.values[service] = key


class EngineTests(unittest.TestCase):
    def test_correction_options_follow_current_stage_and_manual_arrival_is_saved(self):
        origin, dest, route = fixture("transit")
        self.e.state.update(origin=origin, destination=dest, routes=[route], selected=route["id"])
        self.cmd("start")
        self.assertEqual([o["label"] for o in self.e.view()["card"]["corrections"]], ["我已出发"])
        self.cmd("correct", stage="departed")
        options = self.e.view()["card"]["corrections"]
        self.assertEqual([(o["label"], o["stage"], o["segment"]) for o in options], [("我已在候车", "waiting", 1), ("我已上车", "riding", 1)])
        with self.assertRaises(ValueError):
            self.cmd("correct", stage="arrived")
        self.cmd("correct", stage="waiting", segment=1)
        self.assertEqual([o["label"] for o in self.e.view()["card"]["corrections"]], ["我已上车"])
        self.cmd("correct", stage="riding", segment=1)
        option = self.e.view()["card"]["corrections"][0]
        self.assertEqual(option, {"label": "我已下车", "stage": "lastwalk", "segment": 2})
        self.cmd("correct", **{k: v for k, v in option.items() if k != "label"})
        self.assertEqual([o["label"] for o in self.e.view()["card"]["corrections"]], ["我已到达"])
        self.cmd("correct", stage="arrived", segment=2)
        self.assertEqual(self.e.state["trip"]["status"], "arrived")
        self.assertTrue(self.e.hist[-1]["corrected"])
        self.assertEqual(self.e.view()["card"]["corrections"], [])

    def test_walking_title_identifies_station_and_keeps_instruction_separate(self):
        origin, dest, route = fixture("transit")
        route["segments"][1]["from"] = "良乡西门"
        self.e.state.update(origin=origin, destination=dest, routes=[route], selected=route["id"])
        self.cmd("start")
        self.cmd("correct", stage="departed")
        visual = self.e.view()["card"]["visual"]
        self.assertEqual(visual["title"], "前往良乡西门公交站")
        self.assertTrue(visual["instruction"])
        self.e.state["trip"]["route"]["segments"][1]["name"] = "地铁9号线"
        self.assertEqual(self.e.view()["card"]["visual"]["title"], "前往良乡西门地铁站")
        self.e.state["trip"]["route"]["segments"][1]["from"] = "良乡西门地铁站"
        self.assertEqual(self.e.view()["card"]["visual"]["title"], "前往良乡西门地铁站")
        self.cmd("correct", stage="lastwalk", segment=2)
        self.assertEqual(self.e.view()["card"]["visual"]["title"], "前往测试目的地")

    def test_preparing_card_shows_real_route_boarding_information_without_bus_eta(self):
        origin,dest,route=fixture("transit")
        self.e.state.update(origin=origin,destination=dest,routes=[route],selected=route["id"])
        self.cmd("start")
        visual=self.e.view()["card"]["visual"]
        self.assertEqual(visual["connectionLabel"],"到站信息")
        self.assertEqual(visual["connection"],"测试公交")
        self.assertIn("A站",visual["connectionCaption"])
        self.assertEqual(visual["connectionStatus"],"正在查询到站信息…")

    def test_unset_place_can_be_removed_and_saved_again(self):
        self.e.cfg["places"]=[]
        self.cmd("remove_place",type="work")
        self.assertIn("work",self.e.cfg["hiddenPlaceTypes"])
        self.cmd("save_place",place=place("公司位置",116.43),type="work",placeId="work")
        self.assertNotIn("work",self.e.cfg["hiddenPlaceTypes"])
        self.assertEqual(self.e.cfg["places"][0]["type"],"work")

    def test_start_new_selected_route_ends_old_trip_only_after_validation(self):
        self.depart()
        before=copy.deepcopy(self.e.state["trip"])
        self.e.state["selected"]=None
        with self.assertRaises(ValueError):self.cmd("start")
        self.assertEqual(self.e.state["trip"],before)
        self.e.state["routes"][0]={**self.e.state["routes"][0],"id":"replacement-route"}
        self.e.state["selected"]=self.e.state["routes"][0]["id"]
        self.cmd("start")
        self.assertNotEqual(self.e.state["trip"]["id"],before["id"])
        self.assertEqual(self.e.state["trip"]["status"],"preparing")
        self.assertEqual(self.e.hist[-1]["id"],before["id"])
        self.assertEqual(self.e.hist[-1]["status"],"canceled")

    def test_route_selection_while_trip_active_does_not_change_active_trip(self):
        self.depart()
        before=copy.deepcopy(self.e.state["trip"])
        _,_,route=fixture()
        route={**route,"id":"new-route"}
        self.e.state["routes"]=[route]
        self.cmd("select_route",routeId=route["id"])
        self.assertEqual(self.e.state["selected"],route["id"])
        self.assertEqual(self.e.state["trip"],before)

    def test_new_destination_replaces_planning_target_during_active_trip(self):
        self.depart()
        before=copy.deepcopy(self.e.state["trip"])
        selected=place("新目的地",116.43)
        self.cmd("set_destination",place=selected)
        self.assertEqual(self.e.view()["destination"],selected)
        self.assertEqual(self.e.state["routes"],[])
        self.assertIsNone(self.e.state["selected"])
        self.assertEqual(self.e.state["trip"],before)

    def test_device_location_updates_home_origin_while_active_trip_stays_intact(self):
        self.depart()
        before=copy.deepcopy(self.e.state["trip"])
        fix={"lon":116.1377,"lat":39.7322,"system":"gcj02","timestamp":self.now,"accuracy":192}
        self.e.observe(fix)
        self.assertEqual(self.e.view()["origin"]["lon"],fix["lon"])
        self.assertEqual(self.e.state["trip"]["origin"],before["origin"])
        self.assertEqual(self.e.state["trip"]["destination"],before["destination"])

    def test_nearby_fix_retains_address_and_distant_fix_clears_it(self):
        self.e.state["location"]={"lon":116.4,"lat":39.9,"system":"gcj02","name":"原地址","address":"原地址"}
        self.e.observe({"lon":116.40001,"lat":39.9,"system":"gcj02","timestamp":self.now,"accuracy":192})
        self.assertEqual(self.e.state["origin"]["address"],"原地址")
        self.e.observe({"lon":116.1377,"lat":39.7322,"system":"gcj02","timestamp":self.now+1,"accuracy":192})
        self.assertEqual(self.e.state["origin"]["address"],"")

    def test_location_accepts_300_meter_boundary_and_rejects_above_it(self):
        fix={"lon":116.405,"lat":39.9,"system":"gcj02","timestamp":self.now,"accuracy":300}
        self.assertTrue(valid_fix(fix, self.now))
        self.assertFalse(valid_fix({**fix,"accuracy":300.1}, self.now))
        self.e.observe(fix)
        self.assertEqual(self.e.state["location"]["accuracy"],300)
        self.assertIn("真实定位可用", self.e.state["locationStatus"])

    def test_manual_origin_updates_display_during_trip_without_changing_trip(self):
        self.depart()
        trip_before=copy.deepcopy(self.e.state["trip"])
        selected=place("新起点",116.42)
        self.cmd("set_origin",place=selected)
        self.assertEqual(self.e.view()["origin"],selected)
        self.assertEqual(self.e.state["trip"],trip_before)
        self.e.observe({**selected,"timestamp":self.now,"accuracy":187})
        self.assertEqual(self.e.view()["origin"],selected)

    def test_coarse_location_reports_precision_without_advancing_trip(self):
        self.depart()
        trip = self.e.state["trip"]
        stage = trip["status"]
        self.e.observe({"lon":116.405,"lat":39.9,"system":"gcj02","timestamp":self.now,"accuracy":301})
        self.assertIn("301米", self.e.state["locationStatus"])
        self.assertIn("300米以内", self.e.state["locationStatus"])
        self.assertEqual(trip["status"], stage)
        self.assertTrue(trip["segments"][-1]["uncertain"])

    def test_expired_location_reports_age_instead_of_precision(self):
        self.e.observe({"lon":116.405,"lat":39.9,"system":"gcj02","timestamp":self.now-61,"accuracy":174})
        self.assertIn("位置已过期", self.e.state["locationStatus"])
        self.assertNotIn("174米", self.e.state["locationStatus"])

    def test_card_presentation_preserves_unavailable_vehicle_arrival(self):
        origin,dest,route=fixture("transit")
        self.e.state.update(origin=origin,destination=dest,routes=[route],selected=route["id"])
        self.cmd("start")
        self.cmd("correct",stage="departed")
        self.cmd("correct",stage="waiting",segment=1)
        card=self.e.view()["card"]
        self.assertEqual(card["kind"],"C2")
        self.assertEqual(card["visual"]["hero"],"查询中")
        self.assertEqual(card["visual"]["unit"],"")
        self.assertIn("正在查询",card["visual"]["connectionStatus"])

    def test_summary_presentation_does_not_turn_uncertain_segments_into_known_time(self):
        self.depart()
        trip=self.e.state["trip"]
        trip.update(status="arrived",arrivedAt=self.now+600)
        trip["segments"]=[dict(stage="walking",startedAt=self.now,endedAt=self.now+120,uncertain=True)]
        card=self.e.view()["card"]
        self.assertEqual(card["kind"],"C5")
        self.assertEqual(card["visual"]["segments"][0]["value"],"时长未知")
        self.assertEqual(card["visual"]["segments"][1]["value"],"无记录")

    def test_preferences_order_actual_routes_without_selecting_or_removing_them(self):
        _,_,walk=fixture()
        transit={**walk,"id":"actual-transit","mode":"transit","duration":300,"walkingDistance":200,"transfers":1}
        unknown={**walk,"id":"actual-unknown","duration":None,"walkingDistance":None}
        self.e.state.update(routes=[walk,unknown,transit],selected=walk["id"])
        self.e.cfg["aiEnabled"]=False
        self.assertEqual(self.e.view()["routes"][0]["id"],transit["id"])
        self.assertEqual(self.e.view()["routes"][-1]["id"],unknown["id"])
        self.assertEqual(self.e.state["selected"],walk["id"])
        self.cmd("preferences",mode="walk",route="lesswalk")
        self.assertEqual(self.e.view()["routes"][0]["id"],walk["id"])
        self.assertEqual({r["id"] for r in self.e.view()["routes"]},{walk["id"],transit["id"],unknown["id"]})
        self.assertEqual(self.e.state["routes"][0]["id"],walk["id"])

    def test_incomplete_segments_do_not_enter_learning(self):
        self.depart()
        trip = self.e.state["trip"]
        trip["direction"] = "outbound"
        self.now += 120
        self.e.observe({"lon":116.405,"lat":39.9,"system":"gcj02","timestamp":self.now,"accuracy":301})
        self.fix(116.4099)
        self.assertEqual(trip["status"], "arrived")
        self.assertEqual(self.e.state["learning"], {})
        self.assertFalse(self.e.hist[-1]["completeSegments"])

    def test_confirming_intent_without_selection_requests_real_route_choice(self):
        origin,dest,route=fixture()
        self.e.state.update(selected=None,intent={"id":"today:outbound","expires":self.now+100,"origin":origin,"destination":dest,"plannedAt":self.now,"direction":"outbound"})
        self.cmd("intent_confirm")
        self.assertIsNone(self.e.state["trip"])
        self.assertTrue(self.e.state["intent"]["confirmed"])
        self.assertEqual(self.e.view()["navigation"]["page"],"routes")

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.now = 1791220000.0
        self.e = Engine(self.tmp.name, lambda: self.now)
        origin, dest, route = fixture()
        self.e.state.update(origin=origin, destination=dest, routes=[route], selected=route["id"])
        self.e.cfg["tracking"] = True
        self.serial = 0
    def tearDown(self):
        self.tmp.cleanup()
    def cmd(self, action, **payload):
        self.serial += 1
        self.e.command({"id": str(self.serial), "action": action, "payload": payload})
    def fix(self, lon, seconds=3, acc=5, system="gcj02"):
        self.now += seconds
        p = {"lon": lon, "lat": 39.9, "system": system, "timestamp": self.now, "accuracy": acc}
        self.e.observe(p)
        return p
    def depart(self):
        self.cmd("start")
        self.fix(116.4008)
        self.fix(116.4010)

    def test_start_click_does_not_start_clock(self):
        self.cmd("start")
        self.assertEqual(self.e.state["trip"]["status"], "preparing")
        self.assertIsNone(self.e.state["trip"]["startedAt"])
        self.assertIn("尚未开始计时", self.e.view()["card"]["notice"])

    def test_distinct_consecutive_positions_start_from_first_crossing(self):
        self.cmd("start")
        first = self.fix(116.4008)
        self.e.observe(first)
        self.assertIsNone(self.e.state["trip"]["startedAt"])
        self.fix(116.4010)
        self.assertEqual(self.e.state["trip"]["startedAt"], first["timestamp"])

    def test_single_jump_does_not_start(self):
        self.cmd("start")
        self.fix(116.4)
        self.fix(116.409, seconds=1)
        self.assertIsNone(self.e.state["trip"]["startedAt"])

    def test_invalid_fix_breaks_consecutive_departure(self):
        self.cmd("start")
        self.fix(116.4008)
        self.fix(116.4009, acc=301)
        self.fix(116.4010)
        self.assertIsNone(self.e.state["trip"]["startedAt"])

    def test_stale_or_mixed_coordinates_never_start(self):
        self.cmd("start")
        self.e.observe({"lat":39.9,"lon":116.401,"system":"gcj02","timestamp":self.now-61,"accuracy":5})
        self.fix(116.401, system="wgs84")
        self.assertIsNone(self.e.state["trip"]["startedAt"])

    def test_clock_only_never_advances_trip(self):
        self.cmd("start")
        self.now += 3600
        self.e.view()
        self.assertEqual(self.e.state["trip"]["status"], "preparing")

    def test_arrival_is_immediate_after_actual_departure(self):
        self.depart()
        self.fix(116.4098, seconds=60)
        self.assertEqual(self.e.state["trip"]["status"], "arrived")
        self.assertEqual(len(self.e.hist), 1)
        self.assertEqual(self.e.view()["card"]["kind"], "C5")

    def test_arrival_correction_restores_same_trip_and_deduplicates_history(self):
        self.depart()
        fix = self.fix(116.4098, seconds=60)
        trip_id = self.e.state["trip"]["id"]
        self.e.state["trip"].update(aiSummary="旧结论", summaryRequested=True)
        self.cmd("not_arrived")
        self.assertEqual(self.e.state["trip"]["aiSummary"], "")
        self.assertNotIn("summaryRequested", self.e.state["trip"])
        self.e.observe(fix)
        self.assertEqual(self.e.state["trip"]["id"], trip_id)
        self.assertEqual(len(self.e.hist), 0)
        self.assertNotEqual(self.e.state["trip"]["status"], "arrived")
        self.fix(116.4099)
        self.assertEqual(len(self.e.hist), 1)

    def test_restart_does_not_synthesize_departure_or_duplicate_history(self):
        self.depart()
        trip_id = self.e.state["trip"]["id"]
        self.e.save()
        self.now += 600
        resumed = Engine(self.tmp.name, lambda: self.now)
        self.assertEqual(resumed.state["trip"]["id"], trip_id)
        self.assertEqual(resumed.state["trip"]["status"], "walking")
        self.assertEqual(len(resumed.hist), 0)

    def test_duplicate_command_and_concurrent_start_create_one_trip(self):
        self.cmd("start")
        trip_id = self.e.state["trip"]["id"]
        self.e.command({"id":"1","action":"start","payload":{}})
        with self.assertRaises(ValueError):
            self.cmd("start")
        self.assertEqual(self.e.state["trip"]["id"], trip_id)

    def test_overlap_is_not_a_completed_trip(self):
        self.e.state["destination"] = place("很近", 116.4001)
        with self.assertRaises(ValueError):
            self.cmd("start")
        self.assertEqual(self.e.hist, [])

    def test_manual_departure_marks_source(self):
        self.cmd("start")
        self.cmd("correct", stage="departed")
        self.assertEqual(self.e.state["trip"]["startSource"], "user")
        self.assertTrue(self.e.state["trip"]["corrected"])

    def test_pure_walk_has_no_vehicle_information(self):
        self.depart()
        card = self.e.view()["card"]
        self.assertEqual(card["kind"], "C2")
        self.assertFalse(any(g["title"] == "车辆到站" for g in card["guide"]))

    def test_multitransfer_is_one_trip_with_reusable_cards(self):
        origin, dest, route = fixture("transit")
        route["segments"] = route["segments"] + copy.deepcopy(route["segments"])
        self.e.state["routes"] = [route]
        self.depart()
        trip_id = self.e.state["trip"]["id"]
        for index, status, card in ((1,"riding","C3"),(2,"transfer","C4"),(4,"riding","C3"),(5,"lastwalk","C4")):
            self.cmd("correct", stage=status, segment=index)
            self.assertEqual(self.e.view()["card"]["kind"], card)
            self.assertEqual(self.e.state["trip"]["id"], trip_id)

    def test_cancel_is_not_a_completed_learning_sample(self):
        self.depart()
        self.cmd("cancel")
        self.assertEqual(self.e.hist, [])
        self.assertEqual(self.e.state["learning"], {})

    def test_cancel_returns_home_without_summary_or_new_history(self):
        self.depart()
        self.e.hist = [{"id":"previous-completed-trip", "status":"arrived", "segments":[]}]
        previous = copy.deepcopy(self.e.hist)
        self.now += 60
        self.cmd("cancel")
        self.assertIsNone(self.e.state["trip"])
        view = self.e.view()
        self.assertFalse(view["cardVisible"])
        self.assertEqual(view["navigation"]["page"], "home")
        self.assertEqual(self.e.hist, previous)
        saved = json.loads((self.e.root / "hist-v060.json").read_text(encoding="utf8"))
        self.assertEqual(saved, previous)

    def test_clear_history_deletes_migrated_source(self):
        atomic_json(self.e.root / "hist.json", [{"durMin":10}])
        self.cmd("clear_history")
        self.assertFalse((self.e.root / "hist.json").exists())
        self.assertEqual(self.e.hist, [])

    def test_intent_refusal_is_per_local_date_and_direction(self):
        self.now = datetime(2026,10,6,7,55,tzinfo=TZ).timestamp()
        origin, dest, _ = fixture()
        self.e.cfg["places"] = [origin,dest]
        self.e.cfg["aiEnabled"] = True
        self.e.cfg["commute"].update(originId=origin["id"],destinationId=dest["id"],depart="08:00",returnTime="18:00",dateMode="custom")
        self.e.state["location"] = {**origin,"timestamp":self.now,"accuracy":5}
        candidate = self.e.intent_candidate()
        self.assertEqual(candidate["id"], "2026-10-06:outbound")
        self.e.state["intent"] = candidate
        self.cmd("intent_decline")
        self.assertIsNone(self.e.intent_candidate())
        self.now = datetime(2026,10,6,17,55,tzinfo=TZ).timestamp()
        self.e.state["location"] = {**dest,"timestamp":self.now,"accuracy":5}
        self.assertEqual(self.e.intent_candidate()["direction"], "return")

    def test_snooze_does_not_rewrite_habits(self):
        self.e.state["intent"] = {"id":"today:outbound","expires":self.now+500}
        saved = copy.deepcopy(self.e.cfg["commute"])
        self.cmd("intent_snooze", minutes=30)
        self.assertEqual(saved, self.e.cfg["commute"])

    def test_migration_does_not_promote_prototype_coordinates(self):
        with tempfile.TemporaryDirectory() as tmp:
            atomic_json(Path(tmp)/"cfg.json", {"schema":3,"homeName":"旧示例","home":{"lat":39.9,"lon":116.4}})
            atomic_json(Path(tmp)/"hist.json", [{"epoch":1791220000,"durMin":10}])
            e = Engine(tmp)
            self.assertEqual(e.cfg["places"], [])
            self.assertEqual(len(e.cfg["legacyPlaces"]), 2)
            self.assertEqual(e.hist[0]["status"], "legacy")
            self.assertTrue((Path(tmp)/"cfg.json.backup-0.5.4").exists())

    def test_migration_preserves_location_and_reminder_preferences(self):
        for enabled in (False, True):
            with tempfile.TemporaryDirectory() as tmp:
                atomic_json(Path(tmp)/"cfg.json", {"backgroundLocation":enabled, "anomalyNotice":enabled})
                e = Engine(tmp)
                self.assertIs(e.cfg["tracking"], enabled)
                self.assertIs(e.cfg["reminders"], enabled)


class ServiceTests(unittest.TestCase):
    def test_completed_history_updates_memory_after_summary_closes_and_clear_discards_pending(self):
        with tempfile.TemporaryDirectory() as tmp:
            w = Worker(tmp, Amap(FakeVault()), MiniMax(FakeVault()))
            self.addCleanup(w.pool.shutdown, wait=True)
            e = w.engine
            e.cfg.update(aiEnabled=True, tracking=False)
            origin, dest, route = fixture()
            e.state.update(origin=origin, destination=dest, routes=[route], selected=route["id"])
            for n, (action, payload) in enumerate([("start",{}), ("correct",{"stage":"departed"}), ("correct",{"stage":"arrived"}), ("close_summary",{})]):
                e.command({"id":str(n), "action":action, "payload":payload})
            self.assertEqual(e.view()["navigation"]["page"], "home")
            futures = {}
            def submit(key, fn, metadata=None):
                future = Future();futures[key] = future;w.jobs[key] = (future, metadata)
            w.submit = submit
            w.tick()
            self.assertIn("memory", futures)
            futures["memory"].set_result({"action":"memory", "summary":"样本少且经人工纠正，暂不能推断稳定通勤规律。"})
            w.tick()
            self.assertIn("人工纠正", e.state["aiMemory"]["text"])
            e.hist.append({**e.hist[0], "id":"new-completion"})
            e.state["lastAIByTask"]["memory"] = 0
            w.tick()
            self.assertEqual(e.state["aiMemory"]["status"], "正在分析最新完成行程…")
            e.command({"id":"clear", "action":"clear_history", "payload":{}})
            futures["memory"].set_result({"action":"memory", "summary":"已删除的旧分析"})
            w.tick()
            self.assertEqual(e.hist, [])
            self.assertNotIn("旧分析", e.state["aiMemory"]["text"])

    def test_ai_preferences_retry_after_throttle_and_ignore_disabled_result(self):
        with tempfile.TemporaryDirectory() as tmp:
            w = Worker(tmp, Amap(FakeVault()), MiniMax(FakeVault()))
            self.addCleanup(w.pool.shutdown, wait=True)
            e = w.engine
            now = [1791220000]
            e.clock = lambda: now[0]
            e.cfg["tracking"] = False
            _, _, route = fixture()
            e.state["routes"] = [route]
            e.state["lastAIByTask"]["recommend"] = now[0]
            w.process_command({"id":"enable", "action":"toggle", "payload":{"field":"aiEnabled"}})
            future = Future()
            submitted = []
            def submit(key, fn, metadata=None):
                submitted.append((key, metadata))
                w.jobs[key] = (future, metadata)
            w.submit = submit
            w.tick()
            self.assertTrue(w.recommend_pending)
            self.assertEqual(submitted, [])
            now[0] += 61
            w.tick()
            self.assertFalse(w.recommend_pending)
            self.assertEqual(submitted[0][0], "recommend")
            w.process_command({"id":"pref", "action":"preferences", "payload":{"mode":"transit", "route":"lesswalk"}})
            self.assertTrue(w.recommend_pending)
            future.set_result({"action":"recommend", "routeId":route["id"], "reason":"旧偏好"})
            w.tick()
            self.assertFalse(route["recommended"])
            self.assertEqual(e.cfg["preferences"], {"mode":"transit", "route":"lesswalk"})
            future = Future()
            now[0] += 61
            w.tick()
            self.assertEqual(len(submitted), 2)
            w.process_command({"id":"disable", "action":"toggle", "payload":{"field":"aiEnabled"}})
            future.set_result({"action":"recommend", "routeId":route["id"], "reason":"关闭后的结果"})
            w.tick()
            self.assertFalse(route["recommended"])
            self.assertEqual(route["reason"], "")

    def test_cocomap_uses_bearer_auth_and_unwraps_upstream_response(self):
        seen=[]
        def transport(url, payload, headers):
            seen.append((url,payload,headers))
            return {"ok":True,"data":{"status":"1","pois":[{"id":"real","name":"真实候选","location":"116.4,39.9"}]}}
        amap=Amap(FakeVault({"amap":"sk_coco_test-only"}),transport)
        results=amap.search("测试")
        self.assertEqual(amap.provider,"可可地图网关")
        self.assertTrue(seen[0][0].startswith("https://map.culture09.xyz/v3/place/text?"))
        self.assertNotIn("sk_coco_",seen[0][0])
        self.assertNotIn("key=",seen[0][0])
        self.assertEqual(seen[0][2],{"Authorization":"Bearer sk_coco_test-only"})
        self.assertEqual(results[0]["id"],"real")

    def test_cocomap_rejects_failed_or_malformed_wrappers(self):
        for response in ({"ok":False,"error":"INVALID_API_KEY","message":"private-provider-body"},
                         {"ok":True,"data":[]}, {"status":"1","pois":[]}):
            amap=Amap(FakeVault({"amap":"sk_coco_test-only"}),lambda *args:response)
            with self.assertRaises(ServiceError) as caught:
                amap.search("测试")
            self.assertNotIn("private-provider-body",str(caught.exception))

    def test_cocomap_http_failure_uses_fixed_quota_description(self):
        failure=error.HTTPError("https://map.culture09.xyz/v3/place/text",402,"quota",{},
            io.BytesIO(b'{"ok":false,"error":"QUOTA_EXHAUSTED","message":"private-provider-body"}'))
        with patch("backend.v060.services.request.urlopen",side_effect=failure):
            with self.assertRaises(ServiceError) as caught:
                http_json("https://map.culture09.xyz/v3/place/text",headers={"Authorization":"Bearer test-key"})
        self.assertIn("额度不足",str(caught.exception))
        self.assertNotIn("private-provider-body",str(caught.exception))

    def test_official_amap_key_stays_on_official_host(self):
        seen=[]
        amap=Amap(FakeVault({"amap":"official-test-key"}),lambda url:seen.append(url) or {"status":"1","pois":[]})
        self.assertEqual(amap.provider,"高德官方 Web 服务")
        amap.search("测试")
        self.assertTrue(seen[0].startswith("https://restapi.amap.com/v3/place/text?"))
        self.assertIn("key=official-test-key",seen[0])

    def test_amap_errors_keep_safe_numeric_code_without_provider_body(self):
        amap = Amap(FakeVault({"amap":"test-key"}), lambda url: {"status":"0","infocode":"10001","info":"untrusted-provider-body"})
        with self.assertRaises(ServiceError) as caught:
            amap.search("测试")
        self.assertIn("10001", str(caught.exception))
        self.assertIn("Key 无效或已过期", str(caught.exception))
        self.assertNotIn("untrusted-provider-body", str(caught.exception))
        malformed = Amap(FakeVault({"amap":"test-key"}), lambda url: {"status":"0","infocode":"https://provider.invalid/?key=private"})
        with self.assertRaises(ServiceError) as caught:
            malformed.search("测试")
        self.assertNotIn("private", str(caught.exception))

    def test_worker_surfaces_invalid_amap_key_without_inventing_candidates(self):
        with tempfile.TemporaryDirectory() as tmp:
            w = Worker(tmp, Amap(FakeVault()), MiniMax(FakeVault()))
            try:
                future = Future()
                future.set_exception(ServiceError("高德查询失败 · Key 无效或已过期（10001）"))
                w.jobs["search-0"] = (future, w.engine.pending_generation)
                w.tick()
                self.assertIn("10001", w.engine.view()["queryStatus"])
                self.assertEqual(w.engine.state["search"], [])
                self.assertEqual(w.engine.state["routes"], [])
                self.assertIsNone(w.engine.state["trip"])
            finally:
                w.pool.shutdown()

    def test_pending_coordinate_conversion_does_not_drop_latest_fix(self):
        with tempfile.TemporaryDirectory() as tmp:
            w = Worker(tmp, Amap(FakeVault({"amap":"test-key"})), MiniMax(FakeVault()))
            pending = Future()
            try:
                w.jobs["gps-convert"] = (pending, None)
                gps = {"lat":39.9,"lon":116.4,"system":"wgs84","timestamp":time.time(),"accuracy":5}
                atomic_json(w.engine.root/"device-location.json", gps)
                w.tick()
                self.assertEqual(w.last_fix, 0)
                self.assertIs(w.jobs["gps-convert"][0], pending)
            finally:
                pending.cancel()
                w.pool.shutdown()

    def test_route_identity_ignores_fix_timestamp_and_estimate_changes(self):
        origin, dest, route = fixture()
        first = normalized_route("walk", {"duration": 900}, route["segments"], origin, dest)
        origin["timestamp"] = 999
        origin["name"] = "更新后的当前位置"
        route["segments"][0]["duration"] = 950
        second = normalized_route("walk", {"duration": 950}, route["segments"], origin, dest)
        self.assertEqual(first["id"], second["id"])

    def test_route_progress_uses_geometry_and_rejects_offroute_fix(self):
        _, _, route = fixture()
        self.assertAlmostEqual(route_progress(place("中点", 116.405), route["segments"][0]), .5, places=4)
        self.assertIsNone(route_progress(place("偏离", 116.405, 39.91), route["segments"][0]))

    def test_destination_plan_queries_new_target_during_existing_trip(self):
        with tempfile.TemporaryDirectory() as tmp:
            w=Worker(tmp,Amap(FakeVault()),MiniMax(FakeVault()))
            try:
                origin,dest,route=fixture()
                w.engine.cfg["tracking"]=False
                w.engine.state.update(origin=origin,destination=dest,routes=[route],selected=route["id"])
                w.engine.command({"id":"start-old","action":"start","payload":{}})
                before=copy.deepcopy(w.engine.state["trip"])
                target=place("新查询目标",116.45)
                with patch.object(w.amap,"routes",return_value={"origin":origin,"destination":target,"routes":[route],"warnings":[]}) as query:
                    w.process_command({"id":"plan-new","action":"plan_destination","payload":{"place":target}})
                    next(v[0] for k,v in w.jobs.items() if k.startswith("routes-")).result()
                    w.tick()
                    self.assertEqual(query.call_args.args[1],target)
                    self.assertEqual(w.engine.state["destination"],target)
                    self.assertEqual(w.engine.state["trip"],before)
            finally:w.pool.shutdown()

    def test_connections_report_request_success_and_failure_not_key_presence(self):
        with tempfile.TemporaryDirectory() as tmp:
            vault=FakeVault({"amap":"test-map","minimax":"test-ai"})
            w=Worker(tmp,Amap(vault),MiniMax(vault))
            try:
                w.engine.cfg["tracking"]=False
                with patch.object(w.bus,"check",return_value=True),patch.object(w.amap,"weather",return_value={"status":"available"}),patch.object(w.amap,"search",return_value=[]),patch.object(w.minimax,"analyze",side_effect=ServiceError("测试连接失败")):
                    w.process_command({"id":"connection-test","action":"check_services","payload":{}})
                    for f,_ in list(w.jobs.values()):
                        try:f.result()
                        except ServiceError:pass
                    w.tick()
                self.assertEqual(w.connections,{"amap":"已连接","minimax":"连接失败","beijing_bus":"已连接","weather":"已连接"})
            finally:w.pool.shutdown()

    def test_open_refresh_returns_from_manual_origin_to_device_location(self):
        with tempfile.TemporaryDirectory() as tmp:
            w=Worker(tmp,Amap(FakeVault()),MiniMax(FakeVault()))
            try:
                w.engine.cfg["tracking"]=True
                w.engine.command({"id":"manual","action":"set_origin","payload":{"place":place("旧起点",116.4)}})
                gps={"lon":116.13,"lat":39.73,"system":"wgs84","timestamp":time.time(),"accuracy":200}
                atomic_json(Path(tmp)/"device-location.json",gps)
                w.last_fix=gps["timestamp"]
                w.process_command({"id":"reopen","action":"refresh_location","payload":{}})
                w.tick()
                self.assertEqual(w.engine.state["originMode"],"device")
                self.assertEqual(w.engine.state["origin"]["lon"],gps["lon"])
            finally:w.pool.shutdown()

    def test_reverse_updates_address_without_overwriting_manual_origin(self):
        with tempfile.TemporaryDirectory() as tmp:
            w = Worker(tmp, Amap(FakeVault()), MiniMax(FakeVault()))
            try:
                origin, dest, _ = fixture()
                w.engine.cfg["tracking"] = True
                w.engine.command({"id":"manual-origin-test","action":"set_origin","payload":{"place":dest}})
                w.engine.state["location"] = {**origin, "timestamp": time.time(), "accuracy": 5}
                w.submit("reverse", lambda: {**origin, "name": "返回的实际地址", "address": "实际地址"}, 1)
                w.jobs["reverse"][0].result()
                w.tick()
                self.assertEqual(w.engine.state["origin"]["id"], dest["id"])
                self.assertEqual(w.engine.state["location"]["name"], "返回的实际地址")
            finally:
                w.pool.shutdown()

    def test_missing_amap_never_invokes_network(self):
        calls = []
        amap = Amap(FakeVault(), lambda *args: calls.append(args))
        with self.assertRaises(ServiceError):
            amap.search("北京南站")
        self.assertEqual(calls, [])

    def test_amap_search_discards_candidates_without_coordinates(self):
        amap = Amap(FakeVault({"amap":"test-key"}), lambda url: {"status":"1","pois":[
            {"id":"1","name":"有效","location":"116.4,39.9"}, {"id":"2","name":"无坐标","location":""}]})
        self.assertEqual(len(amap.search("测试")),1)

    def test_walking_and_transit_are_normalized_without_vehicle_countdown(self):
        def transport(url):
            if "/walking" in url:
                return {"status":"1","route":{"paths":[{"distance":"850","duration":"900","steps":[{"instruction":"向东","polyline":"116.4,39.9;116.41,39.9"}]}]}}
            return {"status":"1","route":{"transits":[]}}
        origin,dest,_=fixture()
        result=Amap(FakeVault({"amap":"test-key"}),transport).routes(origin,dest)
        self.assertEqual(len(result["routes"]),1)
        self.assertEqual(result["routes"][0]["duration"],900)

    def test_ai_rejects_fabricated_route_and_intent(self):
        result={"task":"recommend","action":"recommend","routeId":"fake","reason":"","uncertainty":"","summary":""}
        with self.assertRaises(ServiceError):
            validate_ai("recommend",{"routes":[{"id":"actual"}]},result)
        result.update(task="intent",action="intent",candidateId="fake")
        with self.assertRaises(ServiceError):
            validate_ai("intent",{"candidates":[]},result)

    def test_minimax_uses_domestic_m3_and_sanitizes_thinking(self):
        seen=[]
        def transport(url,payload,headers):
            seen.append((url,payload))
            return {"choices":[{"message":{"content":'<think>private</think>{"task":"summary","action":"summary","reason":"","uncertainty":"","summary":"已完成行程。"}'}}]}
        result=MiniMax(FakeVault({"minimax":"test-key"}),transport).analyze("summary",{})
        self.assertEqual(seen[0][0],"https://api.minimax.cn/v1/chat/completions")
        self.assertEqual(seen[0][1]["model"],"MiniMax-M3")
        self.assertEqual(result["action"],"summary")

    def test_config_refuses_missing_token_and_cross_origin(self):
        server,url=configure_server(FakeVault())
        base,token=url.split("#")
        try:
            with self.assertRaises(error.HTTPError) as caught:
                request.urlopen(base+"status")
            caught.exception.close()
            req=request.Request(base+"status",headers={"Authorization":"Bearer "+token,"Origin":"https://untrusted.example"})
            with self.assertRaises(error.HTTPError) as caught:
                request.urlopen(req)
            caught.exception.close()
            req=request.Request(base+"status",headers={"Authorization":"Bearer "+token})
            with request.urlopen(req) as response:
                self.assertEqual(json.load(response),{"amap":False,"minimax":False})
        finally:
            server.shutdown();server.server_close()

    def test_worker_rejects_stale_route_result_and_preserves_input_generation(self):
        with tempfile.TemporaryDirectory() as tmp:
            w=Worker(tmp,Amap(FakeVault()),MiniMax(FakeVault()))
            try:
                w.engine.pending_generation=2
                w.submit("search-1",lambda:[place("old",116.4)],1)
                for _ in range(20):
                    if w.jobs["search-1"][0].done():break
                    time.sleep(.01)
                w.tick()
                self.assertEqual(w.engine.state["search"],[])
            finally:
                w.pool.shutdown()


if __name__ == "__main__":
    unittest.main()
