"""Local setup. Only the URL is distributable; never write a token into source."""
import getpass
import json
from pathlib import Path
from urllib.parse import urlsplit
from .vault import Vault

def main():
    url=input('网关 HTTPS 地址: ').strip().rstrip('/')
    parsed=urlsplit(url)
    if parsed.scheme!='https' or not parsed.hostname or parsed.username or parsed.password or parsed.path or parsed.query or parsed.fragment:
        raise SystemExit('请输入不含路径和登录信息的 HTTPS 地址')
    token=getpass.getpass('网关访问凭证（输入不显示）: ').strip()
    if not 32<=len(token)<=256:
        raise SystemExit('访问凭证长度必须为 32 至 256 个字符')
    Vault().set('gateway',token)
    path=Path(__file__).resolve().parents[2]/'gateway-client.json'
    path.write_text(json.dumps({'url':url},ensure_ascii=False,indent=2),encoding='utf-8')
    print('网关地址已保存，访问凭证已加密保存。重启应用生效。')

if __name__=='__main__':main()
