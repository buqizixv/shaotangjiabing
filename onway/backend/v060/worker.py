"""Shell-owned worker: private command files, asynchronous services, one writer."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import ctypes
import hashlib
import json
import os
from pathlib import Path
import secrets
import threading
import time
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from .engine import Engine, atomic_json, read_json, valid_place, valid_fix, distance, fix_failure, TZ
from datetime import datetime
from .services import Amap, MiniMax, ServiceError
from .vault import Vault
from .gateway_client import service_configured
from .bus import BeijingBus, BusError


class Worker:
    def __init__(self, root, amap=None, minimax=None, bus=None, parent_pid=0, seed_history=False):
        self.engine = Engine(root, seed_history=seed_history)
        self.amap, self.minimax = amap or Amap(), minimax or MiniMax()
        self.bus, self.parent_pid = bus or BeijingBus(), parent_pid
        self.bus_marker, self.bus_due, self.bus_failures = None, 0, 0
        self.bus_visible = False
        self.pool = ThreadPoolExecutor(max_workers=3, thread_name_prefix="onway-service")
        self.jobs = {}
        self.recommend_pending = True
        self.last_fix = 0
        self.last_minute = -1
        self.last_intent_error = 0
        self.last_reverse = 0
        self.reverse_fix = None
        self.last_assist = None
        self.config_url = None
        self.connections = {"amap":"尚未检查", "minimax":"尚未检查", "beijing_bus":"尚未检查", "weather":"尚未检查"}
        self.engine.root.joinpath("commands").mkdir(exist_ok=True)
        trip = self.engine.state.get("trip")
        if trip and trip["status"] == "arrived" and not trip.get("aiSummary"):
            trip["summaryRequested"] = False


    def submit(self, key, fn, metadata=None):
        if key not in self.jobs:
            self.jobs[key] = (self.pool.submit(fn), metadata)

    def process_command(self, cmd):
        e = self.engine
        action, p = cmd.get("action"), cmd.get("payload", {})
        if cmd.get("id") in e.state["receipts"]:
            return
        if action == "search":
            query = str(p.get("text", "")).strip()
            e.state["search"] = []
            e.state["queryStatus"] = "正在查询地点…"
            e.pending_generation += 1
            generation = e.pending_generation
            self.submit("search-" + str(generation), lambda: self.amap.search(query, e.cfg["city"]), generation)
        elif action in {"query_routes", "plan_destination"}:
            if action == "plan_destination":
                e.command({**cmd, "action": "set_destination"})
            origin, dest = e.state["origin"], e.state["destination"]
            if not valid_place(origin) or not valid_place(dest):
                raise ValueError("请先选择有效的起点和目的地")
            e.state["routes"], e.state["selected"] = [], None
            e.state["queryStatus"] = "正在查询路线…"
            e.pending_generation += 1
            generation = e.pending_generation
            self.submit("routes-" + str(generation), lambda: self.amap.routes(origin, dest, e.cfg["city"]), generation)
        elif action == "refresh_bus":
            self.bus_due = min(self.bus_due, e.clock()) if not self.bus_failures else self.bus_due
        elif action == "check_services":
            weather_origin = e.state.get("origin") or next(iter(e.cfg["places"]), None)
            self.connections["weather"] = "正在检查" if service_configured(self.amap, "amap") else "未配置"
            if service_configured(self.amap, "amap"):
                self.submit("connection-weather", lambda: self.amap.weather(weather_origin or {"name":"北京", "lon":116.4074,"lat":39.9042,"system":"gcj02","source":"amap"}))
            self.connections["beijing_bus"]="正在检查"
            self.submit("connection-beijing_bus", self.bus.check)
            for name, service in (("amap",self.amap),("minimax",self.minimax)):
                if not service_configured(service, name):
                    self.connections[name]="未配置"
                    continue
                self.connections[name]="正在检查"
                fn = (lambda: self.amap.search("北京南站")) if name == "amap" else (lambda: self.minimax.analyze("summary", {"trip":{"status":"connection_test","note":"连接测试，没有个人出行数据"}}))
                self.submit("connection-"+name,fn)
        elif action == "preview_intent":
            if e.state.get("trip") or e.state.get("intent"):
                raise ValueError("请先关闭当前行程卡片")
            if not e.cfg["aiEnabled"]:
                raise ValueError("请先开启 AI 功能")
            habit = e.cfg["commute"]
            origin = next((p for p in e.cfg["places"] if p["type"] == "home"), None)
            destination = next((p for p in e.cfg["places"] if p["type"] == "work"), None)
            if not origin or not destination:
                raise ValueError("请先在常用地点设置家和公司")
            candidate = {"id":"preview-"+cmd["id"], "preview":True, "direction":"outbound",
                "origin":origin, "destination":destination,
                "plannedAt":e.clock(), "requiredArrival":habit.get("requiredArrival", ""), "expires":e.clock()+600}
            generation = e.pending_generation
            self.submit("intent-prepare", lambda: self.prepare_intent(candidate, generation))
            e.state["previewStatus"] = "正在生成推荐卡片…"
            e.message("正在准备 AI 出行推荐卡片…")
        elif action == "service_settings":
            if self.config_url:
                webbrowser.open(self.config_url)
                e.message("已打开本机受保护服务配置")
            else:
                e.message("服务配置尚未启动，请使用本机启动脚本")
        elif action == "refresh_location":
            if e.cfg["tracking"]:
                e.state["originMode"] = "device"
                e.state["origin"] = None
                e.state["location"] = None
            e.state["locationStatus"] = "正在更新当前位置…" if e.cfg["tracking"] else "定位已关闭"
            self.last_fix = 0
            self.last_reverse = 0
            self.reverse_fix = None
        elif action == "refresh":
            e.message("已重新检查后台状态")
        elif action in {"notification_settings", "location_settings"}:
            if os.name == "nt":
                os.startfile("ms-settings:notifications" if action == "notification_settings" else "ms-settings:privacy-location")
                e.message("已打开 Windows 系统设置")
            else:
                e.message("当前本机版只支持打开 Windows 系统设置")
        else:
            e.command(cmd)
            if action == "preferences" or action == "toggle" and p.get("field") == "aiEnabled":
                self.recommend_pending = True
            return
        e.state["receipts"] = (e.state["receipts"] + [cmd["id"]])[-100:]
        e.save()

    def tick(self):
        e = self.engine
        for path in sorted((e.root / "commands").glob("cmd-*.json"))[:50]:
            cmd = read_json(path, None)
            if not isinstance(cmd, dict):
                # A Splash fs.write can be observed before it finishes. Retry the next tick.
                if time.time() - path.stat().st_mtime > 10:
                    path.unlink(missing_ok=True)
                continue
            try:
                self.process_command(cmd)
            except (ValueError, KeyError, TypeError, ServiceError) as exc:
                e.message(str(exc) if isinstance(exc, (ValueError, ServiceError)) else "操作数据不完整")
                if cmd.get("action") == "preview_intent":
                    e.state["previewStatus"] = e.state["message"]
                if isinstance(cmd.get("id"), str):
                    e.state["receipts"] = (e.state["receipts"] + [cmd["id"]])[-100:]
                e.save()
            path.unlink(missing_ok=True)
        if self.recommend_pending and self.recommend():
            self.recommend_pending = False
        dirty = False
        for key, (future, meta) in list(self.jobs.items()):
            if not future.done():
                continue
            del self.jobs[key]
            if key.startswith(("routes-", "search-")) and meta != e.pending_generation:
                continue
            if key == "bus-arrivals" and meta != self.bus_marker:
                continue
            try:
                data = future.result()
                if key.startswith("connection-"):
                    self.connections[key.removeprefix("connection-")]="已连接"
                elif key == "bus-arrivals":
                    e.state["busArrival"] = {**data, "marker":list(meta)}
                    self.connections["beijing_bus"] = "已连接"
                    self.bus_failures = 0
                    self.bus_due = e.clock()+30
                elif key.startswith("search-"):
                    e.state["search"] = data
                    e.state["queryStatus"] = "请选择搜索结果" if data else "没有匹配的地点，请调整关键词"
                elif key.startswith("routes-"):
                    e.state.update({"origin": {**e.state["origin"], **data["origin"]},
                                    "destination": {**e.state["destination"], **data["destination"]},
                                    "routes": data["routes"], "queryStatus": "；".join(data["warnings"]) or "路线查询成功"})
                    self.recommend_pending = True
                elif key == "gps-convert":
                    e.observe(data)
                    self.reverse_location(data)
                elif key == "reverse":
                    current = e.state.get("location")
                    if current and e.cfg["tracking"] and current["system"] == data["system"] and distance(current, data) <= 100:
                        e.state["location"] = {**current, "name": data["name"], "address": data["address"], "source": "device"}
                        origin = e.state.get("origin")
                        if e.state.get("originMode", "device") == "device":
                            e.state["origin"] = dict(e.state["location"])
                elif key == "intent":
                    candidate, routes, generation = meta
                    if e.cfg["aiEnabled"] and not e.state.get("trip") and candidate["expires"] > e.clock() and not e.state["declined"].get(candidate["id"]) and generation == e.pending_generation:
                        if data["action"] == "intent":
                            e.state["origin"], e.state["destination"], e.state["routes"] = candidate["origin"], candidate["destination"], routes
                            e.state["intent"] = {**candidate, "reason": data["reason"], "uncertainty": data["uncertainty"]}
                    e.state["aiStatus"] = "MiniMax M3 判断已更新"
                elif key == "intent-prepare":
                    candidate, generation = data["candidate"], data["generation"]
                    decision = data["result"]
                    if e.cfg["aiEnabled"] and not e.state.get("trip") and candidate["expires"] > e.clock() and generation == e.pending_generation and not e.state["declined"].get(candidate["id"]) and e.state["snoozed"].get(candidate["id"], 0) <= e.clock():
                        if decision["action"] == "intent":
                            if not candidate.get("preview") and (not e.intent_candidate() or e.intent_candidate()["id"] != candidate["id"]):
                                continue
                            if not candidate.get("preview") and e.clock() < candidate["suggestedAt"] - e.cfg["commute"]["lead"] * 60:
                                continue
                            if candidate.get("preview"):
                                candidate["previous"] = {key:e.state[key] for key in ("origin", "destination", "routes", "selected")}
                            e.state["origin"], e.state["destination"], e.state["routes"] = candidate["origin"], candidate["destination"], data["routes"]
                            e.state["selected"] = None
                            e.state["intent"] = {**candidate, "reason": decision["reason"], "uncertainty": decision["uncertainty"]}
                            e.state["previewStatus"] = ""
                        elif candidate.get("preview"):
                            e.state["previewStatus"] = "AI 暂未生成推荐，请稍后重试"
                    e.state["aiStatus"] = "MiniMax M3 判断已更新"
                elif key == "recommend":
                    if meta == e.pending_generation and e.cfg["aiEnabled"] and data["action"] == "recommend":
                        for route in e.state["routes"]:
                            route["recommended"] = route["id"] == data["routeId"]
                            route["reason"] = data["reason"] if route["recommended"] else ""
                        e.state["recommendationStatus"] = ""
                    elif meta == e.pending_generation and e.cfg["aiEnabled"]:
                        e.state["recommendationStatus"] = "AI暂未给出推荐"

                    e.state["aiStatus"] = "路线建议已更新"
                elif key == "summary":
                    trip = e.state.get("trip")
                    if e.cfg["aiEnabled"] and trip and (trip["id"], trip["arrivedAt"]) == meta and trip["status"] == "arrived":
                        trip["aiSummary"] = data["summary"] if data["action"] == "summary" else ""
                        trip["summaryRequested"] = True
                        trip["aiSummaryStatus"] = ""
                elif key == "memory":
                    if e.cfg["aiEnabled"] and meta == self.memory_context()[0]:
                        e.state["aiMemory"] = {"text":data["summary"], "status":"已按最新完成行程更新", "historyKey":meta}
                elif key == "assist":
                    trip = e.state.get("trip")
                    if trip and e.cfg["aiEnabled"] and (trip["id"], trip["segment"], trip["status"]) == meta[:3] and e.clock() - meta[3] <= 60:
                        seg = trip["route"]["segments"][trip["segment"]]
                        allowed = {"waiting", "riding"} if seg["mode"] == "transit" else {"transfer", "lastwalk"}
                        if data["action"] == "assist" and data["stage"] in allowed:
                            e.message("AI 阶段建议（需你确认）· " + data["reason"] + "；" + data["uncertainty"])
                dirty = True
            except (ServiceError, ValueError, OSError, KeyError, TypeError) as exc:
                # ServiceError contains only our fixed messages and numeric provider codes.
                # Other exceptions may contain complete URLs or request data.
                detail = str(exc) if isinstance(exc, ServiceError) else "服务返回数据不完整，请检查服务配置并重试"
                if key.startswith("connection-"):
                    self.connections[key.removeprefix("connection-")]="连接失败"
                elif key == "bus-arrivals":
                    prior = e.state.get("busArrival") or {}
                    self.bus_failures += 1
                    e.state["busArrival"] = {**prior, "marker":list(meta), "status":"error", "error":detail}
                    self.connections["beijing_bus"] = "连接失败"
                    self.bus_due = e.clock()+max(getattr(exc,"retry_after",30), min(120,30*2**min(self.bus_failures-1,2)))
                elif key.startswith(("routes-", "search-")):
                    e.state["queryStatus"] = "查询失败 · " + detail
                elif key == "gps-convert":
                    e.state["locationStatus"] = "定位失败 · " + detail + "，保留最近阶段"
                elif key == "reverse":
                    e.state["locationStatus"] = "真实定位可用 · " + detail + "，保留设备位置"
                elif key == "summary":
                    trip = e.state.get("trip")
                    if trip and (trip["id"], trip.get("arrivedAt")) == meta:
                        trip["summaryRequested"] = False
                        trip["aiSummaryStatus"] = "生成失败，将重试 · " + detail
                elif key == "intent-prepare":
                    e.state["previewStatus"] = "推荐生成失败 · " + detail
                    e.state["aiStatus"] = e.state["previewStatus"]
                elif key == "memory":
                    if meta == self.memory_context()[0]:
                        e.state["aiMemory"]["status"] = "分析失败，将重试 · " + detail
                elif key == "recommend":
                    if meta == e.pending_generation:
                        self.recommend_pending = True
                        e.state["recommendationStatus"] = "AI推荐失败，将重试 · " + detail
                else:
                    e.state["aiStatus"] = "查询失败 · 服务未配置、连接失败或结果核验未通过"
                dirty = True
        if e.cfg["tracking"]:
            gps = read_json(e.root / "device-location.json", {})
            if gps.get("timestamp", 0) > self.last_fix:
                trip = e.state.get("trip")
                needs_conversion = (trip and trip["origin"]["system"] == "gcj02") or (not trip and service_configured(self.amap, "amap"))
                if needs_conversion and gps.get("system") == "wgs84" and valid_fix(gps, e.clock()):
                    if "gps-convert" not in self.jobs:
                        self.last_fix = gps["timestamp"]
                        self.submit("gps-convert", lambda gps=gps: self.amap.convert(gps))
                else:
                    self.last_fix = gps["timestamp"]
                    e.observe(gps)
                    self.reverse_location(gps)
                dirty = True
            elif e.state.get("location") and not valid_fix(e.state["location"], e.clock()):
                if not e.state["locationStatus"].startswith("定位失败"):
                    e.state["locationStatus"] = "定位失败 · 位置已过期，保留最近阶段"
                    e.outside, e.recent = [], []
                    trip = e.state.get("trip")
                    if trip and trip["segments"]:
                        trip["segments"][-1]["uncertain"] = True
                    dirty = True
            elif not e.state.get("location"):
                note = {0: "当前设备定位不可用", 1: "正在获取真实定位", 2: fix_failure(gps, e.clock()), 3: "系统定位权限未开启", 4: "系统暂无定位数据", 5: "系统定位服务异常"}.get(gps.get("status"), "等待真实定位")
                if e.state["locationStatus"] != note:
                    e.state["locationStatus"] = note
                    dirty = True
        trip = e.state.get("trip")
        intent = e.state.get("intent")
        if intent and intent["expires"] <= e.clock():
            e.state["intent"] = None
            e.pending_generation += 1
            dirty = True
        if trip and trip["status"] in {"waiting", "riding", "transfer", "lastwalk"} and e.cfg["aiEnabled"] and "assist" not in self.jobs:
            loc = e.state.get("location")
            marker = (trip["id"], trip["segment"], trip["status"])
            if marker != self.last_assist and loc and valid_fix(loc, e.clock()) and len(e.recent) >= 3:
                context = {"trip": {"stage": trip["status"], "segment": trip["segment"],
                    "segmentMode": trip["route"]["segments"][trip["segment"]]["mode"],
                    "basis": "连续有效设备位置与实际路线分段", "accuracy": loc["accuracy"],
                    "sampleCount": len(e.recent), "manuallyCorrected": trip["corrected"]}}
                if self.ai_allowed("assist", context):
                    self.last_assist = marker
                    self.submit("assist", lambda context=context: self.minimax.analyze("assist", context), (*marker, e.clock()))
        if trip and trip["status"] == "arrived" and e.cfg["aiEnabled"] and not trip.get("summaryRequested") and "summary" not in self.jobs:
            context = {"trip": {"status": "arrived", "duration": trip["arrivedAt"] - trip["startedAt"],
                "initialEstimateSeconds": trip.get("initialEstimate"), "durationMinutes":round((trip["arrivedAt"]-trip["startedAt"])/60,1),
                "routeMode":trip["route"]["mode"], "stageNames":{"walking":"步行", "waiting":"候车", "riding":"乘车", "transfer":"换乘", "lastwalk":"末端步行"},
                "corrected":trip.get("corrected"), "units":"duration 和 segments 的时间差单位是秒，durationMinutes 单位是分钟", "segments": trip["segments"]}}
            if self.ai_allowed("summary", context):
                trip["summaryRequested"] = True
                trip["aiSummaryStatus"] = "正在生成…"
                dirty = True
                self.submit("summary", lambda context=context: self.minimax.analyze("summary", context), (trip["id"], trip["arrivedAt"]))
        if e.cfg["aiEnabled"] and "memory" not in self.jobs:
            memory_key, memory_context = self.memory_context()
            if memory_context["history"] and memory_key != e.state["aiMemory"]["historyKey"] and self.ai_allowed("memory", memory_context):
                e.state["aiMemory"]["status"] = "正在分析最新完成行程…"
                self.submit("memory", lambda context=memory_context: self.minimax.analyze("memory", context), memory_key)
                dirty = True
        if e.cfg["aiEnabled"] and not trip and "intent-prepare" not in self.jobs and e.clock() - self.last_intent_error > 300:
            candidate = e.intent_candidate()
            if candidate and not e.state.get("intent"):
                self.last_intent_error = e.clock()
                if self.ai_allowed("intent", {"id": candidate["id"]}):
                    # Route lookup is also asynchronous; its result carries the original context generation.
                    generation = e.pending_generation
                    def job(candidate=candidate, generation=generation):
                        return self.prepare_intent(candidate, generation)
                    self.submit("intent-prepare", job)
        if self.poll_bus():
            dirty = True
        minute = int(e.clock() // 60)
        if minute != self.last_minute:
            self.last_minute = minute
            dirty = True
        if dirty:
            e.save()
        try:
            services = {name: "已配置" if service_configured(service, name) else "未配置" for name, service in (("amap", self.amap), ("minimax", self.minimax))}
            if service_configured(self.amap, "amap"):
                services["amap"] += " · " + self.amap.provider
        except (OSError, RuntimeError, ValueError):
            services = {"amap": "凭据读取失败", "minimax": "凭据读取失败"}
        atomic_json(e.root / "worker-health.json", {"schema": 6, "epoch": time.time(), "pid": os.getpid(), "services": services,
            "connections": self.connections, "model": self.minimax.model, "tracking": e.cfg["tracking"]})

    def poll_bus(self):
        e = self.engine
        target = e.bus_target()
        marker = (target["tripId"],target["segment"]) if target else None
        changed = marker != self.bus_marker
        if changed:
            self.bus_marker, self.bus_due, self.bus_failures = marker, 0, 0
            e.state["busArrival"] = None
        heartbeat = read_json(e.root/"bus-view.json", {})
        visible = 0 <= e.clock()-heartbeat.get("epoch",0) <= 6
        if self.parent_pid and os.name == "nt":
            ctypes.windll.user32.GetForegroundWindow.restype = ctypes.c_void_p
            ctypes.windll.user32.GetWindowThreadProcessId.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_ulong)]
            pid = ctypes.c_ulong()
            ctypes.windll.user32.GetWindowThreadProcessId(ctypes.windll.user32.GetForegroundWindow(), ctypes.byref(pid))
            visible = visible and pid.value == self.parent_pid
        if visible and not self.bus_visible and not self.bus_failures:
            self.bus_due = 0
        self.bus_visible = visible
        if target and visible:
            if e.clock() >= self.bus_due and "bus-arrivals" not in self.jobs:
                self.submit("bus-arrivals", lambda segment=dict(target["routeSegment"]): self.bus.arrivals(segment), marker)
            # Recompute elapsed prediction time while the card is visible.
            return True
        return changed

    def ai_allowed(self, task, context):
        e = self.engine
        now = e.clock()
        fingerprint = hashlib.sha256(json.dumps(context, sort_keys=True).encode()).hexdigest()
        if now - e.state["lastAIByTask"].get(task, 0) < 60 or now - e.state["aiHashes"].get(task + fingerprint, 0) < (600 if task == "assist" else 60):
            return False
        e.state["lastAI"] = now
        e.state["lastAIByTask"][task] = now
        e.state["aiHashes"][task + fingerprint] = now
        e.state["aiHashes"] = {k: v for k, v in e.state["aiHashes"].items() if now - v < 3600}
        e.save()  # Limit survives process restarts, including failed calls.
        return True

    def reverse_location(self, fix):
        e = self.engine
        moved = not self.reverse_fix or self.reverse_fix["system"] != fix.get("system") or distance(self.reverse_fix, fix) > 100
        elapsed = e.clock() - self.last_reverse
        if valid_fix(fix, e.clock()) and service_configured(self.amap, "amap") and "reverse" not in self.jobs and (elapsed >= 300 or moved and elapsed >= 10):
            self.last_reverse = e.clock()
            self.reverse_fix = dict(fix)
            self.submit("reverse", lambda fix=dict(fix): self.amap.reverse(fix), fix["timestamp"])

    def prepare_intent(self, candidate, generation):
        e = self.engine
        routes = self.amap.routes(candidate["origin"], candidate["destination"], e.cfg["city"])["routes"]
        if not routes:
            raise ServiceError("暂无可用通勤路线")
        try:
            weather = self.amap.weather(candidate["origin"])
        except ServiceError:
            weather = {"status":"unavailable"}
        preferences = dict(e.cfg["preferences"])
        context = {"candidates":[{"id":candidate["id"], "direction":candidate["direction"], "preview":candidate.get("preview",False),
                    "basis":"用户明确保存的通勤地点、日期与时间", "requiredArrival":candidate.get("requiredArrival", "")}],
                   "routes":[{k:r.get(k) for k in ("id", "mode", "duration", "walkingDistance", "transfers")} for r in routes],
                   "preferences":preferences, "weather":weather, "memory":e.state["aiMemory"]["text"],
                   "limitations":"路线时长是预估，没有实时道路拥堵数据；这是出发邀请，不能替用户确认出发。"}
        try:
            decision = self.minimax.analyze("intent", context)
            if candidate.get("preview") and decision["action"] != "intent":
                raise ServiceError("AI 暂未生成推荐")
        except ServiceError:
            if not candidate.get("preview"):
                raise
            # A user-requested preview remains useful with verified routes/weather.
            # It must not pretend that a deterministic fallback is an AI choice.
            decision = {"action":"intent", "routeId":None,
                        "reason":"先看看路线和天气，再决定什么时候出门吧。AI 建议暂时不可用。", "uncertainty":"AI 建议未获取，展示依据出行偏好排序的路线预览。"}
        metric = {"fast":"duration", "lesswalk":"walkingDistance", "lesstransfer":"transfers"}[preferences["route"]]
        def score(route):
            return (preferences["mode"] != "all" and route["mode"] != preferences["mode"], route.get(metric) if route.get(metric) is not None else float("inf"))
        selected = next((r for r in routes if r["id"] == decision.get("routeId")), min(routes,key=score))
        for route in routes:
            route["recommended"] = route["id"] == decision.get("routeId")
        candidate = dict(candidate)
        suggested = candidate["plannedAt"]
        if candidate.get("requiredArrival") and not candidate.get("preview"):
            local = datetime.fromtimestamp(e.clock(), TZ)
            deadline = datetime.combine(local.date(), datetime.strptime(candidate["requiredArrival"], "%H:%M").time(), TZ).timestamp()
            if selected.get("duration") is not None:
                rainy = weather.get("status") == "available" and any(x in weather.get("condition", "") for x in ("雨", "雪"))
                buffer = 10 if rainy else 5
                suggested = deadline-selected["duration"]-buffer*60
                candidate["timingNote"] = f"按路线预估，预留 {buffer} 分钟余量。"
        candidate.update(weather=weather, routeId=selected["id"], suggestedAt=suggested)
        return {"result":decision, "routes":routes, "candidate":candidate, "generation":generation}

    def memory_context(self):
        e = self.engine
        context = {"memoryFormat":2, "preferences":e.cfg["preferences"], "history":[{k:h.get(k) for k in
            ("id", "originName", "destinationName", "date", "duration", "initialEstimate", "mode", "walkingDistance", "transfers", "corrected", "completeSegments")}
            for h in e.hist if h["status"] == "arrived"][-30:],
                   "units":"duration 和 initialEstimate 是秒，walkingDistance 是米；人工纠正和异常短行程仅供参考"}
        if any(h.get("source") == "imported" for h in e.hist):
            context["recordSources"] = "包含用户导入的初始记录，日期和耗时未经过真实定位核验；不完整分段不能作为可靠速度证据。"
        return hashlib.sha256(json.dumps(context,sort_keys=True).encode()).hexdigest(), context

    def recommend(self):
        e = self.engine
        if not e.cfg["aiEnabled"] or not e.state["routes"] or "recommend" in self.jobs:
            return False
        context = {"routes": [{k: r.get(k) for k in ("id", "mode", "duration", "walkingDistance", "transfers")} for r in e.state["routes"]],
                   "preferences": e.cfg["preferences"], "history": e.state["learning"], "memory":e.state["aiMemory"]["text"] if e.state["aiMemory"]["historyKey"] else ""}
        if self.ai_allowed("recommend", context):
            e.state["recommendationStatus"] = "AI正在分析出行方案…"
            e.save()
            self.submit("recommend", lambda: self.minimax.analyze("recommend", context), e.pending_generation)
            return True
        return False


CONFIG_HTML = '''<!doctype html><html lang="zh-CN"><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<title>在途 · 服务配置</title><style>body{margin:48px auto;max-width:560px;padding:24px;font:17px system-ui;color:#1d1d1f;background:#f5f5f7}h1{font-size:32px}section{background:white;padding:24px;border-radius:18px;margin:20px 0}input{box-sizing:border-box;width:100%;padding:12px;border:1px solid #d2d2d7;border-radius:10px;font:inherit}button{margin:12px 8px 0 0;padding:10px 16px;border:0;border-radius:24px;color:white;background:#0066cc;font:inherit}small{display:block;color:#666;line-height:1.6}#message{white-space:pre-wrap}</style>
<h1>服务配置</h1><p>凭据由本机后台管理，使用当前 Windows 用户加密保存。</p>
<small>MiniMax 接收出行习惯、路线候选和必要历史摘要，不发送完整位置轨迹。地图查询按功能发送关键词或起终点坐标；使用可可地图 Key 时这些数据发送至你指定的 map.culture09.xyz 网关。应用只读取服务状态。</small>
<section><h2>MiniMax M3</h2><p id="minimax-status">读取状态…</p><input id="minimax" type="password" autocomplete="off" placeholder="MiniMax 开放平台 API Key"><button onclick="save('minimax')">保存</button><button onclick="test('minimax')">测试连接</button><button onclick="removeKey('minimax')">移除</button><small>国内接口 · MiniMax-M3</small></section>
<section><h2>地图服务 · 高德 / 可可地图</h2><p id="amap-status">读取状态…</p><input id="amap" type="password" autocomplete="off" placeholder="高德 Web 服务 Key 或可可地图 Key"><button onclick="save('amap')">保存</button><button onclick="test('amap')">测试连接</button><button onclick="removeKey('amap')">移除</button><small>可可地图使用指定网关的 Bearer 认证；其他高德 Key 使用官方 Web 服务。暂无 Key 时可以继续使用设置与历史。</small></section><p id="message"></p>
<script>const token=location.hash.slice(1);history.replaceState(null,'',location.pathname);
async function api(path,data){const r=await fetch(path,{method:data?'POST':'GET',headers:{'Authorization':'Bearer '+token,'Content-Type':'application/json'},body:data?JSON.stringify(data):undefined});const value=await r.json();if(!r.ok)throw Error(value.message);return value;}
async function status(){try{const s=await api('/status');for(const n of ['minimax','amap'])document.getElementById(n+'-status').textContent=s[n]?'已配置':'未配置';}catch(e){document.getElementById('message').textContent=e.message;}}
async function save(service){try{const input=document.getElementById(service);await api('/credentials',{service,key:input.value});input.value='';document.getElementById('message').textContent='已加密保存';await status();}catch(e){document.getElementById('message').textContent=e.message;}}
async function removeKey(service){if(confirm('移除该服务凭据？')){try{await api('/credentials',{service,key:''});await status();}catch(e){document.getElementById('message').textContent=e.message;}}}
async function test(service){document.getElementById('message').textContent='正在测试…';try{const s=await api('/test',{service});document.getElementById('message').textContent=s.message;}catch(e){document.getElementById('message').textContent=e.message;}}status();</script></html>'''


def configure_server(vault):
    token = secrets.token_urlsafe(32)
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass
        def respond(self, code, value, mime="application/json"):
            body = value.encode() if isinstance(value, str) else json.dumps(value, ensure_ascii=False).encode()
            self.send_response(code)
            self.send_header("Content-Type", mime + "; charset=utf-8")
            self.send_header("Cache-Control", "no-store")
            self.send_header("X-Content-Type-Options", "nosniff")
            self.send_header("Content-Security-Policy", "default-src 'self'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        def authorized(self):
            host = self.headers.get("Host", "")
            expected = f"127.0.0.1:{self.server.server_port}"
            origin = self.headers.get("Origin")
            return host == expected and origin in (None, "http://" + expected) and secrets.compare_digest(self.headers.get("Authorization", ""), "Bearer " + token)
        def do_GET(self):
            if self.path == "/":
                self.respond(200, CONFIG_HTML, "text/html")
            elif self.path == "/status" and self.authorized():
                self.respond(200, {n: service_configured(service,n) for n,service in (("minimax",MiniMax(vault)),("amap",Amap(vault)))})
            else:
                self.respond(403, {"message": "请从在途服务配置入口打开"})
        def do_POST(self):
            if not self.authorized():
                self.respond(403, {"message": "配置访问未授权"})
                return
            try:
                size = int(self.headers.get("Content-Length", "0"))
                if not 1 <= size <= 8192:
                    raise ValueError("请求大小不合法")
                p = json.loads(self.rfile.read(size))
                name = p.get("service")
                if name not in {"minimax", "amap"}:
                    raise ValueError("未知服务")
                if self.path == "/credentials":
                    if not isinstance(p.get("key"), str):
                        raise ValueError("凭据格式不合法")
                    vault.set(name, p["key"])
                    self.respond(200, {"message": "已保存"})
                elif self.path == "/test":
                    if name == "amap":
                        Amap(vault).search("北京南站")
                    else:
                        MiniMax(vault).analyze("summary", {"trip": {"status": "connection_test", "note": "连接测试，没有个人出行数据"}})
                    self.respond(200, {"message": "连接与结构化返回验证成功"})
                else:
                    self.respond(404, {"message": "接口不存在"})
            except (ValueError, OSError, RuntimeError, ServiceError) as exc:
                self.respond(422, {"message": str(exc) if isinstance(exc, (ServiceError, ValueError)) else "凭据配置失败"})
    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, f"http://127.0.0.1:{server.server_port}/#{token}"


def parent_alive(pid):
    if not pid:
        return True
    if os.name == "nt":
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.OpenProcess.restype = ctypes.c_void_p
        kernel.WaitForSingleObject.argtypes = (ctypes.c_void_p, ctypes.c_ulong)
        kernel.CloseHandle.argtypes = (ctypes.c_void_p,)
        handle = kernel.OpenProcess(0x100000, False, pid)
        if not handle:
            return False
        try:
            return kernel.WaitForSingleObject(handle, 0) == 258
        finally:
            kernel.CloseHandle(handle)
    try:
        os.kill(pid, 0)
        return True
    except OSError:
        return False


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--data-root", type=Path, required=True)
    parser.add_argument("--parent-pid", type=int, default=0)
    args = parser.parse_args()
    # A file lock is held for the entire worker lifetime; stale lock files are harmless.
    root = args.data_root / "onway"
    root.mkdir(parents=True, exist_ok=True)
    lock = open(root / "worker.lock", "a+b")
    if os.name == "nt":
        import msvcrt
        if lock.tell() == 0:
            lock.write(b"0")
            lock.flush()
        lock.seek(0)
        try:
            msvcrt.locking(lock.fileno(), msvcrt.LK_NBLCK, 1)
        except OSError:
            return
    worker = Worker(root, parent_pid=args.parent_pid, seed_history=True)
    server, worker.config_url = configure_server(Vault())
    try:
        while parent_alive(args.parent_pid):
            try:
                worker.tick()
            except Exception:
                # Persistent visible error; no traceback can leak personal data.
                worker.engine.message("后台操作失败，最近已保存的行程保留，请重试")
                worker.engine.save()
            time.sleep(1)
    finally:
        server.shutdown()
        worker.pool.shutdown(wait=False, cancel_futures=True)
        lock.close()


if __name__ == "__main__":
    main()
