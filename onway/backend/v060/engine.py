"""Deterministic trip state. Only the worker writes settings, trips and history."""
import copy
from datetime import datetime, timezone, timedelta
import hashlib
import json
import math
import os
import re
from pathlib import Path
import time
import uuid
from .services import ServiceError, coordinate

TZ = timezone(timedelta(hours=8))
ACTIVE = {"preparing", "walking", "waiting", "riding", "transfer", "lastwalk"}
MAX_FIX_ACCURACY = 300
STAGE_LABELS = {"preparing": "准备出发", "walking": "步行", "waiting": "候车", "riding": "乘车", "transfer": "换乘步行", "lastwalk": "最后一段步行"}

def memory_sections(text, preferences):
    """Render old and new memory in the same order without losing stored text."""
    headers = list(re.finditer(r"(个性化偏好(?:分析)?|历史分析|历史记忆)[：:]", text))
    sections = {}
    for i, match in enumerate(headers):
        value = text[match.end():headers[i+1].start() if i+1 < len(headers) else len(text)].strip()
        sections["personal" if match.group(1).startswith("个性") else "historical"] = value
    mode = {"all":"不限方式", "transit":"公共交通", "walk":"步行"}.get(preferences.get("mode"), "不限方式")
    priority = {"fast":"最快", "lesswalk":"少步行", "lesstransfer":"少换乘"}.get(preferences.get("route"), "最快")
    return {"personal":sections.get("personal", f"已设置：{mode}，优先{priority}。完成更多出行后，逐步了解你的习惯。"),
            "historical":sections.get("historical", text)}


def atomic_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(value, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
    os.replace(tmp, path)


def read_json(path, default):
    try:
        return json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return copy.deepcopy(default)


def distance(a, b):
    if a.get("system") != b.get("system"):
        raise ValueError("坐标系不一致")
    lat = math.radians((a["lat"] + b["lat"]) / 2)
    return math.hypot((a["lon"] - b["lon"]) * math.cos(lat), a["lat"] - b["lat"]) * 111320


def valid_place(p):
    return isinstance(p, dict) and isinstance(p.get("name"), str) and bool(p["name"].strip()) and \
        p.get("system") in {"wgs84", "gcj02"} and type(p.get("lat")) in (float, int) and \
        type(p.get("lon")) in (float, int) and math.isfinite(p["lat"]) and math.isfinite(p["lon"]) and \
        abs(p["lat"]) <= 90 and abs(p["lon"]) <= 180


def valid_fix(p, now):
    return valid_place({"name": "GPS", **p}) and type(p.get("timestamp")) in (int, float) and \
        -5 <= now - p["timestamp"] <= 60 and type(p.get("accuracy")) in (int, float) and \
        0 <= p["accuracy"] <= MAX_FIX_ACCURACY


def fix_failure(p, now):
    timestamp = p.get("timestamp")
    if type(timestamp) in (int, float) and not -5 <= now - timestamp <= 60:
        return "定位失败 · 位置已过期，等待设备更新"
    accuracy = p.get("accuracy")
    if type(accuracy) in (int, float) and math.isfinite(accuracy) and accuracy > MAX_FIX_ACCURACY:
        return f"定位失败 · 当前精度约{accuracy:.0f}米，自动识别要求{MAX_FIX_ACCURACY}米以内"
    return "定位失败 · 设备暂无有效位置"


def fmt_time(epoch):
    return datetime.fromtimestamp(epoch, TZ).strftime("%H:%M") if epoch is not None else "待确认"


def fmt_minutes(seconds):
    return f"{max(1, math.ceil(seconds / 60))} 分钟" if seconds is not None else "暂无时长数据"


class Engine:
    def __init__(self, root, clock=time.time, seed_history=False):
        self.root, self.clock = Path(root), clock
        self.root.mkdir(parents=True, exist_ok=True)
        first_install = not any((self.root / name).exists() for name in
                                ("cfg-v060.json", "state-v060.json", "hist-v060.json", "cfg.json", "hist.json"))
        self.migrate()
        self.cfg = read_json(self.root / "cfg-v060.json", {})
        self.cfg.setdefault("hiddenPlaceTypes", [])
        self.cfg["commute"].setdefault("requiredArrival", "")
        self.cfg["commute"].setdefault("dateMode", "official")
        self.hist = read_json(self.root / "hist-v060.json", [])
        self.state = read_json(self.root / "state-v060.json", {"schema": 6, "revision": 0, "trip": None,
            "origin": None, "destination": None, "routes": [], "selected": None, "search": [], "message": "",
            "location": None, "locationStatus": "定位尚未开启", "queryStatus": "高德接口待配置",
            "aiStatus": "智能出行尚未开启", "intent": None, "snoozed": {}, "declined": {}, "receipts": [],
            "learning": {}, "alternatives": [], "lastAI": 0, "aiHashes": {}})
        self.state.setdefault("aiMemory", {"text":"暂无已完成行程，完成一次出行后更新分析。", "status":"", "historyKey":""})
        self.state.setdefault("recommendationStatus", "")
        self.state.setdefault("previewStatus", "")
        self.state.setdefault("lastAIByTask", {})
        self.state.setdefault("originMode", "device")
        if seed_history and first_install:
            initial = Path(__file__).parent / "data"
            self.hist = read_json(initial / "initial-history.json", [])
            memory = read_json(initial / "initial-memory.json", {})
            self.state["aiMemory"] = {"text":memory.get("text", ""), "status":"", "historyKey":""}
        self.last_observation = None
        self.outside = []
        self.recent = []
        self.pending_generation = 0
        trip = self.state.get("trip")
        if trip and trip["status"] in ACTIVE and trip["segments"]:
            if self.clock() - self.state.get("updatedAt", 0) > 5:
                trip["segments"][-1]["uncertain"] = True
        self.state["locationStatus"] = "等待新的真实定位；停机期间没有轨迹记录" if self.cfg["tracking"] else "定位尚未开启"
        self.save()

    def migrate(self):
        if (self.root / "cfg-v060.json").exists():
            return
        old = read_json(self.root / "cfg.json", {})
        old_hist = read_json(self.root / "hist.json", [])
        for name in ("cfg.json", "hist.json"):
            source = self.root / name
            backup = self.root / (name + ".backup-0.5.4")
            if source.exists() and not backup.exists():
                backup.write_bytes(source.read_bytes())
        places, unverified = [], []
        for field, kind in (("home", "home"), ("work", "work")):
            p = {**(old.get(field) if isinstance(old.get(field), dict) else {}),
                 "id": kind, "name": old.get(field + "Name", "家" if kind == "home" else "公司"),
                 "address": "", "type": kind, "system": old.get(field + "CoordSystem"),
                 "source": old.get(field + "Source")}
            if valid_place(p) and p["source"] in {"device", "amap", "user"}:
                places.append(p)
            elif p["name"]:
                unverified.append({"name": p["name"], "type": kind, "reason": "旧版地点来源未验证，请重新选择位置"})
        cfg = {"schema": 6, "places": places, "legacyPlaces": unverified, "city": "北京",
               "tracking": old.get("backgroundLocation", True) is True,
               "aiEnabled": False, "reminders": old.get("anomalyNotice", True) is True,
               "preferences": {"mode": "all", "route": "fast"},
               "commute": {"originId": "", "destinationId": "", "days": [0, 1, 2, 3, 4],
                           "depart": "08:00", "returnTime": "18:00", "lead": 10, "exceptions": []}}
        migrated = []
        for index, rec in enumerate(old_hist if isinstance(old_hist, list) else []):
            if isinstance(rec, dict):
                migrated.append({"id": "legacy-" + str(index), "status": "legacy", "source": "legacy",
                    "originName": old.get("homeName", "旧版起点"), "destinationName": old.get("workName", "旧版目的地"),
                    "startedAt": rec.get("epoch"), "arrivedAt": None, "duration": rec.get("durMin", 0) * 60,
                    "segments": [], "legacy": rec, "note": "保留的旧版记录；起止与数据来源未验证，不参与学习"})
        atomic_json(self.root / "cfg-v060.json", cfg)
        atomic_json(self.root / "hist-v060.json", migrated)

    def save(self):
        self.state["revision"] += 1
        self.state["updatedAt"] = self.clock()
        atomic_json(self.root / "cfg-v060.json", self.cfg)
        atomic_json(self.root / "hist-v060.json", self.hist)
        atomic_json(self.root / "state-v060.json", self.state)
        atomic_json(self.root / "view-state.json", self.view())

    def message(self, text):
        self.state["message"] = text

    def command(self, cmd):
        cid = cmd.get("id")
        if not isinstance(cid, str) or not 1 <= len(cid) <= 100:
            raise ValueError("操作 ID 不合法")
        if cid in self.state["receipts"]:
            return False
        payload, action = cmd.get("payload", {}), cmd.get("action")
        trip = self.state.get("trip")
        if cmd.get("tripId") and (not trip or cmd["tripId"] != trip["id"]):
            raise ValueError("该操作所属行程已变化")
        if action == "set_origin" or action == "set_destination":
            p = payload.get("place")
            if not valid_place(p):
                raise ValueError("请选择有有效位置的地点")
            self.state["origin" if action == "set_origin" else "destination"] = p
            if action == "set_origin":
                self.state["originMode"] = "device" if p.get("source") == "device" or p.get("timestamp") is not None else "manual"
            self.state["routes"], self.state["selected"] = [], None
            self.pending_generation += 1
        elif action == "save_place":
            p = payload.get("place")
            if not valid_place(p):
                raise ValueError("请先搜索地点或取得有效当前位置")
            p = {**p, "id": payload.get("placeId") or str(uuid.uuid4()), "type": payload.get("type", "custom"),
                 "name": str(payload.get("name") or p["name"])[:80]}
            self.cfg["places"] = [x for x in self.cfg["places"] if x["id"] != p["id"]] + [p]
            self.cfg["legacyPlaces"] = [x for x in self.cfg.get("legacyPlaces", []) if x["type"] != p["type"]]
            self.cfg["hiddenPlaceTypes"] = [k for k in self.cfg.get("hiddenPlaceTypes", []) if k != p["type"]]
            self.message("常用地点已保存")
        elif action == "remove_place":
            kind = next((p.get("type") for p in self.cfg["places"] if p["id"] == payload.get("placeId")), payload.get("type"))
            if kind in {"home","work","school"}:
                self.cfg["hiddenPlaceTypes"] = list(dict.fromkeys(self.cfg.get("hiddenPlaceTypes", []) + [kind]))
                self.cfg["legacyPlaces"] = [p for p in self.cfg.get("legacyPlaces", []) if p.get("type") != kind]
            self.cfg["places"] = [p for p in self.cfg["places"] if p["id"] != payload.get("placeId")]
            self.message("常用地点已移除")
        elif action == "select_route":
            if payload.get("routeId") not in {r["id"] for r in self.state["routes"]}:
                raise ValueError("路线已失效，请重新查询")
            self.state["selected"] = payload["routeId"]
        elif action == "start":
            intent = self.state.get("intent")
            if intent and intent.get("confirmed") and intent["expires"] > self.clock():
                self.start("commute", intent["plannedAt"], intent["direction"])
            else:
                self.start()
        elif action == "toggle":
            field = payload.get("field")
            if field not in {"tracking", "reminders", "aiEnabled"}:
                raise ValueError("未知设置")
            self.cfg[field] = not self.cfg[field]
            if field == "tracking" and not self.cfg[field]:
                self.outside, self.recent = [], []
                self.state["location"] = None
                self.state["locationStatus"] = "定位已关闭；自动阶段识别暂停"
            if field == "aiEnabled":
                self.state["recommendationStatus"] = "AI正在分析出行方案…" if self.cfg[field] else ""
                self.state["aiMemory"]["status"] = "" if self.cfg[field] else "AI 功能已关闭"
                if trip and not trip.get("aiSummary"):
                    trip["summaryRequested"] = False
                self.pending_generation += 1
                for route in self.state["routes"]:
                    route["recommended"], route["reason"] = False, ""
            if field == "aiEnabled" and not self.cfg[field]:
                self.state["intent"] = None
                self.state["aiStatus"] = "智能出行已关闭"
        elif action == "preferences":
            if payload.get("mode") not in {"all", "walk", "transit"} or payload.get("route") not in {"fast", "lesswalk", "lesstransfer"}:
                raise ValueError("出行偏好不合法")
            self.cfg["preferences"] = payload
            self.state["recommendationStatus"] = "AI正在分析出行方案…" if self.cfg["aiEnabled"] else ""
            for route in self.state["routes"]:
                route["recommended"], route["reason"] = False, ""
            self.pending_generation += 1
        elif action == "commute":
            self.set_commute(payload)
        elif action == "cancel":
            if not trip or trip["status"] not in ACTIVE:
                raise ValueError("没有进行中的行程")
            now = self.clock()
            self.state["trip"] = None
            self.state["busArrival"] = None
            self.state["navigation"] = {"id": str(uuid.uuid4()), "expires": now + 30, "page": "home"}
            self.message("行程已结束")
        elif action == "not_arrived":
            if not trip or trip["status"] != "arrived":
                raise ValueError("没有可恢复的到达行程")
            trip["status"] = trip.pop("beforeArrival", "lastwalk")
            trip["rejectedArrivalAt"] = trip["arrivedAt"]
            trip["arrivedAt"] = None
            trip["corrected"] = True
            trip["aiSummary"] = ""
            trip.pop("summaryRequested", None)
            self.hist = [h for h in self.hist if h["id"] != trip["id"]]
            self.recompute_learning()
            self.message("已恢复原行程；等待新的有效定位")
        elif action == "close_summary":
            if trip and trip["status"] in {"arrived", "canceled"}:
                self.state["trip"] = None
                self.state["navigation"] = {"id":str(uuid.uuid4()), "expires":self.clock()+30, "page":"home"}
        elif action == "correct":
            self.correct(payload)
        elif action == "delete_history":
            record_id = payload.get("id")
            if not isinstance(record_id, str) or not any(h["id"] == record_id for h in self.hist):
                raise ValueError("这条历史记录已不存在")
            self.hist = [h for h in self.hist if h["id"] != record_id]
            self.recompute_learning()
            self.state["aiMemory"] = {"text":"历史记录已更新，等待重新分析。" if self.hist else "暂无已完成行程，完成一次出行后更新分析。",
                                      "status":"", "historyKey":""}
            self.message("历史记录已删除")
        elif action == "clear_history":
            self.hist = []
            self.state["learning"] = {}
            self.state["aiMemory"] = {"text":"暂无已完成行程，完成一次出行后更新分析。", "status":"", "historyKey":""}
            # Clear the original files too: hiding migrated records is not deletion.
            for name in ("hist.json", "hist.json.backup-0.5.4", "ai-context.json"):
                if (self.root / name).exists():
                    (self.root / name).unlink()
            self.message("历史记录与学习摘要已清除")
        elif action in {"intent_confirm", "intent_decline", "intent_snooze"}:
            self.intent_feedback(action, payload)
        elif action == "show_routes":
            self.state["navigation"] = {"id": str(uuid.uuid4()), "expires": self.clock() + 30, "page": "routes"}
        else:
            raise ValueError("未知操作")
        self.state["receipts"] = (self.state["receipts"] + [cid])[-100:]
        self.save()
        return True

    def set_commute(self, p):
        if p.get("dateMode", "official") not in {"official", "custom"}:
            raise ValueError("请选择官方日期或自定义日期")
        ids = {place["id"] for place in self.cfg["places"]}
        if p.get("originId") not in ids or p.get("destinationId") not in ids or p["originId"] == p["destinationId"]:
            raise ValueError("请选择两个已保存的不同地点")
        for field in ("depart", "returnTime"):
            try:
                datetime.strptime(p[field], "%H:%M")
            except (ValueError, KeyError, TypeError):
                raise ValueError("时间格式应为 HH:MM") from None
        days = p.get("days")
        if not isinstance(days, list) or not days or any(type(d) is not int or d not in range(7) for d in days):
            raise ValueError("请选择通勤日期")
        lead = p.get("lead", 10)
        if type(lead) is float and math.isfinite(lead) and lead.is_integer():
            lead = int(lead)
        if type(lead) is not int or not 1 <= lead <= 120:
            raise ValueError("提前提醒应为 1 至 120 分钟")
        exceptions = p.get("exceptions", [])
        if not isinstance(exceptions, list) or len(exceptions) > 366:
            raise ValueError("例外日期不合法")
        try:
            for day in exceptions:
                datetime.strptime(day, "%Y-%m-%d")
        except (ValueError, TypeError):
            raise ValueError("例外日期格式应为 YYYY-MM-DD") from None
        if p.get("requiredArrival"):
            try:
                datetime.strptime(p["requiredArrival"], "%H:%M")
            except (ValueError, TypeError):
                raise ValueError("要求到达时间格式应为 HH:MM") from None
        self.cfg["commute"] = {**self.cfg["commute"], **p}
        self.cfg["commute"]["lead"] = lead
        self.pending_generation += 1
        self.state["intent"] = None
        self.message("出行习惯已保存，自动学习不会覆盖此设置")

    def start(self, source="manual", planned=None, direction=None):
        current = self.state.get("trip")
        route = next((r for r in self.state["routes"] if r["id"] == self.state["selected"]), None)
        if not route or not route.get("segments") or not valid_place(self.state["origin"]) or not valid_place(self.state["destination"]):
            raise ValueError("需要明确起终点并选择实际可用路线")
        if self.state["origin"]["system"] != self.state["destination"]["system"]:
            raise ValueError("起终点坐标系不一致")
        if distance(self.state["origin"], self.state["destination"]) <= 100:
            raise ValueError("起终点距离过近，出发与到达范围重叠，请调整地点")
        now = self.clock()
        if current and current["status"] in ACTIVE:
            if current["route"]["id"] == route["id"] and current["destination"] == self.state["destination"] and current["origin"] == self.state["origin"]:
                raise ValueError("当前行程已使用这个方案")
            current["status"], current["endedAt"] = "canceled", now
            self.record(current)
        self.cfg["tracking"] = True
        self.state["trip"] = {"id": str(uuid.uuid4()), "status": "preparing", "source": source,
            "origin": copy.deepcopy(self.state["origin"]), "destination": copy.deepcopy(self.state["destination"]),
            "route": copy.deepcopy(route), "createdAt": now, "plannedAt": planned or now, "startedAt": None,
            "arrivedAt": None, "segment": 0, "segments": [], "corrected": False, "direction": direction,
            "startSource": None, "rejectedArrivalAt": 0, "initialEstimate": None, "lastPositionAt": None}
        self.outside, self.recent = [], []
        self.state["intent"] = None
        self.message("准备出发 · 等待离开本次固定起点，尚未开始计时")

    def observe(self, fix):
        now, trip = self.clock(), self.state.get("trip")
        if not self.cfg["tracking"]:
            return
        if not valid_fix(fix, now):
            self.state["locationStatus"] = fix_failure(fix, now)
            self.outside, self.recent = [], []
            if trip and trip["segments"]:
                trip["segments"][-1]["uncertain"] = True
            return
        previous = self.state.get("location")
        nearby = previous and previous["system"] == fix["system"] and distance(previous, fix) <= 100
        self.state["location"] = {**fix, "source": "device", "name": fix.get("name") or (previous.get("name") if nearby else None) or "当前位置",
                                  "address": fix.get("address") or (previous.get("address") if nearby else "") or ""}
        if self.state.get("originMode", "device") == "device":
            self.state["origin"] = copy.deepcopy(self.state["location"])
        self.state["locationStatus"] = "真实定位可用 · 精度约 " + str(round(fix["accuracy"])) + " 米"
        if not trip or trip["status"] not in ACTIVE:
            if not trip and self.state["origin"] is None:
                self.state["origin"] = copy.deepcopy(self.state["location"])
            return
        if fix["timestamp"] <= (trip.get("lastPositionAt") or 0):
            return  # Re-reading the same source fix never counts as consecutive movement.
        if fix["system"] != trip["origin"]["system"]:
            self.state["locationStatus"] = "定位失败 · 等待坐标转换，保留最近行程阶段"
            return
        if self.last_observation:
            dt = fix["timestamp"] - self.last_observation["timestamp"]
            if dt > 90:
                self.outside, self.recent = [], []
                if trip["segments"]:
                    trip["segments"][-1]["uncertain"] = True
            elif dt <= 0 or distance(fix, self.last_observation) / dt > 55:
                self.outside, self.recent = [], []
                self.state["locationStatus"] = "定位失败 · 单次跳点已忽略"
                return
        self.last_observation = fix
        trip["lastPositionAt"] = fix["timestamp"]
        self.recent = (self.recent + [fix])[-5:]
        if trip["status"] == "preparing":
            if distance(fix, trip["origin"]) > 50:
                self.outside.append(fix["timestamp"])
                if len(self.outside) >= 2 and self.outside[-1] - self.outside[0] >= 2:
                    trip["startedAt"], trip["startSource"] = self.outside[0], "device_estimated_first_crossing"
                    trip["initialEstimate"] = trip["route"].get("duration")
                    self.transition("walking" if trip["route"]["segments"][0]["mode"] == "walk" else "waiting", fix["timestamp"])
                    self.message("已确认实际出发")
            else:
                self.outside = []
            return
        if fix["timestamp"] > trip.get("rejectedArrivalAt", 0) and distance(fix, trip["destination"]) <= 50:
            trip["beforeArrival"], trip["status"], trip["arrivedAt"] = trip["status"], "arrived", fix["timestamp"]
            self.close_segment(fix["timestamp"])
            self.record(trip)
            self.message("已到达，行程已保存")
            return
        self.detect_segment(fix)

    def close_segment(self, now):
        trip = self.state["trip"]
        if trip["segments"] and trip["segments"][-1].get("endedAt") is None:
            trip["segments"][-1]["endedAt"] = now

    def transition(self, status, now, segment=None, source="device"):
        trip = self.state["trip"]
        self.close_segment(now)
        trip["status"] = status
        if segment is not None:
            trip["segment"] = segment
        trip["segments"].append({"stage": status, "index": trip["segment"], "startedAt": now,
                                 "endedAt": None, "source": source, "uncertain": False})

    def detect_segment(self, fix):
        trip = self.state["trip"]
        segments, index = trip["route"]["segments"], trip["segment"]
        segment = segments[index]
        if len(self.recent) < 3:
            return
        a, b = self.recent[-3], self.recent[-1]
        dt = b["timestamp"] - a["timestamp"]
        if dt < 4 or dt > 90:
            return
        speed = distance(a, b) / dt
        if segment["mode"] == "walk":
            next_index = index + 1
            if next_index < len(segments) and segments[next_index]["mode"] == "transit":
                stop = segments[next_index].get("start")
                if stop and distance(fix, stop) <= 50 and speed < 1.5:
                    self.transition("waiting", fix["timestamp"], next_index)
        elif trip["status"] == "waiting":
            start = segment.get("start")
            if start and speed > 3 and distance(fix, start) > 60 and route_distance(fix, segment) < 80:
                self.transition("riding", fix["timestamp"])
        elif trip["status"] == "riding":
            end = segment.get("end")
            if end and all(distance(p, end) <= 65 for p in self.recent[-2:]) and speed < 2.2:
                next_index = index + 1
                if next_index < len(segments):
                    rest = segments[next_index:]
                    status = "transfer" if any(s["mode"] == "transit" for s in rest) else "lastwalk"
                    self.transition(status, fix["timestamp"], next_index)
        offroute = route_distance(fix, segment)
        if offroute > 150:
            self.message("可能偏离路线 · 请重新查询并选择方案；原路线保留")

    def correct(self, p):
        trip = self.state.get("trip")
        if not trip or trip["status"] not in ACTIVE:
            raise ValueError("没有进行中的行程")
        status, now = p.get("stage"), self.clock()
        if status == "departed":
            if trip["startedAt"] is not None:
                raise ValueError("行程已实际出发")
            trip["startedAt"], trip["startSource"] = now, "user"
            trip["initialEstimate"] = trip["route"].get("duration")
            self.transition("walking" if trip["route"]["segments"][0]["mode"] == "walk" else "waiting", now, source="user")
        elif status == "arrived":
            if not any(o["stage"] == "arrived" for o in self.correction_options(trip)):
                raise ValueError("请先完成当前乘车或步行阶段")
            trip["beforeArrival"], trip["status"], trip["arrivedAt"] = trip["status"], "arrived", now
            trip["corrected"] = True
            self.close_segment(now)
            self.record(trip)
        else:
            if trip["startedAt"] is None or status not in {"walking", "waiting", "riding", "transfer", "lastwalk"}:
                raise ValueError("请先确认实际出发，再纠正有效阶段")
            index = p.get("segment", trip["segment"])
            segments = trip["route"]["segments"]
            if type(index) is not int or index not in range(len(segments)):
                raise ValueError("路线分段不存在")
            if status in {"waiting", "riding"} and segments[index]["mode"] != "transit":
                raise ValueError("当前分段不是乘车路线")
            if status in {"walking", "transfer", "lastwalk"} and segments[index]["mode"] != "walk":
                raise ValueError("当前分段不是步行路线")
            self.transition(status, now, index, "user")
        trip["corrected"] = True
        self.message("已按你的纠正更新行程，记录来源为人工")

    def record(self, trip):
        ended = trip.get("arrivedAt") or trip.get("endedAt")
        record = {"id": trip["id"], "status": trip["status"], "source": trip["source"],
            "originName": trip["origin"]["name"], "destinationName": trip["destination"]["name"],
            "startedAt": trip["startedAt"], "arrivedAt": ended,
            "duration": ended - trip["startedAt"] if trip["startedAt"] is not None else None,
            "distance": trip["route"].get("distance"), "routeId": trip["route"]["id"],
            "direction": trip["direction"], "segments": copy.deepcopy(trip["segments"]),
            "corrected": trip["corrected"], "startSource": trip["startSource"],
            "completeSegments": bool(trip["segments"]) and not any(s.get("uncertain") for s in trip["segments"]),
            "mode": trip["route"]["mode"], "walkingDistance":trip["route"].get("walkingDistance"),
            "transfers":trip["route"].get("transfers"),
            "initialEstimate": trip["initialEstimate"], "date": datetime.fromtimestamp(ended, TZ).strftime("%Y-%m-%d")}
        self.hist = [h for h in self.hist if h["id"] != trip["id"]] + [record]
        self.recompute_learning()

    def recompute_learning(self):
        groups = {}
        for record in self.hist:
            if record["status"] == "arrived" and record.get("direction") and record.get("duration") is not None and record.get("completeSegments") and not record.get("corrected"):
                key = record["direction"] + ":" + record["routeId"]
                groups.setdefault(key, []).append(record)
        self.state["learning"] = {k: {"samples": len(v), "meanSeconds": sum(x["duration"] for x in v) / len(v),
            "from": v[0].get("date"), "to": v[-1].get("date")} for k, v in groups.items()}

    def intent_candidate(self):
        now = self.clock()
        if not self.cfg["aiEnabled"] or not self.cfg["reminders"] or self.state.get("trip") or not self.cfg["tracking"]:
            return None
        loc = self.state.get("location")
        if not loc or not valid_fix(loc, now):
            return None
        habit = self.cfg["commute"]
        local = datetime.fromtimestamp(now, TZ)
        day = local.strftime("%Y-%m-%d")
        from .workcalendar import is_workday
        working = local.weekday() in habit["days"] if habit.get("dateMode") == "custom" else is_workday(local.date())
        if working is not True or day in habit.get("exceptions", []):
            return None
        places = {p["id"]: p for p in self.cfg["places"]}
        for direction, source_id, target_id, field in (("outbound", habit["originId"], habit["destinationId"], "depart"),
                ("return", habit["destinationId"], habit["originId"], "returnTime")):
            if source_id not in places or target_id not in places:
                continue
            origin, dest = places[source_id], places[target_id]
            key = day + ":" + direction
            if self.state["declined"].get(key) or self.state["snoozed"].get(key, 0) > now:
                continue
            if any(h.get("date") == day and h.get("direction") == direction and h["status"] == "arrived" for h in self.hist):
                continue
            planned = datetime.combine(local.date(), datetime.strptime(habit[field], "%H:%M").time(), TZ).timestamp()
            if direction == "outbound" and habit.get("requiredArrival"):
                # Look up a real route in the hour before the user's arrival deadline.
                planned = datetime.combine(local.date(), datetime.strptime(habit["requiredArrival"], "%H:%M").time(), TZ).timestamp() - 3600
            expires = planned + (90 if direction == "outbound" and habit.get("requiredArrival") else 30) * 60
            if planned - habit["lead"] * 60 <= now <= expires and loc["system"] == origin["system"] and distance(loc, origin) <= 100:
                return {"id": key, "direction": direction, "origin": origin, "destination": dest,
                        "plannedAt": planned, "requiredArrival":habit.get("requiredArrival", "") if direction == "outbound" else "", "expires": expires}
        return None

    def intent_feedback(self, action, p):
        intent = self.state.get("intent")
        if not intent or intent["expires"] < self.clock():
            raise ValueError("出行邀请已失效")
        if intent.get("preview"):
            self.state.update(intent.get("previous", {}))
            self.state["intent"] = None
            return
        if action == "intent_decline":
            self.state["declined"][intent["id"]] = True
            self.state["intent"] = None
        elif action == "intent_snooze":
            minutes = p.get("minutes")
            if type(minutes) is not int or not 1 <= minutes <= 120:
                raise ValueError("请选择延后分钟数")
            self.state["snoozed"][intent["id"]] = self.clock() + minutes * 60
            self.state["intent"] = None
        else:
            self.message("请选择出行方案后开始导航")
            intent["confirmed"] = True
            self.state["navigation"] = {"id": str(uuid.uuid4()), "expires": self.clock() + 30, "page": "routes"}

    def view(self):
        now, trip = self.clock(), self.state.get("trip")
        def ui_route(route):
            lines = [s["name"].split("(")[0].split("（")[0] for s in route["segments"] if s["mode"] == "transit"]
            return {"recommended": False, "reason": "", **route,
                    "displaySummary": " → ".join(lines) if lines else "步行方案",
                    "durationMinutes": math.ceil(route["duration"] / 60) if route.get("duration") is not None else None,
                    "arrivalText": fmt_time(now + route["duration"]) if route.get("duration") is not None else "暂无预估",
                    "queriedText": fmt_time(route.get("queriedAt")),
                    "segments": [{"to": "", **s} for s in route["segments"]]}
        ui_trip = {"aiSummary": "", "aiSummaryStatus":"正在生成…" if self.cfg["aiEnabled"] else "AI 功能未开启", **trip, "route": ui_route(trip["route"])} if trip else None
        preference = self.cfg["preferences"]
        metric = {"fast": "duration", "lesswalk": "walkingDistance", "lesstransfer": "transfers"}[preference["route"]]
        def route_order(route):
            def known(field):
                value = route.get(field)
                return value if value is not None else float("inf")
            return (preference["mode"] != "all" and route["mode"] != preference["mode"], known(metric), known("duration"))
        history = [{"date": "", "distance": None, "startedAt": None, "arrivedAt": None, **h,
                    "resultText": {"arrived": "已完成", "canceled": "已取消", "legacy": "旧版来源未验证"}.get(h["status"], "已保存"),
                    "startedText": fmt_time(h.get("startedAt")), "endedText": fmt_time(h.get("arrivedAt")),
                    "segments": [{"uncertain": False, **s, "stageText": STAGE_LABELS.get(s["stage"], "来源未验证的阶段")} for s in h["segments"]]}
                   for h in reversed(self.hist[-200:])]
        result = {"schema": 6, "revision": self.state["revision"], "updatedAt": self.state.get("updatedAt", now),
            "config": self.cfg, "locationStatus": self.state["locationStatus"], "location": self.state["location"],
            "originMode": self.state.get("originMode", "device"), "origin": self.state["origin"], "destination": self.state["destination"], "search": self.state["search"],
            "routes": [ui_route(r) for r in sorted(self.state["routes"], key=route_order)], "selected": self.state["selected"], "message": self.state["message"],
            "aiMemory":{**self.state["aiMemory"], **memory_sections(self.state["aiMemory"]["text"], self.cfg["preferences"])}, "recommendationStatus":self.state["recommendationStatus"],
            "queryStatus": self.state["queryStatus"], "previewStatus":self.state["previewStatus"], "aiStatus": self.state["aiStatus"], "receipts": self.state["receipts"],
            "intent": self.state["intent"], "history": history,
            "learning": self.state["learning"], "trip": ui_trip, "cardVisible": bool(trip or self.state["intent"] and not self.state["intent"].get("confirmed")),
            "card": {"kind": "C1", "title": "等待出行", "subtitle": "请选择目的地与实际可用路线", "facts": [],
                     "guide": [], "arrival": "", "notice": "", "summary": "", "stage": "idle"}}
        nav = self.state.get("navigation")
        result["navigation"] = nav if nav and nav["expires"] > now else None
        card = result["card"]
        if trip:
            stage, route = trip["status"], trip["route"]
            card["stage"] = stage
            card["kind"] = {"preparing": "C1", "walking": "C2", "waiting": "C2" if trip["segment"] <= 1 else "C4",
                "riding": "C3", "transfer": "C4", "lastwalk": "C4", "arrived": "C5", "canceled": "C5"}[stage]
            card["title"] = {"preparing": "准备出发", "walking": "正在步行", "waiting": "等候乘车", "riding": "乘车中",
                "transfer": "继续换乘", "lastwalk": "前往目的地", "arrived": "已到达", "canceled": "行程已结束"}[stage]
            card["subtitle"] = trip["origin"]["name"] + " → " + trip["destination"]["name"]
            duration = route.get("duration")
            departed = max(trip["plannedAt"], math.floor(now / 60) * 60) if stage == "preparing" else trip["startedAt"]
            card["facts"] = [{"label": "预计出发" if stage == "preparing" else "实际出发", "value": fmt_time(departed)},
                {"label": "预估时长", "value": fmt_minutes(duration)},
                {"label": "路线距离", "value": f'{route["distance"] / 1000:.1f} 公里' if route.get("distance") is not None else "暂无距离数据"}]
            # Static route duration is an estimate, never a realtime vehicle countdown.
            if stage == "preparing":
                card["arrival"] = fmt_time(departed + duration) if duration is not None else "暂无到达预估"
                card["notice"] = "等待实际出发 · 尚未开始计时"
            elif stage in {"arrived", "canceled"}:
                end = trip.get("arrivedAt") or trip.get("endedAt")
                card["arrival"] = fmt_time(end)
                card["facts"] = [{"label": "实际出发", "value": fmt_time(trip["startedAt"])},
                    {"label": "实际耗时", "value": fmt_minutes(end - trip["startedAt"]) if trip["startedAt"] is not None else "尚未实际出发"}]
                card["facts"].append({"label": "路线距离", "value": f'{route["distance"] / 1000:.1f} 公里' if route.get("distance") is not None else "暂无距离数据"})
                estimate = trip.get("initialEstimate")
                if estimate is not None and trip["startedAt"] is not None:
                    delta = round((end - trip["startedAt"] - estimate) / 60)
                    card["summary"] = f'较实际出发时预估 {"多" if delta >= 0 else "少"} {abs(delta)} 分钟'
                prior = [h for h in self.hist if h["id"] != trip["id"] and h["status"] == "arrived"
                         and h.get("routeId") == route["id"] and h.get("direction") == trip.get("direction")
                         and h.get("duration") is not None and h.get("completeSegments") and not h.get("corrected")]
                if prior and trip["startedAt"] is not None:
                    mean = sum(h["duration"] for h in prior) / len(prior)
                    delta = round((end - trip["startedAt"] - mean) / 60)
                    card["summary"] += f'；较此前 {len(prior)} 次同方向同方案 {"多" if delta >= 0 else "少"} {abs(delta)} 分钟'
                else:
                    card["summary"] += "；暂无可比较的同方向同方案历史"
                card["guide"] = [{"title": STAGE_LABELS.get(s["stage"], "来源未验证的阶段"), "text": "该段包含定位中断 · 可靠时长未知" if s.get("uncertain") else
                    fmt_minutes(s["endedAt"] - s["startedAt"]) if s.get("endedAt") else "该段时间未知"} for s in trip["segments"]]
            else:
                segments, index = route["segments"], trip["segment"]
                seg = segments[index]
                remaining = self.remaining_duration(trip)
                card["arrival"] = fmt_time(now + remaining) if remaining is not None else "暂无到达预估"
                card["notice"] = "路线时长为预估 · 更新 " + fmt_time(route.get("queriedAt"))
                if not self.state["location"] or not valid_fix(self.state["location"], now):
                    card["notice"] = "定位失败 · 保留最近已确认阶段与信息"
                if seg["mode"] == "transit":
                    card["guide"] = [{"title": seg["name"], "text": "开往 " + seg.get("direction", "")},
                        {"title": "下车站", "text": seg.get("to", "")},
                        {"title": "车辆到站", "text": "查询失败 · 暂无可用到站数据"}]
                    if stage == "riding":
                        card["guide"][-1] = {"title": "剩余站数", "text": "暂无可核验的剩余站数"}
                        loc = self.state.get("location")
                        if loc and valid_fix(loc, now) and loc["system"] == trip["origin"]["system"]:
                            progress = route_progress(loc, seg)
                            stops = [coordinate(s.get("location", "")) for s in seg.get("stops", [])]
                            fractions = [route_progress(p, seg) for p in stops if p]
                            if progress is not None and fractions and all(f is not None for f in fractions):
                                left = sum(f > progress for f in fractions) + 1
                                card["guide"][-1]["text"] = f"约 {left} 站 · 根据当前定位与路线估算"
                    loc = self.state.get("location")
                    if stage == "riding" and loc and valid_fix(loc, now) and seg.get("end") and distance(loc, seg["end"]) < 300:
                        card["title"] = "即将到站，请准备下车"
                else:
                    steps = seg.get("steps", [])
                    loc = self.state.get("location")
                    if steps and loc and valid_fix(loc, now) and loc["system"] == trip["origin"]["system"]:
                        nearest = min(range(len(steps)), key=lambda i: route_distance(loc, steps[i]))
                        if route_distance(loc, steps[nearest]) <= 100:
                            steps = steps[nearest:]
                    card["guide"] = [{"title": "下一步", "text": s["instruction"]} for s in steps[:3]]
                    loc = self.state.get("location")
                    progress = route_progress(loc, seg) if loc and valid_fix(loc, now) and loc["system"] == trip["origin"]["system"] else None
                    fraction = 1 - progress if progress is not None else 1
                    card["facts"] = [{"label": "本段剩余步行预估", "value": fmt_minutes(seg.get("duration") * fraction if seg.get("duration") is not None else None)},
                        {"label": "本段剩余距离预估", "value": f'{round(seg["distance"] * fraction)} 米' if seg.get("distance") is not None else "暂无距离数据"}]
                if seg["mode"] == "transit" and stage == "riding":
                    loc = self.state.get("location")
                    progress = route_progress(loc, seg) if loc and valid_fix(loc, now) and loc["system"] == trip["origin"]["system"] else None
                    fraction = 1 - progress if progress is not None else 1
                    ride_time = seg.get("duration") * fraction if seg.get("duration") is not None else None
                    card["facts"] = [{"label": "剩余乘车预估", "value": fmt_minutes(ride_time)},
                        {"label": "预计下车", "value": fmt_time(now + ride_time) if ride_time is not None else "暂无预估"}]
                if index + 1 < len(segments):
                    nxt = segments[index + 1]
                    card["guide"].append({"title": "下一段", "text": nxt["name"] + " · " + nxt.get("from", trip["destination"]["name"])})
                    if nxt["mode"] == "transit":
                        card["guide"].append({"title": "车辆到站", "text": "查询失败 · 暂无可用到站数据"})
        elif self.state["intent"]:
            intent = self.state["intent"]
            card.update({"kind": "C1", "stage": "intent_confirmed" if intent.get("confirmed") else "intent", "title": "准备出发 · 请选择方案" if intent.get("confirmed") else "今天需要出行吗？",
                "subtitle": intent["origin"]["name"] + " → " + intent["destination"]["name"],
                "facts": [{"label": "预计出发", "value": fmt_time(max(intent["plannedAt"], now))}],
                "summary": intent.get("reason", "依据你保存的出行习惯"), "notice": "确认后仍等待实际出发"})
            if self.state["routes"]:
                durations = [r["duration"] for r in self.state["routes"] if r.get("duration") is not None]
                if durations:
                    card["facts"].append({"label": "可用方案预估", "value": fmt_minutes(min(durations)) + "起 · 方案待选"})
        selected = next((r for r in result["routes"] if r["id"] == result["selected"]), None)
        result["selectedCaption"] = (fmt_minutes(selected.get("duration")) + " · 预计 " + selected["arrivalText"] + " 到达"
                                     if selected else "请先选择出行方案")
        card["corrections"] = self.correction_options(trip)
        card["visual"] = self.card_visual(card, trip, now)
        return result

    def bus_target(self):
        trip = self.state.get("trip")
        if not trip or trip["status"] not in ACTIVE or "北京" not in str(self.cfg.get("city", "北京")):
            return None
        segments, index, stage = trip["route"]["segments"], trip["segment"], trip["status"]
        start = 0 if stage == "preparing" else index if stage == "waiting" else index + 1
        candidate = next((i for i in range(start,len(segments)) if segments[i]["mode"] == "transit"), None)
        if candidate is None or "地铁" in segments[candidate]["name"]:
            return None
        return {"tripId":trip["id"], "segment":candidate, "routeSegment":segments[candidate]}

    def apply_bus_visual(self, visual, trip, now):
        target = self.bus_target()
        if not target:
            return
        visual["busVisible"] = True
        data = self.state.get("busArrival") or {}
        if data.get("marker") != [target["tripId"], target["segment"]]:
            data = {}
        status, fetched = data.get("status", "loading"), data.get("fetchedAt")
        nearest = data.get("nearest")
        stale = fetched is not None and (now-fetched > 90 or status == "error")
        label, detail, hero, unit = "正在查询到站信息…", "", "查询中", ""
        if stale:
            label = "数据已过期"
            if status == "error":label = "查询失败 · " + label
            hero = "数据已过期"
        elif status == "predicted" and nearest:
            seconds = max(0, nearest["seconds"]-(now-fetched))
            if nearest["seconds"] > 0 and seconds == 0:
                label, hero = "预测时间已到，请刷新", "请刷新"
            else:
                hero = str(math.ceil(seconds/60)) if seconds else "即将到站"
                unit = "分钟" if seconds else ""
                label = "约 " + hero + " 分钟到站" if seconds else hero
            meters = nearest.get("distance")
            detail = f"距离约 {round(meters)} 米" if meters is not None else "距离未知"
        elif status == "no_prediction":
            label, hero = "暂无到站预测", "暂无预测"
            detail = ""
        elif status == "error":
            label, hero = data.get("error", "查询失败"), "查询失败"
        if stale:
            detail = ""
        visual["connectionStatus"], visual["busDetail"] = label, detail
        if trip["status"] == "waiting":
            visual.update(hero=hero, unit=unit, secondary="", heroLabel="下一班预计到站")

    def correction_options(self, trip):
        if not trip or trip["status"] not in ACTIVE:
            return []
        stage, index = trip["status"], trip["segment"]
        segments = trip["route"]["segments"]
        def option(label, status, target):
            return {"label": label, "stage": status, "segment": target}
        if stage == "preparing":
            return [option("我已出发", "departed", index)]
        if stage in {"walking", "transfer", "lastwalk"}:
            following = next((i for i in range(index + 1, len(segments)) if segments[i]["mode"] == "transit"), None)
            if following is not None:
                return [option("我已在候车", "waiting", following), option("我已上车", "riding", following)]
            return [option("我已到达", "arrived", index)]
        if stage == "waiting":
            return [option("我已上车", "riding", index)]
        if stage == "riding":
            following = index + 1
            if following >= len(segments):
                return [option("我已到达", "arrived", index)]
            next_stage = "waiting" if segments[following]["mode"] == "transit" else "transfer" if any(s["mode"] == "transit" for s in segments[following:]) else "lastwalk"
            return [option("我已下车", next_stage, following)]
        return []

    def card_visual(self, card, trip, now):
        """Prototype presentation fields; never infer realtime bus arrivals from route estimates."""
        facts = {f["label"]: f["value"] for f in card["facts"]}
        v = dict(eyebrow=card["subtitle"], title=card["title"], subtitle=card["subtitle"],
                 heroLabel="预计出发", hero=facts.get("预计出发", "—"), unit="", secondary="",
                 distance=facts.get("路线距离", "暂无距离数据"), duration=facts.get("预估时长", "方案待选"),
                 instruction="", instructionCaption="", connectionLabel="", connection="",
                 connectionCaption="", connectionStatus="", alight="暂无预估", arrival=card["arrival"],
                 busVisible=False, busDetail="", intentPreview=False, footer="尚未开始计时", segments=[], comparison=card["summary"], departed="—", ended="—", confirmLabel="今天去")
        if not trip:
            v["subtitle"] = card["summary"] or card["subtitle"]
            intent = self.state.get("intent")
            if intent:
                v["intentPreview"] = bool(intent.get("preview"))
                labels = {"home": "家", "work": "公司", "school": "学校"}
                destination = intent["destination"]
                label = labels.get(destination.get("type"))
                v["confirmLabel"] = "今天回家" if label == "家" else "今天去" + label if label else "今天出发"
                v["title"] = "准备出发" if intent.get("confirmed") else v["confirmLabel"] + "吗？"
                v["eyebrow"] = "AI出行推荐 · 预览" if intent.get("preview") else "AI出行推荐"
                v["title"] = "看看出行方案" if intent.get("confirmed") else "准备去上班吗？" if intent["direction"] == "outbound" else "准备回家了吗？"
                v["subtitle"] = intent.get("reason") or "快到平时出门的时间了，要不要准备一下？"
                v["confirmLabel"] = "看看出行方案"
                v["heroLabel"] = "建议出发"
                v["hero"] = fmt_time(intent.get("suggestedAt", intent["plannedAt"]))
                selected = next((r for r in self.state["routes"] if r["id"] == intent.get("routeId")), None)
                if selected:
                    v["duration"] = fmt_minutes(selected.get("duration"))
                    v["distance"] = f'{selected["distance"]/1000:.1f} 公里' if selected.get("distance") is not None else "暂无距离数据"
                    v["arrival"] = fmt_time(max(now,intent.get("suggestedAt",intent["plannedAt"]))+selected["duration"]) if selected.get("duration") is not None else "方案待选"
                    lines = [s["name"].split("(")[0].split("（")[0] for s in selected["segments"] if s["mode"] == "transit"]
                    v["intentRoute"] = " → ".join(lines) or "步行方案"
                else:
                    v["intentRoute"] = "出行方案待查询"
                weather = intent.get("weather") or {}
                available = weather.get("status") == "available" and -300 <= now-weather.get("reportedAt",0) <= 4*3600
                v["intentWeather"] = weather.get("city", "")+" · "+weather.get("condition", "")+" · "+weather.get("temperature", "")+"℃" if available else "天气暂未获取，出门前留意一下天气。"
                v["intentWeatherNote"] = "记得带把伞，路上慢一点。" if available and any(x in weather["condition"] for x in ["雨","雪"]) else "出门前留意天气，按自己的节奏准备。" if available else ""
                v["intentDestination"] = destination["name"]
                v["intentTimingNote"] = intent.get("timingNote", "")
                v["duration"] = v.get("duration", "方案待选")
            return v
        stage, route = trip["status"], trip["route"]
        destination = trip["destination"]["name"]
        relationship = {"home": "家", "work": "公司", "school": "学校"}.get(trip["destination"].get("type"), destination)
        if stage == "preparing":
            v["subtitle"] = "路线已确认，等待实际出发。"
            transit = next((s for s in route["segments"] if s["mode"] == "transit"), None)
            if transit:
                stop = transit.get("from") or "上车点"
                direction = transit.get("direction")
                v.update(connectionLabel="到站信息", connection=transit["name"].split("(")[0].split("（")[0],
                         connectionCaption="上车站：" + stop + (" · 开往 " + direction if direction else ""),
                         connectionStatus="暂无可用到站数据")
            else:
                v.update(connectionLabel="到站信息", connection="步行前往目的地", connectionCaption=destination,
                         connectionStatus="本方案无需乘车")
            self.apply_bus_visual(v, trip, now)
            return v
        if stage in {"arrived", "canceled"}:
            v.update(eyebrow=card["arrival"] + (" 到达" if stage == "arrived" else " 结束"),
                     title="已到达" + destination if stage == "arrived" else "行程已结束", subtitle=destination,
                     heroLabel="本次实际耗时", hero=facts.get("实际耗时", "暂无记录"),
                     departed=facts.get("实际出发", "—"), ended=card["arrival"], footer="行程已保存，用于积累同方向的出行记录。")
            if v["hero"].endswith(" 分钟"):
                v["hero"], v["unit"] = v["hero"][:-3], "分钟"
            groups = {"步行": [], "候车": [], "乘车与衔接": []}
            for s in trip["segments"]:
                key = "候车" if s["stage"] == "waiting" else "步行" if s["stage"] in {"walking", "lastwalk"} else "乘车与衔接"
                groups[key].append(s)
            for label, entries in groups.items():
                unknown = any(s.get("uncertain") or not s.get("endedAt") for s in entries)
                value = "时长未知" if unknown else fmt_minutes(sum(s["endedAt"] - s["startedAt"] for s in entries)) if entries else "无记录"
                v["segments"].append({"label": label, "value": value})
            return v
        segs, i = route["segments"], trip["segment"]
        seg = segs[i]
        next_transit = next((s for s in segs[i+1:] if s["mode"] == "transit"), None)
        if stage == "riding":
            v.update(eyebrow=seg["name"].split("(")[0].split("（")[0] + (" · " + seg.get("direction", "") if seg.get("direction") else ""),
                     title="准备下车" if "准备下车" in card["title"] else "正在乘车", subtitle="在" + seg.get("to", destination) + "下车",
                     heroLabel="预计剩余乘车", hero=facts.get("剩余乘车预估", "暂无预估"), alight=facts.get("预计下车", "暂无预估"),
                     secondary=next((g["text"] for g in card["guide"] if g["title"] == "剩余站数"), ""), footer="临近下车时自动提醒")
        elif stage == "waiting":
            stop = seg.get("from", "上车点")
            v.update(eyebrow="已到上车点" if card["kind"] == "C2" else "换乘候车", title=stop + "候车",
                     subtitle=seg["name"] + " · " + seg.get("direction", ""), heroLabel="下一班预计到站",
                     hero="暂无数据", secondary="暂无可用到站数据", footer="位置更新时调整指引")
        else:
            stop = next_transit.get("from", "上车点") if next_transit else destination
            v.update(eyebrow="前往上车点" if card["kind"] == "C2" and next_transit else "最后一段步行" if stage == "lastwalk" else "下车后衔接" if stage == "transfer" else "步行出行",
                     title="前往" + stop if next_transit else "快到" + relationship + "了" if stage == "lastwalk" else "步行前往目的地",
                     subtitle="", heroLabel="剩余步行", hero=facts.get("本段剩余步行预估", "暂无预估"),
                     secondary="剩余 " + facts.get("本段剩余距离预估", "暂无距离数据"),
                     instruction=next((g["text"] for g in card["guide"] if g["title"] == "下一步"), "沿当前路线前行"),
                     instructionCaption="前往" + stop, footer="预估每分钟更新" if card["kind"] == "C4" else "位置更新时调整指引")
            remaining = facts.get("本段剩余距离预估", "暂无距离数据")
            meters = remaining.removesuffix(" 米")
            v.update(heroLabel="还差", hero=meters, unit="米" if remaining.endswith(" 米") else "",
                     secondary="")
            if stage == "transfer":
                v.update(title="前往换乘站", subtitle=stop)
            elif stage == "lastwalk":
                v["subtitle"] = destination
            if next_transit and not any(label in stop for label in ("公交站", "地铁站")):
                stop += "地铁站" if "地铁" in next_transit["name"] else "公交站"
            v["title"] = "前往" + stop
            v["instructionCaption"] = "前往" + stop
            v["subtitle"] = ""
        if v["hero"].endswith(" 分钟"):
            v["hero"], v["unit"] = v["hero"][:-3], "分钟"
        nxt = segs[i+1] if i+1 < len(segs) else None
        connection = seg if stage == "waiting" else next_transit if stage != "riding" else nxt
        if connection:
            v["connectionLabel"] = "下车后的衔接" if stage == "riding" else "到站信息" if card["kind"] == "C2" else "下一段出行"
            v["connection"] = connection["name"].split("(")[0].split("（")[0]
            v["connectionCaption"] = ("开往 " + connection.get("direction", "")) if connection["mode"] == "transit" else fmt_minutes(connection.get("duration")) + " · 步行衔接"
            v["connectionStatus"] = "暂无可用到站数据" if connection["mode"] == "transit" else "按路线预估，位置更新时调整。"
            if stage == "riding" and connection["mode"] == "walk" and next_transit:
                walk = f'步行 {round(connection["distance"])} 米' if connection.get("distance") is not None else "步行衔接"
                v["connection"] = walk + "，换乘 " + next_transit["name"].split("(")[0].split("（")[0]
                v["connectionCaption"] = "开往 " + next_transit.get("direction", "")
                v["connectionStatus"] = "暂无可用到站数据"
        self.apply_bus_visual(v, trip, now)
        return v

    def remaining_duration(self, trip):
        segments = trip["route"]["segments"][trip["segment"]:]
        if any(s.get("duration") is None for s in segments):
            return None
        remaining = sum(s["duration"] for s in segments)
        loc = self.state.get("location")
        if loc and valid_fix(loc, self.clock()) and loc["system"] == trip["origin"]["system"] and trip["status"] != "waiting":
            fraction = route_progress(loc, segments[0])
            if fraction is not None:
                remaining -= segments[0]["duration"] * fraction
        return max(0, remaining)


def route_progress(p, segment):
    """Project a real fix onto route geometry; never infer progress from elapsed time."""
    points = [coordinate(s) for s in segment.get("polyline", "").split(";")]
    points = [v for v in points if v]
    if len(points) < 2:
        return None
    scale_x = 111320 * math.cos(math.radians(p["lat"]))
    xy = [((q["lon"] - p["lon"]) * scale_x, (q["lat"] - p["lat"]) * 111320) for q in points]
    total, best, along = 0.0, float("inf"), 0.0
    for a, b in zip(xy, xy[1:]):
        dx, dy = b[0] - a[0], b[1] - a[1]
        length = math.hypot(dx, dy)
        t = max(0, min(1, -(a[0] * dx + a[1] * dy) / length ** 2)) if length else 0
        offset = math.hypot(a[0] + t * dx, a[1] + t * dy)
        if offset < best:
            best, along = offset, total + t * length
        total += length
    return min(1, along / total) if total > 0 and best <= 100 else None


def route_distance(p, segment):
    points = [coordinate(s) for s in segment.get("polyline", "").split(";")]
    points = [v for v in points if v]
    if not points:
        return float("inf")
    # Distance to line segments rather than only to vertices.
    scale_x = 111320 * math.cos(math.radians(p["lat"]))
    projected = [((q["lon"] - p["lon"]) * scale_x, (q["lat"] - p["lat"]) * 111320) for q in points]
    best = min(math.hypot(x, y) for x, y in projected)
    for a, b in zip(projected, projected[1:]):
        dx, dy = b[0] - a[0], b[1] - a[1]
        length = dx * dx + dy * dy
        t = max(0, min(1, -(a[0] * dx + a[1] * dy) / length)) if length else 0
        best = min(best, math.hypot(a[0] + t * dx, a[1] + t * dy))
    return best
