"""Run as root on Ubuntu. Secrets arrive on stdin, never in command arguments."""
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import sys

def run(*args):
    subprocess.run(args,check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)

def main():
    settings=json.load(sys.stdin)
    source=Path('/home/ubuntu/onway-gateway-upload')
    target=Path('/opt/onway-gateway')
    config=Path('/etc/onway-gateway')
    config.mkdir(mode=0o700,parents=True,exist_ok=True)
    os.chmod(config,0o700)
    if not settings.get('minimax') or not settings.get('amap'):
        raise RuntimeError('服务器密钥不能为空')
    for value in settings.values():
        if not isinstance(value,str) or any(c in value for c in '\r\n\x00'):
            raise RuntimeError('配置内容不合法')
    run('apt-get','update')
    run('apt-get','install','-y','python3','openssl','caddy')
    if subprocess.run(['id','onway-gateway'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode:
        run('useradd','--system','--home','/var/lib/onway-gateway','--shell','/usr/sbin/nologin','onway-gateway')
    target.mkdir(parents=True,exist_ok=True)
    for name in ('gateway','backend'):
        shutil.copytree(source/name,target/name,dirs_exist_ok=True)
    # Reuse the certificate on redeploy so existing clients remain trusted.
    if not (config/'server.crt').exists():
        run('openssl','req','-x509','-newkey','rsa:3072','-sha256','-days','365','-nodes',
            '-keyout',str(config/'server.key'),'-out',str(config/'server.crt'),
            '-subj','/CN=120.53.104.174','-addext','subjectAltName=IP:120.53.104.174')
    run('chown','root:caddy',str(config))
    os.chmod(config,0o750)
    for name in ('server.key','server.crt'):
        run('chown','root:caddy',str(config/name))
        os.chmod(config/name,0o640)
    text='\n'.join([
        'MINIMAX_API_KEY='+settings['minimax'], 'AMAP_API_KEY='+settings['amap'],
        'MINIMAX_MODEL=MiniMax-M3', 'ONWAY_TOKEN_HASHES='+settings['tokenHash'],
        'ONWAY_QUOTA_DB=/var/lib/onway-gateway/usage.sqlite3',
        'AI_DAILY_LIMIT=100','AI_GLOBAL_DAILY_LIMIT=1000','MAP_DAILY_LIMIT=2000','MAP_GLOBAL_DAILY_LIMIT=10000'])+'\n'
    # EnvironmentFile is root-readable only; systemd reads it before changing user.
    (config/'secrets.env').write_text(text,encoding='utf-8')
    os.chmod(config/'secrets.env',0o600)
    shutil.copy2(target/'gateway/onway-gateway.service','/etc/systemd/system/onway-gateway.service')
    caddy=Path('/etc/caddy/Caddyfile')
    original=caddy.read_text() if caddy.exists() else ''
    prefix=re.match(r'\s*(?:#[^\n]*\n\s*)*',original).end()
    if original[prefix:].startswith('{'):
        if not re.search(r'(?m)^\s*default_sni\s',original):
            original=original[:prefix+1]+'\n    default_sni 120.53.104.174\n'+original[prefix+1:]
    else:
        original='{\n    default_sni 120.53.104.174\n}\n'+original
    marker='import /etc/caddy/onway-gateway.caddy'
    if marker not in original:
        original+='\n'+marker+'\n'
    caddy.write_text(original)
    shutil.copy2(target/'gateway/Caddyfile.ip','/etc/caddy/onway-gateway.caddy')
    run('caddy','validate','--config',str(caddy))
    run('systemctl','daemon-reload')
    run('systemctl','enable','--now','onway-gateway')
    run('systemctl','restart','onway-gateway')
    run('systemctl','enable','--now','caddy')
    run('systemctl','reload','caddy')
    # Add rules only when UFW exists; never alter the SSH rule or enable/reset UFW.
    if shutil.which('ufw'):
        run('ufw','allow','80/tcp')
        run('ufw','allow','443/tcp')
    run('systemctl','is-active','--quiet','onway-gateway')
    print('DEPLOYED')
    print((config/'server.crt').read_text(),end='')

if __name__=='__main__':
    try:main()
    except Exception:
        print('部署未完成，请在服务器检查服务状态、Caddy 配置或网络；未输出凭据。',file=sys.stderr)
        sys.exit(1)
