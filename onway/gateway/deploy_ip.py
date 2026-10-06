"""Interactive password deployment. No password, upstream key or token is printed."""
import base64
import argparse
import getpass
import hashlib
import io
import json
from pathlib import Path
import secrets
import shlex
import socket
import ssl
import sys
from urllib.request import urlopen

def main():
    import paramiko
    parser=argparse.ArgumentParser()
    parser.add_argument('--key')
    parser.add_argument('--fingerprint')
    parser.add_argument('--use-local-keys',action='store_true')
    args=parser.parse_args()
    app=Path(__file__).resolve().parents[1]
    status=app/'_debug/gateway-deploy-status.json'
    status.parent.mkdir(exist_ok=True)
    def progress(stage):
        status.write_text(json.dumps({'stage':stage}),encoding='utf-8')
    progress('connecting')
    transport=paramiko.Transport(socket.create_connection(('120.53.104.174',22),timeout=10))
    transport.start_client(timeout=15)
    key=transport.get_remote_server_key()
    fingerprint='SHA256:'+base64.b64encode(hashlib.sha256(key.asbytes()).digest()).decode().rstrip('=')
    pin=app/'gateway-host-key.json'
    known=args.fingerprint or (json.loads(pin.read_text())['fingerprint'] if pin.exists() else None)
    if known and known!=fingerprint:
        transport.close();raise RuntimeError('服务器主机密钥已变化，停止连接')
    if not known:
        print('服务器 SSH 指纹：'+fingerprint)
        print('首次信任：请与云控制台核对；没有独立核验来源时，这仍存在首次连接风险。')
        if input('确认首次信任此服务器？输入 yes: ').strip()!='yes':
            transport.close();return
    password=None
    if args.key:
        progress('authenticating_key')
        transport.auth_publickey('ubuntu',paramiko.Ed25519Key.from_private_key_file(args.key))
    else:
        progress('awaiting_password')
        password=getpass.getpass('ubuntu SSH / sudo 密码（不显示）: ')
        transport.auth_password('ubuntu',password)
    pin.write_text(json.dumps({'fingerprint':fingerprint}),encoding='utf-8')
    sys.path.insert(0,str(app))
    from backend.v060.vault import Vault
    vault=Vault()
    minimax=vault.get('minimax')
    amap=vault.get('amap')
    if args.use_local_keys and (not minimax or not amap):
        raise RuntimeError('本机 API 密钥缺失，未部署')
    if minimax and amap and not args.use_local_keys:
        if input('使用本机已有 MiniMax 和地图密钥部署到此服务器？输入 yes: ').strip()!='yes':
            minimax=amap=''
    minimax=minimax or getpass.getpass('服务器 MiniMax 密钥（不显示）: ')
    amap=amap or getpass.getpass('服务器高德 / 可可地图密钥（不显示）: ')
    if not minimax or not amap:
        transport.close();raise RuntimeError('密钥不能为空')
    token=secrets.token_urlsafe(32)
    progress('uploading')
    sftp=paramiko.SFTPClient.from_transport(transport)
    remote='/home/ubuntu/onway-gateway-upload'
    def mkdir(path):
        try:sftp.stat(path)
        except OSError:sftp.mkdir(path)
    mkdir(remote)
    for name in ('backend','gateway'):
        mkdir(remote+'/'+name)
        for path in (app/name).rglob('*'):
            if '__pycache__' in path.parts:continue
            if path.is_file() and path.suffix not in {'.py','.json','.md','.service','.example','.ip'}:continue
            destination=remote+'/'+path.relative_to(app).as_posix()
            if path.is_dir():mkdir(destination)
            elif path.is_file():sftp.put(str(path),destination)
    code=(app/'gateway/bootstrap_ip.py').read_text(encoding='utf-8')
    channel=transport.open_session()
    progress('installing')
    sudo='sudo -n' if args.key else 'sudo -k -S -p '+shlex.quote('')
    channel.exec_command(sudo+' python3 -c '+shlex.quote(code))
    payload={'minimax':minimax,'amap':amap,'tokenHash':hashlib.sha256(token.encode()).hexdigest()}
    channel.sendall(((password+'\n' if password is not None else '')+json.dumps(payload)+'\n').encode())
    channel.shutdown_write()
    stdout=channel.makefile('rb').read().decode('utf-8',errors='replace')
    channel.makefile_stderr('rb').read() # Never echo remote exceptions or credentials.
    if channel.recv_exit_status()!=0 or 'DEPLOYED' not in stdout:
        transport.close();raise RuntimeError('服务器部署失败，请检查云控制台或服务器服务状态')
    certificate=stdout[stdout.index('-----BEGIN CERTIFICATE-----'):]
    (app/'gateway-ca.pem').write_text(certificate,encoding='ascii')
    (app/'gateway-client.json').write_text(json.dumps({'url':'https://120.53.104.174'},indent=2),encoding='utf-8')
    sys.path.insert(0,str(app))
    from backend.v060.vault import Vault
    Vault().set('gateway',token)
    transport.close()
    context=ssl.create_default_context(cafile=str(app/'gateway-ca.pem'))
    try:
        with urlopen('https://120.53.104.174/health',context=context,timeout=10) as response:
            healthy=json.load(response).get('ok') is True
    except Exception:
        healthy=False
    progress('https_ready' if healthy else 'cloud_firewall_pending')
    print('服务器部署完成，证书已信任，访问凭证已加密保存。')
    print('公网 HTTPS 验证成功。' if healthy else '公网尚未连通；请在云安全组放行 TCP 80/443 后重试。')
    print('重启项目后，在服务配置中测试地图和 AI 连接。')

if __name__=='__main__':
    try:main()
    except Exception as exc:
        status=Path(__file__).resolve().parents[1]/'_debug/gateway-deploy-status.json'
        try:previous=json.loads(status.read_text())
        except (OSError,ValueError):previous={}
        kind=type(exc).__name__
        status.write_text(json.dumps({'stage':'failed','failedAt':previous.get('stage','unknown'),'errorType':kind}),encoding='utf-8')
        messages={'AuthenticationException':'SSH 认证失败，请确认 ubuntu 用户的登录密码。',
                  'BadAuthenticationType':'服务器不支持密码登录，请检查 SSH 登录配置。',
                  'TimeoutError':'连接超时，请检查网络和服务器防火墙。',
                  'NoValidConnectionsError':'无法连接服务器 SSH 端口，请检查服务器状态。'}
        print(str(exc) if isinstance(exc,RuntimeError) else messages.get(kind,'连接或部署失败（'+kind+'），未输出凭据。'))
        sys.exit(1)
