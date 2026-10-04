"""Exact v1 wire identities and bounded normalized streaming output for application exporters."""
import hashlib
import json
import pathlib
import struct
import os

VERSION = '1.0.0'
MAX_RECORD = 8 * 1024 * 1024

def canonical(value):
    def check(v, depth=0):
        if depth > 128: raise ValueError('JSON nesting limit')
        if isinstance(v, float): raise ValueError('encode floating observations as typed bits')
        if isinstance(v, int) and not isinstance(v, bool) and abs(v) > 9007199254740991:
            raise ValueError('encode large integers as strings')
        if isinstance(v, str): v.encode('utf-8', 'strict')
        elif isinstance(v, (list, tuple)):
            for x in v: check(x, depth+1)
        elif isinstance(v, dict):
            for k, x in v.items():
                if not isinstance(k, str): raise ValueError('JSON key must be text')
                check(k, depth+1); check(x, depth+1)
        elif v is not None and not isinstance(v, (int, bool)): raise ValueError('unsupported JSON value')
    check(value)
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':'), allow_nan=False).encode('utf-8')

def sha(raw): return hashlib.sha256(raw).hexdigest()
def digest(value): return sha(canonical(value))
def identity(kind, *parts):
    raw = b'MUDCRAB-KG-ID-v1\0'
    for part in (kind, *parts):
        data = part.encode('utf-8', 'strict'); raw += struct.pack('>Q', len(data)) + data
    return kind + '_' + sha(raw)
def float_value(value, bits=64):
    fmt = '>f' if bits == 32 else '>d'
    return {'type': 'f'+str(bits), 'bits': '0x'+struct.pack(fmt, value).hex(), 'display': str(value)}
def file_hash(path):
    h=hashlib.sha256()
    with pathlib.Path(path).open('rb') as f:
        for block in iter(lambda: f.read(1024*1024), b''): h.update(block)
    return h.hexdigest()
def portable(path):
    if not path or '\\' in path or '\0' in path or ':' in path or path.startswith('/') or any(x in ('', '.', '..') for x in path.split('/')):
        raise ValueError('portable relative artifact path required')
    return path

class Builder:
    def __init__(self, source_id, kind, revision, content, tool, config, scope, synthetic=False, dependencies=()):
        visibility='public' if synthetic else 'restricted'
        self.source={'id':source_id,'kind':'synthetic' if synthetic else kind,'origin_uri':kind+':'+source_id,
                     'visibility':visibility,'license_expression':'MIT OR Apache-2.0' if synthetic else None,
                     'lineage_roots':[source_id],'redistribution':'with_notice' if synthetic else 'review_required'}
        deps=sorted(set(dependencies))
        self.snapshot={'id':identity('snap',source_id,revision,content,tool,VERSION,digest(config),digest(scope),digest(deps)),
                       'source_id':source_id,'revision':revision,'content_sha256':content,
                       'extraction':{'tool':tool,'version':VERSION,'configuration_sha256':digest(config),'scope_sha256':digest(scope)},
                       'scope':scope,'dependencies':deps,'visibility':visibility,'capabilities':[]}
        self.records={k:{} for k in ('artifacts','evidence','nodes','assertions')};self.bytes={};self.diagnostics=[]
    @property
    def snapshot_id(self): return self.snapshot['id']
    @property
    def visibility(self): return self.source['visibility']
    def artifact(self, path, raw, media='application/json'):
        portable(path);h=sha(raw);key=identity('art',self.snapshot_id,path,h)
        self.records['artifacts'][key]={'id':key,'path':path,'sha256':h,'media_type':media,'byte_size':len(raw),'visibility':self.visibility,'blob_sha256':h}
        self.bytes[path]=raw;return key
    def evidence(self, artifact, locator, method, provenance='extracted'):
        key=identity('ev',self.snapshot_id,artifact,digest(locator),method,provenance)
        self.records['evidence'][key]={'id':key,'artifact_id':artifact,'locator':locator,'method':method,'provenance_class':provenance,'visibility':self.visibility};return key
    def node(self, kind, logical, name, attrs, evidence):
        key=identity('node',self.snapshot_id,kind,logical)
        value={'id':key,'kind':kind,'logical_key':logical,'display_name':name,'attributes':attrs,'evidence_ids':sorted(set(evidence)),'visibility':self.visibility}
        if key in self.records['nodes'] and self.records['nodes'][key]!=value: raise ValueError('conflicting node extraction')
        self.records['nodes'][key]=value;return key
    def edge(self, subject, predicate, obj, attrs, evidence, provenance='extracted'):
        links=[{'id':x,'role':'supports'} for x in sorted(set(evidence))]
        if not links: raise ValueError('supporting evidence required')
        key=identity('as',self.snapshot_id,subject,predicate,obj,digest(attrs),digest(links),provenance)
        self.records['assertions'][key]={'id':key,'subject':subject,'predicate':predicate,'object':obj,'attributes':attrs,'evidence':links,
          'provenance_class':provenance,'initial_state':'proposed' if provenance in ('manual','hypothesis') else 'admitted','visibility':self.visibility};return key
    def diagnostic(self, code, message, selector, capability, severity='warning'):
        self.diagnostics.append({'code':code,'severity':severity,'message':message,'source_selector':selector,'capability':capability})
    def capability(self, name, attempted, failed=0, skipped=0, detail='', status=None):
        self.snapshot['capabilities'].append({'name':name,'status':status or ('partial' if failed or skipped else 'complete'),'detail':detail,
          'counts':{'inputs_enumerated':None if skipped is None else attempted+skipped,'inputs_attempted':attempted,'records_emitted':sum(map(len,self.records.values())), 'inputs_failed':failed,'inputs_skipped':skipped}})
    def write(self, destination, max_bytes=128*1024*1024):
        if not self.snapshot['capabilities']: raise ValueError('measured capability required')
        files=[];total=0;output={}
        for kind in (*self.records,'diagnostics'):
            records=self.diagnostics if kind=='diagnostics' else [self.records[kind][k] for k in sorted(self.records[kind])]
            lines=[]
            for r in records:
                raw=canonical(r)+b'\n'
                if len(raw)>MAX_RECORD: raise ValueError('record byte limit')
                lines.append(raw)
            raw=b''.join(lines);path=kind+'.jsonl';output[path]=raw;total+=len(raw)
            files.append({'kind':kind,'path':path,'sha256':sha(raw),'byte_size':len(raw),'record_count':len(records)})
        for path,raw in self.bytes.items():output['artifacts/'+path]=raw;total+=len(raw)
        manifest={'format':'mudcrab-kg-stream','schema_version':1,'source':self.source,'snapshot':self.snapshot,'files':files}
        output['manifest.json']=canonical(manifest)+b'\n';total+=len(output['manifest.json'])
        if total>max_bytes: raise ValueError('export byte limit exceeded')
        dst=pathlib.Path(destination);dst.mkdir(parents=True,exist_ok=False)
        try:
            for path,raw in output.items():
                target=dst/path;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(raw)
        except BaseException:
            import shutil
            shutil.rmtree(dst);raise
        return {'snapshot_id':self.snapshot_id,'bytes':total,'nodes':len(self.records['nodes']),'assertions':len(self.records['assertions'])}

def supervised(script):
    """Bound the application session in a disposable child process, including replay/native calls."""
    import subprocess
    import sys
    if '--kg-worker' in sys.argv:
        sys.argv.remove('--kg-worker');return False
    try:
        index=sys.argv.index('--selection');path=pathlib.Path(sys.argv[index+1])
        if path.stat().st_size>MAX_RECORD:raise ValueError('selection byte limit')
        cfg=json.loads(path.read_text());seconds=cfg.get('max_seconds',300)
        if type(seconds) is not int or not 1<=seconds<=3600:raise ValueError('invalid request timeout')
        result=subprocess.run([sys.executable,str(script),'--kg-worker',*sys.argv[1:]],timeout=seconds+60)
        sys.exit(result.returncode)
    except subprocess.TimeoutExpired:
        print('{"code":"TIMEOUT","message":"application export process exceeded selected timeout"}',file=sys.stderr);sys.exit(7)
    except (ValueError,IndexError,OSError):
        print('{"code":"INVALID_INPUT","message":"readable bounded --selection required"}',file=sys.stderr);sys.exit(2)
