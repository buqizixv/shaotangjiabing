"""Per-user Windows DPAPI credentials, stored outside source and app data."""
import ctypes
import getpass
import json
import os
from pathlib import Path
from ctypes import wintypes


class Blob(ctypes.Structure):
    _fields_ = [("size", wintypes.DWORD), ("data", ctypes.POINTER(ctypes.c_ubyte))]


def _protect(raw, decrypt=False):
    if os.name != "nt":
        raise RuntimeError("此交付使用 Windows 受保护凭据存储")
    buf = ctypes.create_string_buffer(raw)
    source = Blob(len(raw), ctypes.cast(buf, ctypes.POINTER(ctypes.c_ubyte)))
    result = Blob()
    crypt = ctypes.windll.crypt32
    fn = crypt.CryptUnprotectData if decrypt else crypt.CryptProtectData
    # CRYPTPROTECT_UI_FORBIDDEN: no unattended OS dialog.
    if not fn(ctypes.byref(source), None, None, None, None, 1, ctypes.byref(result)):
        raise RuntimeError("Windows 凭据存储失败")
    try:
        return ctypes.string_at(result.data, result.size)
    finally:
        ctypes.windll.kernel32.LocalFree(result.data)


class Vault:
    def __init__(self, path=None):
        self.path = Path(path) if path else Path(os.environ["LOCALAPPDATA"]) / "Onway" / "credentials.dpapi"

    def read(self):
        if not self.path.exists():
            return {}
        return json.loads(_protect(self.path.read_bytes(), True))

    def get(self, service):
        env = {"minimax": "MINIMAX_API_KEY", "amap": "AMAP_API_KEY", "gateway":"ONWAY_GATEWAY_TOKEN"}[service]
        return os.getenv(env) or self.read().get(service, "")

    def set(self, service, key):
        if service not in {"minimax", "amap", "gateway"}:
            raise ValueError("未知服务")
        values = self.read()
        if key.strip():
            values[service] = key.strip()
        else:
            values.pop(service, None)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        tmp = self.path.with_suffix(".tmp")
        tmp.write_bytes(_protect(json.dumps(values).encode()))
        os.replace(tmp, self.path)


if __name__ == "__main__":
    import sys
    service = sys.argv[1]
    Vault().set(service, getpass.getpass("API Key（输入不显示）: "))
    print("凭据已加密保存；未输出密钥。")
