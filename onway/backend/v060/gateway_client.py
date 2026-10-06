"""Only an access token goes to the user-configured HTTPS gateway."""
import json
import os
import ssl
from pathlib import Path
from urllib.parse import urlsplit


class GatewayClient:
    def __init__(self, vault, transport):
        self.vault, self.transport = vault, transport
        config = Path(__file__).resolve().parents[2] / 'gateway-client.json'
        try:
            default = json.loads(config.read_text(encoding='utf-8')).get('url', '')
        except (OSError, ValueError):
            default = ''
        self.url = os.getenv('ONWAY_GATEWAY_URL', default).strip().rstrip('/')
        self.ca = os.getenv('ONWAY_GATEWAY_CA', '')
        bundled_ca = config.parent / 'gateway-ca.pem'
        if not self.ca and bundled_ca.exists():
            self.ca = str(bundled_ca)
        if self.url:
            parsed = urlsplit(self.url)
            if parsed.scheme != 'https' or not parsed.hostname or parsed.username or parsed.password or parsed.query or parsed.fragment or parsed.path:
                raise ValueError('网关地址必须是 HTTPS 域名，不含路径或登录信息')

    @property
    def enabled(self):
        return bool(self.url)

    @property
    def configured(self):
        return self.enabled and bool(self.vault.get('gateway'))

    def call(self, operation, payload):
        from .services import ServiceError
        token = self.vault.get('gateway')
        if not token:
            raise ServiceError('网关访问凭证未配置')
        if operation not in {'amap', 'ai'}:
            raise ServiceError('网关不支持此操作')
        options = {'ssl_context':ssl.create_default_context(cafile=self.ca)} if self.ca else {}
        result = self.transport(self.url + '/v1/' + operation, payload,
                                {'Authorization':'Bearer '+token, 'Content-Type':'application/json'}, **options)
        if not isinstance(result, dict) or result.get('ok') is not True or not isinstance(result.get('data'), dict):
            raise ServiceError('网关返回数据不合法')
        return result['data']


def service_configured(service, name):
    configured = getattr(service, 'configured', None)
    return bool(configured) if configured is not None else bool(service.vault.get(name))
