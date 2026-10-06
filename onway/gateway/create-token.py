import getpass
import hashlib

token=getpass.getpass('网关访问凭证（自行生成至少 32 个随机字符，输入不显示）: ')
if len(token)<32 or len(token)>256:
    raise SystemExit('凭证长度必须为 32 至 256 个字符')
print('ONWAY_TOKEN_HASHES='+hashlib.sha256(token.encode()).hexdigest())
