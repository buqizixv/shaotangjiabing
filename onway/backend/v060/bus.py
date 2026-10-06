"""Beijing bus predictions from the documented Chelaile HTTP gateway."""
from datetime import datetime, timezone, timedelta
from email.utils import parsedate_to_datetime
import json
import math
import os
import re
import time
from urllib import request, error, parse
from .services import ServiceError


class BusError(ServiceError):
    def __init__(self, message, retry_after=30):
        super().__init__(message)
        self.retry_after = retry_after


class BusMatchError(BusError):
    pass


def bus_http(url):
    req = request.Request(url, headers={"User-Agent": "Onway/0.6", "Accept": "application/json"})
    try:
        with request.urlopen(req, timeout=20) as response:
            raw = response.read(2 * 1024 * 1024 + 1)
        if len(raw) > 2 * 1024 * 1024:
            raise BusError("公交接口返回数据过大")
        return json.loads(raw)
    except error.HTTPError as exc:
        retry = 300 if exc.code in {400,401,403,404} else 60 if exc.code == 429 else 30
        if exc.code == 429:
            value = exc.headers.get("Retry-After", "")
            try:
                retry = max(60, float(value))
            except ValueError:
                try:
                    retry = max(60, parsedate_to_datetime(value).timestamp()-time.time())
                except (ValueError, TypeError, OverflowError):
                    pass
        code = exc.code
        exc.close()
        raise BusError(f"公交查询失败（HTTP {code}）", retry) from None
    except (error.URLError, TimeoutError, ValueError, OSError):
        raise BusError("公交查询失败 · 连接超时或返回格式错误") from None


def finite(value):
    return type(value) in (int, float) and math.isfinite(value) and value >= 0


def station_name(value):
    return re.sub(r"[（(](?:公交站|地铁站)[）)]$", "", value or "").removesuffix("公交站")


def line_name(value):
    return re.split(r"[（(]", value or "")[0].removesuffix("路")


class BeijingBus:
    def __init__(self, transport=bus_http, clock=time.time, base_url=None):
        self.transport, self.clock = transport, clock
        self.base = (base_url or os.getenv("BUS_API_BASE_URL", "https://ts-api.tundrey.com")).rstrip("/")
        parts = parse.urlsplit(self.base)
        if parts.scheme != "https" or not parts.hostname or parts.username or parts.password or parts.query or parts.fragment:
            raise ValueError("北京公交 API 地址必须是 HTTPS 地址")
        self.resolved = {}

    def get(self, path, **params):
        data = self.transport(self.base+path+("?"+parse.urlencode(params) if params else ""))
        if not isinstance(data, dict) or "error" in data:
            raise BusError("公交接口返回格式异常")
        return data

    def check(self):
        data = self.get("/v1/health")
        if data.get("status") != "ok":
            raise BusError("公交服务健康检查失败")
        return True

    def resolve(self, segment):
        name, start, end = line_name(segment.get("name")), station_name(segment.get("from")), station_name(segment.get("to"))
        key = (name, start, end)
        cached = self.resolved.get(key)
        if cached and self.clock()-cached[0] < (300 if cached[1] is None else 3600):
            if cached[1] is None:
                raise BusMatchError("暂无到站预测 · 线路或站台方向未能唯一匹配", 300)
            return cached[1]
        search = self.get("/v1/search", city_id="027", keyword=name)
        if not isinstance(search.get("lines"), list) or any(not isinstance(line,dict) for line in search["lines"]):
            raise BusError("公交线路搜索格式异常")
        directions = []
        for line in search["lines"]:
            if not line.get("isSubway") and line_name(line.get("lineNo") or line.get("name")) == name:
                directions.extend(line.get("directions", []))
        candidates = []
        for line_id in dict.fromkeys(d.get("lineId") for d in directions if d.get("lineId")):
            detail = self.get("/v1/lines/detail", city_id="027", line_id=line_id)
            if not isinstance(detail.get("line"),dict) or detail["line"].get("lineId") != line_id:
                raise BusError("公交线路详情不匹配")
            stations = detail.get("stations")
            if not isinstance(stations,list) or any(not isinstance(station,dict) for station in stations):
                raise BusError("公交站台列表格式异常")
            for pos, station in enumerate(stations):
                if station_name(station.get("sn")) != start:
                    continue
                # Match the alighting station after the boarding station, never choose the first namesake platform.
                if not any(station_name(s.get("sn")) == end for s in stations[pos+1:]):
                    continue
                lat, lng, order = station.get("wgsLat"), station.get("wgsLng"), station.get("order")
                if not finite(order) or order >= 999 or type(lat) not in (int,float) or type(lng) not in (int,float) or not (0 < lat < 90 and 0 < lng < 180) or not station.get("sId"):
                    continue
                candidates.append({"city_id":"027", "line_id":line_id, "target_order":str(order),
                    "station_id":station["sId"], "lat":str(lat), "lng":str(lng),
                    "direction":detail["line"].get("endSn", ""), "station":start})
        if len(candidates) != 1:
            self.resolved[key] = (self.clock(), None)
            raise BusMatchError("暂无到站预测 · 线路或站台方向未能唯一匹配", 300)
        self.resolved[key] = (self.clock(), candidates[0])
        return candidates[0]

    def arrivals(self, segment):
        try:
            config = self.resolve(segment)
        except BusMatchError as exc:
            return {"status":"no_prediction", "fetchedAt":self.clock(), "nearest":None, "note":str(exc)}
        query = {k:v for k,v in config.items() if k not in {"direction","station"}}
        data = self.get("/v1/lines/realtime", **query)
        if not isinstance(data.get("line"),dict) or data["line"].get("lineId") != config["line_id"] or str(data.get("targetOrder")) != config["target_order"] or not isinstance(data.get("buses"), list):
            raise BusError("公交返回线路或站序不匹配")
        buses = []
        if data.get("realData") is True:
            for bus in data["buses"]:
                eta = bus.get("eta") if isinstance(bus, dict) else None
                if not isinstance(eta, dict) or not finite(eta.get("travelTime")):
                    continue
                arrival = eta.get("arrivalTime")
                text = datetime.fromtimestamp(arrival/1000, timezone(timedelta(hours=8))).strftime("%H:%M") if finite(arrival) and 0 < arrival < 1e13 else eta.get("displayTime")
                if not isinstance(text, str) or not re.fullmatch(r"\d{2}:\d{2}",text):text = "暂无预估"
                buses.append({"seconds":eta["travelTime"], "arrivalText":text,
                    "distance":bus.get("distanceToWaitStn") if finite(bus.get("distanceToWaitStn")) else None})
        buses.sort(key=lambda b:b["seconds"])
        return {"status":"predicted" if buses else "no_prediction", "fetchedAt":self.clock(),
            "nearest":buses[0] if buses else None, "direction":config["direction"], "station":config["station"],
            "lineId":config["line_id"], "targetOrder":config["target_order"]}
