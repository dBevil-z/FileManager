#!/usr/bin/env python3
"""Package a FileManager API v1 extension. No npm or Rust build is required."""
import argparse, hashlib, json, pathlib, re, zipfile
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('source',type=pathlib.Path,help='Directory containing manifest.json and main.js')
parser.add_argument('output',type=pathlib.Path,nargs='?')
args=parser.parse_args()
manifest=(args.source/'manifest.json').read_bytes()
script=(args.source/'main.js').read_bytes()
m=json.loads(manifest)
if m.get('schema_version')!=1 or not re.fullmatch(r'[a-z0-9][a-z0-9.-]{2,79}',m.get('id','')) or '..' in m['id']:
    parser.error('Expected API v1 and a valid plugin id')
if len(manifest)>16384 or len(script)>128*1024:
    parser.error('manifest.json must be <=16 KiB; main.js <=128 KiB')
if not set(m.get('permissions',[])) <= {'context:metadata','files:read','directories:scan'} or 'context:metadata' not in m.get('permissions',[]):
    parser.error('Unknown permissions, or missing context:metadata')
script.decode('utf-8')
output=args.output or pathlib.Path(f"{m['id']}-{m['version']}.fmp")
if output.exists():
    parser.error('Output already exists; use another filename')
output.parent.mkdir(parents=True,exist_ok=True)
with zipfile.ZipFile(output,'w',compression=zipfile.ZIP_DEFLATED) as z:
    for name,data in [('manifest.json',manifest),('main.js',script)]:
        info=zipfile.ZipInfo(name,date_time=(1980,1,1,0,0,0));info.external_attr=0o100644<<16;info.compress_type=zipfile.ZIP_DEFLATED
        z.writestr(info,data)
print(f'{output}\nSHA256 (package): {hashlib.sha256(output.read_bytes()).hexdigest()}')
